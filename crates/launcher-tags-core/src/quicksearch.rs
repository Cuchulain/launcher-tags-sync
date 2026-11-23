//! Quick Search `.quicksearch` backup (JSON, `formatVersion` 2).
//!
//! The whole document is kept as a `serde_json::Value` (insertion order and number text
//! preserved) so that every unrelated section, preference and unknown field round-trips.

use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::Error;

/// The only supported backup format version.
pub const FORMAT_VERSION: u64 = 2;
/// SharedPreferences file holding nicknames (`BasePreferences.PREFS_NAME`).
pub const PREFS_FILE: &str = "user_app_preferences";
/// Key prefix of app nicknames; base profile key is `nickname_app_<package>`.
pub const APP_NICKNAME_PREFIX: &str = "nickname_app_";
/// Key prefix of app-shortcut nicknames (not apps).
pub const SHORTCUT_NICKNAME_PREFIX: &str = "nickname_app_shortcut_";
/// Export item that must be selected for `user_app_preferences` to be restored on import.
pub const SETTINGS_ITEM: &str = "SETTINGS";

/// A validated Quick Search backup.
#[derive(Debug, Clone, PartialEq)]
pub struct QuickSearchBackup {
    root: Value,
}

/// Structural overview without any values.
#[derive(Debug, Clone, Serialize)]
pub struct TargetSummary {
    pub format_version: u64,
    pub root_sections: Vec<String>,
    /// (preference file, number of entries)
    pub preference_files: Vec<(String, usize)>,
    pub selected_export_items: Option<Vec<String>>,
    pub base_app_nicknames: usize,
    pub profile_app_nicknames: usize,
    pub shortcut_nicknames: usize,
    pub other_nicknames: usize,
}

impl QuickSearchBackup {
    /// Parse and validate UTF-8 JSON bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let root: Value =
            serde_json::from_slice(bytes).map_err(|e| Error::json("Quick Search backup", &e))?;
        Self::from_value(root)
    }

    /// Validate an already-parsed document.
    pub fn from_value(root: Value) -> Result<Self, Error> {
        validate(&root)?;
        Ok(QuickSearchBackup { root })
    }

    pub fn as_value(&self) -> &Value {
        &self.root
    }

    pub fn into_value(self) -> Value {
        self.root
    }

    /// Pretty-printed UTF-8 JSON with a trailing newline; re-validated after serialization.
    pub fn to_pretty_bytes(&self) -> Result<Vec<u8>, Error> {
        let mut bytes = serde_json::to_vec_pretty(&self.root)
            .map_err(|e| Error::json("serializing output", &e))?;
        bytes.push(b'\n');
        let reparsed = Self::parse(&bytes)?;
        if reparsed.root != self.root {
            return Err(Error::Schema(
                "serialized output does not reparse to the same document".into(),
            ));
        }
        Ok(bytes)
    }

    /// The `preferences.user_app_preferences` map (guaranteed present by validation).
    pub fn user_app_preferences(&self) -> &Map<String, Value> {
        self.root["preferences"][PREFS_FILE]
            .as_object()
            .expect("validated")
    }

    pub(crate) fn user_app_preferences_mut(&mut self) -> &mut Map<String, Value> {
        self.root["preferences"][PREFS_FILE]
            .as_object_mut()
            .expect("validated")
    }

    pub fn summary(&self) -> TargetSummary {
        let root = self.root.as_object().expect("validated");
        let prefs = root["preferences"].as_object().expect("validated");
        let mut s = TargetSummary {
            format_version: FORMAT_VERSION,
            root_sections: root.keys().cloned().collect(),
            preference_files: prefs
                .iter()
                .map(|(k, v)| (k.clone(), v.as_object().map_or(0, Map::len)))
                .collect(),
            selected_export_items: root.get("selectedExportItems").and_then(|v| {
                v.as_array().map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
            }),
            base_app_nicknames: 0,
            profile_app_nicknames: 0,
            shortcut_nicknames: 0,
            other_nicknames: 0,
        };
        for key in self.user_app_preferences().keys() {
            match classify_key(key) {
                KeyKind::BaseApp => s.base_app_nicknames += 1,
                KeyKind::ProfileApp => s.profile_app_nicknames += 1,
                KeyKind::Shortcut => s.shortcut_nicknames += 1,
                KeyKind::OtherNickname => s.other_nicknames += 1,
                KeyKind::Other => {}
            }
        }
        s
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyKind {
    BaseApp,
    ProfileApp,
    Shortcut,
    OtherNickname,
    Other,
}

pub(crate) fn classify_key(key: &str) -> KeyKind {
    if key.starts_with(SHORTCUT_NICKNAME_PREFIX) {
        KeyKind::Shortcut
    } else if let Some(rest) = key.strip_prefix(APP_NICKNAME_PREFIX) {
        if rest.contains(':') {
            KeyKind::ProfileApp
        } else {
            KeyKind::BaseApp
        }
    } else if key.starts_with("nickname_") {
        KeyKind::OtherNickname
    } else {
        KeyKind::Other
    }
}

/// Validate the parts of the import schema this tool relies on. Fails closed.
pub fn validate(root: &Value) -> Result<(), Error> {
    let obj = root
        .as_object()
        .ok_or_else(|| Error::Schema("Quick Search backup root is not a JSON object".into()))?;
    match obj.get("formatVersion") {
        Some(v) if v.as_u64() == Some(FORMAT_VERSION) => {}
        Some(Value::Number(n)) => {
            return Err(Error::UnsupportedFormat(format!(
                "Quick Search formatVersion {n} (only {FORMAT_VERSION} is supported)"
            )))
        }
        Some(other) => {
            return Err(Error::UnsupportedFormat(format!(
                "Quick Search formatVersion has type {} (expected integer {FORMAT_VERSION})",
                type_name(other)
            )))
        }
        None => {
            return Err(Error::UnsupportedFormat(
                "Quick Search backup has no formatVersion".into(),
            ))
        }
    }
    let prefs = obj
        .get("preferences")
        .and_then(Value::as_object)
        .ok_or_else(|| Error::Schema("'preferences' is missing or not a JSON object".into()))?;
    match prefs.get(PREFS_FILE) {
        Some(Value::Object(_)) => {}
        Some(other) => {
            return Err(Error::Schema(format!(
                "preferences.{PREFS_FILE} has type {} (expected object)",
                type_name(other)
            )))
        }
        None => {
            return Err(Error::Schema(format!(
                "preferences.{PREFS_FILE} is missing; export the Quick Search backup with Settings selected"
            )))
        }
    }
    if let Some(items) = obj.get("selectedExportItems") {
        let items = items
            .as_array()
            .ok_or_else(|| Error::Schema("selectedExportItems is not an array".into()))?;
        if !items.iter().all(Value::is_string) {
            return Err(Error::Schema(
                "selectedExportItems contains a non-string item".into(),
            ));
        }
        if !items.iter().any(|v| v.as_str() == Some(SETTINGS_ITEM)) {
            return Err(Error::Schema(format!(
                "selectedExportItems does not include {SETTINGS_ITEM}; Quick Search would ignore nickname changes on import"
            )));
        }
    }
    Ok(())
}

/// Value of a well-formed `{"type":"string","value":"..."}` preference entry.
pub(crate) fn typed_string(v: &Value) -> Option<&str> {
    let o = v.as_object()?;
    if o.get("type")?.as_str()? != "string" {
        return None;
    }
    o.get("value")?.as_str()
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_classes() {
        assert_eq!(classify_key("nickname_app_a.b"), KeyKind::BaseApp);
        assert_eq!(classify_key("nickname_app_a.b:10"), KeyKind::ProfileApp);
        assert_eq!(classify_key("nickname_app_shortcut_x"), KeyKind::Shortcut);
        assert_eq!(classify_key("nickname_contact_1"), KeyKind::OtherNickname);
        assert_eq!(classify_key("search_theme"), KeyKind::Other);
    }

    #[test]
    fn number_text_is_preserved() {
        let src = br#"{"formatVersion":2,"preferences":{"user_app_preferences":{}},"big":12345678901234567890123,"f":1.50,"e":1e400,"u":1E5,"g":-0.0,"h":1.5e-7}"#;
        let b = QuickSearchBackup::parse(src).unwrap();
        let out = String::from_utf8(b.to_pretty_bytes().unwrap()).unwrap();
        assert!(out.contains("12345678901234567890123"));
        assert!(out.contains("1.50"));
        assert!(out.contains("-0.0"));
        assert!(out.contains("1.5e-7"));
        // Documented: serde_json canonicalizes exponent spelling (1e400 -> 1e+400, 1E5 -> 1e+5);
        // the numeric value and digits are unchanged.
        assert!(out.contains("1e+400"));
        assert!(out.contains("1e+5"));
    }
}
