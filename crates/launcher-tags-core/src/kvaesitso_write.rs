//! Rewrite a Kvaesitso backup with edited app tags (v0.2).
//!
//! Everything except `customizations.NNNN` is copied raw (same bytes, same compression,
//! same order). Customization records are kept verbatim and in order except removed tag
//! records; new tag records are appended. Records are re-paginated 100 per page like
//! Kvaesitso's own backup, and the pages take the position of the first original page.

use std::collections::{BTreeMap, HashSet};
use std::io::{Read, Seek, Write};

use serde_json::{json, Value};
use zip::write::SimpleFileOptions;

use crate::alias;
use crate::error::Error;
use crate::kvaesitso::{open_archive, parse_app_key, read_pages, AppKey, Limits};

/// Records per `customizations.NNNN` page (Kvaesitso uses 100).
pub const PAGE_SIZE: usize = 100;

/// Tag edits for main-profile app keys.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagEdits {
    /// package → (component key `app://pkg:Class`, alias texts to add, in order)
    pub add: BTreeMap<String, (String, Vec<String>)>,
    /// package → alias matching keys ([`alias::match_key`]) whose tag records are removed
    /// from every main-profile `app://pkg:*` key.
    pub remove: BTreeMap<String, HashSet<String>>,
}

impl TagEdits {
    pub fn is_empty(&self) -> bool {
        self.add.values().all(|(_, a)| a.is_empty()) && self.remove.values().all(HashSet::is_empty)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RewriteStats {
    pub removed_records: usize,
    pub added_records: usize,
    pub pages: usize,
    pub copied_members: usize,
}

fn zip_err(e: impl std::fmt::Display) -> Error {
    Error::Zip(e.to_string())
}

/// Write the edited backup to `output`.
pub fn rewrite<R: Read + Seek, W: Write + Seek>(
    input: R,
    output: W,
    edits: &TagEdits,
    limits: &Limits,
) -> Result<RewriteStats, Error> {
    let (mut archive, scan) = open_archive(input, limits)?;
    if scan.stray_customizations > 0 {
        return Err(Error::Schema(
            "archive has members starting with 'customizations.' that are not customizations.NNNN; refusing to rewrite it".into(),
        ));
    }
    if scan.customizations.is_empty() {
        return Err(Error::NoCustomizations);
    }
    let mut read_total = 0;
    let pages = read_pages(
        &mut archive,
        &scan.customizations,
        "customizations",
        &mut read_total,
        limits,
    )?;

    let mut stats = RewriteStats::default();
    let mut records: Vec<Value> = Vec::new();
    for (page, value) in pages {
        let Value::Array(items) = value else {
            return Err(Error::Schema(format!(
                "customizations.{page:04} is not a JSON array"
            )));
        };
        for rec in items {
            if is_removed(&rec, edits) {
                stats.removed_records += 1;
            } else {
                records.push(rec);
            }
        }
    }
    for (key, aliases) in edits.add.values() {
        for a in aliases {
            records.push(json!({"key": key, "value": a, "type": "tag"}));
            stats.added_records += 1;
        }
    }

    let first = *scan.customizations.values().min().expect("non-empty");
    let skip: HashSet<usize> = scan.customizations.values().copied().collect();
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut w = zip::ZipWriter::new(output);
    for i in 0..archive.len() {
        if i == first {
            let chunks: Vec<&[Value]> = if records.is_empty() {
                vec![&[]]
            } else {
                records.chunks(PAGE_SIZE).collect()
            };
            for (n, chunk) in chunks.iter().enumerate() {
                w.start_file(format!("customizations.{n:04}"), opts)
                    .map_err(zip_err)?;
                let bytes =
                    serde_json::to_vec(chunk).map_err(|e| Error::json("customizations", &e))?;
                w.write_all(&bytes)?;
                stats.pages += 1;
            }
        } else if !skip.contains(&i) {
            let f = archive.by_index_raw(i).map_err(zip_err)?;
            w.raw_copy_file(f).map_err(zip_err)?;
            stats.copied_members += 1;
        }
    }
    w.finish().map_err(zip_err)?;
    Ok(stats)
}

fn is_removed(rec: &Value, edits: &TagEdits) -> bool {
    let Some(obj) = rec.as_object() else {
        return false;
    };
    if obj.get("type").and_then(Value::as_str) != Some("tag") {
        return false;
    }
    let (Some(key), Some(value)) = (
        obj.get("key").and_then(Value::as_str),
        obj.get("value").and_then(Value::as_str),
    ) else {
        return false;
    };
    let Some(AppKey::Main(pkg)) = parse_app_key(key) else {
        return false;
    };
    edits
        .remove
        .get(pkg)
        .is_some_and(|set| set.contains(&alias::match_key(value)))
}
