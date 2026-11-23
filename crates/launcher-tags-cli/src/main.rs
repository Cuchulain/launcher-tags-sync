//! `launcher-tags-sync` — one-way Kvaesitso → Quick Search app tag conversion.

mod fsio;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::{ArgGroup, Args, Parser, Subcommand};
use launcher_tags_core::{
    kvaesitso, merge, quicksearch::QuickSearchBackup, report::Report, Limits, SourceTags,
};

const AFTER_HELP: &str = "\
Direction: v0.1 is ONE-WAY, Kvaesitso -> Quick Search. Quick Search nicknames are never
written back to Kvaesitso, and existing nicknames are never removed.

Privacy: backups are read locally; reports and warnings contain counts, categories and
package names only, never tag values, notes or other settings.";

#[derive(Parser)]
#[command(
    name = "launcher-tags-sync",
    version,
    about = "One-way conversion of Kvaesitso app tags into Quick Search app nicknames",
    after_help = AFTER_HELP
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show the structure and counts of a backup (never prints values)
    Inspect(InspectArgs),
    /// One-way: add Kvaesitso app tags as Quick Search app nicknames (Kvaesitso -> Quick Search)
    #[command(
        long_about = "One-way merge (Kvaesitso -> Quick Search, despite the command name).\n\n\
Reads app tags from a Kvaesitso backup and appends them as aliases to Quick Search app \
nicknames (preferences.user_app_preferences[\"nickname_app_<package>\"]). Existing aliases \
stay first and verbatim; all other Quick Search data is preserved. App shortcut tags, \
work-profile app tags and tags containing commas are skipped with a count/warning.\n\n\
The inputs are never modified. The output is written atomically and only replaces an \
existing file with --force.",
        after_help = AFTER_HELP
    )]
    Sync(SyncArgs),
}

#[derive(Args)]
#[command(group(ArgGroup::new("input").required(true).args(["kvaesitso", "quicksearch"])))]
struct InspectArgs {
    /// Kvaesitso backup (*.kvaesitso, ZIP)
    #[arg(long, value_name = "FILE")]
    kvaesitso: Option<PathBuf>,
    /// Quick Search backup (*.quicksearch, JSON formatVersion 2)
    #[arg(long, value_name = "FILE")]
    quicksearch: Option<PathBuf>,
    /// Print machine-readable JSON instead of text
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct SyncArgs {
    /// Source: Kvaesitso backup (*.kvaesitso, ZIP); read only
    #[arg(long, value_name = "FILE")]
    kvaesitso: PathBuf,
    /// Target template: Quick Search backup (*.quicksearch); read only
    #[arg(long, value_name = "FILE")]
    quicksearch: PathBuf,
    /// Where to write the merged Quick Search backup (must not be an input)
    #[arg(long, value_name = "FILE", required_unless_present = "dry_run")]
    output: Option<PathBuf>,
    /// Write a JSON report with counts and warnings (no backup values)
    #[arg(long, value_name = "FILE")]
    report: Option<PathBuf>,
    /// Show planned changes without writing the output backup
    #[arg(long)]
    dry_run: bool,
    /// Allow replacing an existing --output / --report file (never an input)
    #[arg(long)]
    force: bool,
    /// Also print the alias texts being added (to your terminal only)
    #[arg(long)]
    show_aliases: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Inspect(a) => inspect(a),
        Command::Sync(a) => sync(a),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn read_source(path: &Path) -> Result<SourceTags> {
    kvaesitso::read_file(path, &Limits::default())
        .with_context(|| format!("cannot read Kvaesitso backup {}", path.display()))
}

fn read_target(path: &Path) -> Result<QuickSearchBackup> {
    let bytes = fsio::read_limited(path, fsio::MAX_QUICKSEARCH_BYTES, "Quick Search backup")?;
    QuickSearchBackup::parse(&bytes)
        .with_context(|| format!("cannot use Quick Search backup {}", path.display()))
}

fn print_warnings(source: &SourceTags) {
    for w in &source.warnings {
        eprintln!("warning: {} ({})", w, w.category.human());
    }
}

fn inspect(a: InspectArgs) -> Result<()> {
    if let Some(path) = a.kvaesitso {
        let s = read_source(&path)?;
        if a.json {
            let v = serde_json::json!({
                "meta": s.meta,
                "customization_pages": s.pages,
                "stats": s.stats,
                "warnings": s.warnings,
            });
            println!("{}", serde_json::to_string_pretty(&v)?);
            return Ok(());
        }
        let st = &s.stats;
        println!("Kvaesitso backup: {}", path.display());
        match &s.meta {
            Some(m) => println!(
                "  backup format: {} (app version {})",
                m.format.as_deref().unwrap_or("?"),
                m.version_name.as_deref().unwrap_or("?")
            ),
            None => println!("  backup format: unknown (no meta)"),
        }
        println!("  customizations pages: {}", s.pages.len());
        println!("  customization records: {}", st.customization_records);
        println!("  app tag records: {}", st.app_tag_records);
        println!("  app packages with tags: {}", st.unique_app_packages);
        println!(
            "  unique app tags (after filtering): {}",
            st.accepted_app_tags
        );
        println!(
            "  duplicate app tag records: {}",
            st.duplicate_app_tag_records
        );
        println!(
            "  shortcut tag records (ignored): {}",
            st.shortcut_tag_records_skipped
        );
        println!(
            "  work-profile app tags (skipped): {}",
            st.profile_app_tags_skipped
        );
        println!(
            "  malformed app keys (skipped): {}",
            st.invalid_app_keys_skipped
        );
        println!(
            "  comma-containing tags (skipped): {}",
            st.comma_tags_skipped
        );
        println!("  blank tags (skipped): {}", st.blank_tags_skipped);
        println!(
            "  tags on non-app items (ignored): {}",
            st.other_tag_records_skipped
        );
        println!(
            "  non-tag customizations (ignored): {}",
            st.non_tag_records_ignored
        );
        println!(
            "  malformed records (skipped): {}",
            st.malformed_records_skipped
        );
        print_warnings(&s);
    }
    if let Some(path) = a.quicksearch {
        let t = read_target(&path)?;
        let s = t.summary();
        if a.json {
            println!("{}", serde_json::to_string_pretty(&s)?);
            return Ok(());
        }
        println!("Quick Search backup: {}", path.display());
        println!("  formatVersion: {}", s.format_version);
        println!("  root sections: {}", s.root_sections.join(", "));
        match &s.selected_export_items {
            Some(items) => println!("  selectedExportItems: {}", items.join(", ")),
            None => println!("  selectedExportItems: (absent, legacy full restore)"),
        }
        for (file, n) in &s.preference_files {
            println!("  preferences.{file}: {n} entries");
        }
        println!("  app nicknames (base profile): {}", s.base_app_nicknames);
        println!(
            "  app nicknames (work/clone profiles): {}",
            s.profile_app_nicknames
        );
        println!("  app shortcut nicknames: {}", s.shortcut_nicknames);
        println!("  other nicknames: {}", s.other_nicknames);
    }
    Ok(())
}

fn sync(a: SyncArgs) -> Result<()> {
    let inputs: [(&str, &Path); 2] = [
        ("--kvaesitso", a.kvaesitso.as_path()),
        ("--quicksearch", a.quicksearch.as_path()),
    ];
    if fsio::same_file(&a.kvaesitso, &a.quicksearch) {
        bail!("--kvaesitso and --quicksearch point to the same file");
    }
    // All path checks happen before anything is read or written.
    let output = if a.dry_run { None } else { a.output.as_deref() };
    if let Some(out) = output {
        fsio::check_writable("output", out, &inputs, a.force)?;
    }
    if let Some(rep) = &a.report {
        let mut protected: Vec<(&str, &Path)> = inputs.to_vec();
        if let Some(out) = a.output.as_deref() {
            protected.push(("--output", out));
        }
        fsio::check_writable("report", rep, &protected, a.force)?;
    }

    let source = read_source(&a.kvaesitso)?;
    let mut target = read_target(&a.quicksearch)?;
    let outcome = merge::merge(&source, &mut target).context("merge failed")?;
    let bytes = target.to_pretty_bytes()?;
    let report = Report::build(&source, &outcome, a.dry_run);

    print_warnings(&source);
    print_plan(&source, &outcome, a.dry_run, a.show_aliases);

    if let Some(out) = output {
        fsio::write_atomic(out, &bytes, a.force)?;
        // Re-read what landed on disk and validate it once more before reporting success.
        let written = std::fs::read(out)
            .with_context(|| format!("cannot re-read written output {}", out.display()))?;
        if written != bytes {
            bail!(
                "output verification failed: {} differs from what was written",
                out.display()
            );
        }
        QuickSearchBackup::parse(&written).context("output verification failed")?;
        println!("Wrote {}", out.display());
    }
    if let Some(rep) = &a.report {
        fsio::write_atomic(rep, &report.to_pretty_json(), a.force)?;
        println!("Wrote report {}", rep.display());
    }
    Ok(())
}

fn print_plan(source: &SourceTags, o: &merge::MergeOutcome, dry_run: bool, show: bool) {
    if dry_run {
        println!("Dry run: no backup will be written.");
    }
    let s = &source.stats;
    println!(
        "Source: {} customization page(s), {} app tag record(s), {} package(s); skipped {} shortcut tag(s), {} malformed key(s), {} comma tag(s), {} profile tag(s)",
        source.pages.len(),
        s.app_tag_records,
        s.unique_app_packages,
        s.shortcut_tag_records_skipped,
        s.invalid_app_keys_skipped,
        s.comma_tags_skipped,
        s.profile_app_tags_skipped,
    );
    if o.changes.is_empty() {
        println!("No nickname changes needed.");
    } else {
        println!(
            "{} nickname change(s):",
            if dry_run { "Planned" } else { "Applied" }
        );
        let width = o.changes.iter().map(|c| c.package.len()).max().unwrap_or(0);
        for c in &o.changes {
            let kind = match c.kind {
                merge::ChangeKind::New => "new",
                merge::ChangeKind::Extended => "extended",
            };
            let n = c.added.len();
            print!(
                "  {kind:<8}  {:<width$}  +{n} alias{}",
                c.package,
                if n == 1 { "" } else { "es" }
            );
            if show {
                print!("  [{}]", c.added.join(" | "));
            }
            println!();
        }
    }
    let m = &o.stats;
    println!(
        "Summary: {} new, {} extended, {} existing app nickname(s) unchanged, {} alias(es) added",
        m.new_entries, m.extended_entries, m.unchanged_existing_entries, m.added_unique_aliases
    );
}
