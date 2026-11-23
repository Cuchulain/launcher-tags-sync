//! In-memory entry points shared by the CLI, the FFI layer and other front-ends:
//! bytes in, bytes + plan + report out. No file system access.

use std::io::Cursor;

use crate::bisync::{self, InputInfo, Options, Plan, SyncState};
use crate::error::Error;
use crate::kvaesitso::{self, Limits, SourceTags};
use crate::merge::{self, MergeOutcome};
use crate::quicksearch::QuickSearchBackup;
use crate::report::{BisyncReport, Report};

/// Result of a one-way (Kvaesitso → Quick Search) run.
#[derive(Debug, Clone)]
pub struct OneWay {
    pub source: SourceTags,
    pub outcome: MergeOutcome,
    /// Merged Quick Search backup (validated, reparsed).
    pub quicksearch: Vec<u8>,
    pub report: Report,
}

pub fn one_way(kvaesitso: &[u8], quicksearch: &[u8], dry_run: bool) -> Result<OneWay, Error> {
    let source = kvaesitso::read(Cursor::new(kvaesitso), &Limits::default())?;
    let mut target = QuickSearchBackup::parse(quicksearch)?;
    let outcome = merge::merge(&source, &mut target)?;
    let bytes = target.to_pretty_bytes()?;
    let report = Report::build(&source, &outcome, dry_run);
    Ok(OneWay {
        source,
        outcome,
        quicksearch: bytes,
        report,
    })
}

/// Settings of a two-way run.
#[derive(Debug, Clone, Copy, Default)]
pub struct BothSettings {
    pub options: Options,
    pub allow_stale: bool,
    pub dry_run: bool,
}

/// Result of a two-way run. Persist `state` only after both backups were written.
#[derive(Debug, Clone)]
pub struct Both {
    pub source: SourceTags,
    pub plan: Plan,
    pub quicksearch: Vec<u8>,
    /// Rewritten Kvaesitso backup (the unchanged input if nothing changed for Kvaesitso).
    pub kvaesitso: Vec<u8>,
    pub state: Vec<u8>,
    pub report: BisyncReport,
}

/// `state == None` means first run (`--init-state`).
pub fn both(
    kvaesitso: &[u8],
    quicksearch: &[u8],
    state: Option<&[u8]>,
    settings: &BothSettings,
) -> Result<Both, Error> {
    let limits = Limits::default();
    let source = kvaesitso::read(Cursor::new(kvaesitso), &limits)?;
    let target = QuickSearchBackup::parse(quicksearch)?;
    let state = state.map(SyncState::parse).transpose()?;

    let k_info = InputInfo {
        fingerprint: bisync::fingerprint(kvaesitso),
        exported_at_ms: source.meta.as_ref().and_then(|m| m.timestamp),
    };
    let q_info = InputInfo {
        fingerprint: bisync::fingerprint(quicksearch),
        exported_at_ms: target.exported_at_ms(),
    };
    bisync::check_freshness(state.as_ref(), &k_info, &q_info, settings.allow_stale)?;
    let mut plan = bisync::plan(&source, &target, state.as_ref(), &settings.options)?;

    let mut merged = target;
    bisync::apply_quicksearch(&plan, &mut merged)?;
    let q_out = merged.to_pretty_bytes()?;
    let k_out = if plan.to_kvaesitso.is_empty() {
        kvaesitso.to_vec()
    } else {
        bisync::apply_kvaesitso(&plan, &source, Cursor::new(kvaesitso), &limits)?.0
    };
    bisync::record_run(&mut plan, &k_info, &k_out, &q_info, &q_out);
    let report = BisyncReport::build(&source, &plan, settings.dry_run);
    let state_bytes = plan.new_state.to_pretty_bytes();
    Ok(Both {
        source,
        plan,
        quicksearch: q_out,
        kvaesitso: k_out,
        state: state_bytes,
        report,
    })
}
