//! Property tests: idempotence, verbatim preservation, de-duplication, Unicode safety.

use launcher_tags_core::{
    alias, kvaesitso::SourceTags, merge, quicksearch::QuickSearchBackup, PackageTags,
};
use proptest::prelude::*;
use serde_json::{json, Value};

/// Alias-ish strings mixing ASCII, Czech diacritics (pre- and decomposed), emoji, CJK,
/// whitespace and occasional commas (which the source side must never emit).
fn alias_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            Just("a".to_string()),
            Just("B".to_string()),
            Just("č".to_string()),
            Just("Č".to_string()),
            Just("c\u{030c}".to_string()),
            Just("é".to_string()),
            Just("e\u{0301}".to_string()),
            Just("ß".to_string()),
            Just("🎵".to_string()),
            Just("漢".to_string()),
            Just(" ".to_string()),
            Just("-".to_string()),
            Just("'".to_string()),
            Just("\"".to_string()),
            Just("\\".to_string()),
        ],
        1..6,
    )
    .prop_map(|parts| parts.concat())
}

fn existing_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            alias_strategy(),
            Just(String::new()),
            Just("  ".to_string())
        ],
        0..4,
    )
    .prop_map(|v| v.join(","))
}

fn source_strategy() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(alias_strategy(), 0..6).prop_map(|v| {
        // Mimic the source reader: trimmed, non-blank, no commas, de-duplicated.
        let mut out: Vec<String> = Vec::new();
        for a in v {
            let t = a.trim().to_string();
            if t.is_empty() || t.contains(',') {
                continue;
            }
            if !out
                .iter()
                .any(|o| alias::match_key(o) == alias::match_key(&t))
            {
                out.push(t);
            }
        }
        out
    })
}

fn target(existing: Option<&str>) -> QuickSearchBackup {
    let mut prefs = serde_json::Map::new();
    prefs.insert(
        "unrelated".into(),
        json!({"type": "string", "value": "keep, me"}),
    );
    prefs.insert(
        "nickname_app_p.q:10".into(),
        json!({"type": "string", "value": "profile"}),
    );
    if let Some(e) = existing {
        prefs.insert(
            "nickname_app_p.q".into(),
            json!({"type": "string", "value": e}),
        );
    }
    QuickSearchBackup::from_value(json!({
        "formatVersion": 2,
        "preferences": {"user_app_preferences": prefs, "other_file": {"x": {"type":"int","value":1}}},
        "notes": [{"text": "n"}],
        "selectedExportItems": ["SETTINGS"],
        "unknownField": {"nested": [1, 2.5, null, true]}
    }))
    .unwrap()
}

fn source(aliases: Vec<String>) -> SourceTags {
    SourceTags::from_packages(vec![PackageTags {
        package: "p.q".into(),
        aliases,
    }])
}

fn value_of(t: &QuickSearchBackup) -> Option<String> {
    t.as_value()["preferences"]["user_app_preferences"]
        .get("nickname_app_p.q")
        .map(|v| v["value"].as_str().unwrap().to_string())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn merge_is_idempotent(existing in prop::option::of(existing_strategy()), src in source_strategy()) {
        let mut t = target(existing.as_deref());
        let s = source(src);
        merge::merge(&s, &mut t).unwrap();
        let once = t.as_value().clone();
        let second = merge::merge(&s, &mut t).unwrap();
        prop_assert!(second.changes.is_empty());
        prop_assert_eq!(second.stats.added_unique_aliases, 0);
        prop_assert_eq!(t.as_value(), &once);
        // Serialization round trip must be lossless.
        let bytes = t.to_pretty_bytes().unwrap();
        let back = QuickSearchBackup::parse(&bytes).unwrap();
        prop_assert_eq!(back.as_value(), &once);
    }

    #[test]
    fn existing_aliases_are_kept_first_and_verbatim(existing in existing_strategy(), src in source_strategy()) {
        let mut t = target(Some(&existing));
        merge::merge(&source(src), &mut t).unwrap();
        let new_value = value_of(&t).unwrap();
        let kept = existing.trim_end_matches(|c: char| c == ',' || c.is_whitespace());
        prop_assert!(new_value == existing || new_value.starts_with(kept),
            "existing {:?} -> {:?}", existing, new_value);
        let old: Vec<String> = alias::split(&existing).into_iter().map(str::to_string).collect();
        let new: Vec<String> = alias::split(&new_value).into_iter().map(str::to_string).collect();
        prop_assert_eq!(&new[..old.len()], &old[..]);
    }

    #[test]
    fn every_source_alias_is_present_once(existing in prop::option::of(existing_strategy()), src in source_strategy()) {
        let mut t = target(existing.as_deref());
        merge::merge(&source(src.clone()), &mut t).unwrap();
        let result = value_of(&t).unwrap_or_default();
        let keys: Vec<String> = alias::split(&result).iter().map(|a| alias::match_key(a)).collect();
        let pre: Vec<String> = existing.as_deref().map(alias::split).unwrap_or_default()
            .iter().map(|a| alias::match_key(a)).collect();
        for a in &src {
            let k = alias::match_key(a);
            prop_assert!(keys.contains(&k), "missing {:?} in {:?}", a, result);
            // Appended aliases are never duplicates of anything before them.
            if !pre.contains(&k) {
                prop_assert_eq!(keys.iter().filter(|x| **x == k).count(), 1);
            }
        }
    }

    #[test]
    fn unrelated_data_is_untouched(existing in prop::option::of(existing_strategy()), src in source_strategy()) {
        let mut t = target(existing.as_deref());
        let before = t.as_value().clone();
        merge::merge(&source(src), &mut t).unwrap();
        let after = t.as_value();
        for root in ["formatVersion", "notes", "selectedExportItems", "unknownField"] {
            prop_assert_eq!(&before[root], &after[root]);
        }
        prop_assert_eq!(&before["preferences"]["other_file"], &after["preferences"]["other_file"]);
        let (b, a) = (&before["preferences"]["user_app_preferences"], &after["preferences"]["user_app_preferences"]);
        prop_assert_eq!(&b["unrelated"], &a["unrelated"]);
        prop_assert_eq!(&b["nickname_app_p.q:10"], &a["nickname_app_p.q:10"]);
    }

    #[test]
    fn match_key_is_case_and_normalization_insensitive(a in alias_strategy()) {
        let upper = a.to_uppercase();
        prop_assert_eq!(alias::match_key(&a), alias::match_key(&format!("  {a}  ")));
        // Uppercasing can change length (ß -> SS); compare only when it round-trips.
        if upper.to_lowercase() == a.to_lowercase() {
            prop_assert_eq!(alias::match_key(&a), alias::match_key(&upper));
        }
    }
}

#[test]
fn json_escaping_round_trips() {
    let mut t = target(None);
    merge::merge(
        &source(vec![
            "quote \" back\\slash".into(),
            "tab\tx".into(),
            "nul\u{1}".into(),
        ]),
        &mut t,
    )
    .unwrap();
    let bytes = t.to_pretty_bytes().unwrap();
    let back: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        back["preferences"]["user_app_preferences"]["nickname_app_p.q"]["value"],
        json!("quote \" back\\slash, tab\tx, nul\u{1}")
    );
    assert!(std::str::from_utf8(&bytes).is_ok());
}
