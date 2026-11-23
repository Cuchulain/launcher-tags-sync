# Fixture test contract and acceptance matrix

Everything under `fixtures/` is **synthetic** (fake package names, no personal data). Expected results are semantic JSON comparisons, not byte-identical pretty printing.

## Basic fixture

Inputs: `fixtures/basic/input.kvaesitso` + `fixtures/basic/input.quicksearch`.
Expected: `fixtures/basic/expected.quicksearch`, expected stats in `expected-report.json`.

- ZIP has `customizations.0000` **and** `.0001`. Both must be parsed.
- Reader is tagged from two activities, with a repeated tag in different lettercase.
- Existing Reader nickname must keep `e-book, čtení` and gain only `Knihy`.
- Existing Maps nickname must keep `navigace` and gain `mapy`.
- Music has two source tags and needs a **new base-profile** entry, while `nickname_app_org.example.music:10` is preserved unchanged.
- Silent app gets a new entry `zábava`.
- `nickname_app_org.example.unrelated`, `nickname_app_shortcut_example-shortcut`, `nickname_contact_17`, the theme, boolean flag, session state, notes, and all root fields are preserved.
- Skip both `shortcut://` tag records; ignore icon record.

Expected source 7 app-tag rows => 4 packages, 1 duplicate source-tag record, 2 new nickname keys, 2 extended nickname keys, 2 unchanged existing app nickname keys, 5 newly appended aliases, 2 shortcut tags skipped.

## Edge-case fixture

Inputs: `fixtures/edgecases/input.kvaesitso` + `fixtures/edgecases/input.quicksearch`.
Expected: `fixtures/edgecases/expected.quicksearch`, `expected-report.json`.

- Duplicate tag `Síť`/`síť` must not duplicate the existing lowercase alias.
- `Věci` must be trimmed from source and appended after existing nicknames.
- Tag containing a comma (`LAN, domácí`) must be skipped and warned by default.
- Malformed app keys (`app://`, missing colon) skipped with warnings.
- Ignore non-tag `icon` record and shortcut row.
- Preserve `misc_setting` (typed int) exactly.

## Invalid inputs

- `fixtures/invalid/invalid-quicksearch.quicksearch` — fail before output with parse error.
- `fixtures/invalid/missing-customizations.kvaesitso` — fail or explicit empty source warning; **must not silently produce misleading successful sync**.
- Input/output paths identical — fail without modifying input.
- Existing output target without `--force` — fail without modification.
- ZIP entry path traversal `../...` — never extract to filesystem; reject unsafe archive entries.
- ZIP decompression bomb limits — reject excessively large / numerous members.
- `formatVersion` other than 2 — reject with clear error; older/newer format opt-in only after tests.
- Wrong `preferences` value type — reject; do not reconstruct unknown schema.
- Invalid app nickname preference type — reject or flag collision, never overwrite it.
- UTF-8, emoji, diacritics, punctuation, Unicode normalization variants — no corruption.
- Alias with comma in source — skip with report (default).

## Invariants

- **Idempotence:** merge(Basic output, same source) == Basic output, no additional aliases.
- **Source stays unchanged:** hash source Kvaesitso and input Quick Search before/after tool run.
- **Unrelated Quick Search JSON stays unchanged** by structural comparison; only allowed changed paths start with `preferences.user_app_preferences.nickname_app_` and target only base-profile app keys.
- **Stability:** repeated runs on same inputs produce identical semantic results and stable report ordering.
- **Dry-run:** does not create output and does not modify any source.
- **No disclosure:** report does not include original notes, tokens, usage history or personal texts.

## Optional private integration (do not commit)

If the user **locally** supplies their real backups, compare v0.1 output to the already successfully imported one-off output. Expected **metrics**: 521 source app-tag rows, 325 source packages, 318 new keys, 3 extended keys, 4 unchanged existing, 8 shortcut tags skipped. Real import was confirmed successful by user; byte-identical output is unnecessary if aliases and all preserved settings match semantically. Keep private files in ignored `local-private/` and never upload to CI or public issue trackers.
