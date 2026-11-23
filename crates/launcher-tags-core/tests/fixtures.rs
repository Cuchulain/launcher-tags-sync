//! Fixture-driven acceptance tests (docs/TESTS.md). All fixtures are synthetic.

use std::path::PathBuf;

use launcher_tags_core::{kvaesitso, merge, quicksearch, report::Report, Limits, WarningCategory};
use serde_json::Value;

fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(rel)
}

fn read_json(rel: &str) -> Value {
    serde_json::from_slice(&std::fs::read(fixture(rel)).unwrap()).unwrap()
}

struct Run {
    output: Value,
    report: Report,
    input: Value,
}

fn run(dir: &str) -> Run {
    let source = kvaesitso::read_file(
        fixture(&format!("{dir}/input.kvaesitso")),
        &Limits::default(),
    )
    .expect("source parses");
    let input_bytes = std::fs::read(fixture(&format!("{dir}/input.quicksearch"))).unwrap();
    let mut target = quicksearch::QuickSearchBackup::parse(&input_bytes).expect("target parses");
    let input = target.as_value().clone();
    let outcome = merge::merge(&source, &mut target).expect("merge succeeds");
    let bytes = target.to_pretty_bytes().expect("serializes");
    // The generated output must reparse and validate as an importable backup.
    let reparsed = quicksearch::QuickSearchBackup::parse(&bytes).expect("output reparses");
    let report = Report::build(&source, &outcome, false);
    Run {
        output: reparsed.into_value(),
        report,
        input,
    }
}

#[test]
fn basic_matches_expected_output() {
    let r = run("basic");
    assert_eq!(r.output, read_json("basic/expected.quicksearch"));
}

#[test]
fn basic_report_matches_expected_counts() {
    let r = run("basic");
    let actual = serde_json::to_value(&r.report).unwrap();
    let expected = read_json("basic/expected-report.json");
    for (k, v) in expected.as_object().unwrap() {
        assert_eq!(actual.get(k), Some(v), "report field {k}");
    }
}

#[test]
fn basic_parses_all_customization_pages() {
    let source =
        kvaesitso::read_file(fixture("basic/input.kvaesitso"), &Limits::default()).unwrap();
    assert_eq!(source.pages, vec![0, 1]);
    // `mapy` and `zábava` live only on page 0001.
    let pkgs: Vec<&str> = source.packages.iter().map(|p| p.package.as_str()).collect();
    assert_eq!(
        pkgs,
        vec![
            "org.example.reader",
            "org.example.music",
            "org.example.maps",
            "org.example.silent"
        ]
    );
    let reader = &source.packages[0];
    assert_eq!(reader.aliases, vec!["Knihy", "čtení"]);
}

#[test]
fn basic_never_imports_shortcut_tags_as_app_aliases() {
    let r = run("basic");
    let text = serde_json::to_string(&r.output).unwrap();
    assert!(!text.contains("kapitola"));
    assert!(!text.contains("rychle"));
    assert_eq!(r.report.source_shortcut_tag_records_skipped, 2);
}

#[test]
fn basic_preserves_profile_and_unrelated_entries() {
    let r = run("basic");
    let prefs = &r.output["preferences"]["user_app_preferences"];
    for key in [
        "nickname_app_org.example.unrelated",
        "nickname_app_org.example.music:10",
        "nickname_app_shortcut_example-shortcut",
        "nickname_contact_17",
        "search_theme",
        "search_enabled",
    ] {
        assert_eq!(
            prefs[key], r.input["preferences"]["user_app_preferences"][key],
            "{key}"
        );
    }
}

#[test]
fn only_base_app_nickname_paths_change() {
    for dir in ["basic", "edgecases"] {
        let r = run(dir);
        let changed = changed_paths(&r.input, &r.output);
        for path in &changed {
            let key = path
                .strip_prefix("preferences.user_app_preferences.")
                .unwrap_or_else(|| panic!("{dir}: unexpected changed path {path}"));
            assert!(key.starts_with("nickname_app_"), "{dir}: {path}");
            assert!(!key.starts_with("nickname_app_shortcut_"), "{dir}: {path}");
            assert!(!key.contains(':'), "{dir}: profile key changed: {path}");
        }
        assert!(r.report.preserved_non_app_settings);
    }
}

/// Top-level paths (down to individual preference keys) whose parsed JSON differs.
fn changed_paths(a: &Value, b: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let (ao, bo) = (a.as_object().unwrap(), b.as_object().unwrap());
    let keys: std::collections::BTreeSet<&String> = ao.keys().chain(bo.keys()).collect();
    for k in keys {
        if k == "preferences" {
            let (pa, pb) = (ao[k].as_object().unwrap(), bo[k].as_object().unwrap());
            let files: std::collections::BTreeSet<&String> = pa.keys().chain(pb.keys()).collect();
            for f in files {
                let (fa, fb) = (pa.get(f), pb.get(f));
                match (fa.and_then(Value::as_object), fb.and_then(Value::as_object)) {
                    (Some(fa), Some(fb)) => {
                        let ks: std::collections::BTreeSet<&String> =
                            fa.keys().chain(fb.keys()).collect();
                        for pk in ks {
                            if fa.get(pk) != fb.get(pk) {
                                out.push(format!("preferences.{f}.{pk}"));
                            }
                        }
                    }
                    _ if fa != fb => out.push(format!("preferences.{f}")),
                    _ => {}
                }
            }
        } else if ao.get(k) != bo.get(k) {
            out.push(k.clone());
        }
    }
    out
}

#[test]
fn edgecases_match_expected_output() {
    let r = run("edgecases");
    assert_eq!(r.output, read_json("edgecases/expected.quicksearch"));
}

#[test]
fn edgecases_report_counts_and_warning_categories() {
    let r = run("edgecases");
    let expected = read_json("edgecases/expected-report.json");
    let actual = serde_json::to_value(&r.report).unwrap();
    for (k, v) in expected.as_object().unwrap() {
        if k == "warnings_expected" {
            continue;
        }
        assert_eq!(actual.get(k), Some(v), "report field {k}");
    }
    let wanted: Vec<&str> = expected["warnings_expected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    for w in wanted {
        assert!(
            r.report.warnings.iter().any(|x| x.category.human() == w),
            "missing warning category {w:?}: {:?}",
            r.report.warnings
        );
    }
    let malformed = r
        .report
        .warnings
        .iter()
        .filter(|w| w.category == WarningCategory::MalformedAppKey)
        .count();
    assert_eq!(malformed, 2);
}

#[test]
fn warnings_do_not_disclose_tag_values() {
    let r = run("edgecases");
    let text = serde_json::to_string(&r.report).unwrap();
    for secret in [
        "LAN",
        "domácí",
        "bad2",
        "jen shortcut",
        "icon-value",
        "moje",
    ] {
        assert!(!text.contains(secret), "report leaks {secret:?}");
    }
    let comma = r
        .report
        .warnings
        .iter()
        .find(|w| w.category == WarningCategory::CommaInTagValue)
        .unwrap();
    assert_eq!(comma.package.as_deref(), Some("org.example.edge"));
}

#[test]
fn basic_report_does_not_disclose_quicksearch_values() {
    let r = run("basic");
    let text = serde_json::to_string(&r.report).unwrap();
    for secret in [
        "safe sample query",
        "synthetic-dark",
        "jen-moje",
        "Známý",
        "spustit",
    ] {
        assert!(!text.contains(secret), "report leaks {secret:?}");
    }
}

#[test]
fn idempotent_second_run_adds_nothing() {
    for dir in ["basic", "edgecases"] {
        let first = run(dir);
        let source = kvaesitso::read_file(
            fixture(&format!("{dir}/input.kvaesitso")),
            &Limits::default(),
        )
        .unwrap();
        let mut again = quicksearch::QuickSearchBackup::from_value(first.output.clone()).unwrap();
        let outcome = merge::merge(&source, &mut again).unwrap();
        assert_eq!(outcome.stats.new_entries, 0, "{dir}");
        assert_eq!(outcome.stats.extended_entries, 0, "{dir}");
        assert_eq!(outcome.stats.added_unique_aliases, 0, "{dir}");
        assert!(outcome.changes.is_empty());
        assert_eq!(again.into_value(), first.output, "{dir}");
    }
}

#[test]
fn repeated_runs_are_deterministic() {
    for dir in ["basic", "edgecases"] {
        let a = run(dir);
        let b = run(dir);
        assert_eq!(
            serde_json::to_vec_pretty(&a.output).unwrap(),
            serde_json::to_vec_pretty(&b.output).unwrap()
        );
        assert_eq!(
            serde_json::to_string(&a.report).unwrap(),
            serde_json::to_string(&b.report).unwrap()
        );
    }
}

#[test]
fn invalid_quicksearch_fails_to_parse() {
    let bytes = std::fs::read(fixture("invalid/invalid-quicksearch.quicksearch")).unwrap();
    assert!(quicksearch::QuickSearchBackup::parse(&bytes).is_err());
}

#[test]
fn missing_customizations_is_an_error() {
    let err = kvaesitso::read_file(
        fixture("invalid/missing-customizations.kvaesitso"),
        &Limits::default(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("customizations"), "{err}");
}
