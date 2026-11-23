# Mapping, merge and conflict semantics

## v0.1 source selection

1. Read every ZIP member named exactly `customizations.` followed by 4 or more ASCII digits; parse each as JSON array.
2. Select objects with `type == "tag"` and a nonempty string `value` and `key` matching `app://<valid.package>:<nonempty.activity>`.
3. Extract Android package from after `app://` up to the **first colon**. Validate package conservatively: dot-separated Java/Android identifier-ish labels. Do not rely on the human-facing app label.
4. Aggregate all activities under the same package, preserving first occurrence order by sorted numeric page number and list index. De-duplicate tag values using Unicode normalization (NFC) plus Unicode-aware case-insensitive comparison; preserve the first seen spelling.
5. Skip `shortcut://...` tag rows (count separately), unknown key prefixes, malformed app keys, and non-tag records. Never convert a shortcut tag to a package nickname.
6. Trim leading/trailing whitespace. If a tag contains a comma, **skip with warning by default**: commas delimit aliases in Quick Search, so silent conversion changes its meaning. Whitespace-only tags skip with warning.

## v0.1 target merge

- Base nickname key `nickname_app_<package>`. Do not touch `nickname_app_<package>:<profile>` or `nickname_app_shortcut_...`.
- If existing key is a proper typed string, split existing comma-separated aliases, trim for matching but **do not rewrite existing string content** unless at least one genuinely new alias is appended. When appending, preserve the exact original string prefix as far as safely possible (if it has trailing whitespace, new resulting alias canonicalization is an implementation choice documented by tests).
- Match aliases case-insensitively after Unicode NFC and trim. Keep original alias spelling and order, including accents. Append new aliases with `, ` in deterministic source order. Example: `"e-book, čtení"` + `"Knihy", "čtení"` -> `"e-book, čtení, Knihy"`.
- If key absent, create `{"type":"string","value":"alias1, alias2"}`.
- If a collision targets a non-string preference value, **abort or skip+report** rather than overwrite; establish one clear policy and test it. Recommended: abort conversion.
- Never delete aliases in v0.1, even if tags disappeared from Kvaesitso.
- Maintain all target data outside changed nickname entries as parsed-JSON deep equality.
- Repeating the merge against its own output must create zero changes. Report should show zeros for new/extended keys on second run.

## Ambiguity warnings

- `app://` (empty package or activity), `app://org.example.reader` (no activity), or any other broken key is skipped and warned.
- Two source spellings that differ only by case are treated as duplicates, preserving the first spelling.
- Existing target work-profile nicknames stay unchanged; main-profile alias is a separate key.
- If a tag contains a comma, do not silently split; report a warning with **package key only or a redacted identifier**, not sensitive tag value.

## v0.2+ deletion/conflict semantics (not implemented in v0.1)

A true bidirectional sync must remember a previous agreed state for each app and alias (`sync-state.json` or comparable) to distinguish additions from removals. A two-way union alone is not synchronization: it resurrects deleted aliases forever. Keep provenance, detect concurrent edits, and prefer explicit conflict reports over silent deletion. A Quick Search nickname is not necessarily a Kvaesitso organizational category, so make reverse mapping opt-in and reviewable.
