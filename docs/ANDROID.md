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

Not yet done: release signing (needs a keystore decision) and a test with the real Kvaesitso
and Quick Search apps installed.
