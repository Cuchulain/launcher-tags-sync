# Release guidance

## Checklist

1. `CHANGELOG.md`: move entries from *Unreleased* to the new version with the date.
2. Bump `version` in the root `Cargo.toml` (`[workspace.package]`) and the CLI's
   `launcher-tags-core` dependency version.
3. Re-verify upstream formats if anything drifted (see `docs/SOURCES.md`, record the revisions).
4. Run locally on a clean tree:

   ```shell
   cargo fmt --all --check
   cargo clippy --workspace --all-targets --locked -- -D warnings
   cargo test --workspace --locked
   # optional, private, never in CI:
   cargo test -p launcher-tags-core --test private_integration -- --ignored
   ```

5. Commit `Cargo.lock`, let CI pass on Linux and macOS, then tag and push:

   ```shell
   git tag -a vX.Y.Z -m "vX.Y.Z" && git push origin vX.Y.Z
   ```

   `.github/workflows/release.yml` then checks that the tag equals the workspace version,
   tests and builds both targets with the pinned toolchain (1.88.0), packages
   `launcher-tags-sync-vX.Y.Z-<target>.tar.gz` + `.sha256`, smoke-tests the binary and creates
   the GitHub release with notes taken from this version's `CHANGELOG.md` section. Tags with a
   `-` (e.g. `v0.2.0-rc.1`) become pre-releases.

## Reproducible builds

The binaries are built with the committed `Cargo.lock` (`--locked`), `lto = true` and
`codegen-units = 1`. To make two builds of the same tag comparable:

```shell
export SOURCE_DATE_EPOCH="$(git log -1 --format=%ct)"
export RUSTFLAGS="--remap-path-prefix=$(pwd)=. --remap-path-prefix=$HOME/.cargo=~/.cargo"
rustup toolchain install 1.88.0   # pin the same toolchain for every build
cargo +1.88.0 build --release --locked
shasum -a 256 target/release/launcher-tags-sync
```

Build each target on its native platform:

| Target | Host | Artifact |
|---|---|---|
| `aarch64-apple-darwin` | macOS arm64 | `launcher-tags-sync-vX.Y.Z-aarch64-apple-darwin.tar.gz` |
| `x86_64-unknown-linux-gnu` | Linux x86_64 | `launcher-tags-sync-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` |

Publish SHA-256 checksums next to the archives. Unsigned macOS binaries downloaded from the
web need `xattr -d com.apple.quarantine launcher-tags-sync` (or build from source).

## Manual Android import check (before announcing a release)

1. Export a fresh Quick Search backup with **Settings** selected and keep it untouched.
2. Create a Kvaesitso backup.
3. `launcher-tags-sync sync … --dry-run`, review the planned changes, then run with
   `--output merged.quicksearch --report report.json`.
4. Import `merged.quicksearch` in Quick Search; search for 2–3 tags and confirm the apps appear;
   check that an existing nickname kept its original aliases first, and that other settings
   (theme, notes, pinned items) are unchanged.
5. If anything is wrong, re-import the untouched original export.
