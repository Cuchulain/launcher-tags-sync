//! End-to-end CLI tests: file safety, dry-run, report, exit codes.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_launcher-tags-sync"))
}

fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(rel)
}

/// Copy fixture inputs into a temp dir so tests can prove they stay byte-identical.
struct Work {
    dir: tempfile::TempDir,
}

impl Work {
    fn new(case: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        for f in ["input.kvaesitso", "input.quicksearch"] {
            std::fs::copy(fixture(&format!("{case}/{f}")), dir.path().join(f)).unwrap();
        }
        Work { dir }
    }
    fn p(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
    fn inputs_snapshot(&self) -> (Vec<u8>, Vec<u8>) {
        (
            std::fs::read(self.p("input.kvaesitso")).unwrap(),
            std::fs::read(self.p("input.quicksearch")).unwrap(),
        )
    }
    fn sync(&self, extra: &[&str]) -> Output {
        let mut c = bin();
        c.arg("sync")
            .arg("--kvaesitso")
            .arg(self.p("input.kvaesitso"))
            .arg("--quicksearch")
            .arg(self.p("input.quicksearch"));
        for a in extra {
            // Relative names are resolved inside the work dir.
            if a.starts_with('-') {
                c.arg(a);
            } else {
                c.arg(self.p(a));
            }
        }
        c.output().unwrap()
    }
    fn entries(&self) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(self.dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }
}

fn json_file(p: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn sync_basic_end_to_end() {
    let w = Work::new("basic");
    let before = w.inputs_snapshot();
    let o = w.sync(&["--output", "out.quicksearch", "--report", "report.json"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(
        json_file(&w.p("out.quicksearch")),
        json_file(&fixture("basic/expected.quicksearch"))
    );
    let report = json_file(&w.p("report.json"));
    let expected = json_file(&fixture("basic/expected-report.json"));
    for (k, v) in expected.as_object().unwrap() {
        assert_eq!(report.get(k), Some(v), "{k}");
    }
    assert_eq!(before, w.inputs_snapshot(), "inputs must stay unchanged");
    assert_eq!(
        w.entries(),
        vec![
            "input.kvaesitso",
            "input.quicksearch",
            "out.quicksearch",
            "report.json"
        ],
        "no temp files left behind"
    );
}

#[test]
fn sync_edgecases_end_to_end_with_warnings() {
    let w = Work::new("edgecases");
    let o = w.sync(&["--output", "out.quicksearch", "--report", "report.json"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(
        json_file(&w.p("out.quicksearch")),
        json_file(&fixture("edgecases/expected.quicksearch"))
    );
    let report = json_file(&w.p("report.json"));
    let cats: Vec<&str> = report["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["category"].as_str().unwrap())
        .collect();
    assert!(cats.contains(&"malformed_app_key"));
    assert!(cats.contains(&"comma_in_tag_value"));
    let err = stderr(&o);
    assert!(err.contains("warning"), "{err}");
    assert!(
        !err.contains("LAN"),
        "warnings must not print tag values: {err}"
    );
}

#[test]
fn second_run_on_output_is_a_no_op() {
    let w = Work::new("basic");
    assert!(w.sync(&["--output", "out1.quicksearch"]).status.success());
    let o = bin()
        .arg("sync")
        .arg("--kvaesitso")
        .arg(w.p("input.kvaesitso"))
        .arg("--quicksearch")
        .arg(w.p("out1.quicksearch"))
        .arg("--output")
        .arg(w.p("out2.quicksearch"))
        .arg("--report")
        .arg(w.p("r2.json"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(
        json_file(&w.p("out1.quicksearch")),
        json_file(&w.p("out2.quicksearch"))
    );
    let r = json_file(&w.p("r2.json"));
    assert_eq!(r["new_app_nickname_entries"], 0);
    assert_eq!(r["extended_app_nickname_entries"], 0);
    assert_eq!(r["added_unique_aliases"], 0);
}

#[test]
fn dry_run_writes_nothing() {
    let w = Work::new("basic");
    let before = w.inputs_snapshot();
    let o = w.sync(&["--dry-run", "--output", "out.quicksearch"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("org.example.music"), "{out}");
    assert!(out.to_lowercase().contains("dry run"), "{out}");
    assert!(!w.p("out.quicksearch").exists());
    assert_eq!(w.entries(), vec!["input.kvaesitso", "input.quicksearch"]);
    assert_eq!(before, w.inputs_snapshot());
    // Without --output too.
    let o = w.sync(&["--dry-run"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(w.entries(), vec!["input.kvaesitso", "input.quicksearch"]);
}

#[test]
fn refuses_to_overwrite_existing_output_without_force() {
    let w = Work::new("basic");
    std::fs::write(w.p("out.quicksearch"), b"precious").unwrap();
    let o = w.sync(&["--output", "out.quicksearch"]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("--force"), "{}", stderr(&o));
    assert_eq!(std::fs::read(w.p("out.quicksearch")).unwrap(), b"precious");

    let o = w.sync(&["--output", "out.quicksearch", "--force"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(
        json_file(&w.p("out.quicksearch")),
        json_file(&fixture("basic/expected.quicksearch"))
    );
}

#[test]
fn refuses_output_equal_to_input_even_with_force() {
    let w = Work::new("basic");
    let before = w.inputs_snapshot();
    for target in ["input.quicksearch", "input.kvaesitso"] {
        let o = w.sync(&["--output", target, "--force"]);
        assert!(!o.status.success(), "{target}");
        assert_eq!(before, w.inputs_snapshot());
    }
    // Same file through a different spelling of the path.
    let alias = w.dir.path().join(".").join("input.quicksearch");
    let o = bin()
        .arg("sync")
        .arg("--kvaesitso")
        .arg(w.p("input.kvaesitso"))
        .arg("--quicksearch")
        .arg(w.p("input.quicksearch"))
        .arg("--output")
        .arg(&alias)
        .arg("--force")
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert_eq!(before, w.inputs_snapshot());
}

#[cfg(unix)]
#[test]
fn refuses_output_that_is_a_symlink_or_hardlink_to_an_input() {
    let w = Work::new("basic");
    let before = w.inputs_snapshot();
    std::os::unix::fs::symlink(w.p("input.quicksearch"), w.p("link.quicksearch")).unwrap();
    std::fs::hard_link(w.p("input.quicksearch"), w.p("hard.quicksearch")).unwrap();
    for target in ["link.quicksearch", "hard.quicksearch"] {
        let o = w.sync(&["--output", target, "--force"]);
        assert!(!o.status.success(), "{target}: {}", stderr(&o));
    }
    assert_eq!(before, w.inputs_snapshot());
}

#[test]
fn report_path_must_not_collide_with_inputs_or_output() {
    let w = Work::new("basic");
    let before = w.inputs_snapshot();
    let o = w.sync(&[
        "--output",
        "out.quicksearch",
        "--report",
        "input.quicksearch",
        "--force",
    ]);
    assert!(!o.status.success());
    let o = w.sync(&["--output", "out.quicksearch", "--report", "out.quicksearch"]);
    assert!(!o.status.success());
    assert_eq!(before, w.inputs_snapshot());
    assert!(!w.p("out.quicksearch").exists());
}

#[test]
fn invalid_quicksearch_fails_without_output() {
    let w = Work::new("basic");
    std::fs::copy(
        fixture("invalid/invalid-quicksearch.quicksearch"),
        w.p("input.quicksearch"),
    )
    .unwrap();
    let o = w.sync(&["--output", "out.quicksearch"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(!w.p("out.quicksearch").exists());
    assert!(stderr(&o).to_lowercase().contains("json"), "{}", stderr(&o));
}

#[test]
fn missing_customizations_fails_without_output() {
    let w = Work::new("basic");
    std::fs::copy(
        fixture("invalid/missing-customizations.kvaesitso"),
        w.p("input.kvaesitso"),
    )
    .unwrap();
    let o = w.sync(&["--output", "out.quicksearch"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(!w.p("out.quicksearch").exists());
    assert!(stderr(&o).contains("customizations"), "{}", stderr(&o));
}

#[test]
fn missing_input_file_fails() {
    let w = Work::new("basic");
    std::fs::remove_file(w.p("input.kvaesitso")).unwrap();
    let o = w.sync(&["--output", "out.quicksearch"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(!w.p("out.quicksearch").exists());
}

#[test]
fn sync_requires_output_or_dry_run() {
    let w = Work::new("basic");
    let o = w.sync(&[]);
    assert_eq!(o.status.code(), Some(2));
}

#[test]
fn inspect_both_formats() {
    let o = bin()
        .args(["inspect", "--kvaesitso"])
        .arg(fixture("basic/input.kvaesitso"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("customizations pages: 2"), "{out}");
    assert!(out.contains("app tag records: 7"), "{out}");
    assert!(
        !out.contains("Knihy"),
        "inspect must not print tag values: {out}"
    );

    let o = bin()
        .args(["inspect", "--quicksearch"])
        .arg(fixture("basic/input.quicksearch"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("formatVersion: 2"), "{out}");
    assert!(!out.contains("safe sample query"), "{out}");
    assert!(!out.contains("e-book"), "{out}");

    let o = bin()
        .args(["inspect", "--quicksearch"])
        .arg(fixture("invalid/invalid-quicksearch.quicksearch"))
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1));
}

#[test]
fn help_states_one_way_direction() {
    let o = bin().args(["sync", "--help"]).output().unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("one-way") || out.contains("One-way"), "{out}");
}
