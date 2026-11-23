import java.util.Properties
import org.gradle.api.tasks.Exec

plugins {
  alias(libs.plugins.android.application)
  alias(libs.plugins.compose.compiler)
  alias(libs.plugins.kotlin.serialization)
}

// ---------------------------------------------------------------- versions
// Single source of truth: the Rust workspace version. versionCode = MAJOR*10000 + MINOR*100 + PATCH.
val repoRoot: File = rootProject.projectDir.parentFile
val appVersion: String =
  Regex("""(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"""")
    .find(File(repoRoot, "Cargo.toml").readText())!!.groupValues[1]
val appVersionCode: Int = appVersion.split(".").map(String::toInt).let { (a, b, c) -> a * 10000 + b * 100 + c }

// Pinned for reproducible builds (F-Droid verifies our signed APK against its own build).
val rustToolchain = findProperty("rustToolchain") as String? ?: "1.100.0"
val pinnedNdk = "30.0.16248370"

// ---------------------------------------------------------------- signing
// Release signing comes from env (CI) or ../../local-private/signing.properties (git-ignored).
// Without it, release builds are unsigned (e.g. F-Droid's own build).
// (Not `.apply {}` / `java.util.*`: in a build script those resolve to Gradle's apply/java.)
fun loadProperties(f: File): Properties =
  Properties().also { p -> if (f.isFile) f.inputStream().use { p.load(it) } }

val signingProps = loadProperties(File(repoRoot, "local-private/signing.properties"))

fun signing(key: String, env: String): String? = System.getenv(env)?.takeIf { it.isNotEmpty() } ?: signingProps.getProperty(key)

val releaseStoreFile = signing("storeFile", "ANDROID_KEYSTORE_FILE")

// ---------------------------------------------------------------- Rust core via UniFFI
// `cargo ndk` builds liblauncher_tags_ffi.so for every ABI into build/rustJniLibs;
// uniffi-bindgen generates Kotlin from the first ABI's library.
val rustJniLibs = layout.buildDirectory.dir("rustJniLibs")
val uniffiKotlin = layout.buildDirectory.dir("generated/uniffi/kotlin")
// Prefer rustup's cargo (Homebrew's rustc has no Android targets and no +toolchain).
val cargoHome = System.getenv("CARGO_HOME") ?: (System.getProperty("user.home") + "/.cargo")
val cargoPath = listOf("$cargoHome/bin", System.getenv("PATH") ?: "").joinToString(File.pathSeparator)
val rustAbis = (findProperty("rustAbis") as String? ?: "arm64-v8a,armeabi-v7a,x86_64").split(",")

fun androidSdkDir(): File? {
  val local = loadProperties(rootProject.file("local.properties")).getProperty("sdk.dir")
  return (local ?: System.getenv("ANDROID_HOME") ?: System.getenv("ANDROID_SDK_ROOT"))?.let(::File)
}

val ndkHome: String? =
  System.getenv("ANDROID_NDK_HOME") ?: androidSdkDir()?.resolve("ndk/$pinnedNdk")?.takeIf { it.isDirectory }?.path

// Absolute paths must not end up in the binaries.
val rustFlags = listOf(
  "--remap-path-prefix=${repoRoot.path}=/src",
  "--remap-path-prefix=$cargoHome=/cargo",
).joinToString("\u001f")

val cargoBuildAndroid by tasks.registering(Exec::class) {
  group = "rust"
  description = "Builds the Rust FFI library for Android ABIs"
  workingDir = repoRoot
  environment("PATH", cargoPath)
  environment("CARGO_ENCODED_RUSTFLAGS", rustFlags)
  environment("SOURCE_DATE_EPOCH", "0")
  // Not `cargo +toolchain`: cargo-ndk re-invokes the real cargo binary, which ignores `+`.
  environment("RUSTUP_TOOLCHAIN", rustToolchain)
  ndkHome?.let { environment("ANDROID_NDK_HOME", it) }
  val args = mutableListOf("cargo", "ndk", "-P", "26", "-o", rustJniLibs.get().asFile.path)
  rustAbis.forEach { args += listOf("-t", it) }
  args += listOf("build", "-p", "launcher-tags-ffi", "--release", "--locked")
  commandLine(args)
  inputs.dir(File(repoRoot, "crates"))
  inputs.file(File(repoRoot, "Cargo.lock"))
  inputs.property("rustToolchain", rustToolchain)
  inputs.property("rustAbis", rustAbis)
  outputs.dir(rustJniLibs)
}

val uniffiBindgen by tasks.registering(Exec::class) {
  group = "rust"
  description = "Generates Kotlin bindings for the Rust FFI library"
  dependsOn(cargoBuildAndroid)
  workingDir = repoRoot
  environment("PATH", cargoPath)
  environment("RUSTUP_TOOLCHAIN", rustToolchain)
  val lib = rustJniLibs.get().file("${rustAbis.first()}/liblauncher_tags_ffi.so").asFile.path
  commandLine(
    "cargo", "run", "--locked", "-q", "-p", "launcher-tags-ffi", "--features", "bindgen",
    "--bin", "uniffi-bindgen", "--", "generate", "--library", lib, "--language", "kotlin",
    "--no-format", "--out-dir", uniffiKotlin.get().asFile.path,
  )
  inputs.dir(rustJniLibs)
  outputs.dir(uniffiKotlin)
}

tasks.named("preBuild") { dependsOn(uniffiBindgen) }

android {
  namespace = "cz.jancejka.launchertagssync"
  compileSdk = 37
  ndkVersion = pinnedNdk
  defaultConfig {
    applicationId = "cz.jancejka.launchertagssync"
    minSdk = 26
    targetSdk = 37
    versionCode = appVersionCode
    versionName = appVersion
    testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    ndk { abiFilters += rustAbis }
  }

  sourceSets {
    getByName("main") {
      jniLibs.directories.add(rustJniLibs.get().asFile.path)
      kotlin.directories.add(uniffiKotlin.get().asFile.path)
    }
  }

  signingConfigs {
    if (releaseStoreFile != null) {
      create("release") {
        storeFile = file(releaseStoreFile)
        storePassword = signing("storePassword", "ANDROID_KEYSTORE_PASSWORD")
        keyAlias = signing("keyAlias", "ANDROID_KEY_ALIAS")
        keyPassword = signing("keyPassword", "ANDROID_KEYSTORE_PASSWORD")
        enableV1Signing = false
        enableV2Signing = true
        enableV3Signing = true
      }
    }
  }

  // No Google dependency-metadata blob in the APK (F-Droid policy, reproducibility).
  dependenciesInfo {
    includeInApk = false
    includeInBundle = false
  }

  buildTypes {
    release {
      signingConfig = signingConfigs.findByName("release")
      isMinifyEnabled = false
      proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
    }
  }
  compileOptions {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
  }
  buildFeatures {
    compose = true
    aidl = false
    buildConfig = false
    shaders = false
  }

  packaging {
    resources {
      excludes += "/META-INF/{AL2.0,LGPL2.1}"
    }
  }
}

kotlin {
  jvmToolchain(17)
}

dependencies {
  val composeBom = platform(libs.androidx.compose.bom)
  implementation(composeBom)
  androidTestImplementation(composeBom)

  // Core Android dependencies
  implementation(libs.androidx.core.ktx)
  implementation(libs.androidx.lifecycle.runtime.ktx)
  implementation(libs.androidx.activity.compose)

  // Arch Components
  implementation(libs.androidx.lifecycle.runtime.compose)
  implementation(libs.androidx.lifecycle.viewmodel.compose)

  // Compose
  implementation(libs.androidx.compose.ui)
  implementation(libs.androidx.compose.ui.tooling.preview)
  implementation(libs.androidx.compose.material3)
  implementation(libs.androidx.compose.material.icons.core)
  // Tooling
  debugImplementation(libs.androidx.compose.ui.tooling)
  // Instrumented tests
  androidTestImplementation(libs.androidx.compose.ui.test.junit4)
  debugImplementation(libs.androidx.compose.ui.test.manifest)

  // Local tests: jUnit, coroutines, Android runner
  testImplementation(libs.junit)
  testImplementation(libs.kotlinx.coroutines.test)

  // Instrumented tests: jUnit rules and runners
  androidTestImplementation(libs.androidx.test.core)
  androidTestImplementation(libs.androidx.test.ext.junit)
  androidTestImplementation(libs.androidx.test.runner)
  androidTestImplementation(libs.androidx.test.espresso.core)

  // Rust core bindings (UniFFI uses JNA)
  implementation("net.java.dev.jna:jna:5.17.0@aar")
  implementation(libs.kotlinx.serialization.json)

  // Navigation
  implementation(libs.androidx.navigation3.ui)
  implementation(libs.androidx.navigation3.runtime)
  implementation(libs.androidx.lifecycle.viewmodel.navigation3)
}
