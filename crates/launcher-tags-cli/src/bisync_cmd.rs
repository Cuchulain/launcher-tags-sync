//! `sync --direction both` (v0.2): three-way sync with a state file.

use std::io::Cursor;
use std::path::Path;

use anyhow::{bail, Context, Result};
use launcher_tags_core::bisync::{
    self, InputInfo, Options, PendingCheck, PendingPolicy, Plan, SyncState,
};
use launcher_tags_core::quicksearch::QuickSearchBackup;
use launcher_tags_core::report::BisyncReport;
use launcher_tags_core::{kvaesitso, Limits, SourceTags};

use crate::{fsio, print_warnings, PendingArg, SyncArgs};

/// Upper bound for reading a Kvaesitso backup into memory (the ZIP limits apply on top).
const MAX_KVAESITSO_BYTES: u64 = 512 * 1024 * 1024;
const MAX_STATE_BYTES: u64 = 64 * 1024 * 1024;

pub(crate) fn run(a: SyncArgs) -> Result<()> {
    let Some(state_path) = a.state.as_deref() else {
        bail!("--direction both requires --state FILE (use --init-state on the first run)");
    };
    let state_exists = state_path.symlink_metadata().is_ok();
    if a.init_state && state_exists {
        bail!(
            "--init-state given but {} already exists; drop --init-state to continue syncing with it",
            state_path.display()
        );
    }
    if !a.init_state && !state_exists {
        bail!(
            "state file {} does not exist; for the very first sync pass --init-state",
            state_path.display()
        );
    }
    if !a.dry_run && a.kvaesitso_output.is_none() {
        bail!("--direction both requires --kvaesitso-output FILE (or --dry-run)");
    }

    // ---- path checks before reading anything
    let inputs: Vec<(&str, &Path)> = vec![
        ("--kvaesitso", a.kvaesitso.as_path()),
        ("--quicksearch", a.quicksearch.as_path()),
        ("--state", state_path),
    ];
    for (i, (n1, p1)) in inputs.iter().enumerate() {
        for (n2, p2) in &inputs[i + 1..] {
            if fsio::same_file(p1, p2) {
                bail!("{n1} and {n2} point to the same file");
            }
        }
    }
    let output = if a.dry_run { None } else { a.output.as_deref() };
    let kout = if a.dry_run {
        None
    } else {
        a.kvaesitso_output.as_deref()
    };
    let mut protected = inputs.clone();
    if let Some(o) = output {
        fsio::check_writable("output", o, &protected, a.force)?;
        protected.push(("--output", o));
    }
    if let Some(k) = kout {
        fsio::check_writable("Kvaesitso output", k, &protected, a.force)?;
        protected.push(("--kvaesitso-output", k));
    }
    if let Some(r) = &a.report {
        fsio::check_writable("report", r, &protected, a.force)?;
    }

    // ---- read
    let limits = Limits::default();
    let kbytes = fsio::read_limited(&a.kvaesitso, MAX_KVAESITSO_BYTES, "Kvaesitso backup")?;
    let source = kvaesitso::read(Cursor::new(&kbytes), &limits)
        .with_context(|| format!("cannot read Kvaesitso backup {}", a.kvaesitso.display()))?;
    let qraw = fsio::read_limited(
        &a.quicksearch,
        fsio::MAX_QUICKSEARCH_BYTES,
        "Quick Search backup",
    )?;
    let target = QuickSearchBackup::parse(&qraw)
        .with_context(|| format!("cannot use Quick Search backup {}", a.quicksearch.display()))?;
    let state = if a.init_state {
        None
    } else {
        let b = fsio::read_limited(state_path, MAX_STATE_BYTES, "state file")?;
        Some(
            SyncState::parse(&b)
                .with_context(|| format!("cannot use state file {}", state_path.display()))?,
        )
    };

    // ---- plan + apply in memory
    let opts = Options {
        pending: match a.pending {
            PendingArg::Error => PendingPolicy::Error,
            PendingArg::Discard => PendingPolicy::Discard,
            PendingArg::Accept => PendingPolicy::Accept,
        },
        allow_mass_delete: a.allow_mass_delete,
    };
    let k_info = InputInfo {
        fingerprint: bisync::fingerprint(&kbytes),
        exported_at_ms: source.meta.as_ref().and_then(|m| m.timestamp),
    };
    let q_info = InputInfo {
        fingerprint: bisync::fingerprint(&qraw),
        exported_at_ms: target.exported_at_ms(),
    };
    bisync::check_freshness(state.as_ref(), &k_info, &q_info, a.allow_stale)
        .context("sync refused")?;
    let mut plan = bisync::plan(&source, &target, state.as_ref(), &opts).context("sync failed")?;
    let mut merged = target.clone();
    bisync::apply_quicksearch(&plan, &mut merged)?;
    let qbytes = merged.to_pretty_bytes()?;
    let kout_bytes = if plan.to_kvaesitso.is_empty() {
        kbytes.clone()
    } else {
        bisync::apply_kvaesitso(&plan, &source, Cursor::new(&kbytes), &limits)
            .context("cannot produce the Kvaesitso backup")?
            .0
    };
    bisync::record_run(&mut plan, &k_info, &kout_bytes, &q_info, &qbytes);
    let report = BisyncReport::build(&source, &plan, a.dry_run);

    print_warnings(&source);
    print_plan(&source, &plan, a.dry_run, a.show_aliases);

    // ---- write: outputs first, state last
    if let Some(o) = output {
        fsio::write_atomic(o, &qbytes, a.force)?;
        let back = std::fs::read(o)?;
        if back != qbytes {
            bail!("output verification failed for {}", o.display());
        }
        println!("Wrote Quick Search backup {}", o.display());
    }
    if let Some(k) = kout {
        fsio::write_atomic(k, &kout_bytes, a.force)?;
        let back = std::fs::read(k)?;
        if back != kout_bytes {
            bail!("output verification failed for {}", k.display());
        }
        kvaesitso::read(Cursor::new(&back), &limits)
            .context("written Kvaesitso backup does not read back")?;
        println!("Wrote Kvaesitso backup {}", k.display());
    }
    if !a.dry_run {
        if state_exists {
            let prev = state_path.with_file_name(format!(
                "{}.prev",
                state_path.file_name().unwrap_or_default().to_string_lossy()
            ));
            let old = std::fs::read(state_path)?;
            fsio::write_atomic(&prev, &old, true)?;
        }
        fsio::write_atomic(state_path, &plan.new_state.to_pretty_bytes(), state_exists)?;
        println!("Updated state {}", state_path.display());
        println!("Next: import BOTH generated backups on the phone before the next sync.");
    }
    if let Some(r) = &a.report {
        fsio::write_atomic(r, &report.to_pretty_json(), a.force)?;
        println!("Wrote report {}", r.display());
    }
    Ok(())
}

fn print_plan(source: &SourceTags, plan: &Plan, dry_run: bool, show: bool) {
    if dry_run {
        println!("Dry run: no backup and no state will be written.");
    }
    if plan.initial {
        println!("Initial sync: union of both sides, nothing is deleted.");
    }
    for (side, check) in [
        ("Quick Search", plan.pending_quicksearch),
        ("Kvaesitso", plan.pending_kvaesitso),
    ] {
        match check {
            PendingCheck::Discarded => {
                println!("{side}: previous output treated as NOT imported (--pending discard)")
            }
            PendingCheck::Accepted => {
                println!("{side}: current export treated as authoritative (--pending accept)")
            }
            _ => {}
        }
    }
    println!(
        "Source: {} app tag record(s) in {} package(s); skipped {} shortcut, {} comma, {} profile tag(s)",
        source.stats.app_tag_records,
        source.stats.unique_app_packages,
        source.stats.shortcut_tag_records_skipped,
        source.stats.comma_tags_skipped,
        source.stats.profile_app_tags_skipped
    );
    for (title, deltas) in [
        ("Quick Search", &plan.to_quicksearch),
        ("Kvaesitso", &plan.to_kvaesitso),
    ] {
        if deltas.is_empty() {
            println!("{title}: no changes");
            continue;
        }
        println!("{title} changes:");
        let width = deltas.iter().map(|d| d.package.len()).max().unwrap_or(0);
        for d in deltas.iter() {
            print!(
                "  {:<width$}  +{} -{}",
                d.package,
                d.added.len(),
                d.removed.len()
            );
            if show {
                if !d.added.is_empty() {
                    print!("  add [{}]", d.added.join(" | "));
                }
                if !d.removed.is_empty() {
                    print!("  remove [{}]", d.removed.join(" | "));
                }
            }
            println!();
        }
    }
    for c in &plan.conflicts {
        let side = match c.kind {
            bisync::ConflictKind::PackageMissingInKvaesitso => "Kvaesitso",
            bisync::ConflictKind::PackageMissingInQuickSearch => "Quick Search",
        };
        println!(
            "conflict: {} is missing in the {side} backup; nothing deleted for it",
            c.package
        );
    }
    if !plan.unresolvable.is_empty() {
        println!(
            "note: {} alias(es) of {} app(s) cannot become Kvaesitso tags yet (app unknown to the Kvaesitso backup); retried next run",
            plan.unresolvable_aliases,
            plan.unresolvable.len()
        );
    }
    println!(
        "Summary: Quick Search {} entr(y/ies) changed, Kvaesitso {} package(s) changed, {} deletion(s), state {} package(s)",
        plan.to_quicksearch.len(),
        plan.to_kvaesitso.len(),
        plan.removals(),
        plan.new_state.packages.len()
    );
}
