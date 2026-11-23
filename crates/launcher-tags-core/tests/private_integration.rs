//! Optional private integration test against the user's real backups.
//!
//! Never runs in CI (ignored). Put private files into the git-ignored `local-private/`:
//!
//! ```text
//! local-private/kvaesitso.kvaesitso
//! local-private/quicksearch.quicksearch
//! local-private/expected.quicksearch   (optional: the already-imported one-off output)
//! ```
//!
//! or point `LTS_PRIVATE_KVAESITSO`, `LTS_PRIVATE_QUICKSEARCH`, `LTS_PRIVATE_EXPECTED` at
//! them, then run `cargo test -p launcher-tags-core --test private_integration -- --ignored`.
//! Only counts are printed; no values.

use std::path::PathBuf;

use launcher_tags_core::{kvaesitso, merge, quicksearch::QuickSearchBackup, Limits};

fn path(env: &str, default: &str) -> PathBuf {
    std::env::var_os(env).map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../local-private")
            .join(default)
    })
}

#[test]
#[ignore = "needs private backups in local-private/"]
fn private_backups_match_reference_metrics() {
    let k = path("LTS_PRIVATE_KVAESITSO", "kvaesitso.kvaesitso");
    let q = path("LTS_PRIVATE_QUICKSEARCH", "quicksearch.quicksearch");
    let source = kvaesitso::read_file(&k, &Limits::default()).expect("read private Kvaesitso");
    let mut target =
        QuickSearchBackup::parse(&std::fs::read(&q).expect("read private Quick Search"))
            .expect("parse private Quick Search");
    let outcome = merge::merge(&source, &mut target).expect("merge");
    let s = &source.stats;
    let m = &outcome.stats;
    eprintln!("source stats: {s:?}");
    eprintln!("merge stats: {m:?}");
    eprintln!(
        "warning categories: {:?}",
        source
            .warnings
            .iter()
            .map(|w| w.category)
            .collect::<Vec<_>>()
    );

    // Reference from the successful one-off import on 2026-11-23 (README.md).
    assert_eq!(s.app_tag_records, 521, "source app-tag rows");
    assert_eq!(s.unique_app_packages, 325, "unique packages");
    assert_eq!(m.new_entries, 318, "new nickname keys");
    assert_eq!(m.extended_entries, 3, "extended nickname keys");
    assert_eq!(m.unchanged_existing_entries, 4, "unchanged existing keys");
    assert_eq!(s.shortcut_tag_records_skipped, 8, "shortcut tags skipped");
    assert!(outcome.preserved_non_app_settings);

    let expected = path("LTS_PRIVATE_EXPECTED", "expected.quicksearch");
    if expected.exists() {
        let exp: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&expected).unwrap()).unwrap();
        // Compare without printing anything on mismatch.
        assert!(
            target.as_value() == &exp,
            "output differs semantically from the imported reference"
        );
    }
}

#[test]
#[ignore = "needs private backups in local-private/"]
fn private_backups_are_idempotent() {
    let k = path("LTS_PRIVATE_KVAESITSO", "kvaesitso.kvaesitso");
    let q = path("LTS_PRIVATE_QUICKSEARCH", "quicksearch.quicksearch");
    let source = kvaesitso::read_file(&k, &Limits::default()).unwrap();
    let mut target = QuickSearchBackup::parse(&std::fs::read(&q).unwrap()).unwrap();
    merge::merge(&source, &mut target).unwrap();
    let once = target.as_value().clone();
    let again = merge::merge(&source, &mut target).unwrap();
    assert!(again.changes.is_empty());
    assert!(target.as_value() == &once);
}

#[test]
#[ignore = "needs private backups in local-private/"]
fn private_backups_bidirectional_initial_sync() {
    use launcher_tags_core::bisync::{self, Options};
    let k = path("LTS_PRIVATE_KVAESITSO", "kvaesitso.kvaesitso");
    let q = path("LTS_PRIVATE_QUICKSEARCH", "quicksearch.quicksearch");
    let kbytes = std::fs::read(&k).unwrap();
    let lim = Limits::default();
    let source = kvaesitso::read(std::io::Cursor::new(&kbytes), &lim).unwrap();
    let target = QuickSearchBackup::parse(&std::fs::read(&q).unwrap()).unwrap();
    let plan = bisync::plan(&source, &target, None, &Options::default()).unwrap();
    let mut merged = target.clone();
    bisync::apply_quicksearch(&plan, &mut merged).unwrap();
    let (kout, stats) =
        bisync::apply_kvaesitso(&plan, &source, std::io::Cursor::new(&kbytes), &lim).unwrap();
    eprintln!(
        "initial bisync: QS entries changed {}, KV packages changed {}, unresolvable apps {} ({} aliases), state packages {}, rewrite {:?}",
        plan.to_quicksearch.len(),
        plan.to_kvaesitso.len(),
        plan.unresolvable.len(),
        plan.unresolvable_aliases,
        plan.new_state.packages.len(),
        stats
    );
    assert_eq!(plan.removals(), 0);
    // A follow-up run on the outputs must be a no-op.
    let source2 = kvaesitso::read(std::io::Cursor::new(&kout), &lim).unwrap();
    let plan2 = bisync::plan(
        &source2,
        &merged,
        Some(&plan.new_state),
        &Options::default(),
    )
    .unwrap();
    assert!(plan2.is_noop());
}
