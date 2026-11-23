//! v0.2 bidirectional sync: three-way merge of Kvaesitso app tags and Quick Search app
//! nicknames against a persisted agreed state. See `docs/BISYNC.md`.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::{Cursor, Read, Seek};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::alias;
use crate::error::Error;
use crate::kvaesitso::{self, valid_package, Limits, SourceTags};
use crate::kvaesitso_write::{self, RewriteStats, TagEdits};
use crate::merge::{appended_value, preserved_except};
use crate::quicksearch::{
    classify_key, typed_string, KeyKind, QuickSearchBackup, APP_NICKNAME_PREFIX,
};

pub const STATE_FORMAT: &str = "launcher-tags-sync/state";
pub const STATE_VERSION: u32 = 1;

// ---------------------------------------------------------------- state file

/// Changes written to one side by the previous run, used to detect un-imported outputs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SideChanges {
    #[serde(default)]
    pub added: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub removed: BTreeMap<String, Vec<String>>,
}

impl SideChanges {
    pub fn count(&self) -> usize {
        self.added
            .values()
            .chain(self.removed.values())
            .map(Vec::len)
            .sum()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending {
    #[serde(default)]
    pub quicksearch: SideChanges,
    #[serde(default)]
    pub kvaesitso: SideChanges,
}

/// Persisted agreed state. Contains alias texts: keep it private.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncState {
    pub format: String,
    pub version: u32,
    /// package → agreed aliases (spelling as in Quick Search)
    pub packages: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub pending: Pending,
    /// Fingerprints/timestamps of the previous run, to detect stale exports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run: Option<LastRun>,
}

/// One side of a run: what was read and what was written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSide {
    pub input_fingerprint: String,
    pub output_fingerprint: String,
    /// Export timestamp of the input (Kvaesitso `meta.timestamp`, Quick Search
    /// `exportedAtEpochMs`), ms since epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exported_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastRun {
    pub kvaesitso: RunSide,
    pub quicksearch: RunSide,
}

/// What we know about one input file for the staleness check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputInfo {
    pub fingerprint: String,
    pub exported_at_ms: Option<i64>,
}

/// Stable 64-bit FNV-1a fingerprint of file bytes (not a security hash; only used to
/// recognise files this tool has seen before).
pub fn fingerprint(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{h:016x}:{}", bytes.len())
}

fn check_side(side: &'static str, last: &RunSide, now: &InputInfo) -> Result<(), Error> {
    if now.fingerprint == last.output_fingerprint {
        return Ok(()); // the file this tool produced last time
    }
    if now.fingerprint == last.input_fingerprint {
        return Err(Error::StaleInput {
            side,
            reason: "it is the same export the previous sync started from",
        });
    }
    if let (Some(n), Some(l)) = (now.exported_at_ms, last.exported_at_ms) {
        // Outputs keep the timestamp of the export they were made from, and a real new
        // export from the phone is always later; so different content with the same or an
        // older timestamp is the old export (or a copy of it).
        if n <= l {
            return Err(Error::StaleInput {
                side,
                reason: "it is not newer than the export used by the previous sync",
            });
        }
    }
    Ok(())
}

/// Refuse inputs that predate the previous run (unless `allow_stale`).
pub fn check_freshness(
    state: Option<&SyncState>,
    kvaesitso: &InputInfo,
    quicksearch: &InputInfo,
    allow_stale: bool,
) -> Result<(), Error> {
    let Some(last) = state.and_then(|s| s.last_run.as_ref()) else {
        return Ok(());
    };
    if allow_stale {
        return Ok(());
    }
    check_side("Kvaesitso", &last.kvaesitso, kvaesitso)?;
    check_side("Quick Search", &last.quicksearch, quicksearch)
}

/// Record this run's inputs/outputs in the state that will be persisted.
pub fn record_run(
    plan: &mut Plan,
    kvaesitso: &InputInfo,
    kvaesitso_output: &[u8],
    quicksearch: &InputInfo,
    quicksearch_output: &[u8],
) {
    plan.new_state.last_run = Some(LastRun {
        kvaesitso: RunSide {
            input_fingerprint: kvaesitso.fingerprint.clone(),
            output_fingerprint: fingerprint(kvaesitso_output),
            exported_at_ms: kvaesitso.exported_at_ms,
        },
        quicksearch: RunSide {
            input_fingerprint: quicksearch.fingerprint.clone(),
            output_fingerprint: fingerprint(quicksearch_output),
            exported_at_ms: quicksearch.exported_at_ms,
        },
    });
}

impl SyncState {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let v: Value = serde_json::from_slice(bytes).map_err(|e| Error::json("sync state", &e))?;
        if v.get("format").and_then(Value::as_str) != Some(STATE_FORMAT) {
            return Err(Error::State(format!("not a {STATE_FORMAT} file")));
        }
        if v.get("version").and_then(Value::as_u64) != Some(u64::from(STATE_VERSION)) {
            return Err(Error::State(format!(
                "unsupported state version (expected {STATE_VERSION})"
            )));
        }
        let st: SyncState = serde_json::from_value(v)
            .map_err(|e| Error::State(format!("invalid structure: {e}")))?;
        for p in st.packages.keys() {
            if !valid_package(p) {
                return Err(Error::State(format!("invalid package name '{p}'")));
            }
        }
        Ok(st)
    }

    pub fn to_pretty_bytes(&self) -> Vec<u8> {
        let mut v = serde_json::to_vec_pretty(self).expect("state serializes");
        v.push(b'\n');
        v
    }

    pub fn alias_count(&self) -> usize {
        self.packages.values().map(Vec::len).sum()
    }
}

// ---------------------------------------------------------------- alias sets

/// Insertion-ordered set of aliases keyed by [`alias::match_key`]; first spelling wins.
#[derive(Debug, Clone, Default)]
struct AliasSet {
    items: Vec<(String, String)>,
    keys: HashSet<String>,
}

impl AliasSet {
    fn from_texts<'a>(texts: impl IntoIterator<Item = &'a str>) -> Self {
        let mut s = AliasSet::default();
        for t in texts {
            s.insert(t);
        }
        s
    }
    fn insert(&mut self, text: &str) {
        let t = text.trim();
        if t.is_empty() {
            return;
        }
        let k = alias::match_key(t);
        if self.keys.insert(k.clone()) {
            self.items.push((k, t.to_string()));
        }
    }
    fn contains(&self, text: &str) -> bool {
        self.keys.contains(&alias::match_key(text))
    }
    fn has_key(&self, key: &str) -> bool {
        self.keys.contains(key)
    }
    fn remove_text(&mut self, text: &str) {
        let k = alias::match_key(text);
        if self.keys.remove(&k) {
            self.items.retain(|(x, _)| *x != k);
        }
    }
    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

// ---------------------------------------------------------------- plan

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingPolicy {
    /// Stop when an output of the previous run looks un-imported.
    Error,
    /// Treat the previous output for that side as never imported.
    Discard,
    /// Treat the current export as authoritative.
    Accept,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub pending: PendingPolicy,
    pub allow_mass_delete: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            pending: PendingPolicy::Error,
            allow_mass_delete: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingCheck {
    /// No state or nothing was written to this side last time.
    NotApplicable,
    /// The export reflects (some of) the previous changes.
    Imported,
    Discarded,
    Accepted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    /// Package is in the agreed state but absent from the Kvaesitso backup.
    PackageMissingInKvaesitso,
    /// Package is in the agreed state but has no `nickname_app_<package>` key.
    PackageMissingInQuickSearch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Conflict {
    pub kind: ConflictKind,
    pub package: String,
}

/// Alias changes for one package on one side (texts; never put them in reports).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackageDelta {
    pub package: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub initial: bool,
    pub to_quicksearch: Vec<PackageDelta>,
    pub to_kvaesitso: Vec<PackageDelta>,
    pub conflicts: Vec<Conflict>,
    /// Packages that should receive Kvaesitso tags but have no known component key.
    pub unresolvable: Vec<String>,
    pub unresolvable_aliases: usize,
    pub pending_quicksearch: PendingCheck,
    pub pending_kvaesitso: PendingCheck,
    /// State to persist after both outputs have been written.
    pub new_state: SyncState,
}

impl Plan {
    pub fn is_noop(&self) -> bool {
        self.to_quicksearch.is_empty() && self.to_kvaesitso.is_empty()
    }
    pub fn removals(&self) -> usize {
        self.to_quicksearch
            .iter()
            .chain(&self.to_kvaesitso)
            .map(|d| d.removed.len())
            .sum()
    }
}

/// Current Quick Search base-profile app aliases per package (and which keys exist).
fn quicksearch_sets(
    target: &QuickSearchBackup,
) -> Result<(BTreeMap<String, AliasSet>, BTreeSet<String>), Error> {
    let mut sets = BTreeMap::new();
    let mut present = BTreeSet::new();
    for (key, v) in target.user_app_preferences() {
        if classify_key(key) != KeyKind::BaseApp {
            continue;
        }
        let pkg = &key[APP_NICKNAME_PREFIX.len()..];
        if !valid_package(pkg) {
            continue;
        }
        let value =
            typed_string(v).ok_or_else(|| Error::NicknameTypeCollision { key: key.clone() })?;
        present.insert(pkg.to_string());
        let set = AliasSet::from_texts(alias::split(value));
        if !set.is_empty() {
            sets.insert(pkg.to_string(), set);
        }
    }
    Ok((sets, present))
}

fn check_pending(
    side: &'static str,
    changes: &SideChanges,
    current: &BTreeMap<String, AliasSet>,
    state: &mut BTreeMap<String, AliasSet>,
    policy: PendingPolicy,
) -> Result<PendingCheck, Error> {
    let n = changes.count();
    if n == 0 {
        return Ok(PendingCheck::NotApplicable);
    }
    let has = |p: &str, a: &str| current.get(p).is_some_and(|s| s.contains(a));
    let visible = changes
        .added
        .iter()
        .any(|(p, aa)| aa.iter().any(|a| has(p, a)))
        || changes
            .removed
            .iter()
            .any(|(p, rr)| rr.iter().any(|r| !has(p, r)));
    if visible {
        return Ok(PendingCheck::Imported);
    }
    match policy {
        PendingPolicy::Error => Err(Error::PendingNotImported { side, count: n }),
        PendingPolicy::Accept => Ok(PendingCheck::Accepted),
        PendingPolicy::Discard => {
            // Roll this side's view of the agreed state back to before the previous run.
            for (p, aa) in &changes.added {
                if let Some(s) = state.get_mut(p) {
                    for a in aa {
                        s.remove_text(a);
                    }
                }
            }
            for (p, rr) in &changes.removed {
                let s = state.entry(p.clone()).or_default();
                for r in rr {
                    s.insert(r);
                }
            }
            Ok(PendingCheck::Discarded)
        }
    }
}

/// Compute the bidirectional plan. `state == None` means first run (union, no deletions).
pub fn plan(
    source: &SourceTags,
    target: &QuickSearchBackup,
    state: Option<&SyncState>,
    opts: &Options,
) -> Result<Plan, Error> {
    let initial = state.is_none();
    let k_sets: BTreeMap<String, AliasSet> = source
        .packages
        .iter()
        .map(|p| {
            (
                p.package.clone(),
                AliasSet::from_texts(p.aliases.iter().map(String::as_str)),
            )
        })
        .collect();
    let (q_sets, q_present) = quicksearch_sets(target)?;
    let mut s_sets: BTreeMap<String, AliasSet> = state
        .map(|st| {
            st.packages
                .iter()
                .map(|(p, aa)| {
                    (
                        p.clone(),
                        AliasSet::from_texts(aa.iter().map(String::as_str)),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    let (mut pending_quicksearch, mut pending_kvaesitso) =
        (PendingCheck::NotApplicable, PendingCheck::NotApplicable);
    if let Some(st) = state {
        pending_quicksearch = check_pending(
            "Quick Search",
            &st.pending.quicksearch,
            &q_sets,
            &mut s_sets,
            opts.pending,
        )?;
        pending_kvaesitso = check_pending(
            "Kvaesitso",
            &st.pending.kvaesitso,
            &k_sets,
            &mut s_sets,
            opts.pending,
        )?;
    }
    let state_total: usize = s_sets.values().map(|s| s.items.len()).sum();

    let empty = AliasSet::default();
    let packages: BTreeSet<&String> = k_sets
        .keys()
        .chain(q_sets.keys())
        .chain(q_present.iter())
        .chain(s_sets.keys())
        .collect();

    let mut plan = Plan {
        initial,
        to_quicksearch: Vec::new(),
        to_kvaesitso: Vec::new(),
        conflicts: Vec::new(),
        unresolvable: Vec::new(),
        unresolvable_aliases: 0,
        pending_quicksearch,
        pending_kvaesitso,
        new_state: SyncState {
            format: STATE_FORMAT.into(),
            version: STATE_VERSION,
            packages: BTreeMap::new(),
            pending: Pending::default(),
            last_run: None,
        },
    };

    for p in packages {
        let k = k_sets.get(p).unwrap_or(&empty);
        let q = q_sets.get(p).unwrap_or(&empty);
        let s = s_sets.get(p).unwrap_or(&empty);
        let k_here = source.is_present(p);
        let q_here = q_present.contains(p);

        if !s.is_empty() {
            if !k_here && !q_here {
                continue; // gone everywhere: forget
            }
            let missing = if !k_here {
                Some(ConflictKind::PackageMissingInKvaesitso)
            } else if !q_here {
                Some(ConflictKind::PackageMissingInQuickSearch)
            } else {
                None
            };
            if let Some(kind) = missing {
                plan.conflicts.push(Conflict {
                    kind,
                    package: p.clone(),
                });
                plan.new_state
                    .packages
                    .insert(p.clone(), s.items.iter().map(|(_, t)| t.clone()).collect());
                continue;
            }
        }

        let mut add_q = Vec::new();
        let mut add_k = Vec::new();
        let mut rem_q = Vec::new();
        let mut rem_k = Vec::new();
        let mut seen = HashSet::new();
        for (key, text) in k.items.iter().chain(&q.items).chain(&s.items) {
            if !seen.insert(key) {
                continue;
            }
            match (k.has_key(key), q.has_key(key), s.has_key(key)) {
                (true, true, _) | (false, false, _) => {}
                (true, false, false) => add_q.push(text.clone()),
                (false, true, false) => add_k.push(text.clone()),
                (true, false, true) => rem_k.push(text.clone()),
                (false, true, true) => rem_q.push(text.clone()),
            }
        }
        if !add_k.is_empty() && !source.components.contains_key(p) {
            plan.unresolvable.push(p.clone());
            plan.unresolvable_aliases += add_k.len();
            add_k.clear();
        }

        // Agreed result: aliases present on both sides afterwards, Quick Search spelling first.
        let mut final_k = k.clone();
        rem_k.iter().for_each(|t| final_k.remove_text(t));
        add_k.iter().for_each(|t| final_k.insert(t));
        let mut final_q = q.clone();
        rem_q.iter().for_each(|t| final_q.remove_text(t));
        add_q.iter().for_each(|t| final_q.insert(t));
        let agreed: Vec<String> = final_q
            .items
            .iter()
            .filter(|(key, _)| final_k.has_key(key))
            .map(|(_, t)| t.clone())
            .collect();
        if !agreed.is_empty() {
            plan.new_state.packages.insert(p.clone(), agreed);
        }

        let pend = &mut plan.new_state.pending;
        if !add_q.is_empty() || !rem_q.is_empty() {
            if !add_q.is_empty() {
                pend.quicksearch.added.insert(p.clone(), add_q.clone());
            }
            if !rem_q.is_empty() {
                pend.quicksearch.removed.insert(p.clone(), rem_q.clone());
            }
            plan.to_quicksearch.push(PackageDelta {
                package: p.clone(),
                added: add_q,
                removed: rem_q,
            });
        }
        if !add_k.is_empty() || !rem_k.is_empty() {
            if !add_k.is_empty() {
                pend.kvaesitso.added.insert(p.clone(), add_k.clone());
            }
            if !rem_k.is_empty() {
                pend.kvaesitso.removed.insert(p.clone(), rem_k.clone());
            }
            plan.to_kvaesitso.push(PackageDelta {
                package: p.clone(),
                added: add_k,
                removed: rem_k,
            });
        }
    }

    let removals = plan.removals();
    let limit = (state_total / 4).max(10);
    if removals > limit && !opts.allow_mass_delete {
        return Err(Error::MassDelete {
            count: removals,
            limit,
        });
    }
    Ok(plan)
}

// ---------------------------------------------------------------- apply

/// Apply the Quick Search side of `plan` (base nickname keys only). On error the target is
/// unchanged.
pub fn apply_quicksearch(plan: &Plan, target: &mut QuickSearchBackup) -> Result<(), Error> {
    let before = target.as_value().clone();
    let mut keys = Vec::new();
    let mut writes = Vec::new();
    for d in &plan.to_quicksearch {
        let key = format!("{APP_NICKNAME_PREFIX}{}", d.package);
        let prev = match target.user_app_preferences().get(&key) {
            None => None,
            Some(v) => Some(
                typed_string(v).ok_or_else(|| Error::NicknameTypeCollision { key: key.clone() })?,
            ),
        };
        let value = if d.removed.is_empty() {
            appended_value(prev, &d.added)
        } else {
            let gone: HashSet<String> = d.removed.iter().map(|r| alias::match_key(r)).collect();
            let mut items: Vec<&str> = prev
                .map(alias::split)
                .unwrap_or_default()
                .into_iter()
                .filter(|a| !gone.contains(&alias::match_key(a)))
                .collect();
            items.extend(d.added.iter().map(String::as_str));
            // An empty value (not a missing key) so that importing it clears the nickname.
            items.join(alias::JOINER)
        };
        writes.push((key.clone(), value));
        keys.push(key);
    }
    let prefs = target.user_app_preferences_mut();
    for (key, value) in writes {
        match prefs.get_mut(&key) {
            Some(Value::Object(o)) => {
                o.insert("value".into(), Value::String(value));
            }
            _ => {
                prefs.insert(key, json!({"type": "string", "value": value}));
            }
        }
    }
    let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    if !preserved_except(&before, target.as_value(), &refs) {
        *target = QuickSearchBackup::from_value(before)?;
        return Err(Error::Schema(
            "internal error: sync changed data outside the intended nickname keys".into(),
        ));
    }
    Ok(())
}

/// Tag edits for the Kvaesitso side of `plan`.
pub fn kvaesitso_edits(plan: &Plan, source: &SourceTags) -> TagEdits {
    let mut edits = TagEdits::default();
    for d in &plan.to_kvaesitso {
        if !d.added.is_empty() {
            let key = source.components[&d.package].clone();
            edits.add.insert(d.package.clone(), (key, d.added.clone()));
        }
        if !d.removed.is_empty() {
            edits.remove.insert(
                d.package.clone(),
                d.removed.iter().map(|r| alias::match_key(r)).collect(),
            );
        }
    }
    edits
}

/// Rewrite the Kvaesitso backup and verify the result by reading it back: every package
/// must end up with exactly the planned tag set, all other tags unchanged.
pub fn apply_kvaesitso<R: Read + Seek>(
    plan: &Plan,
    source: &SourceTags,
    input: R,
    limits: &Limits,
) -> Result<(Vec<u8>, RewriteStats), Error> {
    let edits = kvaesitso_edits(plan, source);
    let mut out = Cursor::new(Vec::new());
    let stats = kvaesitso_write::rewrite(input, &mut out, &edits, limits)?;
    let bytes = out.into_inner();

    let written = kvaesitso::read(Cursor::new(&bytes), limits)?;
    let mut expected: BTreeMap<String, AliasSet> = source
        .packages
        .iter()
        .map(|p| {
            (
                p.package.clone(),
                AliasSet::from_texts(p.aliases.iter().map(String::as_str)),
            )
        })
        .collect();
    for d in &plan.to_kvaesitso {
        let s = expected.entry(d.package.clone()).or_default();
        d.removed.iter().for_each(|t| s.remove_text(t));
        d.added.iter().for_each(|t| s.insert(t));
    }
    expected.retain(|_, s| !s.is_empty());
    let actual: BTreeMap<String, AliasSet> = written
        .packages
        .iter()
        .map(|p| {
            (
                p.package.clone(),
                AliasSet::from_texts(p.aliases.iter().map(String::as_str)),
            )
        })
        .collect();
    let same = expected.len() == actual.len()
        && expected
            .iter()
            .all(|(p, e)| actual.get(p).is_some_and(|a| a.keys == e.keys));
    let counts_kept = written.stats.shortcut_tag_records_skipped
        == source.stats.shortcut_tag_records_skipped
        && written.stats.non_tag_records_ignored == source.stats.non_tag_records_ignored
        && written.stats.other_tag_records_skipped == source.stats.other_tag_records_skipped
        && written.stats.profile_app_tags_skipped == source.stats.profile_app_tags_skipped
        && written.stats.comma_tags_skipped == source.stats.comma_tags_skipped;
    if !same || !counts_kept {
        return Err(Error::Schema(
            "verification of the rewritten Kvaesitso backup failed".into(),
        ));
    }
    Ok((bytes, stats))
}
