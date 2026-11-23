# Launcher Tags Sync

Turn your **[Kvaesitso](https://github.com/MM2-0/Kvaesitso)** app tags into
**[Quick Search](https://github.com/teja2495/quick-search)** app nicknames, so an app tagged
`music` in Kvaesitso can be found by typing `music` in Quick Search.

> **By default `sync` is one-way: Kvaesitso → Quick Search** (v0.1): nothing is written back to
> Kvaesitso and no existing nickname is ever removed. Two-way sync with deletions is opt-in
> via `--direction both` (v0.2, see below).

The tool works offline on exported backup files. It never touches the phone, app databases
or the network.

## Build

Requires Rust ≥ 1.88 (`rustup` or Homebrew `rust`).

```shell
# macOS arm64 (Apple Silicon) and Linux x86_64: same commands
cargo build --release --locked
./target/release/launcher-tags-sync --help

# optional: install into ~/.cargo/bin
cargo install --path crates/launcher-tags-cli --locked
```

## Use

1. In Kvaesitso, create a backup (Settings, Backup) → `kvaesitso.kvaesitso`.
2. In Quick Search, export a settings backup with **Settings** selected →
   `quick-search.quicksearch`. Keep this original export as your rollback.
3. Inspect and preview:

   ```shell
   launcher-tags-sync inspect --kvaesitso kvaesitso.kvaesitso
   launcher-tags-sync inspect --quicksearch quick-search.quicksearch
   launcher-tags-sync sync --kvaesitso kvaesitso.kvaesitso \
     --quicksearch quick-search.quicksearch --dry-run
   ```

4. Convert:

   ```shell
   launcher-tags-sync sync \
     --kvaesitso kvaesitso.kvaesitso \
     --quicksearch quick-search.quicksearch \
     --output merged.quicksearch \
     --report merged-report.json
   ```

5. Copy `merged.quicksearch` to the phone and import it in Quick Search's backup settings. Check a few apps' nicknames.

### Two-way sync (v0.2)

`--direction both` keeps both apps in sync, including deletions, using a private state file
(see [`docs/BISYNC.md`](docs/BISYNC.md)). **Every** base-profile Quick Search app alias
becomes a Kvaesitso tag of that app and vice versa.

```shell
# first time: union of both sides, nothing deleted
launcher-tags-sync sync --direction both --init-state --state local-private/sync-state.json \
  --kvaesitso kvaesitso.kvaesitso --quicksearch quick-search.quicksearch \
  --output merged.quicksearch --kvaesitso-output merged.kvaesitso --dry-run
# then without --dry-run; import BOTH outputs on the phone

# every later sync: fresh exports, same state, no --init-state
launcher-tags-sync sync --direction both --state local-private/sync-state.json \
  --kvaesitso kvaesitso.kvaesitso --quicksearch quick-search.quicksearch \
  --output merged.quicksearch --kvaesitso-output merged.kvaesitso
```

Safety nets: a deletion is only propagated when the alias was in the agreed state; an app
missing from one backup is a reported conflict, not a deletion; more than
`max(10, 25 %)` deletions need `--allow-mass-delete`; stale exports (older than those of the
previous run) are refused unless `--allow-stale`; outputs that were apparently never imported
stop the run unless `--pending discard|accept`. Kvaesitso's restore replaces all of its data
with the backup, so import `merged.kvaesitso` right after exporting.

### Options

| Option | Meaning |
|---|---|
| `--output FILE` | merged Quick Search backup (required unless `--dry-run`) |
| `--report FILE` | JSON report: counts + warnings, no backup values |
| `--dry-run` | print planned changes; the output backup is not written (a `--report` file still is, if requested) |
| `--force` | allow replacing an existing `--output`/`--report` file; inputs can never be overwritten |
| `--show-aliases` | also print the alias texts being added/removed (terminal only) |
| `--direction both` | v0.2 two-way sync; needs `--state`, `--kvaesitso-output` |
| `--state FILE` / `--init-state` | agreed state (private); create it on the first run |
| `--pending error\|discard\|accept` | what to do when the previous outputs look un-imported |
| `--allow-mass-delete`, `--allow-stale` | override the safety nets above |

Exit codes: `0` success, `1` invalid input / refused / I/O error (no output written),
`2` invalid command line.

## What exactly happens

* All `customizations.NNNN` pages of the Kvaesitso ZIP are read in numeric order.
* Records with `"type":"tag"` and key `app://<package>:<activity>` are grouped per Android
  package (several activities collapse into one package). Matching never uses app labels.
* Each tag is trimmed. Tags are de-duplicated per package with Unicode NFC + case-insensitive
  matching; the first spelling wins.
* For every package the Quick Search preference
  `preferences.user_app_preferences["nickname_app_<package>"]` gets the tags as aliases,
  stored as `{"type":"string","value":"a, b"}`:
  * existing aliases stay **first and verbatim**; only genuinely new aliases are appended with
    `", "` (e.g. `"e-book, čtení"` + `Knihy`, `čtení` → `"e-book, čtení, Knihy"`);
  * trailing whitespace/commas of an existing value are dropped before appending; a value
    with nothing new is left byte-identical;
  * a missing key is created.
* Skipped, with counts and warnings: app **shortcut** tags (`shortcut://…`), work/clone-profile
  app tags (`app://pkg:Activity:<serial>`), malformed app keys, blank tags, and tags containing
  a comma (Quick Search would split them into separate aliases). Tags on contacts, files, etc.
  and non-tag customizations (icons, labels) are ignored.
* Untouched: profile nickname keys (`nickname_app_<pkg>:<userId>`), app-shortcut nicknames,
  every other preference, notes, history, API data and unknown fields. This is verified by a
  deep JSON comparison on every run.
* The run is idempotent: converting again against its own output changes nothing.

See [`docs/SEMANTICS.md`](docs/SEMANTICS.md) and [`docs/FORMATS.md`](docs/FORMATS.md).

## Safety

* Inputs are read only. The output must not be (or link to) an input, even with `--force`;
  symlinks, `..`, hard links and case aliases are detected.
* Output and report are written to a temporary file in the target directory, fsynced and then
  renamed into place (no partial files; existing files are only replaced with `--force`).
  New files get owner-only permissions (0600) because backups are private.
* The written output is re-read, compared and re-validated before success is reported.
* The ZIP is parsed in memory: no extraction, member names checked for traversal, limits on
  member count (10 000), per-member size (64 MiB), total size (512 MiB), sizes verified while
  decompressing.
* Fails closed on: unsupported Quick Search `formatVersion` (≠ 2), Kvaesitso format major ≠ 1,
  missing `customizations.*`, malformed JSON, missing `preferences.user_app_preferences`,
  `selectedExportItems` without `SETTINGS` (Quick Search would ignore the nicknames), and an
  existing nickname entry that is not a typed string (never overwritten).
* Reports and warnings contain counts, categories, package names and page/record indexes, never
  tag values, nicknames or other settings.

See [`docs/SECURITY.md`](docs/SECURITY.md).

## Report

`--report` writes e.g.:

```json
{
  "tool": "launcher-tags-sync",
  "direction": "kvaesitso_to_quicksearch",
  "source_app_tag_records": 7,
  "source_unique_app_packages": 4,
  "source_shortcut_tag_records_skipped": 2,
  "source_duplicate_app_tag_records": 1,
  "new_app_nickname_entries": 2,
  "extended_app_nickname_entries": 2,
  "unchanged_existing_app_nickname_entries": 2,
  "added_unique_aliases": 5,
  "preserved_non_app_settings": true,
  "warnings": []
}
```

(abridged; see `crates/launcher-tags-core/src/report.rs` for all fields).
`unchanged_existing_app_nickname_entries` counts existing app nickname keys, including
profile keys, that were not modified.

## Android app

`android/` holds an Android app with the same engine (Rust core via UniFFI) and a guided
export → review → save → import flow, in English and Czech. Debug APKs are built by CI
(`.github/workflows/android.yml`, artifact `launcher-tags-sync-debug-apk`). Build and design
notes: [`docs/ANDROID.md`](docs/ANDROID.md).

## Development

```shell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

* `crates/launcher-tags-core` – pure library (no CLI, no file writes).
* `crates/launcher-tags-cli` – the `launcher-tags-sync` binary (path checks, atomic writes).
* `crates/launcher-tags-ffi` – UniFFI bindings for the Android app.
* `android/` – the Android app (Kotlin, Compose).
* `fixtures/` – synthetic backups and expected outputs; the tests are driven by them.

Optional private check against your own backups (never committed; `local-private/` is
git-ignored):

```shell
mkdir -p local-private   # put kvaesitso.kvaesitso, quicksearch.quicksearch,
                         # optionally expected.quicksearch there
cargo test -p launcher-tags-core --test private_integration -- --ignored --nocapture
```

Release process: [`docs/RELEASE.md`](docs/RELEASE.md). Changes: [`CHANGELOG.md`](CHANGELOG.md).
Upstream revisions the formats were verified against: [`docs/SOURCES.md`](docs/SOURCES.md).

## Known limitations

* One-way mode never removes aliases; use `--direction both` for deletions.
* Two-way mode: a Quick Search alias can only become a Kvaesitso tag for apps the Kvaesitso
  backup already knows (in favorites or with a tag); others are reported and retried.
  Restoring the generated Kvaesitso backup on a device is the user's final check.
* Work/clone-profile app tags are skipped (only base-profile nicknames are written).
* Tags containing a comma are skipped (no opt-in splitting yet).
* Duplicate keys inside a JSON object are not detected (last one wins, like most parsers).
* Number spelling may be canonicalized (`1e5` → `1e+5`); values are unchanged. Whitespace and
  indentation of the Quick Search file are not preserved (it is pretty-printed).

## License

[MIT](LICENSE). Not affiliated with Kvaesitso or Quick Search.
