import org.gradle.api.tasks.Exec

plugins {
  alias(libs.plugins.android.application)
  alias(libs.plugins.compose.compiler)
  alias(libs.plugins.kotlin.serialization)
}

// ---------------------------------------------------------------- Rust core via UniFFI
// The Rust workspace is two levels up. `cargo ndk` builds liblauncher_tags_ffi.so for every
// ABI into build/rustJniLibs; uniffi-bindgen generates Kotlin from the arm64 library.
val repoRoot = rootProject.projectDir.parentFile
val rustJniLibs = layout.buildDirectory.dir("rustJniLibs")
val uniffiKotlin = layout.buildDirectory.dir("generated/uniffi/kotlin")
// Prefer rustup's cargo (Homebrew's rustc has no Android targets).
val cargoPath = listOf(System.getProperty("user.home") + "/.cargo/bin", System.getenv("PATH") ?: "")
  .joinToString(File.pathSeparator)
val rustAbis = (findProperty("rustAbis") as String? ?: "arm64-v8a,armeabi-v7a,x86_64").split(",")

val cargoBuildAndroid by tasks.registering(Exec::class) {
  group = "rust"
  description = "Builds the Rust FFI library for Android ABIs"
  workingDir = repoRoot
  environment("PATH", cargoPath)
  val args = mutableListOf("cargo", "ndk", "-P", "26", "-o", rustJniLibs.get().asFile.path)
  rustAbis.forEach { args += listOf("-t", it) }
  args += listOf("build", "-p", "launcher-tags-ffi", "--release", "--locked")
  commandLine(args)
  inputs.dir(File(repoRoot, "crates"))
  inputs.file(File(repoRoot, "Cargo.lock"))
  outputs.dir(rustJniLibs)
}

val uniffiBindgen by tasks.registering(Exec::class) {
  group = "rust"
  description = "Generates Kotlin bindings for the Rust FFI library"
  dependsOn(cargoBuildAndroid)
  workingDir = repoRoot
  environment("PATH", cargoPath)
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
  defaultConfig {
    applicationId = "cz.jancejka.launchertagssync"
    minSdk = 26
    targetSdk = 37
    versionCode = 1
    versionName = "0.3.0"
    testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    ndk { abiFilters += rustAbis }
  }

  sourceSets {
    getByName("main") {
      jniLibs.directories.add(rustJniLibs.get().asFile.path)
      kotlin.directories.add(uniffiKotlin.get().asFile.path)
    }
  }

  buildTypes {
    release {
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
