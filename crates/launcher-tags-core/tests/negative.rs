//! Negative / robustness tests: unsafe archives, unsupported formats, type collisions.

use std::io::{Cursor, Write};

use launcher_tags_core::{
    kvaesitso, merge, quicksearch::QuickSearchBackup, Error, Limits, WarningCategory,
};
use serde_json::json;
use zip::write::SimpleFileOptions;

const META: &str = r#"{"format":"1.9","versionName":"1.41.0"}"#;

fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in entries {
        w.start_file(*name, opts).unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

fn read(bytes: Vec<u8>) -> Result<kvaesitso::SourceTags, Error> {
    kvaesitso::read(Cursor::new(bytes), &Limits::default())
}

fn page(records: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&records).unwrap()
}

fn tag(key: &str, value: &str) -> serde_json::Value {
    json!({"key": key, "value": value, "type": "tag"})
}

#[test]
fn rejects_path_traversal_member() {
    let p = page(json!([tag("app://a.b:a.b.M", "x")]));
    let bytes = zip_of(&[
        ("meta", META.as_bytes()),
        ("customizations.0000", &p),
        ("../evil", b"x"),
    ]);
    assert!(matches!(read(bytes), Err(Error::UnsafeArchiveEntry { .. })));
}

#[test]
fn rejects_absolute_member() {
    let p = page(json!([tag("app://a.b:a.b.M", "x")]));
    let bytes = zip_of(&[("customizations.0000", &p), ("/etc/passwd", b"x")]);
    assert!(matches!(read(bytes), Err(Error::UnsafeArchiveEntry { .. })));
}

#[test]
fn rejects_oversized_member_zip_bomb() {
    // 4 MiB of highly compressible JSON whitespace inside a matched member.
    let mut big = b"[".to_vec();
    big.extend(std::iter::repeat_n(b' ', 4 * 1024 * 1024));
    big.push(b']');
    let bytes = zip_of(&[("customizations.0000", &big)]);
    assert!(bytes.len() < 64 * 1024, "fixture should compress well");
    let limits = Limits {
        max_member_bytes: 1024 * 1024,
        ..Limits::default()
    };
    let err = kvaesitso::read(Cursor::new(bytes), &limits).unwrap_err();
    assert!(matches!(err, Error::ArchiveLimit(_)), "{err}");
}

#[test]
fn rejects_too_many_entries() {
    let names: Vec<String> = (0..50).map(|i| format!("junk{i}")).collect();
    let mut entries: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b"x"[..])).collect();
    let p = page(json!([]));
    entries.push(("customizations.0000", &p));
    let limits = Limits {
        max_entries: 10,
        ..Limits::default()
    };
    let err = kvaesitso::read(Cursor::new(zip_of(&entries)), &limits).unwrap_err();
    assert!(matches!(err, Error::ArchiveLimit(_)), "{err}");
}

#[test]
fn rejects_total_uncompressed_limit() {
    let p = page(json!([tag("app://a.b:a.b.M", "x")]));
    let filler = vec![b'a'; 300 * 1024];
    let bytes = zip_of(&[
        ("customizations.0000", &p),
        ("favorites.0000", &filler),
        ("favorites.0001", &filler),
    ]);
    let limits = Limits {
        max_total_bytes: 512 * 1024,
        ..Limits::default()
    };
    assert!(matches!(
        kvaesitso::read(Cursor::new(bytes), &limits),
        Err(Error::ArchiveLimit(_))
    ));
}

#[test]
fn rejects_non_zip_source() {
    assert!(matches!(
        read(b"{\"not\":\"a zip\"}".to_vec()),
        Err(Error::Zip(_))
    ));
}

#[test]
fn rejects_malformed_customization_json() {
    let bytes = zip_of(&[("customizations.0000", b"[{\"key\":")]);
    assert!(matches!(read(bytes), Err(Error::Json { .. })));
}

#[test]
fn rejects_non_array_customization_page() {
    let bytes = zip_of(&[("customizations.0000", b"{\"key\":1}")]);
    assert!(matches!(read(bytes), Err(Error::Schema(_))));
}

#[test]
fn rejects_unsupported_source_major_format() {
    let p = page(json!([tag("app://a.b:a.b.M", "x")]));
    let bytes = zip_of(&[
        ("meta", br#"{"format":"2.0"}"#),
        ("customizations.0000", &p),
    ]);
    assert!(matches!(read(bytes), Err(Error::UnsupportedFormat(_))));
}

#[test]
fn ambiguous_page_numbers_are_rejected() {
    let p = page(json!([]));
    let bytes = zip_of(&[("customizations.0001", &p), ("customizations.00001", &p)]);
    assert!(matches!(read(bytes), Err(Error::Schema(_))));
}

#[test]
fn pages_are_read_in_numeric_order_regardless_of_archive_order() {
    let p0 = page(json!([tag("app://a.b:a.b.M", "first")]));
    let p1 = page(json!([tag("app://a.b:a.b.M", "second")]));
    let p10 = page(json!([tag("app://a.b:a.b.M", "tenth")]));
    let bytes = zip_of(&[
        ("meta", META.as_bytes()),
        ("customizations.0010", &p10),
        ("customizations.0001", &p1),
        ("customizations.0000", &p0),
        ("customizations.0000.bak", b"not json"),
        ("sub/customizations.0002", b"not json"),
    ]);
    let s = read(bytes).unwrap();
    assert_eq!(s.pages, vec![0, 1, 10]);
    assert_eq!(s.packages[0].aliases, vec!["first", "second", "tenth"]);
}

#[test]
fn work_profile_app_tags_are_not_attributed_to_base_package() {
    let p = page(json!([
        tag("app://a.b:a.b.Main:12", "profile-only"),
        tag("app://a.b:a.b.Main", "main"),
    ]));
    let s = read(zip_of(&[
        ("meta", META.as_bytes()),
        ("customizations.0000", &p),
    ]))
    .unwrap();
    assert_eq!(s.packages[0].aliases, vec!["main"]);
    assert_eq!(s.stats.profile_app_tags_skipped, 1);
    assert!(s
        .warnings
        .iter()
        .any(|w| w.category == WarningCategory::ProfileAppTag));
}

#[test]
fn malformed_keys_and_values_are_skipped_with_warnings() {
    let p = page(json!([
        tag("app://", "a"),
        tag("app://:Activity", "b"),
        tag("app://pkg.ok:", "c"),
        tag("app://no package!:X", "d"),
        tag("app://single:X", "e"),
        tag("app://a.b:a.b.M", "   "),
        tag("app://a.b:a.b.M", ""),
        json!({"key": "app://a.b:a.b.M", "type": "tag"}),
        json!({"key": "app://a.b:a.b.M", "value": 7, "type": "tag"}),
        json!("not an object"),
        tag("contact://42", "other domain"),
        json!({"key": "app://a.b:a.b.M", "value": "Label", "type": "label"}),
        tag("app://a.b:a.b.M", "ok"),
    ]));
    let s = read(zip_of(&[
        ("meta", META.as_bytes()),
        ("customizations.0000", &p),
    ]))
    .unwrap();
    assert_eq!(s.packages.len(), 1);
    assert_eq!(s.packages[0].aliases, vec!["ok"]);
    assert_eq!(s.stats.invalid_app_keys_skipped, 5);
    assert_eq!(s.stats.blank_tags_skipped, 2);
    assert_eq!(s.stats.malformed_records_skipped, 3);
    assert_eq!(s.stats.other_tag_records_skipped, 1);
    assert_eq!(s.stats.non_tag_records_ignored, 1);
}

#[test]
fn missing_meta_is_a_warning_not_an_error() {
    let p = page(json!([tag("app://a.b:a.b.M", "x")]));
    let s = read(zip_of(&[("customizations.0000", &p)])).unwrap();
    assert!(s
        .warnings
        .iter()
        .any(|w| w.category == WarningCategory::MissingSourceMeta));
}

#[test]
fn empty_source_warns() {
    let p = page(json!([]));
    let s = read(zip_of(&[
        ("meta", META.as_bytes()),
        ("customizations.0000", &p),
    ]))
    .unwrap();
    assert!(s
        .warnings
        .iter()
        .any(|w| w.category == WarningCategory::NoAppTags));
}

// ---------- Quick Search target validation ----------

fn qs(v: serde_json::Value) -> Result<QuickSearchBackup, Error> {
    QuickSearchBackup::parse(&serde_json::to_vec(&v).unwrap())
}

fn base_prefs() -> serde_json::Value {
    json!({"user_app_preferences": {}})
}

#[test]
fn rejects_unsupported_format_version() {
    for v in [json!(1), json!(3), json!("2"), json!(2.5), json!(null)] {
        let r = qs(json!({"formatVersion": v, "preferences": base_prefs()}));
        assert!(matches!(r, Err(Error::UnsupportedFormat(_))), "{v}");
    }
    let r = qs(json!({"preferences": base_prefs()}));
    assert!(matches!(r, Err(Error::UnsupportedFormat(_))));
}

#[test]
fn rejects_wrong_preferences_types() {
    assert!(qs(json!({"formatVersion": 2, "preferences": []})).is_err());
    assert!(qs(json!({"formatVersion": 2})).is_err());
    assert!(qs(json!({"formatVersion": 2, "preferences": {"user_app_preferences": "x"}})).is_err());
    assert!(qs(json!({"formatVersion": 2, "preferences": {}})).is_err());
    assert!(qs(json!([1, 2])).is_err());
}

#[test]
fn rejects_backup_without_settings_section_selected() {
    let r = qs(json!({
        "formatVersion": 2,
        "preferences": base_prefs(),
        "selectedExportItems": ["NOTES"]
    }));
    assert!(matches!(r, Err(Error::Schema(_))));
    // Legacy backups without selectedExportItems are accepted.
    assert!(qs(json!({"formatVersion": 2, "preferences": base_prefs()})).is_ok());
}

fn source_with(pkg: &str, aliases: &[&str]) -> kvaesitso::SourceTags {
    let recs: Vec<_> = aliases
        .iter()
        .map(|a| tag(&format!("app://{pkg}:{pkg}.Main"), a))
        .collect();
    let p = page(serde_json::Value::Array(recs));
    read(zip_of(&[
        ("meta", META.as_bytes()),
        ("customizations.0000", &p),
    ]))
    .unwrap()
}

#[test]
fn non_string_existing_nickname_aborts_and_is_not_overwritten() {
    for bad in [
        json!({"type": "boolean", "value": true}),
        json!({"type": "string", "value": 5}),
        json!("plain string"),
        json!(null),
    ] {
        let mut t = qs(json!({
            "formatVersion": 2,
            "preferences": {"user_app_preferences": {"nickname_app_a.b": bad.clone()}}
        }))
        .unwrap();
        let before = t.as_value().clone();
        let err = merge::merge(&source_with("a.b", &["x"]), &mut t).unwrap_err();
        assert!(matches!(err, Error::NicknameTypeCollision { .. }), "{bad}");
        assert_eq!(t.as_value(), &before, "target must be untouched on error");
    }
}

#[test]
fn existing_trailing_separator_and_whitespace_policy() {
    // Policy: trailing whitespace/commas of the existing value are dropped before appending;
    // everything before them is kept verbatim.
    let cases = [
        ("a,b", "a,b, x"),
        ("a ,  b  ", "a ,  b, x"),
        ("a, b, ", "a, b, x"),
        (" , ", "x"),
        ("", "x"),
        ("  lead", "  lead, x"),
    ];
    for (existing, expected) in cases {
        let mut t = qs(json!({
            "formatVersion": 2,
            "preferences": {"user_app_preferences": {
                "nickname_app_a.b": {"type": "string", "value": existing}
            }}
        }))
        .unwrap();
        merge::merge(&source_with("a.b", &["x"]), &mut t).unwrap();
        assert_eq!(
            t.as_value()["preferences"]["user_app_preferences"]["nickname_app_a.b"]["value"],
            json!(expected),
            "existing {existing:?}"
        );
    }
}

#[test]
fn unicode_normalization_variants_are_duplicates() {
    // "café" precomposed vs decomposed, plus case variant.
    let mut t = qs(json!({
        "formatVersion": 2,
        "preferences": {"user_app_preferences": {
            "nickname_app_a.b": {"type": "string", "value": "caf\u{00e9}"}
        }}
    }))
    .unwrap();
    let out = merge::merge(
        &source_with("a.b", &["cafe\u{0301}", "CAFÉ", "🎵 hudba", "ŽLUŤOUČKÝ"]),
        &mut t,
    )
    .unwrap();
    assert_eq!(out.stats.added_unique_aliases, 2);
    assert_eq!(
        t.as_value()["preferences"]["user_app_preferences"]["nickname_app_a.b"]["value"],
        json!("caf\u{00e9}, 🎵 hudba, ŽLUŤOUČKÝ")
    );
}

#[test]
fn shortcut_prefixed_package_is_rejected() {
    // `nickname_app_shortcut_*` is Quick Search's app-shortcut namespace.
    let p = page(json!([tag("app://shortcut_x.y:shortcut_x.y.Main", "z")]));
    let s = read(zip_of(&[
        ("meta", META.as_bytes()),
        ("customizations.0000", &p),
    ]))
    .unwrap();
    assert!(s.packages.is_empty());
    assert_eq!(s.stats.invalid_app_keys_skipped, 1);
}

#[test]
fn from_pages_sorts_numerically() {
    let s = kvaesitso::SourceTags::from_pages(vec![
        (2, json!([tag("app://a.b:a.b.M", "two")])),
        (0, json!([tag("app://a.b:a.b.M", "zero")])),
    ])
    .unwrap();
    assert_eq!(s.pages, vec![0, 2]);
    assert_eq!(s.packages[0].aliases, vec!["zero", "two"]);
}
