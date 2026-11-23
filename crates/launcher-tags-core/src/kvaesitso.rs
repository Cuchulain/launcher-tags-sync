//! Safe reader for Kvaesitso `.kvaesitso` ZIP backups (read-only, in-memory, no extraction).
//!
//! Only `meta` and members named exactly `customizations.` + ≥4 ASCII digits are read.
//! Every member name is still checked for path traversal and every declared size counts
//! against the limits, so hostile archives are rejected as a whole.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::{Read, Seek};
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::alias;
use crate::error::Error;
use crate::warning::{Warning, WarningCategory};

/// Backup format version the semantics were verified against (Kvaesitso 1.41.0).
pub const VERIFIED_FORMAT: &str = "1.9";

/// Resource limits for reading an archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum number of members in the archive.
    pub max_entries: usize,
    /// Maximum uncompressed size of any member that is read.
    pub max_member_bytes: u64,
    /// Maximum sum of declared (and of actually read) uncompressed sizes.
    pub max_total_bytes: u64,
    /// Maximum size of the `meta` member.
    pub max_meta_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_entries: 10_000,
            max_member_bytes: 64 * 1024 * 1024,
            max_total_bytes: 512 * 1024 * 1024,
            max_meta_bytes: 1024 * 1024,
        }
    }
}

/// Diagnostics from the `meta` member.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SourceMeta {
    pub format: Option<String>,
    pub version_name: Option<String>,
    /// Backup creation time (ms since epoch), if present.
    pub timestamp: Option<i64>,
}

/// Tags for one Android package, in first-seen order, de-duplicated, trimmed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PackageTags {
    pub package: String,
    pub aliases: Vec<String>,
}

/// Counters describing the source. Contains no values.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SourceStats {
    /// All records in all customization pages.
    pub customization_records: usize,
    /// `type == "tag"` records whose key starts with `app://` (valid or not).
    pub app_tag_records: usize,
    pub unique_app_packages: usize,
    /// Unique (package, alias) pairs that survived filtering.
    pub accepted_app_tags: usize,
    pub shortcut_tag_records_skipped: usize,
    pub duplicate_app_tag_records: usize,
    pub invalid_app_keys_skipped: usize,
    pub comma_tags_skipped: usize,
    pub blank_tags_skipped: usize,
    pub profile_app_tags_skipped: usize,
    pub malformed_records_skipped: usize,
    /// Tags on non-app searchables (contacts, files, websites, …).
    pub other_tag_records_skipped: usize,
    /// Non-tag customizations (icons, labels, …).
    pub non_tag_records_ignored: usize,
}

/// App tags extracted from a Kvaesitso backup.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceTags {
    pub meta: Option<SourceMeta>,
    /// Numeric page numbers that were read, ascending.
    pub pages: Vec<u64>,
    pub packages: Vec<PackageTags>,
    pub stats: SourceStats,
    pub warnings: Vec<Warning>,
    /// Main-profile component key (`app://pkg:Class`) per package, usable for writing new
    /// tags: a key present in `favorites.*` is preferred (Kvaesitso resolves tagged items
    /// there), otherwise the first existing tag key.
    pub components: BTreeMap<String, String>,
    /// Packages that appear anywhere in customizations or favorites (any record type).
    pub present_packages: BTreeSet<String>,
}

impl SourceTags {
    /// Whether the backup contains any trace of `package`.
    pub fn is_present(&self, package: &str) -> bool {
        self.present_packages.contains(package)
            || self.packages.iter().any(|p| p.package == package)
    }
}

/// Indexes of interesting members, after safety checks of every member.
pub(crate) struct Scan {
    pub meta: Option<usize>,
    pub customizations: BTreeMap<u64, usize>,
    pub favorites: BTreeMap<u64, usize>,
    /// Members whose name starts with `customizations.` but is not `customizations.NNNN`
    /// (Kvaesitso's restore would still import them).
    pub stray_customizations: usize,
}

pub(crate) fn open_archive<R: Read + Seek>(
    reader: R,
    limits: &Limits,
) -> Result<(zip::ZipArchive<R>, Scan), Error> {
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| Error::Zip(e.to_string()))?;
    if archive.len() > limits.max_entries {
        return Err(Error::ArchiveLimit(format!(
            "{} members (limit {})",
            archive.len(),
            limits.max_entries
        )));
    }
    let mut scan = Scan {
        meta: None,
        customizations: BTreeMap::new(),
        favorites: BTreeMap::new(),
        stray_customizations: 0,
    };
    let mut declared_total: u64 = 0;
    for i in 0..archive.len() {
        let entry = archive
            .by_index_raw(i)
            .map_err(|e| Error::Zip(e.to_string()))?;
        let name = entry.name().to_string();
        if !is_safe_member_name(&name) || entry.enclosed_name().is_none() {
            return Err(Error::UnsafeArchiveEntry { index: i });
        }
        declared_total = declared_total.saturating_add(entry.size());
        if declared_total > limits.max_total_bytes {
            return Err(Error::ArchiveLimit(format!(
                "declared uncompressed size exceeds {} bytes",
                limits.max_total_bytes
            )));
        }
        if name == "meta" {
            if scan.meta.replace(i).is_some() {
                return Err(Error::Schema("duplicate 'meta' member".into()));
            }
        } else if let Some(page) = parse_page_name("customizations.", &name)? {
            if scan.customizations.insert(page, i).is_some() {
                return Err(Error::Schema(format!(
                    "ambiguous customizations page number {page}"
                )));
            }
        } else if name.starts_with("customizations.") {
            scan.stray_customizations += 1;
        } else if let Some(page) = parse_page_name("favorites.", &name)? {
            if scan.favorites.insert(page, i).is_some() {
                return Err(Error::Schema(format!(
                    "ambiguous favorites page number {page}"
                )));
            }
        }
    }
    Ok((archive, scan))
}

/// Read a `.kvaesitso` file.
pub fn read_file(path: impl AsRef<Path>, limits: &Limits) -> Result<SourceTags, Error> {
    let file = std::fs::File::open(path)?;
    read(std::io::BufReader::new(file), limits)
}

/// Read a Kvaesitso ZIP backup from any seekable reader.
pub fn read<R: Read + Seek>(reader: R, limits: &Limits) -> Result<SourceTags, Error> {
    let (mut archive, scan) = open_archive(reader, limits)?;
    let mut read_total: u64 = 0;
    let mut warnings = Vec::new();
    if scan.stray_customizations > 0 {
        warnings.push(Warning::new(
            WarningCategory::UnexpectedMember,
            format!(
                "{} member(s) start with 'customizations.' but are not customizations.NNNN; ignored here, although Kvaesitso's restore would read them",
                scan.stray_customizations
            ),
        ));
    }
    let meta = match scan.meta {
        Some(i) => {
            let bytes = read_member(
                &mut archive,
                i,
                limits.max_meta_bytes,
                &mut read_total,
                limits,
            )?;
            Some(parse_meta(&bytes, &mut warnings)?)
        }
        None => {
            warnings.push(Warning::new(
                WarningCategory::MissingSourceMeta,
                "Kvaesitso backup has no 'meta' member; format version unknown",
            ));
            None
        }
    };

    if scan.customizations.is_empty() {
        return Err(Error::NoCustomizations);
    }

    let pages = read_pages(
        &mut archive,
        &scan.customizations,
        "customizations",
        &mut read_total,
        limits,
    )?;

    let mut tags = SourceTags::from_pages(pages)?;
    tags.meta = meta;
    warnings.append(&mut tags.warnings);

    // favorites.NNNN: only `key` fields are used (component keys, package presence).
    let mut favorite_keys: BTreeMap<String, String> = BTreeMap::new();
    let mut unreadable_favorites = 0usize;
    for (_, value) in read_pages_lenient(&mut archive, &scan.favorites, &mut read_total, limits)? {
        let Some(records) = value.as_array() else {
            unreadable_favorites += 1;
            continue;
        };
        for rec in records {
            let Some(key) = rec.get("key").and_then(Value::as_str) else {
                continue;
            };
            if let Some(pkg) = key_package(key) {
                tags.present_packages.insert(pkg.to_string());
            }
            if let Some(AppKey::Main(pkg)) = parse_app_key(key) {
                favorite_keys
                    .entry(pkg.to_string())
                    .or_insert_with(|| key.to_string());
            }
        }
    }
    if unreadable_favorites > 0 {
        warnings.push(Warning::new(
            WarningCategory::UnexpectedMember,
            format!("{unreadable_favorites} favorites page(s) are not JSON arrays; ignored"),
        ));
    }
    for (pkg, key) in favorite_keys {
        tags.components.insert(pkg, key);
    }
    tags.warnings = warnings;
    Ok(tags)
}

pub(crate) fn read_pages<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    index: &BTreeMap<u64, usize>,
    prefix: &str,
    read_total: &mut u64,
    limits: &Limits,
) -> Result<Vec<(u64, Value)>, Error> {
    let mut pages = Vec::with_capacity(index.len());
    for (&page, &i) in index {
        let bytes = read_member(archive, i, limits.max_member_bytes, read_total, limits)?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|e| Error::json(format!("{prefix}.{page:04}"), &e))?;
        pages.push((page, value));
    }
    Ok(pages)
}

/// Like [`read_pages`] but invalid JSON becomes `Value::Null` (limits still enforced).
fn read_pages_lenient<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    index: &BTreeMap<u64, usize>,
    read_total: &mut u64,
    limits: &Limits,
) -> Result<Vec<(u64, Value)>, Error> {
    let mut pages = Vec::with_capacity(index.len());
    for (&page, &i) in index {
        let bytes = read_member(archive, i, limits.max_member_bytes, read_total, limits)?;
        pages.push((page, serde_json::from_slice(&bytes).unwrap_or(Value::Null)));
    }
    Ok(pages)
}

/// Package of any `app://pkg:…` key (any profile), for presence detection.
fn key_package(key: &str) -> Option<&str> {
    let rest = key.strip_prefix("app://")?;
    let pkg = rest.split(':').next()?;
    valid_package(pkg).then_some(pkg)
}

fn is_safe_member_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && !name.contains('\0')
        && !name.split('/').any(|c| c == "..")
        && !(name.len() >= 2 && name.as_bytes()[1] == b':')
}

/// `<prefix>` (e.g. `customizations.`) followed by ≥4 ASCII digits, nothing else.
fn parse_page_name(prefix: &str, name: &str) -> Result<Option<u64>, Error> {
    let Some(digits) = name.strip_prefix(prefix) else {
        return Ok(None);
    };
    if digits.len() < 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(None);
    }
    let significant = digits.trim_start_matches('0');
    if significant.len() > 18 {
        return Err(Error::Schema("customizations page number too large".into()));
    }
    Ok(Some(if significant.is_empty() {
        0
    } else {
        significant.parse().expect("validated digits")
    }))
}

fn read_member<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    index: usize,
    limit: u64,
    read_total: &mut u64,
    limits: &Limits,
) -> Result<Vec<u8>, Error> {
    let entry = archive
        .by_index(index)
        .map_err(|e| Error::Zip(e.to_string()))?;
    let name = entry.name().to_string();
    if entry.size() > limit {
        return Err(Error::ArchiveLimit(format!(
            "member '{name}' declares {} bytes (limit {limit})",
            entry.size()
        )));
    }
    // Never trust the declared size: cap the actual decompressed stream.
    let mut buf = Vec::with_capacity(entry.size().min(limit) as usize);
    entry
        .take(limit + 1)
        .read_to_end(&mut buf)
        .map_err(|e| Error::Zip(format!("failed to decompress member '{name}': {e}")))?;
    if buf.len() as u64 > limit {
        return Err(Error::ArchiveLimit(format!(
            "member '{name}' decompresses beyond {limit} bytes"
        )));
    }
    *read_total += buf.len() as u64;
    if *read_total > limits.max_total_bytes {
        return Err(Error::ArchiveLimit(format!(
            "decompressed data exceeds {} bytes",
            limits.max_total_bytes
        )));
    }
    Ok(buf)
}

fn parse_meta(bytes: &[u8], warnings: &mut Vec<Warning>) -> Result<SourceMeta, Error> {
    let v: Value = serde_json::from_slice(bytes).map_err(|e| Error::json("meta", &e))?;
    let obj = v
        .as_object()
        .ok_or_else(|| Error::Schema("'meta' is not a JSON object".into()))?;
    let get_str = |k: &str| -> Result<Option<String>, Error> {
        match obj.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(Error::Schema(format!("meta.{k} is not a string"))),
        }
    };
    let meta = SourceMeta {
        format: get_str("format")?,
        version_name: get_str("versionName")?,
        timestamp: obj.get("timestamp").and_then(Value::as_i64),
    };
    match meta.format.as_deref() {
        Some(VERIFIED_FORMAT) => {}
        Some(f) => {
            let major = f.split('.').next().and_then(|m| m.parse::<u32>().ok());
            if major != Some(1) {
                return Err(Error::UnsupportedFormat(format!(
                    "Kvaesitso backup format '{f}' (only 1.x is supported, verified {VERIFIED_FORMAT})"
                )));
            }
            warnings.push(Warning::new(
                WarningCategory::UntestedSourceFormat,
                format!("Kvaesitso backup format {f} differs from verified {VERIFIED_FORMAT}"),
            ));
        }
        None => warnings.push(Warning::new(
            WarningCategory::UntestedSourceFormat,
            "Kvaesitso meta has no 'format' field",
        )),
    }
    Ok(meta)
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AppKey<'a> {
    Main(&'a str),
    Profile(&'a str),
}

/// `app://<package>:<activity>` (main profile) or `app://<package>:<activity>:<serial>`
/// (other profile; see Kvaesitso `LauncherApp.key`).
pub(crate) fn parse_app_key(key: &str) -> Option<AppKey<'_>> {
    let rest = key.strip_prefix("app://")?;
    let mut parts = rest.split(':');
    let package = parts.next()?;
    let activity = parts.next()?;
    let serial = parts.next();
    if parts.next().is_some() || !valid_package(package) || !valid_activity(activity) {
        return None;
    }
    match serial {
        None => Some(AppKey::Main(package)),
        Some(s) if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit() || b == b'-') => {
            Some(AppKey::Profile(package))
        }
        Some(_) => None,
    }
}

/// Conservative Android application-id check: ≥2 dot-separated Java-identifier-like ASCII
/// labels. Packages starting with `shortcut_` are rejected because
/// `nickname_app_shortcut_` is Quick Search's app-shortcut key namespace.
pub(crate) fn valid_package(p: &str) -> bool {
    let labels: Vec<&str> = p.split('.').collect();
    labels.len() >= 2
        && !p.starts_with("shortcut_")
        && labels.iter().all(|l| {
            let mut chars = l.chars();
            matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
                && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
}

fn valid_activity(a: &str) -> bool {
    !a.is_empty() && !a.chars().any(|c| c.is_whitespace() || c.is_control())
}

impl SourceTags {
    /// Build directly from already-folded package tags (used by tests and library callers).
    pub fn from_packages(packages: Vec<PackageTags>) -> Self {
        let stats = SourceStats {
            unique_app_packages: packages.len(),
            accepted_app_tags: packages.iter().map(|p| p.aliases.len()).sum(),
            ..SourceStats::default()
        };
        SourceTags {
            packages,
            stats,
            ..SourceTags::default()
        }
    }

    /// Fold parsed `customizations.NNNN` pages (any order; sorted numerically here).
    pub fn from_pages(mut pages: Vec<(u64, Value)>) -> Result<Self, Error> {
        pages.sort_by_key(|(n, _)| *n);
        let mut stats = SourceStats::default();
        let mut warnings = Vec::new();
        let mut packages: Vec<PackageTags> = Vec::new();
        let mut by_package: HashMap<String, (usize, HashSet<String>)> = HashMap::new();
        let mut components: BTreeMap<String, String> = BTreeMap::new();
        let mut present_packages: BTreeSet<String> = BTreeSet::new();

        for (page, value) in &pages {
            let page = *page;
            let records = value.as_array().ok_or_else(|| {
                Error::Schema(format!("customizations.{page:04} is not a JSON array"))
            })?;
            for (i, rec) in records.iter().enumerate() {
                stats.customization_records += 1;
                let malformed = |stats: &mut SourceStats, warnings: &mut Vec<Warning>| {
                    stats.malformed_records_skipped += 1;
                    warnings.push(
                        Warning::new(
                            WarningCategory::MalformedRecord,
                            "customization record lacks string key/type/value; skipped",
                        )
                        .at(page, i),
                    );
                };
                let Some(obj) = rec.as_object() else {
                    malformed(&mut stats, &mut warnings);
                    continue;
                };
                if let Some(pkg) = obj.get("key").and_then(Value::as_str).and_then(key_package) {
                    present_packages.insert(pkg.to_string());
                }
                let Some(kind) = obj.get("type").and_then(Value::as_str) else {
                    malformed(&mut stats, &mut warnings);
                    continue;
                };
                if kind != "tag" {
                    stats.non_tag_records_ignored += 1;
                    continue;
                }
                let Some(key) = obj.get("key").and_then(Value::as_str) else {
                    malformed(&mut stats, &mut warnings);
                    continue;
                };
                if key.starts_with("shortcut://") {
                    stats.shortcut_tag_records_skipped += 1;
                    continue;
                }
                if !key.starts_with("app://") {
                    stats.other_tag_records_skipped += 1;
                    continue;
                }
                stats.app_tag_records += 1;
                let Some(raw) = obj.get("value").and_then(Value::as_str) else {
                    malformed(&mut stats, &mut warnings);
                    continue;
                };
                let package = match parse_app_key(key) {
                    None => {
                        stats.invalid_app_keys_skipped += 1;
                        warnings.push(
                            Warning::new(
                                WarningCategory::MalformedAppKey,
                                "malformed app key (expected app://<package>:<activity>); tag skipped",
                            )
                            .at(page, i),
                        );
                        continue;
                    }
                    Some(AppKey::Profile(p)) => {
                        stats.profile_app_tags_skipped += 1;
                        warnings.push(
                            Warning::new(
                                WarningCategory::ProfileAppTag,
                                "tag belongs to a work/clone profile app; v0.1 only writes base-profile nicknames, skipped",
                            )
                            .package(p)
                            .at(page, i),
                        );
                        continue;
                    }
                    Some(AppKey::Main(p)) => {
                        components
                            .entry(p.to_string())
                            .or_insert_with(|| key.to_string());
                        p
                    }
                };
                let tag = raw.trim();
                if tag.is_empty() {
                    stats.blank_tags_skipped += 1;
                    warnings.push(
                        Warning::new(WarningCategory::BlankTagValue, "blank tag value skipped")
                            .package(package)
                            .at(page, i),
                    );
                    continue;
                }
                if tag.contains(alias::SEPARATOR) {
                    stats.comma_tags_skipped += 1;
                    warnings.push(
                        Warning::new(
                            WarningCategory::CommaInTagValue,
                            "tag value contains a comma (Quick Search alias separator); skipped",
                        )
                        .package(package)
                        .at(page, i),
                    );
                    continue;
                }
                let entry = by_package.entry(package.to_string()).or_insert_with(|| {
                    packages.push(PackageTags {
                        package: package.to_string(),
                        aliases: Vec::new(),
                    });
                    (packages.len() - 1, HashSet::new())
                });
                if entry.1.insert(alias::match_key(tag)) {
                    packages[entry.0].aliases.push(tag.to_string());
                } else {
                    stats.duplicate_app_tag_records += 1;
                }
            }
        }

        stats.unique_app_packages = packages.len();
        stats.accepted_app_tags = packages.iter().map(|p| p.aliases.len()).sum();
        if packages.is_empty() {
            warnings.push(Warning::new(
                WarningCategory::NoAppTags,
                "source contains no usable app tags; output would equal the input",
            ));
        }
        Ok(SourceTags {
            meta: None,
            pages: pages.iter().map(|(n, _)| *n).collect(),
            packages,
            stats,
            warnings,
            components,
            present_packages,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_keys() {
        assert_eq!(
            parse_app_key("app://org.a.b:org.a.b.Main"),
            Some(AppKey::Main("org.a.b"))
        );
        assert_eq!(
            parse_app_key("app://org.a.b:org.a.b.Main:10"),
            Some(AppKey::Profile("org.a.b"))
        );
        for bad in [
            "app://",
            "app://org.a.b",
            "app://org.a.b:",
            "app://:X",
            "app://org..b:X",
            "app://1org.b:X",
            "app://org.a-b.c:X",
            "app://org.a.b:X:",
            "app://org.a.b:X:y",
            "app://org.a.b:X:1:2",
            "app://org.a.b:X Y",
            "app://shortcut_a.b:X",
            "shortcut://org.a.b:X",
        ] {
            assert_eq!(parse_app_key(bad), None, "{bad}");
        }
    }

    #[test]
    fn page_names() {
        assert_eq!(
            parse_page_name("customizations.", "customizations.0000").unwrap(),
            Some(0)
        );
        assert_eq!(
            parse_page_name("customizations.", "customizations.0012").unwrap(),
            Some(12)
        );
        assert_eq!(
            parse_page_name("customizations.", "customizations.123456").unwrap(),
            Some(123456)
        );
        for n in [
            "customizations.000",
            "customizations.0000.json",
            "customizations.00a0",
            "xcustomizations.0000",
            "dir/customizations.0000",
            "customizations",
        ] {
            assert_eq!(parse_page_name("customizations.", n).unwrap(), None, "{n}");
        }
        assert!(parse_page_name(
            "customizations.",
            &format!("customizations.{}", "9".repeat(19))
        )
        .is_err());
    }

    #[test]
    fn member_names() {
        for ok in ["meta", "customizations.0000", "a/b", "settings.json"] {
            assert!(is_safe_member_name(ok), "{ok}");
        }
        for bad in ["", "/abs", "../x", "a/../../x", "a\\b", "C:x", "nul\0"] {
            assert!(!is_safe_member_name(bad), "{bad:?}");
        }
    }
}
