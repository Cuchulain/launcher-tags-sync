//! UniFFI bindings for the Android app. Thin wrapper over `launcher_tags_core::run`:
//! byte arrays in, byte arrays + plain records out. No file system access here; the app
//! reads/writes through the Storage Access Framework.

use launcher_tags_core::bisync::{
    Conflict, ConflictKind, Options, PackageDelta, PendingCheck, PendingPolicy,
};
use launcher_tags_core::merge::ChangeKind;
use launcher_tags_core::quicksearch::{QuickSearchBackup, SETTINGS_ITEM};
use launcher_tags_core::run::{self, BothSettings};
use launcher_tags_core::{kvaesitso, Error, Limits, SourceTags};

uniffi::setup_scaffolding!();

/// Error shown to the user. `kind` is stable for UI decisions, `reason` is English text
/// without backup values. (Not named `message`: that clashes with Kotlin's
/// `Throwable.message` in the generated exception class.)
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum SyncError {
    #[error("{reason}")]
    Failed { kind: ErrorKind, reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ErrorKind {
    /// Malformed or unsupported backup.
    InvalidInput,
    /// Quick Search export lacks the Settings section.
    SettingsNotExported,
    /// An input predates the previous sync.
    StaleInput,
    /// The previous outputs were apparently not imported.
    PendingNotImported,
    /// Too many deletions in one run.
    MassDelete,
    /// A nickname preference has an unexpected type.
    TypeCollision,
    /// Sync state file problem.
    State,
    Internal,
}

impl From<Error> for SyncError {
    fn from(e: Error) -> Self {
        let reason = e.to_string();
        let kind = match &e {
            Error::StaleInput { .. } => ErrorKind::StaleInput,
            Error::PendingNotImported { .. } => ErrorKind::PendingNotImported,
            Error::MassDelete { .. } => ErrorKind::MassDelete,
            Error::NicknameTypeCollision { .. } => ErrorKind::TypeCollision,
            Error::State(_) => ErrorKind::State,
            Error::Schema(m) if m.contains(SETTINGS_ITEM) => ErrorKind::SettingsNotExported,
            Error::Io(_) => ErrorKind::Internal,
            _ => ErrorKind::InvalidInput,
        };
        SyncError::Failed { kind, reason }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PendingMode {
    Stop,
    Discard,
    Accept,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PackageChange {
    pub package: String,
    /// True when a new nickname entry is created (one-way mode).
    pub new_entry: bool,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct KvaesitsoInfo {
    pub format: Option<String>,
    pub app_version: Option<String>,
    pub exported_at_ms: Option<i64>,
    pub pages: u32,
    pub app_tag_records: u32,
    pub app_packages: u32,
    pub shortcut_tags_skipped: u32,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct QuickSearchInfo {
    pub exported_at_ms: Option<i64>,
    pub app_nicknames: u32,
    pub profile_app_nicknames: u32,
    pub selected_export_items: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OneWayResult {
    pub quicksearch: Vec<u8>,
    pub report_json: String,
    pub changes: Vec<PackageChange>,
    pub new_entries: u32,
    pub extended_entries: u32,
    pub added_aliases: u32,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BothResult {
    pub initial: bool,
    pub quicksearch: Vec<u8>,
    pub kvaesitso: Vec<u8>,
    /// New state; persist only after both backups were saved.
    pub state: Vec<u8>,
    pub report_json: String,
    pub to_quicksearch: Vec<PackageChange>,
    pub to_kvaesitso: Vec<PackageChange>,
    pub deletions: u32,
    /// Human-readable conflict descriptions (package names only).
    pub conflicts: Vec<String>,
    pub unresolvable_apps: Vec<String>,
    pub pending_note: Option<String>,
    pub warnings: Vec<String>,
}

fn n(x: usize) -> u32 {
    u32::try_from(x).unwrap_or(u32::MAX)
}

fn warnings(s: &SourceTags) -> Vec<String> {
    s.warnings
        .iter()
        .map(|w| format!("{} ({})", w, w.category.human()))
        .collect()
}

fn delta(d: &PackageDelta) -> PackageChange {
    PackageChange {
        package: d.package.clone(),
        new_entry: false,
        added: d.added.clone(),
        removed: d.removed.clone(),
    }
}

fn conflict(c: &Conflict) -> String {
    let side = match c.kind {
        ConflictKind::PackageMissingInKvaesitso => "Kvaesitso",
        ConflictKind::PackageMissingInQuickSearch => "Quick Search",
    };
    format!(
        "{} is missing in the {side} backup; nothing deleted",
        c.package
    )
}

#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[uniffi::export]
pub fn inspect_kvaesitso(bytes: Vec<u8>) -> Result<KvaesitsoInfo, SyncError> {
    let s = kvaesitso::read(std::io::Cursor::new(&bytes), &Limits::default())?;
    Ok(KvaesitsoInfo {
        format: s.meta.as_ref().and_then(|m| m.format.clone()),
        app_version: s.meta.as_ref().and_then(|m| m.version_name.clone()),
        exported_at_ms: s.meta.as_ref().and_then(|m| m.timestamp),
        pages: n(s.pages.len()),
        app_tag_records: n(s.stats.app_tag_records),
        app_packages: n(s.stats.unique_app_packages),
        shortcut_tags_skipped: n(s.stats.shortcut_tag_records_skipped),
        warnings: warnings(&s),
    })
}

#[uniffi::export]
pub fn inspect_quicksearch(bytes: Vec<u8>) -> Result<QuickSearchInfo, SyncError> {
    let t = QuickSearchBackup::parse(&bytes)?;
    let s = t.summary();
    Ok(QuickSearchInfo {
        exported_at_ms: t.exported_at_ms(),
        app_nicknames: n(s.base_app_nicknames),
        profile_app_nicknames: n(s.profile_app_nicknames),
        selected_export_items: s.selected_export_items,
    })
}

/// One-way Kvaesitso → Quick Search (v0.1 semantics).
#[uniffi::export]
pub fn sync_one_way(kvaesitso: Vec<u8>, quicksearch: Vec<u8>) -> Result<OneWayResult, SyncError> {
    let r = run::one_way(&kvaesitso, &quicksearch, false)?;
    let report_json = String::from_utf8(r.report.to_pretty_json()).unwrap_or_default();
    Ok(OneWayResult {
        changes: r
            .outcome
            .changes
            .iter()
            .map(|c| PackageChange {
                package: c.package.clone(),
                new_entry: c.kind == ChangeKind::New,
                added: c.added.clone(),
                removed: Vec::new(),
            })
            .collect(),
        new_entries: n(r.outcome.stats.new_entries),
        extended_entries: n(r.outcome.stats.extended_entries),
        added_aliases: n(r.outcome.stats.added_unique_aliases),
        warnings: warnings(&r.source),
        quicksearch: r.quicksearch,
        report_json,
    })
}

/// Two-way sync (v0.2). `state == None` starts a new sync (union, no deletions).
#[uniffi::export]
pub fn sync_both(
    kvaesitso: Vec<u8>,
    quicksearch: Vec<u8>,
    state: Option<Vec<u8>>,
    pending: PendingMode,
    allow_mass_delete: bool,
    allow_stale: bool,
) -> Result<BothResult, SyncError> {
    let settings = BothSettings {
        options: Options {
            pending: match pending {
                PendingMode::Stop => PendingPolicy::Error,
                PendingMode::Discard => PendingPolicy::Discard,
                PendingMode::Accept => PendingPolicy::Accept,
            },
            allow_mass_delete,
        },
        allow_stale,
        dry_run: false,
    };
    let r = run::both(&kvaesitso, &quicksearch, state.as_deref(), &settings)?;
    let pending_note = [
        ("Quick Search", r.plan.pending_quicksearch),
        ("Kvaesitso", r.plan.pending_kvaesitso),
    ]
    .iter()
    .filter_map(|(side, c)| match c {
        PendingCheck::Discarded => Some(format!("{side}: previous output treated as not imported")),
        PendingCheck::Accepted => Some(format!("{side}: current export treated as authoritative")),
        _ => None,
    })
    .reduce(|a, b| format!("{a}; {b}"));
    Ok(BothResult {
        initial: r.plan.initial,
        report_json: String::from_utf8(r.report.to_pretty_json()).unwrap_or_default(),
        to_quicksearch: r.plan.to_quicksearch.iter().map(delta).collect(),
        to_kvaesitso: r.plan.to_kvaesitso.iter().map(delta).collect(),
        deletions: n(r.plan.removals()),
        conflicts: r.plan.conflicts.iter().map(conflict).collect(),
        unresolvable_apps: r.plan.unresolvable.clone(),
        pending_note,
        warnings: warnings(&r.source),
        quicksearch: r.quicksearch,
        kvaesitso: r.kvaesitso,
        state: r.state,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(rel: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/../../fixtures/{rel}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    #[test]
    fn one_way_basic() {
        let r = sync_one_way(
            fixture("basic/input.kvaesitso"),
            fixture("basic/input.quicksearch"),
        )
        .unwrap();
        assert_eq!(
            (r.new_entries, r.extended_entries, r.added_aliases),
            (2, 2, 5)
        );
        let out: serde_json::Value = serde_json::from_slice(&r.quicksearch).unwrap();
        let exp: serde_json::Value =
            serde_json::from_slice(&fixture("basic/expected.quicksearch")).unwrap();
        assert_eq!(out, exp);
    }

    #[test]
    fn both_then_noop() {
        let k = fixture("basic/input.kvaesitso");
        let q = fixture("basic/input.quicksearch");
        let r1 = sync_both(k, q, None, PendingMode::Stop, false, false).unwrap();
        assert!(r1.initial);
        assert_eq!(r1.unresolvable_apps, vec!["org.example.unrelated"]);
        let r2 = sync_both(
            r1.kvaesitso,
            r1.quicksearch,
            Some(r1.state),
            PendingMode::Stop,
            false,
            false,
        )
        .unwrap();
        assert!(r2.to_quicksearch.is_empty() && r2.to_kvaesitso.is_empty());
    }

    #[test]
    fn errors_are_classified() {
        let e = sync_one_way(
            fixture("invalid/missing-customizations.kvaesitso"),
            fixture("basic/input.quicksearch"),
        )
        .unwrap_err();
        assert!(matches!(
            e,
            SyncError::Failed {
                kind: ErrorKind::InvalidInput,
                ..
            }
        ));
        let q = br#"{"formatVersion":2,"preferences":{"user_app_preferences":{}},"selectedExportItems":["NOTES"]}"#;
        let e = inspect_quicksearch(q.to_vec()).unwrap_err();
        assert!(matches!(
            e,
            SyncError::Failed {
                kind: ErrorKind::SettingsNotExported,
                ..
            }
        ));
    }
}
