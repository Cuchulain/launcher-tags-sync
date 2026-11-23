# Implementation brief — Launcher Tags Sync (Rust)

You are implementing a new **open-source Rust project**. This folder is an implementation handoff, not an existing implementation. Read docs in order: `FORMATS.md`, `SEMANTICS.md`, `TESTS.md`, `PLAN.md`, `SECURITY.md`. Implement a small reusable core crate + a CLI crate. Preserve raw file data whenever possible. Begin by writing fixture-driven tests.

## Verified real-world result / baseline

A one-off backup conversion has already been imported successfully into Quick Search on Android. The private reference produced 521 app-tag rows, 325 unique packages, 318 new nickname keys, 3 expanded existing nickname keys, 4 existing keys unchanged, and 8 shortcut-tag rows skipped. These statistics are **not** synthetic fixture results and must not be hardcoded. The reference files are user-private and intentionally excluded.

## Deliverables for v0.1

- Cargo workspace: `crates/launcher-tags-core` (pure Rust conversion) and `crates/launcher-tags-cli`.
- Read Kvaesitso ZIP backup (*.kvaesitso) including all paginated `customizations.NNNN` entries.
- Read Quick Search JSON backup (*.quicksearch), formatVersion 2 (reject unsupported versions).
- Merge app tag values into Quick Search `preferences.user_app_preferences["nickname_app_<package>"]` as `{"type":"string","value":"a, b"}`.
- Preserve existing nickname aliases **first and verbatim**; append only new unique aliases in stable deterministic order. Preserve all unrelated Quick Search sections and values.
- Ignore app shortcut tags (`shortcut://...`) and non-`tag` customizations; issue counts and warnings, never misattribute shortcuts to apps.
- Default conservative handling: malformed keys and comma-containing tag values are **skipped with a warning** rather than silently mangled. Whitespace-only values skipped. See SEMANTICS for the precise policy.
- `--dry-run` does not create or modify output; prints planned changes. `--report <path>` emits structured JSON counts and warnings. Non-zero exit for bad input.
- Write output via temporary file + sync/flush + atomic rename where supported; do not overwrite input paths; output overwrite only with explicit `--force`.
- Validate generated JSON against Quick Search import schema and reparse before success.
- Unit + fixture + idempotency + property tests including Unicode; must pass on macOS arm64 and Linux x86_64.
- Help, informative failures, changelog, reproducible release guidance, `cargo fmt`, `clippy`, `test`.

## Proposed CLI

```text
launcher-tags-sync inspect --kvaesitso FILE
launcher-tags-sync inspect --quicksearch FILE
launcher-tags-sync sync --kvaesitso FILE --quicksearch FILE --output FILE [--report FILE] [--dry-run] [--force]
```

`sync` v0.1 is **one-way Kvaesitso -> Quick Search** despite the command name. Expose that clearly in help. Do not promise reverse transfer until it is implemented safely.

## Technical details and traps

1. An app source key is `app://<android.package.name>:<activity>`; collapse multiple activities to the same base package; do not use the app label to match identities. The colon is a separator after the package. An app shortcut source key is different and must not be included in v0.1.
2. Quick Search distinguishes default app key `<package>` and work/clone profile key `<package>:<userId>`. Add/modify only the base key in v0.1, preserving profile-specific entries.
3. Quick Search nicknames are **one comma-delimited string**; it normalizes each alias by trimming and dropping blanks/duplicates. The local converter should use Unicode-aware matching for de-duplication without discarding original text or case; preserve existing aliases' text and order.
4. `preferences.user_app_preferences` is a type-tagged map, not a plain string map: store `{"type":"string","value":"..."}`.
5. `selectedExportItems` in real backups can include `SETTINGS`. Preserve it, as well as notes, API data, history, custom preferences and unknown JSON fields. Never log or include those values in reports.
6. ZIP safety: check member paths and uncompressed limits, reject zip bombs and malformed JSON; parse by exact member pattern (avoid arbitrary entry traversal).
7. Some source strings are accents/Unicode. Avoid lossy encoding. Encode output UTF-8.
8. Use `serde`/`serde_json`, `zip`, `clap`, `thiserror` (or `anyhow` at CLI boundary), `tempfile`; optional `unicode-normalization`. Keep dependencies lean.
9. Preserve *semantic JSON data*, not necessarily byte-for-byte whitespace/key ordering. Do not mutate non-nickname values. The fixture expected outputs may be compared by parsed JSON equality.
10. A source tag with commas is ambiguous as a Quick Search alias; **warn and skip by default**. Provide explicit opt-in splitting only in a later release if needed.

## Definition of done

- All synthetic fixtures pass; see `docs/TESTS.md`.
- Running conversion twice against its first output adds zero aliases; output is semantically identical.
- Original Quick Search settings remain intact (JSON deep comparison after excluding only intended nickname key modifications).
- End-to-end basic fixture matches `fixtures/basic/expected.quicksearch` by parsed JSON equality.
- Edge fixture matches `fixtures/edgecases/expected.quicksearch`; emits required warning categories.
- No private fixture data committed, logs redacted; CI tests run without Android device or network.
- Document exact source code revision of both third-party projects that semantics were verified against. Recheck upstream if any structural assumptions drift.

**No code has yet been written in this handoff. You own the implementation, tests, and validation.**
