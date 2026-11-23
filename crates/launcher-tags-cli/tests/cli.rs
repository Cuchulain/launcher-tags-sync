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

// ---------------------------------------------------------------- v0.2 --direction both

fn both(w: &Work, k: &str, q: &str, extra: &[&str]) -> Output {
    let mut c = bin();
    c.args(["sync", "--direction", "both", "--kvaesitso"])
        .arg(w.p(k))
        .arg("--quicksearch")
        .arg(w.p(q));
    for a in extra {
        if a.starts_with('-') || ["discard", "accept", "error"].contains(a) {
            c.arg(a);
        } else {
            c.arg(w.p(a));
        }
    }
    c.output().unwrap()
}

fn kv_tags(p: &Path) -> std::collections::BTreeMap<String, Vec<String>> {
    launcher_tags_core_tags(p)
}

/// Read tags through the public CLI (`inspect --json` has only counts), so parse the ZIP here.
fn launcher_tags_core_tags(p: &Path) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut a = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    let mut out: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    let names: Vec<String> = a.file_names().map(String::from).collect();
    let mut pages: Vec<&String> = names
        .iter()
        .filter(|n| n.starts_with("customizations."))
        .collect();
    pages.sort();
    for n in pages {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut a.by_name(n).unwrap(), &mut s).unwrap();
        for r in serde_json::from_str::<Vec<Value>>(&s).unwrap() {
            let key = r["key"].as_str().unwrap_or("");
            if r["type"] == "tag" && key.starts_with("app://") {
                let pkg = key[6..].split(':').next().unwrap().to_string();
                out.entry(pkg)
                    .or_default()
                    .push(r["value"].as_str().unwrap().to_string());
            }
        }
    }
    out
}

#[test]
fn both_initial_then_noop_then_unimported_detection() {
    let w = Work::new("basic");
    let before = w.inputs_snapshot();
    let o = both(
        &w,
        "input.kvaesitso",
        "input.quicksearch",
        &[
            "--init-state",
            "--state",
            "state.json",
            "--output",
            "q1.quicksearch",
            "--kvaesitso-output",
            "k1.kvaesitso",
            "--report",
            "r1.json",
        ],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(before, w.inputs_snapshot());
    let q1 = json_file(&w.p("q1.quicksearch"));
    let prefs = &q1["preferences"]["user_app_preferences"];
    assert_eq!(
        prefs["nickname_app_org.example.reader"]["value"],
        "e-book, čtení, Knihy"
    );
    assert_eq!(prefs["nickname_app_org.example.music:10"]["value"], "práce");
    let k1 = kv_tags(&w.p("k1.kvaesitso"));
    assert!(k1["org.example.reader"].contains(&"e-book".to_string()));
    assert!(k1["org.example.maps"].contains(&"navigace".to_string()));
    assert!(!k1.contains_key("org.example.unrelated"));
    let r1 = json_file(&w.p("r1.json"));
    assert_eq!(r1["initial_sync"], true);
    assert_eq!(
        r1["unresolvable_apps"],
        serde_json::json!(["org.example.unrelated"])
    );
    let report_text = std::fs::read_to_string(w.p("r1.json")).unwrap();
    assert!(!report_text.contains("e-book") && !report_text.contains("Knihy"));

    // Second run on the imported outputs: nothing to do, state backup kept.
    let o = both(
        &w,
        "k1.kvaesitso",
        "q1.quicksearch",
        &[
            "--state",
            "state.json",
            "--output",
            "q2.quicksearch",
            "--kvaesitso-output",
            "k2.kvaesitso",
            "--report",
            "r2.json",
        ],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    let r2 = json_file(&w.p("r2.json"));
    assert_eq!(r2["aliases_added_to_quicksearch"], 0);
    assert_eq!(r2["tags_added_to_kvaesitso"], 0);
    assert_eq!(r2["pending_quicksearch"], "imported");
    assert!(w.p("state.json.prev").exists());
    assert_eq!(json_file(&w.p("q2.quicksearch")), q1);

    // Third run on the ORIGINAL exports with the new state: stale inputs -> refuse, nothing written.
    let state_before = std::fs::read(w.p("state.json")).unwrap();
    let o = both(
        &w,
        "input.kvaesitso",
        "input.quicksearch",
        &[
            "--state",
            "state.json",
            "--output",
            "q3.quicksearch",
            "--kvaesitso-output",
            "k3.kvaesitso",
        ],
    );
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("stale"), "{}", stderr(&o));
    assert!(!w.p("q3.quicksearch").exists() && !w.p("k3.kvaesitso").exists());
    assert_eq!(std::fs::read(w.p("state.json")).unwrap(), state_before);
    // Forcing it shows that it would delete (that is why it is refused by default).
    let o = both(
        &w,
        "input.kvaesitso",
        "input.quicksearch",
        &["--state", "state.json", "--allow-stale", "--dry-run"],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(!String::from_utf8_lossy(&o.stdout).contains(" 0 deletion(s)"));
}

/// Same content as the original exports but "exported later" (new timestamps).
fn reexport_unchanged(w: &Work, k_name: &str, q_name: &str) {
    let mut q = json_file(&w.p("input.quicksearch"));
    q["exportedAtEpochMs"] = serde_json::json!(1_800_000_000_000i64);
    std::fs::write(w.p(q_name), serde_json::to_vec_pretty(&q).unwrap()).unwrap();
    let mut a = zip::ZipArchive::new(std::fs::File::open(w.p("input.kvaesitso")).unwrap()).unwrap();
    let mut out = zip::ZipWriter::new(std::fs::File::create(w.p(k_name)).unwrap());
    for i in 0..a.len() {
        let mut f = a.by_index(i).unwrap();
        let name = f.name().to_string();
        let mut data = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut data).unwrap();
        if name == "meta" {
            let mut m: Value = serde_json::from_slice(&data).unwrap();
            m["timestamp"] = serde_json::json!(1_800_000_000_000i64);
            data = serde_json::to_vec(&m).unwrap();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut out, &data).unwrap();
    }
    out.finish().unwrap();
}

#[test]
fn both_detects_outputs_that_were_never_imported() {
    let w = Work::new("basic");
    let o = both(
        &w,
        "input.kvaesitso",
        "input.quicksearch",
        &[
            "--init-state",
            "--state",
            "st.json",
            "--output",
            "q1.quicksearch",
            "--kvaesitso-output",
            "k1.kvaesitso",
        ],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    // The user forgot to import and exports again: fresh files without our changes.
    reexport_unchanged(&w, "k2.kvaesitso", "q2.quicksearch");
    let o = both(
        &w,
        "k2.kvaesitso",
        "q2.quicksearch",
        &[
            "--state",
            "st.json",
            "--output",
            "q3.quicksearch",
            "--kvaesitso-output",
            "k3.kvaesitso",
        ],
    );
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("never imported"), "{}", stderr(&o));
    let o = both(
        &w,
        "k2.kvaesitso",
        "q2.quicksearch",
        &[
            "--state",
            "st.json",
            "--output",
            "q3.quicksearch",
            "--kvaesitso-output",
            "k3.kvaesitso",
            "--pending",
            "discard",
        ],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(String::from_utf8_lossy(&o.stdout).contains(" 0 deletion(s)"));
    let q3 = json_file(&w.p("q3.quicksearch"));
    assert_eq!(
        q3["preferences"]["user_app_preferences"]["nickname_app_org.example.reader"]["value"],
        "e-book, čtení, Knihy"
    );
}

#[test]
fn both_argument_and_path_validation() {
    let w = Work::new("basic");
    // State required; must exist unless --init-state.
    assert_eq!(
        both(&w, "input.kvaesitso", "input.quicksearch", &["--dry-run"])
            .status
            .code(),
        Some(1)
    );
    let o = both(
        &w,
        "input.kvaesitso",
        "input.quicksearch",
        &["--state", "missing.json", "--dry-run"],
    );
    assert!(stderr(&o).contains("--init-state"), "{}", stderr(&o));
    // v0.2 flags rejected in one-way mode.
    let o = w.sync(&["--dry-run", "--state", "s.json"]);
    assert_eq!(o.status.code(), Some(1));
    // Kvaesitso output must not be an input, even with --force.
    let before = w.inputs_snapshot();
    let o = both(
        &w,
        "input.kvaesitso",
        "input.quicksearch",
        &[
            "--init-state",
            "--state",
            "s.json",
            "--output",
            "q.quicksearch",
            "--kvaesitso-output",
            "input.kvaesitso",
            "--force",
        ],
    );
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(before, w.inputs_snapshot());
    // Outputs must differ from each other and from the state file.
    let o = both(
        &w,
        "input.kvaesitso",
        "input.quicksearch",
        &[
            "--init-state",
            "--state",
            "s.json",
            "--output",
            "x.out",
            "--kvaesitso-output",
            "x.out",
        ],
    );
    assert_eq!(o.status.code(), Some(1));
    std::fs::write(w.p("s.json"), b"{}").unwrap();
    let o = both(
        &w,
        "input.kvaesitso",
        "input.quicksearch",
        &[
            "--state",
            "s.json",
            "--output",
            "s.json",
            "--kvaesitso-output",
            "k.kvaesitso",
            "--force",
        ],
    );
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(std::fs::read(w.p("s.json")).unwrap(), b"{}");
    // --init-state with an existing state file is refused.
    let o = both(
        &w,
        "input.kvaesitso",
        "input.quicksearch",
        &["--init-state", "--state", "s.json", "--dry-run"],
    );
    assert_eq!(o.status.code(), Some(1));
    // Dry run writes nothing (no state either).
    let o = both(
        &w,
        "input.kvaesitso",
        "input.quicksearch",
        &["--init-state", "--state", "new.json", "--dry-run"],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(!w.p("new.json").exists());
    assert!(String::from_utf8_lossy(&o.stdout).contains("Initial sync"));
}
