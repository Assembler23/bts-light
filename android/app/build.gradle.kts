import groovy.json.JsonSlurper

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// Die Version kommt aus package.json im Repo-Root — bewusst keine vierte
// Versionsdatei neben Cargo.toml, tauri.conf.json und package.json.
val paketVersion: String = run {
    val json = JsonSlurper().parse(rootProject.file("../package.json")) as Map<*, *>
    json["version"] as String
}

// Reine Rechenregel, damit sie testbar bleibt (Version.kt spiegelt sie).
fun versionCodeAus(v: String): Int {
    val t = v.split(".").map { it.toInt() }
    return t[0] * 1_000_000 + t[1] * 10_000 + t[2]
}

// Signatur nur, wenn der Release-Workflow einen Keystore hinlegt; lokal
// und ohne Secret baut die CI die Debug-Variante.
val keystorePfad: String? = System.getenv("ANDROID_KEYSTORE_PATH")

android {
    namespace = "de.badhub.btslight.tablet"
    compileSdk = 34

    defaultConfig {
        applicationId = "de.badhub.btslight.tablet"
        minSdk = 28
        targetSdk = 34
        versionCode = versionCodeAus(paketVersion)
        versionName = paketVersion
    }

    signingConfigs {
        create("release") {
            if (keystorePfad != null) {
                storeFile = file(keystorePfad)
                storePassword = System.getenv("ANDROID_KEYSTORE_PASS")
                keyAlias = "bts-light-tablet"
                keyPassword = System.getenv("ANDROID_KEYSTORE_PASS")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = if (keystorePfad != null) signingConfigs.getByName("release") else null
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }

    buildFeatures { buildConfig = true } // BuildConfig.VERSION_NAME fürs Log

    sourceSets["main"].kotlin.srcDir("src/main/kotlin")
    sourceSets["test"].kotlin.srcDir("src/test/kotlin")
}

dependencies {
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.4")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.8.1")

    testImplementation("junit:junit:4.13.2")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.8.1")
}
