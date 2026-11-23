//! # launcher-tags-core
//!
//! Pure-Rust, I/O-light core for converting **Kvaesitso** app tags into **Quick Search**
//! app nicknames (one-way, v0.1).
//!
//! * [`kvaesitso`] reads a `.kvaesitso` ZIP backup safely (no extraction to disk, size and
//!   entry limits) and folds `tag` customizations per Android package.
//! * [`quicksearch`] parses and validates a `.quicksearch` JSON backup (`formatVersion` 2)
//!   while keeping every unrelated value as raw `serde_json::Value`.
//! * [`merge`] appends new aliases to `nickname_app_<package>` entries, keeping existing
//!   aliases first and verbatim.
//! * [`report`] builds a redacted, counts-only report.
//!
//! The semantics follow `docs/SEMANTICS.md` of this repository.

pub mod alias;
pub mod bisync;
mod error;
pub mod kvaesitso;
pub mod kvaesitso_write;
pub mod merge;
pub mod quicksearch;
pub mod report;
mod warning;

pub use error::Error;
pub use kvaesitso::{Limits, PackageTags, SourceTags};
pub use warning::{Warning, WarningCategory};

/// Human-readable description of the default conversion policy (also written to reports).
pub const POLICY: &str =
    "default conservative (skip comma-bearing tags with warning, trim whitespace)";
