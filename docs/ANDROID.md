# Android app

`android/` contains the Android app (`cz.jancejka.launchertagssync`, minSdk 26). The UI is
Kotlin + Jetpack Compose; all conversion logic is the Rust core, called through
`crates/launcher-tags-ffi` (UniFFI → Kotlin, JNA at runtime). No logic is duplicated in Kotlin.

## What it does

A five-step wizard: export the Kvaesitso backup → export the Quick Search backup →
review the changes → save the generated backup(s) via the system file picker → import guide
(with buttons that open both apps when installed). One-way (Kvaesitso → Quick Search) or two-way
mode (`docs/BISYNC.md`). Refusals such as stale exports, un-imported previous outputs or mass
deletions are explained and can be overridden explicitly.

Privacy: no network permission, no Android backup of app data. The two-way sync state
(`files/sync-state.json` plus `.prev`) and the run history (counts only) live in app-private
storage. The state is committed only after every needed backup has been saved.

## Build

Requirements: JDK 17+, Android SDK with platform `android-37.0` and NDK
(`30.0.16248370` tested), **rustup** with Android targets, and `cargo-ndk`:

```shell
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
cargo install cargo-ndk --locked
cd android
./gradlew assembleDebug testDebugUnitTest          # all ABIs
./gradlew assembleDebug -PrustAbis=arm64-v8a       # faster: one ABI
```

Gradle runs `cargo ndk` (task `cargoBuildAndroid`) into `app/build/rustJniLibs` and generates
the Kotlin bindings with `uniffi-bindgen` (task `uniffiBindgen`) into
`app/build/generated/uniffi`. It prefers `~/.cargo/bin` on `PATH`, because a Homebrew `rustc`
has no Android targets.

Install on a device or emulator: `adb install -r app/build/outputs/apk/debug/app-debug.apk`.

## Verified (2026-11-23, emulator API 37, arm64)

* both fixture backups read through Rust on the device;
* the two-way run on `fixtures/basic` produced backups identical (parsed JSON / ZIP members)
  to the CLI's `sync --direction both --init-state`;
* a second run on those outputs reported nothing to do; using the original export again
  was refused as stale;
* English and Czech UI.

## End-to-end test with the real apps (2026-11-24, emulator API 37)

Kvaesitso 1.41.0 (`de.mm20.launcher2.release`) and Quick Search 5.1 (`com.tk.quicksearch`) from
their GitHub releases, synthetic data only:

1. Kvaesitso: Calendar tagged `planovani`, `schuzky`; Quick Search: Calendar nickname `diar`.
2. Two-way run (first sync): plan = Quick Search +`planovani`,`schuzky`; Kvaesitso +`diar`.
3. Kvaesitso **restored** the rewritten backup (accepted as 1.41.0 backup); Calendar showed
   `#planovani #schuzky #diar` and was found by searching `diar`. All other backup members
   (settings, widgets, favorites, search actions, theme) were byte-identical.
4. Quick Search imported its backup; searching `schuzky` found Calendar; only
   `nickname_app_com.google.android.calendar` differed from the export.
5. Tag `schuzky` deleted in Kvaesitso, fresh exports, second run: plan = remove `schuzky` from
   Quick Search, nothing for Kvaesitso; after import the nickname was `diar, planovani`.

Found and fixed during the test: Quick Search exports through the share sheet, so the app now
accepts backups via **Share → Launcher Tags Sync** (`application/octet-stream`, zip, json) and
detects the type; the chosen direction is remembered.
