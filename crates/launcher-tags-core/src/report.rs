//! Redacted, counts-only JSON report. Never contains tag values, nicknames or any other
//! backup content; warnings carry only a category, a fixed message and structural locators.

use serde::Serialize;

use crate::kvaesitso::SourceTags;
use crate::merge::MergeOutcome;
use crate::warning::Warning;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    pub tool: &'static str,
    pub tool_version: &'static str,
    pub direction: &'static str,
    pub policy: &'static str,
    pub dry_run: bool,
    pub source_format: Option<String>,
    pub source_version_name: Option<String>,
    pub source_customization_pages: usize,
    pub source_customization_records: usize,
    pub source_app_tag_records: usize,
    pub source_unique_app_packages: usize,
    pub source_accepted_app_tags: usize,
    pub source_shortcut_tag_records_skipped: usize,
    pub source_duplicate_app_tag_records: usize,
    pub invalid_app_keys_skipped: usize,
    pub comma_tags_skipped: usize,
    pub blank_tags_skipped: usize,
    pub profile_app_tags_skipped: usize,
    pub malformed_records_skipped: usize,
    pub other_tag_records_skipped: usize,
    pub non_tag_records_ignored: usize,
    pub existing_app_nickname_entries: usize,
    pub new_app_nickname_entries: usize,
    pub extended_app_nickname_entries: usize,
    pub unchanged_existing_app_nickname_entries: usize,
    pub added_unique_aliases: usize,
    pub preserved_non_app_settings: bool,
    pub warnings: Vec<Warning>,
}

impl Report {
    pub fn build(source: &SourceTags, outcome: &MergeOutcome, dry_run: bool) -> Self {
        let s = &source.stats;
        let m = &outcome.stats;
        Report {
            tool: "launcher-tags-sync",
            tool_version: env!("CARGO_PKG_VERSION"),
            direction: "kvaesitso_to_quicksearch",
            policy: crate::POLICY,
            dry_run,
            source_format: source.meta.as_ref().and_then(|m| m.format.clone()),
            source_version_name: source.meta.as_ref().and_then(|m| m.version_name.clone()),
            source_customization_pages: source.pages.len(),
            source_customization_records: s.customization_records,
            source_app_tag_records: s.app_tag_records,
            source_unique_app_packages: s.unique_app_packages,
            source_accepted_app_tags: s.accepted_app_tags,
            source_shortcut_tag_records_skipped: s.shortcut_tag_records_skipped,
            source_duplicate_app_tag_records: s.duplicate_app_tag_records,
            invalid_app_keys_skipped: s.invalid_app_keys_skipped,
            comma_tags_skipped: s.comma_tags_skipped,
            blank_tags_skipped: s.blank_tags_skipped,
            profile_app_tags_skipped: s.profile_app_tags_skipped,
            malformed_records_skipped: s.malformed_records_skipped,
            other_tag_records_skipped: s.other_tag_records_skipped,
            non_tag_records_ignored: s.non_tag_records_ignored,
            existing_app_nickname_entries: m.existing_app_nickname_entries,
            new_app_nickname_entries: m.new_entries,
            extended_app_nickname_entries: m.extended_entries,
            unchanged_existing_app_nickname_entries: m.unchanged_existing_entries,
            added_unique_aliases: m.added_unique_aliases,
            preserved_non_app_settings: outcome.preserved_non_app_settings,
            warnings: source.warnings.clone(),
        }
    }

    pub fn to_pretty_json(&self) -> Vec<u8> {
        let mut v = serde_json::to_vec_pretty(self).expect("report serializes");
        v.push(b'\n');
        v
    }
}

/// Counts-only report for `sync --direction both`. Package names appear only in
/// `conflicts` and `unresolvable_apps`; alias texts never appear.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BisyncReport {
    pub tool: &'static str,
    pub tool_version: &'static str,
    pub direction: &'static str,
    pub dry_run: bool,
    pub initial_sync: bool,
    pub source_customization_pages: usize,
    pub source_app_tag_records: usize,
    pub source_unique_app_packages: usize,
    pub source_shortcut_tag_records_skipped: usize,
    pub comma_tags_skipped: usize,
    pub profile_app_tags_skipped: usize,
    pub quicksearch_nickname_entries_changed: usize,
    pub aliases_added_to_quicksearch: usize,
    pub aliases_removed_from_quicksearch: usize,
    pub kvaesitso_packages_changed: usize,
    pub tags_added_to_kvaesitso: usize,
    pub tags_removed_from_kvaesitso: usize,
    pub unresolvable_apps: Vec<String>,
    pub unresolvable_aliases: usize,
    pub conflicts: Vec<crate::bisync::Conflict>,
    pub pending_quicksearch: crate::bisync::PendingCheck,
    pub pending_kvaesitso: crate::bisync::PendingCheck,
    pub state_packages: usize,
    pub state_aliases: usize,
    pub warnings: Vec<Warning>,
}

impl BisyncReport {
    pub fn build(source: &SourceTags, plan: &crate::bisync::Plan, dry_run: bool) -> Self {
        let sum = |v: &[crate::bisync::PackageDelta],
                   f: fn(&crate::bisync::PackageDelta) -> usize| {
            v.iter().map(f).sum()
        };
        let s = &source.stats;
        BisyncReport {
            tool: "launcher-tags-sync",
            tool_version: env!("CARGO_PKG_VERSION"),
            direction: "both",
            dry_run,
            initial_sync: plan.initial,
            source_customization_pages: source.pages.len(),
            source_app_tag_records: s.app_tag_records,
            source_unique_app_packages: s.unique_app_packages,
            source_shortcut_tag_records_skipped: s.shortcut_tag_records_skipped,
            comma_tags_skipped: s.comma_tags_skipped,
            profile_app_tags_skipped: s.profile_app_tags_skipped,
            quicksearch_nickname_entries_changed: plan.to_quicksearch.len(),
            aliases_added_to_quicksearch: sum(&plan.to_quicksearch, |d| d.added.len()),
            aliases_removed_from_quicksearch: sum(&plan.to_quicksearch, |d| d.removed.len()),
            kvaesitso_packages_changed: plan.to_kvaesitso.len(),
            tags_added_to_kvaesitso: sum(&plan.to_kvaesitso, |d| d.added.len()),
            tags_removed_from_kvaesitso: sum(&plan.to_kvaesitso, |d| d.removed.len()),
            unresolvable_apps: plan.unresolvable.clone(),
            unresolvable_aliases: plan.unresolvable_aliases,
            conflicts: plan.conflicts.clone(),
            pending_quicksearch: plan.pending_quicksearch,
            pending_kvaesitso: plan.pending_kvaesitso,
            state_packages: plan.new_state.packages.len(),
            state_aliases: plan.new_state.alias_count(),
            warnings: source.warnings.clone(),
        }
    }

    pub fn to_pretty_json(&self) -> Vec<u8> {
        let mut v = serde_json::to_vec_pretty(self).expect("report serializes");
        v.push(b'\n');
        v
    }
}
