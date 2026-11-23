//! v0.2 bidirectional sync tests (docs/BISYNC.md).

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use launcher_tags_core::bisync::{
    self, ConflictKind, Options, PendingCheck, PendingPolicy, Plan, SyncState,
};
use launcher_tags_core::{alias, kvaesitso, quicksearch::QuickSearchBackup, Error, Limits};
use serde_json::{json, Value};
use zip::write::SimpleFileOptions;

const META: &str =
    r#"{"device":"t","timestamp":1,"format":"1.9","versionName":"1.41.0","components":[]}"#;

fn tag(key: &str, value: &str) -> Value {
    json!({"key": key, "value": value, "type": "tag"})
}

fn fav(key: &str) -> Value {
    json!({"key": key, "type": "app", "visibility": 0, "launchCount": 3, "pinPosition": 0,
           "searchable": format!("{{\"k\":\"{key}\"}}"), "weight": 0.5})
}

/// Build a Kvaesitso-like backup. `custom` records are split into pages of `page` records.
fn kzip(custom: &[Value], favorites: &[Value], page: usize) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let o = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    w.start_file("meta", o).unwrap();
    w.write_all(META.as_bytes()).unwrap();
    w.start_file("settings.json", stored).unwrap();
    w.write_all(br#"{"ui":{"theme":"dark"},"n":1.50}"#).unwrap();
    let chunks: Vec<&[Value]> = if custom.is_empty() {
        vec![&[]]
    } else {
        custom.chunks(page).collect()
    };
    for (i, c) in chunks.iter().enumerate() {
        w.start_file(format!("customizations.{i:04}"), o).unwrap();
        w.write_all(&serde_json::to_vec(c).unwrap()).unwrap();
    }
    w.start_file("favorites.0000", o).unwrap();
    w.write_all(&serde_json::to_vec(favorites).unwrap())
        .unwrap();
    w.start_file("widgets.0000", o).unwrap();
    w.write_all(b"[{\"opaque\":true}]").unwrap();
    w.finish().unwrap().into_inner()
}

fn qs(prefs: Value) -> QuickSearchBackup {
    let mut p = prefs.as_object().unwrap().clone();
    p.insert(
        "search_theme".into(),
        json!({"type": "string", "value": "dark"}),
    );
    p.insert(
        "nickname_app_a.b:10".into(),
        json!({"type": "string", "value": "profile"}),
    );
    QuickSearchBackup::from_value(json!({
        "formatVersion": 2,
        "preferences": {"user_app_preferences": p, "other": {"x": {"type": "int", "value": 1}}},
        "selectedExportItems": ["SETTINGS", "NOTES"],
        "notes": [{"t": "n"}]
    }))
    .unwrap()
}

fn nick(v: &str) -> Value {
    json!({"type": "string", "value": v})
}

#[derive(Debug)]
struct Out {
    k: Vec<u8>,
    q: QuickSearchBackup,
    plan: Plan,
}

fn cycle(
    k: &[u8],
    q: &QuickSearchBackup,
    state: Option<&SyncState>,
    opts: &Options,
) -> Result<Out, Error> {
    let lim = Limits::default();
    let source = kvaesitso::read(Cursor::new(k), &lim)?;
    let plan = bisync::plan(&source, q, state, opts)?;
    let mut q2 = q.clone();
    bisync::apply_quicksearch(&plan, &mut q2)?;
    let (k2, _) = bisync::apply_kvaesitso(&plan, &source, Cursor::new(k), &lim)?;
    Ok(Out { k: k2, q: q2, plan })
}

fn ktags(k: &[u8]) -> BTreeMap<String, Vec<String>> {
    kvaesitso::read(Cursor::new(k), &Limits::default())
        .unwrap()
        .packages
        .into_iter()
        .map(|p| (p.package, p.aliases))
        .collect()
}

fn qval(q: &QuickSearchBackup, pkg: &str) -> Option<String> {
    q.user_app_preferences()
        .get(&format!("nickname_app_{pkg}"))
        .map(|v| v["value"].as_str().unwrap().to_string())
}

fn keys(v: &[String]) -> Vec<String> {
    let mut k: Vec<String> = v.iter().map(|a| alias::match_key(a)).collect();
    k.sort();
    k
}

fn base_k() -> Vec<u8> {
    kzip(
        &[
            tag("app://org.ex.reader:org.ex.reader.Main", "Knihy"),
            tag("app://org.ex.reader:org.ex.reader.Second", "čtení"),
            json!({"key": "app://org.ex.reader:org.ex.reader.Main", "value": "x", "type": "icon"}),
            tag("shortcut://org.ex.reader:s", "zkratka"),
            tag("app://org.ex.maps:org.ex.maps.Main", "mapy"),
        ],
        &[
            fav("app://org.ex.reader:org.ex.reader.Main"),
            fav("app://org.ex.maps:org.ex.maps.Main"),
            fav("app://org.ex.games:org.ex.games.Main"),
        ],
        2,
    )
}

fn base_q() -> QuickSearchBackup {
    qs(json!({
        "nickname_app_org.ex.reader": nick("e-book, čtení"),
        "nickname_app_org.ex.games": nick("hry"),
        "nickname_app_org.ex.unknown": nick("neznámá")
    }))
}

#[test]
fn initial_sync_is_a_union_and_never_deletes() {
    let o = cycle(&base_k(), &base_q(), None, &Options::default()).unwrap();
    assert!(o.plan.initial);
    assert_eq!(o.plan.removals(), 0);
    assert_eq!(qval(&o.q, "org.ex.reader").unwrap(), "e-book, čtení, Knihy");
    assert_eq!(qval(&o.q, "org.ex.maps").unwrap(), "mapy");
    let k = ktags(&o.k);
    assert_eq!(
        keys(&k["org.ex.reader"]),
        keys(&["Knihy".into(), "čtení".into(), "e-book".into()])
    );
    // games has no tag yet but is in favorites -> resolvable via favorites key.
    assert_eq!(k["org.ex.games"], vec!["hry"]);
    // unknown app: no component key anywhere -> reported, retried later.
    assert_eq!(o.plan.unresolvable, vec!["org.ex.unknown"]);
    assert!(!k.contains_key("org.ex.unknown"));
    assert!(!o.plan.new_state.packages.contains_key("org.ex.unknown"));
}

#[test]
fn second_run_is_a_noop_and_pending_is_recognized() {
    let o1 = cycle(&base_k(), &base_q(), None, &Options::default()).unwrap();
    let o2 = cycle(&o1.k, &o1.q, Some(&o1.plan.new_state), &Options::default()).unwrap();
    assert!(o2.plan.is_noop(), "{:?}", o2.plan);
    assert_eq!(o2.plan.pending_quicksearch, PendingCheck::Imported);
    assert_eq!(o2.plan.pending_kvaesitso, PendingCheck::Imported);
    assert_eq!(o2.q.as_value(), o1.q.as_value());
    assert_eq!(ktags(&o2.k), ktags(&o1.k));
    assert_eq!(o2.plan.new_state.packages, o1.plan.new_state.packages);
    // Unresolvable stays unresolvable, but is never deleted from Quick Search.
    assert_eq!(qval(&o2.q, "org.ex.unknown").unwrap(), "neznámá");
}

#[test]
fn deletion_in_quicksearch_propagates_to_all_activities_in_kvaesitso() {
    let k = kzip(
        &[
            tag("app://org.ex.reader:org.ex.reader.Main", "Knihy"),
            tag("app://org.ex.reader:org.ex.reader.Second", "knihy"),
            tag("app://org.ex.reader:org.ex.reader.Main", "čtení"),
            tag("app://org.ex.reader:org.ex.reader.Main:10", "Knihy"),
        ],
        &[],
        100,
    );
    let q = qs(json!({"nickname_app_org.ex.reader": nick("Knihy, čtení")}));
    let o1 = cycle(&k, &q, None, &Options::default()).unwrap();
    assert!(o1.plan.is_noop());
    let mut q_user = o1.q.clone();
    // User removes "KNIHY" in Quick Search (different case on purpose).
    q_user = QuickSearchBackup::from_value({
        let mut v = q_user.into_value();
        v["preferences"]["user_app_preferences"]["nickname_app_org.ex.reader"] = nick("čtení");
        v
    })
    .unwrap();
    let o2 = cycle(
        &o1.k,
        &q_user,
        Some(&o1.plan.new_state),
        &Options::default(),
    )
    .unwrap();
    assert_eq!(o2.plan.to_kvaesitso.len(), 1);
    let after = kvaesitso::read(Cursor::new(&o2.k), &Limits::default()).unwrap();
    assert_eq!(after.packages[0].aliases, vec!["čtení"]);
    // The work-profile tag is untouched.
    assert_eq!(after.stats.profile_app_tags_skipped, 1);
    assert_eq!(o2.plan.new_state.packages["org.ex.reader"], vec!["čtení"]);
}

#[test]
fn deletion_in_kvaesitso_propagates_to_quicksearch_keeping_other_aliases_verbatim() {
    let k = kzip(
        &[tag("app://a.b:a.b.M", "one"), tag("app://a.b:a.b.M", "Two")],
        &[],
        100,
    );
    let q = qs(json!({"nickname_app_a.b": nick("One,  Two ,mine")}));
    let o1 = cycle(&k, &q, None, &Options::default()).unwrap();
    let st = o1.plan.new_state.clone();
    let k_user = kzip(
        &[
            tag("app://a.b:a.b.M", "one"),
            tag("app://a.b:a.b.M", "mine"),
        ],
        &[],
        100,
    );
    let o2 = cycle(&k_user, &o1.q, Some(&st), &Options::default()).unwrap();
    assert_eq!(qval(&o2.q, "a.b").unwrap(), "One, mine");
    // Removing everything leaves an empty value, not a missing key.
    let st2 = o2.plan.new_state.clone();
    let k_empty = kzip(
        &[json!({"key": "app://a.b:a.b.M", "value": "L", "type": "label"})],
        &[],
        100,
    );
    let o3 = cycle(&k_empty, &o2.q, Some(&st2), &Options::default()).unwrap();
    assert_eq!(qval(&o3.q, "a.b").unwrap(), "");
    assert!(!o3.plan.new_state.packages.contains_key("a.b"));
    // Profile key and unrelated values unchanged throughout.
    assert_eq!(
        o3.q.user_app_preferences()["nickname_app_a.b:10"],
        q.user_app_preferences()["nickname_app_a.b:10"]
    );
    assert_eq!(o3.q.as_value()["notes"], q.as_value()["notes"]);
    assert_eq!(
        o3.q.as_value()["preferences"]["other"],
        q.as_value()["preferences"]["other"]
    );
}

#[test]
fn clearing_quicksearch_nickname_removes_kvaesitso_tags() {
    let k = kzip(
        &[tag("app://a.b:a.b.M", "x"), tag("app://a.b:a.b.M", "y")],
        &[],
        100,
    );
    let q = qs(json!({"nickname_app_a.b": nick("x, y")}));
    let o1 = cycle(&k, &q, None, &Options::default()).unwrap();
    let q_user = qs(json!({"nickname_app_a.b": nick("")}));
    let o2 = cycle(
        &o1.k,
        &q_user,
        Some(&o1.plan.new_state),
        &Options::default(),
    )
    .unwrap();
    assert!(!ktags(&o2.k).contains_key("a.b"));
}

#[test]
fn package_missing_on_one_side_is_a_conflict_not_a_deletion() {
    let o1 = cycle(&base_k(), &base_q(), None, &Options::default()).unwrap();
    // Kvaesitso backup without any trace of maps (uninstalled / partial backup).
    let k_user = kzip(
        &[
            tag("app://org.ex.reader:org.ex.reader.Main", "Knihy"),
            tag("app://org.ex.reader:org.ex.reader.Second", "čtení"),
            tag("app://org.ex.reader:org.ex.reader.Main", "e-book"),
            tag("app://org.ex.games:org.ex.games.Main", "hry"),
        ],
        &[
            fav("app://org.ex.reader:org.ex.reader.Main"),
            fav("app://org.ex.games:org.ex.games.Main"),
        ],
        100,
    );
    let o2 = cycle(
        &k_user,
        &o1.q,
        Some(&o1.plan.new_state),
        &Options {
            pending: PendingPolicy::Accept,
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(qval(&o2.q, "org.ex.maps").unwrap(), "mapy");
    assert_eq!(o2.plan.conflicts.len(), 1);
    assert_eq!(
        o2.plan.conflicts[0].kind,
        ConflictKind::PackageMissingInKvaesitso
    );
    assert_eq!(o2.plan.new_state.packages["org.ex.maps"], vec!["mapy"]);

    // Missing nickname key in Quick Search likewise.
    let mut v = o1.q.clone().into_value();
    v["preferences"]["user_app_preferences"]
        .as_object_mut()
        .unwrap()
        .shift_remove("nickname_app_org.ex.maps");
    let q_user = QuickSearchBackup::from_value(v).unwrap();
    let o3 = cycle(
        &o1.k,
        &q_user,
        Some(&o1.plan.new_state),
        &Options::default(),
    )
    .unwrap();
    assert_eq!(ktags(&o3.k)["org.ex.maps"], vec!["mapy"]);
    assert_eq!(
        o3.plan.conflicts[0].kind,
        ConflictKind::PackageMissingInQuickSearch
    );
}

#[test]
fn unimported_output_is_detected_and_can_be_discarded_or_accepted() {
    let k0 = base_k();
    let q0 = base_q();
    let o1 = cycle(&k0, &q0, None, &Options::default()).unwrap();
    let st = o1.plan.new_state.clone();

    // Nothing imported: the user exports the same old backups again.
    let err = cycle(&k0, &q0, Some(&st), &Options::default()).unwrap_err();
    assert!(matches!(err, Error::PendingNotImported { .. }), "{err}");

    // Discard: the same additions are planned again, nothing is deleted.
    let o2 = cycle(
        &k0,
        &q0,
        Some(&st),
        &Options {
            pending: PendingPolicy::Discard,
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(o2.plan.removals(), 0);
    assert_eq!(o2.q.as_value(), o1.q.as_value());
    assert_eq!(ktags(&o2.k), ktags(&o1.k));
    assert_eq!(o2.plan.pending_quicksearch, PendingCheck::Discarded);

    // Only Quick Search imported: Kvaesitso side is discarded, nothing is lost.
    let o3 = cycle(
        &k0,
        &o1.q,
        Some(&st),
        &Options {
            pending: PendingPolicy::Discard,
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(o3.plan.removals(), 0);
    assert_eq!(o3.plan.pending_quicksearch, PendingCheck::Imported);
    assert_eq!(o3.plan.pending_kvaesitso, PendingCheck::Discarded);
    assert_eq!(ktags(&o3.k), ktags(&o1.k));

    // Accept: the old exports are authoritative -> the previous additions are deleted again.
    let o4 = cycle(
        &k0,
        &q0,
        Some(&st),
        &Options {
            pending: PendingPolicy::Accept,
            allow_mass_delete: true,
        },
    )
    .unwrap();
    assert!(o4.plan.removals() > 0);
}

#[test]
fn mass_deletion_is_refused_without_flag() {
    let recs: Vec<Value> = (0..40)
        .map(|i| tag("app://a.b:a.b.M", &format!("t{i}")))
        .collect();
    let k = kzip(&recs, &[], 100);
    let all: Vec<String> = (0..40).map(|i| format!("t{i}")).collect();
    let q = qs(json!({"nickname_app_a.b": nick(&all.join(", "))}));
    let o1 = cycle(&k, &q, None, &Options::default()).unwrap();
    let q_user = qs(json!({"nickname_app_a.b": nick("t0")}));
    let err = cycle(
        &o1.k,
        &q_user,
        Some(&o1.plan.new_state),
        &Options::default(),
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            Error::MassDelete {
                count: 39,
                limit: 10
            }
        ),
        "{err}"
    );
    let o2 = cycle(
        &o1.k,
        &q_user,
        Some(&o1.plan.new_state),
        &Options {
            allow_mass_delete: true,
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(ktags(&o2.k)["a.b"], vec!["t0"]);
}

fn members(bytes: &[u8]) -> Vec<(String, Vec<u8>, zip::CompressionMethod)> {
    let mut a = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..a.len())
        .map(|i| {
            let mut f = a.by_index(i).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            (f.name().to_string(), b, f.compression())
        })
        .collect()
}

#[test]
fn kvaesitso_rewrite_preserves_everything_else() {
    let k = base_k();
    let o = cycle(&k, &base_q(), None, &Options::default()).unwrap();
    let before = members(&k);
    let after = members(&o.k);
    let names = |m: &[(String, Vec<u8>, zip::CompressionMethod)]| {
        m.iter().map(|x| x.0.clone()).collect::<Vec<_>>()
    };
    // Same member order; customizations re-paginated (5 + 2 added records -> one page of 100).
    assert_eq!(
        names(&before),
        vec![
            "meta",
            "settings.json",
            "customizations.0000",
            "customizations.0001",
            "customizations.0002",
            "favorites.0000",
            "widgets.0000"
        ]
    );
    assert_eq!(
        names(&after),
        vec![
            "meta",
            "settings.json",
            "customizations.0000",
            "favorites.0000",
            "widgets.0000"
        ]
    );
    for name in ["meta", "settings.json", "favorites.0000", "widgets.0000"] {
        let b = before.iter().find(|m| m.0 == name).unwrap();
        let a = after.iter().find(|m| m.0 == name).unwrap();
        assert_eq!(a.1, b.1, "{name} content");
        assert_eq!(a.2, b.2, "{name} compression");
    }
    // Original records verbatim and in order, new tag records appended.
    let old: Vec<Value> = before
        .iter()
        .filter(|m| m.0.starts_with("customizations."))
        .flat_map(|m| serde_json::from_slice::<Vec<Value>>(&m.1).unwrap())
        .collect();
    let new: Vec<Value> = serde_json::from_slice(&after[2].1).unwrap();
    assert_eq!(&new[..old.len()], &old[..]);
    assert_eq!(new[old.len()..].len(), 2);
    for rec in &new[old.len()..] {
        assert_eq!(rec["type"], "tag");
        assert_eq!(
            rec.as_object().unwrap().keys().collect::<Vec<_>>(),
            vec!["key", "value", "type"]
        );
    }
}

#[test]
fn rewritten_backup_restores_like_kvaesitso() {
    // Emulates BackupManager.extractArchive (ZipInputStream, streaming) and the
    // CustomAttributesRepository / SavableSearchableRepository restore parsing.
    let recs: Vec<Value> = (0..250)
        .map(|i| tag("app://a.b:a.b.M", &format!("t{i}")))
        .collect();
    let k = kzip(&recs, &[fav("app://a.b:a.b.M")], 100);
    let q = qs(json!({"nickname_app_a.b": nick("nové, 🎵 hudba")}));
    let o = cycle(&k, &q, None, &Options::default()).unwrap();
    let mut stream = Cursor::new(&o.k);
    let mut files = BTreeMap::new();
    while let Some(mut f) = zip::read::read_zipfile_from_stream(&mut stream).unwrap() {
        let mut b = Vec::new();
        f.read_to_end(&mut b).unwrap();
        files.insert(f.name().to_string(), b);
    }
    let mut tags = 0;
    for (name, b) in &files {
        if name.starts_with("customizations.") {
            let arr: Vec<Value> = serde_json::from_slice(b).unwrap();
            assert!(arr.len() <= 100);
            for r in arr {
                assert!(r["type"].is_string());
                if r["type"] == "tag" {
                    tags += 1;
                }
            }
        }
        if name.starts_with("favorites.") {
            for r in serde_json::from_slice::<Vec<Value>>(b).unwrap() {
                assert!(
                    r["key"].is_string()
                        && r["searchable"].is_string()
                        && r["launchCount"].is_number()
                );
            }
        }
    }
    assert_eq!(tags, 252);
    assert_eq!(
        files
            .keys()
            .filter(|n| n.starts_with("customizations."))
            .count(),
        3
    );
    assert_eq!(files["meta"], META.as_bytes());
}

#[test]
fn rewrite_refuses_stray_customization_members() {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let o = SimpleFileOptions::default();
    for (n, d) in [
        ("customizations.0000", "[]"),
        ("customizations.backup", "[]"),
    ] {
        w.start_file(n, o).unwrap();
        w.write_all(d.as_bytes()).unwrap();
    }
    let k = w.finish().unwrap().into_inner();
    let err = launcher_tags_core::kvaesitso_write::rewrite(
        Cursor::new(&k),
        Cursor::new(Vec::new()),
        &Default::default(),
        &Limits::default(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::Schema(_)));
}

#[test]
fn state_file_round_trip_and_validation() {
    let o = cycle(&base_k(), &base_q(), None, &Options::default()).unwrap();
    let bytes = o.plan.new_state.to_pretty_bytes();
    assert_eq!(SyncState::parse(&bytes).unwrap(), o.plan.new_state);
    assert!(SyncState::parse(br#"{"format":"other","version":1,"packages":{}}"#).is_err());
    assert!(SyncState::parse(
        br#"{"format":"launcher-tags-sync/state","version":2,"packages":{}}"#
    )
    .is_err());
    assert!(SyncState::parse(
        br#"{"format":"launcher-tags-sync/state","version":1,"packages":{"bad name":[]}}"#
    )
    .is_err());
    assert!(SyncState::parse(
        br#"{"format":"launcher-tags-sync/state","version":1,"packages":{"a.b":"x"}}"#
    )
    .is_err());
}

mod props {
    use super::*;
    use proptest::prelude::*;

    fn aliases() -> impl Strategy<Value = Vec<String>> {
        proptest::collection::vec(
            prop_oneof![
                Just("a"),
                Just("A"),
                Just("č"),
                Just("Č"),
                Just("c\u{30c}"),
                Just("🎵"),
                Just("x y"),
                Just("Ž")
            ],
            0..4,
        )
        .prop_map(|v| v.into_iter().map(String::from).collect())
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        /// After an initial sync and a follow-up with random edits on both sides, both sides
        /// hold the same alias set, and a further run is a no-op.
        #[test]
        fn sync_converges(k1 in aliases(), q1 in aliases(), k2 in aliases(), q2 in aliases()) {
            let mk_k = |a: &[String]| {
                let mut recs: Vec<Value> = a.iter().map(|t| tag("app://p.q:p.q.M", t)).collect();
                recs.push(json!({"key": "app://p.q:p.q.M", "value": "L", "type": "label"}));
                kzip(&recs, &[fav("app://p.q:p.q.M")], 3)
            };
            let mk_q = |a: &[String]| qs(json!({"nickname_app_p.q": nick(&a.join(", "))}));
            let opts = Options { pending: PendingPolicy::Accept, allow_mass_delete: true };
            let o1 = cycle(&mk_k(&k1), &mk_q(&q1), None, &opts).unwrap();
            let ks = |b: &[u8]| keys(ktags(b).get("p.q").map(Vec::as_slice).unwrap_or(&[]));
            // Quick Search ignores duplicate aliases on read; user-made duplicates stay verbatim.
            let qk = |q: &QuickSearchBackup| {
                let mut k = keys(&alias::split(&qval(q, "p.q").unwrap_or_default()).into_iter().map(String::from).collect::<Vec<_>>());
                k.dedup();
                k
            };
            prop_assert_eq!(ks(&o1.k), qk(&o1.q));
            let o2 = cycle(&mk_k(&k2), &mk_q(&q2), Some(&o1.plan.new_state), &opts).unwrap();
            prop_assert_eq!(ks(&o2.k), qk(&o2.q));
            let o3 = cycle(&o2.k, &o2.q, Some(&o2.plan.new_state), &Options::default()).unwrap();
            prop_assert!(o3.plan.is_noop());
        }
    }
}

#[test]
fn stale_inputs_are_refused() {
    use bisync::{check_freshness, fingerprint, record_run, InputInfo};
    let o = cycle(&base_k(), &base_q(), None, &Options::default()).unwrap();
    let mut plan = o.plan.clone();
    let k_in = InputInfo {
        fingerprint: fingerprint(b"k-in"),
        exported_at_ms: Some(100),
    };
    let q_in = InputInfo {
        fingerprint: fingerprint(b"q-in"),
        exported_at_ms: Some(200),
    };
    record_run(&mut plan, &k_in, b"k-out", &q_in, b"q-out");
    let st = SyncState::parse(&plan.new_state.to_pretty_bytes()).unwrap();
    let info = |b: &[u8], t| InputInfo {
        fingerprint: fingerprint(b),
        exported_at_ms: Some(t),
    };

    // Our own outputs (same timestamps) are fine; fresh later exports are fine.
    assert!(check_freshness(Some(&st), &info(b"k-out", 100), &info(b"q-out", 200), false).is_ok());
    assert!(check_freshness(Some(&st), &info(b"k-new", 101), &info(b"q-new", 201), false).is_ok());
    // The original inputs, or other content with an old/equal timestamp, are stale.
    for (k, q) in [
        (info(b"k-in", 100), info(b"q-out", 200)),
        (info(b"k-out", 100), info(b"q-in", 200)),
        (info(b"k-edit", 100), info(b"q-out", 200)),
        (info(b"k-out", 100), info(b"q-old", 150)),
    ] {
        let e = check_freshness(Some(&st), &k, &q, false).unwrap_err();
        assert!(matches!(e, Error::StaleInput { .. }), "{e}");
        assert!(check_freshness(Some(&st), &k, &q, true).is_ok());
    }
    // No timestamps and unknown content: cannot tell, allowed.
    let none = |b: &[u8]| InputInfo {
        fingerprint: fingerprint(b),
        exported_at_ms: None,
    };
    assert!(check_freshness(Some(&st), &none(b"x"), &none(b"y"), false).is_ok());
    // First run: nothing to compare.
    assert!(check_freshness(None, &k_in, &q_in, false).is_ok());
    assert_ne!(fingerprint(b"a"), fingerprint(b"b"));
}
