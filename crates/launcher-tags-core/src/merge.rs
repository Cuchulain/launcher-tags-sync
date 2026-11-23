//! Merge app tags into Quick Search base-profile app nicknames (v0.1 semantics).
//!
//! * Existing aliases stay first and verbatim; only new aliases are appended with `", "`.
//! * Trailing whitespace/commas of an existing value are dropped before appending (the rest
//!   of the string is kept byte-for-byte). Values without new aliases are not touched.
//! * Matching is NFC + Unicode-lowercase on trimmed aliases.
//! * Only `nickname_app_<package>` keys are created/modified; profile keys
//!   (`…:<userId>`), shortcut nicknames and every other value are left alone.
//! * A non-string entry under a target key aborts the merge before anything is changed.

use std::collections::HashSet;

use serde::Serialize;
use serde_json::{json, Value};

use crate::alias;
use crate::error::Error;
use crate::kvaesitso::SourceTags;
use crate::quicksearch::{
    classify_key, typed_string, KeyKind, QuickSearchBackup, APP_NICKNAME_PREFIX,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    New,
    Extended,
}

/// One planned/applied nickname change. Contains alias text: show it only to the user on
/// request, never put it in reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NicknameChange {
    pub package: String,
    pub key: String,
    pub kind: ChangeKind,
    pub added: Vec<String>,
    pub previous: Option<String>,
    pub value: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MergeStats {
    /// Existing `nickname_app_*` keys (base + profile, excluding app shortcuts) before merge.
    pub existing_app_nickname_entries: usize,
    pub new_entries: usize,
    pub extended_entries: usize,
    /// Existing app nickname keys (base + profile) left unchanged.
    pub unchanged_existing_entries: usize,
    pub added_unique_aliases: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeOutcome {
    pub changes: Vec<NicknameChange>,
    pub stats: MergeStats,
    /// Deep comparison of everything except the changed keys succeeded.
    pub preserved_non_app_settings: bool,
}

/// Compute changes without modifying the target.
pub fn plan(source: &SourceTags, target: &QuickSearchBackup) -> Result<MergeOutcome, Error> {
    let prefs = target.user_app_preferences();
    let mut stats = MergeStats {
        existing_app_nickname_entries: prefs
            .keys()
            .filter(|k| matches!(classify_key(k), KeyKind::BaseApp | KeyKind::ProfileApp))
            .count(),
        ..MergeStats::default()
    };
    let mut changes = Vec::new();

    for pkg in &source.packages {
        if pkg.aliases.is_empty() {
            continue;
        }
        let key = format!("{APP_NICKNAME_PREFIX}{}", pkg.package);
        debug_assert_eq!(classify_key(&key), KeyKind::BaseApp);
        let previous = match prefs.get(&key) {
            None => None,
            Some(v) => Some(
                typed_string(v)
                    .ok_or_else(|| Error::NicknameTypeCollision { key: key.clone() })?
                    .to_string(),
            ),
        };
        let mut seen: HashSet<String> = previous
            .as_deref()
            .map(|p| alias::split(p).into_iter().map(alias::match_key).collect())
            .unwrap_or_default();
        let added: Vec<String> = pkg
            .aliases
            .iter()
            .filter(|a| !a.trim().is_empty() && !a.contains(alias::SEPARATOR))
            .filter(|a| seen.insert(alias::match_key(a)))
            .cloned()
            .collect();
        if added.is_empty() {
            continue;
        }
        let joined = added.join(alias::JOINER);
        let (kind, value) = match &previous {
            None => (ChangeKind::New, joined),
            Some(p) => {
                let kept = p.trim_end_matches(|c: char| c == alias::SEPARATOR || c.is_whitespace());
                let value = if kept.is_empty() {
                    joined
                } else {
                    format!("{kept}{}{joined}", alias::JOINER)
                };
                (ChangeKind::Extended, value)
            }
        };
        match kind {
            ChangeKind::New => stats.new_entries += 1,
            ChangeKind::Extended => stats.extended_entries += 1,
        }
        stats.added_unique_aliases += added.len();
        changes.push(NicknameChange {
            package: pkg.package.clone(),
            key,
            kind,
            added,
            previous,
            value,
        });
    }
    stats.unchanged_existing_entries = stats.existing_app_nickname_entries - stats.extended_entries;
    Ok(MergeOutcome {
        changes,
        stats,
        preserved_non_app_settings: true,
    })
}

/// Plan and apply. On error the target is left unchanged.
pub fn merge(source: &SourceTags, target: &mut QuickSearchBackup) -> Result<MergeOutcome, Error> {
    let mut outcome = plan(source, target)?;
    let before = target.as_value().clone();
    let prefs = target.user_app_preferences_mut();
    for c in &outcome.changes {
        match (c.kind, prefs.get_mut(&c.key)) {
            // Only the "value" field changes; "type" and any unknown fields stay.
            (ChangeKind::Extended, Some(Value::Object(o))) => {
                o.insert("value".into(), Value::String(c.value.clone()));
            }
            (ChangeKind::New, None) => {
                prefs.insert(c.key.clone(), json!({"type": "string", "value": c.value}));
            }
            _ => unreachable!("plan() was computed against this target"),
        }
    }
    let keys: Vec<&str> = outcome.changes.iter().map(|c| c.key.as_str()).collect();
    outcome.preserved_non_app_settings = preserved_except(&before, target.as_value(), &keys);
    if !outcome.preserved_non_app_settings {
        // Defensive: should be impossible; never hand out a document that changed more.
        *target = QuickSearchBackup::from_value(before)?;
        return Err(Error::Schema(
            "internal error: merge changed data outside the intended nickname keys".into(),
        ));
    }
    for c in &outcome.changes {
        if typed_string(&target.user_app_preferences()[&c.key]) != Some(c.value.as_str()) {
            return Err(Error::Schema(format!(
                "internal error: '{}' not written",
                c.key
            )));
        }
    }
    Ok(outcome)
}

/// True when `a` and `b` are deep-equal after removing `keys` from
/// `preferences.user_app_preferences` in both.
pub fn preserved_except(a: &Value, b: &Value, keys: &[&str]) -> bool {
    let strip = |v: &Value| {
        let mut v = v.clone();
        if let Some(m) = v
            .get_mut("preferences")
            .and_then(|p| p.get_mut(crate::quicksearch::PREFS_FILE))
            .and_then(Value::as_object_mut)
        {
            for k in keys {
                m.shift_remove(*k);
            }
        }
        v
    };
    strip(a) == strip(b)
}
