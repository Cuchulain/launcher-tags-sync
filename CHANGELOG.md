# Changelog

All notable changes to this project are documented here. Format:
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning: SemVer.

## [Unreleased]

### Added
- `sync --direction both` (v0.2): three-way bidirectional sync of Kvaesitso app tags and
  Quick Search app nicknames against a `--state` file, propagating additions and deletions;
  writes a rewritten Kvaesitso backup (`--kvaesitso-output`) with all other members copied
  raw. Safety nets: package-missing conflicts, mass-deletion guard, stale-export detection
  (`--allow-stale`), un-imported-output detection (`--pending`). See `docs/BISYNC.md`.
- `inspect`/reader: Kvaesitso `favorites.*` keys are used to resolve app components; warning
  for unexpected `customizations.*` members.
- Release workflow: tagged builds for macOS arm64 and Linux x86_64 with SHA-256 checksums.
- CI job verifying the MSRV (Rust 1.88).

## [0.1.0] - 2026-11-23

### Added
- `launcher-tags-core` library: safe in-memory reader for Kvaesitso ZIP backups (all
  `customizations.NNNN` pages, size/entry limits, path-traversal rejection), Quick Search
  `formatVersion` 2 parser/validator, nickname merge, redacted report.
- `launcher-tags-sync` CLI: `inspect --kvaesitso|--quicksearch [--json]`,
  `sync --kvaesitso --quicksearch (--output|--dry-run) [--report] [--force] [--show-aliases]`.
- One-way Kvaesitso → Quick Search conversion: app tags become aliases in
  `nickname_app_<package>`; existing aliases kept first and verbatim; Unicode NFC +
  case-insensitive de-duplication.
- Conservative skipping with warnings: shortcut tags, work-profile app tags, malformed app
  keys, blank tags, comma-containing tags.
- Atomic output writes (temp file + fsync + rename), input/output collision detection,
  `--force` required to replace files, post-write verification.
- Fixture, negative, property (proptest) and CLI end-to-end tests; optional private
  integration test; CI on Linux x86_64 and macOS arm64.
