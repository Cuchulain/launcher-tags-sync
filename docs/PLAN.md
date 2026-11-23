# Milestones / development plan

## M0 — Verify fixture contracts

- Inspect ZIP members and Quick Search JSON without writing output.
- Confirm source keys, selected export sections, typed preference values.
- Define deterministic JSON equality and alias comparison rules, as specified.
- Write tests consuming **both** `fixtures/basic` and `fixtures/edgecases`.

## M1 — Rust core v0.1

- Define typed internal structures with `serde_json::Value` for unknown/unrelated data.
- Safely parse ZIP to app-tag records, fold per Android package, filter shortcuts and unsupported entries.
- Parse QS formatVersion 2, merge app nickname preferences by package, preserve original alias order.
- Emit change report and warnings without private values.
- Test happy path, multi-page ZIP, accents, duplicate case, multiple activities, separate work profile.

## M2 — CLI v0.1 + QA

- Workspace with library + CLI, Clap-based commands, robust errors.
- `inspect`, `sync`, dry-run, output file, optional report file, explicit `--force`.
- Atomic or fail-safe output path handling and protections against input path collisions.
- Check deterministic output and zero-delta second run.
- CI: fmt, clippy, unit + integration + negative tests; Linux and macOS.
- Provide executable release build instructions and short manual Android import checklist.

## M3 — Optional v0.2: reverse transfer

- Only if the meaning of a Quick Search alias as Kvaesitso **tag/category** is explicitly accepted.
- Do not derive a Kvaesitso tag from every unrelated Quick Search nickname by default; use ownership/mapping rules.
- Maintain source ZIP entries and metadata, test restore compatibility (not only ZIP validity).
- Manual conflict resolution and log of changes.

## M4 — v0.3+: actual bidirectional synchronization

- Use three-way diff with prior snapshot (`sync-state.json`) and per-app/tag provenance.
- Model add/delete/rename separately; no "union-only forever" policy when deletions matter.
- Decide conflict policy on each field; never auto-delete conflicting user data.
- Mobile/Termux optional after CLI is stable; Android GUI (Kotlin + Rust FFI) much later.

## Anti-goals for v0.1

- No editing Android app databases directly, root/ADB hacks, cloud sync, background daemon or live API.
- No rewriting Kvaesitso backups; read them only.
- No automatic deletion of existing Quick Search nicknames.
- No importing contacts, notes, app shortcuts, bookmarks, custom icons, search history as new aliases.
- No Rust implementation included in this handoff; Claude is responsible.
