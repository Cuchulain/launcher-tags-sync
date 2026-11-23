# v0.2 bidirectional sync design

Status: implemented in v0.2 behind `sync --direction both`. The v0.1 one-way mode stays the
default.

## Decisions (accepted by the project owner, 2026-11-23)

1. **Mapping:** every base-profile Quick Search app alias corresponds to a Kvaesitso app tag
   of the same package, in both directions (fully symmetric).
2. **Deletions propagate** using a three-way comparison against a local sync state file.
   Situations where a deletion could be data loss are reported as conflicts and nothing is
   deleted for them.

## Upstream behaviour that shapes the design

(Kvaesitso `cb9ac2a`, Quick Search `d89b40a`, see `SOURCES.md`.)

* Kvaesitso restore **wipes all customizations and all saved searchables** and re-imports the
  backup. A rewritten backup must therefore carry every member and every record unchanged,
  except the tag records we edit.
* A Kvaesitso tag is only displayed for an item that exists in the saved-searchable table
  (`favorites.*`), because `getItemsForTag` resolves keys there. New tags are therefore only
  added for packages with a known main-profile component key, preferring a key present in
  `favorites.*`, then an existing tag key. Packages without one are reported as
  `unresolvable_app` and retried on the next run.
* Quick Search import (with `selectedExportItems`) does **not clear** preferences missing from
  the backup. Removing every alias of an app therefore writes `{"type":"string","value":""}`
  instead of deleting the key (Quick Search treats a blank nickname as none).

## Model

For every Android package `p` (base profile only):

* `K(p)` – Kvaesitso tags on main-profile `app://p:*` keys (trimmed, without blanks,
  comma-containing tags and profile tags, which stay untouched in the backup).
* `Q(p)` – aliases of `nickname_app_p`.
* `S(p)` – agreed aliases after the previous successful run (state file).

Aliases are compared by the v0.1 matching key (trim, NFC, Unicode lowercase). The text that is
written is always the spelling from the side the alias comes from.

### Decision table (per alias key)

| in K | in Q | in S | meaning | action |
|:-:|:-:|:-:|---|---|
| ✓ | ✓ | any | in sync | none |
| ✓ | – | – | added in Kvaesitso | add to Quick Search |
| – | ✓ | – | added in Quick Search | add to Kvaesitso |
| ✓ | – | ✓ | deleted in Quick Search | delete from Kvaesitso |
| – | ✓ | ✓ | deleted in Kvaesitso | delete from Quick Search |
| – | – | ✓ | deleted on both | forget |

First run (`--init-state`, no state yet): union in both directions, never delete.

### Conflicts (nothing deleted, reported)

* **Package missing on one side**: `p ∈ S`, but the side has no trace of `p` at all
  (Kvaesitso: no customization and no favorites record for the package; Quick Search: no
  `nickname_app_p` key). Typical causes are an uninstalled app or a partial backup. Deletions
  for `p` are skipped and `S(p)` is kept.
* **Mass deletion guard**: if the run would delete more than `max(10, 25 % of all aliases in
  S)` aliases, it aborts unless `--allow-mass-delete` is given.

### Stale exports

Three-way sync trusts that every input is a *current* export. Feeding an export from before
the previous run would look like deletions on that side. The state therefore records, per
side, a fingerprint of the input and of the output of the previous run and the export
timestamp (Kvaesitso `meta.timestamp`, Quick Search `exportedAtEpochMs`). An input is
refused as stale if it is the previous run's input, or if it differs from the previous
output while its timestamp is the same or older (generated outputs keep their source's
timestamp, a new phone export is always later). `--allow-stale` overrides this and treats the
input as authoritative.

### Backups that were never imported

The state also stores the changes the last run wrote to each side (`pending`). If a side's new
export shows **none** of them (every pending addition missing, every pending removal still
present), the generated backup was most likely never imported. Without this check the next
run would read those as user deletions. The run then stops unless `--pending` says what to do:

* `--pending discard`: the previous output was not imported; pretend that side's pending
  changes never happened (they are planned again).
* `--pending accept`: the export is authoritative (the user really undid those changes).

## Files

* `--state FILE` – JSON (`format`, `version`, agreed `packages`, `pending`, `last_run`
  fingerprints), owner-only permissions, written last and atomically; the previous
  version is kept as `FILE.prev`. It contains alias texts, so keep it private (e.g. in
  `local-private/`).
* `--output FILE` – merged Quick Search backup (as in v0.1).
* `--kvaesitso-output FILE` – rewritten Kvaesitso backup: all members copied raw and in the
  original order; `customizations.NNNN` re-paginated (100 records per page as Kvaesitso
  does), untouched records verbatim and in order, new tag records appended.

## Workflow

1. Export both apps' backups.
2. `sync --direction both … --dry-run`, review.
3. Run for real, import **both** generated backups on the phone (Kvaesitso restore replaces
   all of its data with the backup, so do it right after exporting).
4. Next time start again at 1 with fresh exports and the same `--state`.

## Not covered

* Work/clone profile apps; app-shortcut tags; tags with commas (left untouched).
* Renames are a delete + add.
* No ownership/provenance per alias beyond the agreed state.
