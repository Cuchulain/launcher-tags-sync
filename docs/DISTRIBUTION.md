# Distribution: GitHub Releases, F-Droid, Google Play

One signing key is used everywhere, so users can move between the three channels and
updates install over each other.

| | |
|---|---|
| Key | RSA 4096, PKCS#12, alias `launchertagssync`, valid until 2126 |
| Certificate | `CN=Jan Čejka, O=launcher-tags-sync` |
| SHA-256 | `7c19839d1a335f0f5534781eeb322f5f9e91e6ea8384a333b27bc136fc366fdc` |
| Local copy | `local-private/release.jks` + `local-private/signing.properties` (git-ignored) |
| CI copy | repository secrets `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` |

> **Back up the keystore and its password now** (password manager + an offline copy).
> If it is lost, no channel can ship updates to existing installations ever again; if it
> leaks, others can publish updates in your name. Never commit it.

## Versioning

The Rust workspace version in `Cargo.toml` is the only version. Gradle derives
`versionName` from it and `versionCode = MAJOR*10000 + MINOR*100 + PATCH`
(0.2.0 → 200). Store texts live in `fastlane/metadata/android/<locale>/`, release notes in
`changelogs/<versionCode>.txt`.

## 1. GitHub Releases

1. Update `CHANGELOG.md` and `fastlane/.../changelogs/<versionCode>.txt`, bump the version in
   `Cargo.toml` (and `Cargo.lock` via `cargo build`).
2. `git tag -a vX.Y.Z -m vX.Y.Z && git push origin vX.Y.Z`.
3. `.github/workflows/release.yml` builds the CLI for macOS arm64 and Linux x86_64 and the
   signed Android APK + AAB (Rust 1.100.0, NDK 30.0.16248370), verifies the signature and
   creates the release with the CLI archives, the APK and SHA-256 files. The AAB is a
   workflow artifact (`android-release`) for Google Play.

## 2. F-Droid (reproducible build, our signature)

F-Droid builds every app from source. With `AllowedAPKSigningKeys` + `Binaries` it compares
its own unsigned build with our GitHub release APK and, if they are identical, publishes
**our** signed APK. Reproducibility is designed in: pinned Rust toolchain and NDK, path
remapping, no dependency-info blob, versions from `Cargo.toml`. Verified locally: two
checkouts in different directories produce byte-identical unsigned APKs; CI prints the
SHA-256 of the Linux build for comparison.

What is and is not reproducible (verified 2026-11-23):

* Same host OS, different checkout paths, users and cargo homes: **identical** unsigned APKs
  (two macOS checkouts; two independent GitHub Linux runs).
* macOS vs Linux: the native library differs, because host-compiled proc-macros feed into
  symbol hashes and the NDK's host-specific LLD records its build path. Therefore **release
  APKs are only built on Linux x86_64** (the release workflow), which is what F-Droid's build
  server uses too.
* Absolute paths (checkout, cargo home, toolchain `rust-src`) are remapped and do not appear
  in the binaries.

Before the first F-Droid submission, `release.yml` can be run manually
(*Actions → Release → Run workflow*): it builds and signs everything without publishing.

Steps (needs a GitLab account):

1. Release on GitHub first (step 1).
2. Fork https://gitlab.com/fdroid/fdroiddata, add
   `metadata/cz.jancejka.launchertagssync.yml` from [`fdroid/`](fdroid/) (update version,
   commit and the `rustup` srclib version to the one fdroiddata currently provides).
3. Optionally test with `fdroid build -v -l cz.jancejka.launchertagssync` in the fdroidserver
   container.
4. Open a merge request; answer the reviewers. Inclusion usually takes days to weeks.

If F-Droid's build should ever differ, the reviewers will report it; until it is fixed
F-Droid would not publish that version (it never silently re-signs when
`AllowedAPKSigningKeys` is set).

## 3. Google Play (same key as app signing key)

Needs a Google Play developer account (one-time fee). Since 2023 new **personal** accounts must
run a closed test with at least 12 testers for 14 days before production access.

1. Play Console → *Create app* (`Launcher Tags Sync`, free, app).
2. *Release → Setup → App signing*: choose to **use your own key** ("Export and upload a key
   from Java keystore") and follow the PEPK instructions shown there, e.g.
   `java -jar pepk.jar --keystore=local-private/release.jks --alias=launchertagssync --output=upload.zip --include-cert --rsa-aes-encryption --encryption-key-path=<key from Play>`.
   Then the Play-distributed APKs carry the same signature as GitHub and F-Droid.
   (Optionally register a separate upload key later; the app signing key stays the same.)
3. Upload `launcher-tags-sync-vX.Y.Z.aab` from the release workflow artifact
   (`android-release`) to an internal or closed testing track.
4. Store listing: title/short/full description, icon and phone screenshots from
   `fastlane/metadata/android/en-US` (and `cs-CZ` texts).
5. *App content*: privacy policy URL
   `https://github.com/Cuchulain/launcher-tags-sync/blob/main/docs/PRIVACY.md`;
   Data safety: no data collected, no data shared; no ads; content rating questionnaire
   (utility, no user content); target audience 18+ or general.
6. After the closed-test period: promote to production.

Automating Play uploads (service account + `fastlane supply` or the Gradle Play Publisher)
can be added later; it needs a Google Cloud service-account key as another secret.
