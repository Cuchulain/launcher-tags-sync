//! Alias helpers mirroring Quick Search's `NicknameUtils` (split on `,`, trim, drop blanks),
//! plus a Unicode-aware matching key used only for de-duplication.

use unicode_normalization::UnicodeNormalization;

/// Quick Search alias separator.
pub const SEPARATOR: char = ',';
/// Separator used when appending aliases (Quick Search's display separator).
pub const JOINER: &str = ", ";

/// Split a stored nickname string into trimmed, non-blank aliases (original spelling kept,
/// duplicates kept: callers decide).
pub fn split(value: &str) -> Vec<&str> {
    value
        .split(SEPARATOR)
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .collect()
}

/// Matching key: trim, NFC, Unicode lowercase, NFC again. Used only for comparison; the
/// original text is what gets written.
pub fn match_key(alias: &str) -> String {
    let nfc: String = alias.trim().nfc().collect();
    nfc.to_lowercase().nfc().collect()
}
