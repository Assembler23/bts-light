# Tablet-Kiosk-App für Android / Fire-Tablets — Umsetzungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eine dünne Android-App (`android/`), die beim Einschalten den Turnier-PC im WLAN nach dem Pi-Muster findet, die Felder-Lobby im Vollbild lädt, das Tablet als Gerätebesitzer hart sperrt, den Akku wie Fully Kiosk meldet und ihr Log an `/pi-log` schickt.

**Architecture:** Die gesamte Logik (Subnetz, Scanner, Suchreihenfolge, Zustandsmaschine der Hülle, Adressfilter, Log-Ringpuffer, PIN-Regel, Geräte-ID) liegt in **reinen Kotlin-Klassen ohne Android-Import** unter `suche/` und `kern/` und wird mit JUnit auf der JVM getestet. Die Android-Schicht (`KioskActivity`, Sonde, mDNS, Netz-Beobachter, Kiosk-Helfer, JS-Brücke, Receiver) sind dünne Adapter, die Ereignisse in die Zustandsmaschine geben und deren Wirkungen ausführen. Der Rust-Kern und `tablet.html` bleiben unverändert.

**Tech Stack:** Kotlin 2.0, Android Gradle Plugin 8.5, Gradle 8.7, minSdk 28 / compileSdk 34, kotlinx-coroutines, `HttpURLConnection` (keine HTTP-Bibliothek), JUnit 4 + `kotlinx-coroutines-test`. CI auf Ubuntu mit JDK 17.

**Spec:** `docs/features/tablet-android-kiosk-app.md`, ADR `docs/adr/0058-eigene-kiosk-app-statt-fully-kiosk.md`.

## Global Constraints

- **Paket** `de.badhub.btslight.tablet`, Anzeigename „bts-light Tablet", **minSdk 28**, **compileSdk/targetSdk 34**.
- **Version** kommt beim Bauen aus `../package.json` (`versionName` = Wert, `versionCode` = `major*1_000_000 + minor*10_000 + patch`). **Keine vierte Versionsdatei.** Versionsbump der drei bekannten Dateien erst beim Merge (Memory „Versionsnummer erst beim Merge"), im PR-Titel keine Version.
- **Port 8088, plain HTTP.** Sonde: `GET http://<ip>:8088/health`, Treffer nur bei Status 200 **und** JSON-Objekt. Lobby: `http://<ip>:8088/felder`. Log: `POST http://<ip>:8088/pi-log?device=fire-<ANDROID_ID>`, `Content-Type: text/plain`, Timeout 8 s, Body ≤ 2 MB.
- **Suche nur bei Bedarf**: Start, WLAN-Ereignis, Ladefehler, Handgriff. Nach geladener Lobby kein Takt. Fehltoleranz **3 Runden**, Rundenabstand **10 s**, Scan in **Blöcken von 30**, Sonde-Timeout **1 s**, mDNS **3 s** auf `_bts-light._tcp`.
- **Log-Upload** einmal nach jeder erfolgreichen Suche, danach alle **5 min** im Zustand Verbunden. Ringpuffer **800 Zeilen**, Zeile ≤ 1000 Zeichen.
- **Adressfilter**: nur `http://<host>:8088/…` des gefundenen Hosts (plus `about:blank`), alles andere still verworfen — auch `:8443`.
- **PIN** 4–8 Ziffern, getrennt von `tablet_settings_pin`. Ecke links oben, 2 s Fingerdruck.
- **JS-Brücke** heißt `fully`, nur `getBatteryLevel(): Int` und `isPlugged(): Boolean`.
- Kotlin-Kommentare **Deutsch** (was + warum). Nach jeder Code-Änderung `code-reviewer`-Subagent (CLAUDE.md). Tests: `cd android && ./gradlew --no-daemon :app:testDebugUnitTest` grün vor jedem Commit.
- Alle Tests laufen auf der JVM (**kein Robolectric, kein Emulator**). Klassen unter `suche/` und `kern/` importieren **nichts** aus `android.*`.
- Lokale Werkzeugkette (Task 1) landet unter `%LOCALAPPDATA%\Android\Sdk`; `android/local.properties` ist gitignoriert.

---

### Task 1: Gradle-Projekt und lokale Werkzeugkette

**Files:**
- Create: `android/settings.gradle.kts`, `android/build.gradle.kts`, `android/gradle.properties`, `android/gradle/wrapper/gradle-wrapper.properties`, `android/gradle/wrapper/gradle-wrapper.jar`, `android/gradlew`, `android/gradlew.bat`, `android/app/build.gradle.kts`, `android/app/src/main/AndroidManifest.xml`, `android/app/src/main/res/values/strings.xml`, `android/app/src/test/kotlin/de/badhub/btslight/tablet/kern/VersionTest.kt`, `android/app/src/main/kotlin/de/badhub/btslight/tablet/kern/Version.kt`
- Modify: `.gitignore`

**Interfaces:**
- Produces: Gradle-Projekt, in dem `./gradlew :app:testDebugUnitTest` läuft; `kern.Version.versionCode(versionName: String): Int`.

- [ ] **Step 1: Android-SDK-Kommandozeilenwerkzeuge installieren** (einmalig auf diesem Rechner; Bash)

```bash
SDK="$LOCALAPPDATA/Android/Sdk"
mkdir -p "$SDK/cmdline-tools"
curl -L -o "$TEMP/ct.zip" https://dl.google.com/android/repository/commandlinetools-win-11076708_latest.zip
unzip -q "$TEMP/ct.zip" -d "$SDK/cmdline-tools"
mv "$SDK/cmdline-tools/cmdline-tools" "$SDK/cmdline-tools/latest"
yes | "$SDK/cmdline-tools/latest/bin/sdkmanager.bat" --licenses >/dev/null
"$SDK/cmdline-tools/latest/bin/sdkmanager.bat" "platform-tools" "platforms;android-34" "build-tools;34.0.0"
ls "$SDK/platforms" "$SDK/build-tools"
```
Erwartet: `android-34` und `34.0.0` werden gelistet. JDK 17 liegt unter `~/.jdks/temurin-17.0.20.1` (mit `JAVA_HOME` darauf zeigen, falls Gradle es nicht findet).

- [ ] **Step 2: Gradle-Wrapper-Dateien holen**

```bash
mkdir -p android/gradle/wrapper
curl -L -o android/gradle/wrapper/gradle-wrapper.jar https://raw.githubusercontent.com/gradle/gradle/v8.7.0/gradle/wrapper/gradle-wrapper.jar
curl -L -o android/gradlew https://raw.githubusercontent.com/gradle/gradle/v8.7.0/gradlew
curl -L -o android/gradlew.bat https://raw.githubusercontent.com/gradle/gradle/v8.7.0/gradlew.bat
chmod +x android/gradlew
```

`android/gradle/wrapper/gradle-wrapper.properties`:
```properties
distributionBase=GRADLE_USER_HOME
distributionPath=wrapper/dists
distributionUrl=https\://services.gradle.org/distributions/gradle-8.7-bin.zip
networkTimeout=10000
validateDistributionUrl=true
zipStoreBase=GRADLE_USER_HOME
zipStorePath=wrapper/dists
```

- [ ] **Step 3: Projektdateien anlegen**

`android/settings.gradle.kts`:
```kotlin
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}
rootProject.name = "bts-light-tablet"
include(":app")
```

`android/build.gradle.kts`:
```kotlin
// Wurzelprojekt: nur Plugin-Versionen. Die App selbst steht in app/.
plugins {
    id("com.android.application") version "8.5.2" apply false
    id("org.jetbrains.kotlin.android") version "2.0.20" apply false
}
```

`android/gradle.properties`:
```properties
org.gradle.jvmargs=-Xmx2g -Dfile.encoding=UTF-8
android.useAndroidX=true
kotlin.code.style=official
android.nonTransitiveRClass=true
```

`android/app/build.gradle.kts`:
```kotlin
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
```

`android/app/src/main/AndroidManifest.xml` (Minimalfassung; Task 8 erweitert sie):
```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android">
    <application
        android:label="@string/app_name"
        android:allowBackup="false"
        android:usesCleartextTraffic="true"
        android:theme="@style/Theme.AppCompat.NoActionBar">
    </application>
</manifest>
```

`android/app/src/main/res/values/strings.xml`:
```xml
<?xml version="1.0" encoding="utf-8"?>
<resources>
    <string name="app_name">bts-light Tablet</string>
</resources>
```

`.gitignore` ergänzen (am Ende):
```
# Android-Kiosk-App: Build-Ausgaben, lokale SDK-Pfade, Keystores
android/.gradle/
android/build/
android/app/build/
android/local.properties
android/*.keystore
android/*.jks
```

- [ ] **Step 4: Failing Test für die Versionsregel**

`android/app/src/test/kotlin/de/badhub/btslight/tablet/kern/VersionTest.kt`:
```kotlin
package de.badhub.btslight.tablet.kern

import org.junit.Assert.assertEquals
import org.junit.Test

class VersionTest {
    @Test
    fun versionCode_ordnet_versionen_streng_aufsteigend() {
        // Android verlangt einen monoton steigenden versionCode; 0.9.281 muss
        // vor 0.10.0 liegen, obwohl "281" > "0" ist.
        assertEquals(90_281, Version.versionCode("0.9.281"))
        assertEquals(100_000, Version.versionCode("0.10.0"))
        assertEquals(1_000_000, Version.versionCode("1.0.0"))
    }
}
```

- [ ] **Step 5: Test laufen lassen, Fehlschlag sehen**

Run: `cd android && ./gradlew --no-daemon :app:testDebugUnitTest`
Erwartet: Kompilierfehler `Unresolved reference: Version`.

- [ ] **Step 6: Implementierung**

`android/app/src/main/kotlin/de/badhub/btslight/tablet/kern/Version.kt`:
```kotlin
package de.badhub.btslight.tablet.kern

/** Versionsregel der App — dieselbe Formel wie in app/build.gradle.kts. */
object Version {
    /** `major*1_000_000 + minor*10_000 + patch`, damit der Code streng mit der Version steigt. */
    fun versionCode(versionName: String): Int {
        val t = versionName.split(".").map { it.toInt() }
        return t[0] * 1_000_000 + t[1] * 10_000 + t[2]
    }
}
```

- [ ] **Step 7: Tests grün**

Run: `cd android && ./gradlew --no-daemon :app:testDebugUnitTest`
Erwartet: `BUILD SUCCESSFUL`, 1 Test.

- [ ] **Step 8: Commit**

```bash
git add android .gitignore
git commit -m "android: Gradle-Gerüst der Tablet-Kiosk-App, Version aus package.json"
```

---

### Task 2: Subnetz, Antwortprüfung, Scanner

**Files:**
- Create: `android/app/src/main/kotlin/de/badhub/btslight/tablet/suche/Subnetz.kt`, `…/suche/HealthAntwort.kt`, `…/suche/Sonde.kt`, `…/suche/Scanner.kt`
- Test: `android/app/src/test/kotlin/de/badhub/btslight/tablet/suche/SubnetzTest.kt`, `…/HealthAntwortTest.kt`, `…/ScannerTest.kt`

**Interfaces:**
- Produces: `Subnetz.kandidaten(eigeneIp: String?): List<String>`; `HealthAntwort.istTreffer(status: Int, body: String): Boolean`; `fun interface Sonde { suspend fun antwortet(ip: String): Boolean }`; `class Scanner(sonde: Sonde, blockgroesse: Int = 30) { suspend fun finde(kandidaten: List<String>): String? }`.

- [ ] **Step 1: Failing Tests**

`SubnetzTest.kt`:
```kotlin
package de.badhub.btslight.tablet.suche

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SubnetzTest {
    @Test
    fun liefert_alle_254_hosts_des_eigenen_24er_netzes() {
        val k = Subnetz.kandidaten("192.168.16.42")
        assertEquals(254, k.size)
        assertEquals("192.168.16.1", k.first())
        assertEquals("192.168.16.254", k.last())
    }

    @Test
    fun unbrauchbare_adresse_ergibt_keine_kandidaten() {
        // Ohne WLAN-Adresse darf der Scanner nicht ins Leere laufen.
        assertTrue(Subnetz.kandidaten(null).isEmpty())
        assertTrue(Subnetz.kandidaten("").isEmpty())
        assertTrue(Subnetz.kandidaten("fe80::1").isEmpty())
        assertTrue(Subnetz.kandidaten("192.168.300.1").isEmpty())
    }
}
```

`HealthAntwortTest.kt`:
```kotlin
package de.badhub.btslight.tablet.suche

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class HealthAntwortTest {
    @Test
    fun nur_200_mit_json_objekt_ist_bts_light() {
        assertTrue(HealthAntwort.istTreffer(200, """{"courts":[]}"""))
        assertTrue(HealthAntwort.istTreffer(200, "  {\"ok\":true}\n"))
    }

    @Test
    fun fremde_dienste_auf_8088_fallen_durch() {
        // Ein Router-Webinterface oder ein anderer Dienst antwortet mit HTML
        // oder einem Fehlercode — beides darf nicht als Turnier-PC gelten.
        assertFalse(HealthAntwort.istTreffer(200, "<html><body>hi</body></html>"))
        assertFalse(HealthAntwort.istTreffer(200, ""))
        assertFalse(HealthAntwort.istTreffer(404, "{}"))
        assertFalse(HealthAntwort.istTreffer(200, "[1,2]"))
    }
}
```

`ScannerTest.kt`:
```kotlin
package de.badhub.btslight.tablet.suche

import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Collections

class ScannerTest {
    private fun sondeMitTreffer(treffer: String?, gefragt: MutableList<String>) =
        Sonde { ip -> gefragt.add(ip); ip == treffer }

    @Test
    fun erster_treffer_gewinnt_und_beendet_den_scan() = runTest {
        val gefragt = Collections.synchronizedList(mutableListOf<String>())
        val scanner = Scanner(sondeMitTreffer("192.168.16.77", gefragt), blockgroesse = 30)
        val ip = scanner.finde(Subnetz.kandidaten("192.168.16.5"))
        assertEquals("192.168.16.77", ip)
        // .77 liegt im dritten Block (61–90); danach darf kein Block mehr laufen.
        assertTrue("zu viele Sonden: ${gefragt.size}", gefragt.size <= 90)
        assertTrue(gefragt.none { it.endsWith(".91") })
    }

    @Test
    fun ohne_treffer_werden_alle_kandidaten_gefragt_und_null_geliefert() = runTest {
        val gefragt = Collections.synchronizedList(mutableListOf<String>())
        val scanner = Scanner(sondeMitTreffer(null, gefragt))
        assertNull(scanner.finde(Subnetz.kandidaten("10.0.0.9")))
        assertEquals(254, gefragt.size)
    }

    @Test
    fun leere_kandidatenliste_ergibt_null() = runTest {
        assertNull(Scanner(Sonde { true }).finde(emptyList()))
    }
}
```

- [ ] **Step 2: Fehlschlag sehen**

Run: `cd android && ./gradlew --no-daemon :app:testDebugUnitTest`
Erwartet: Kompilierfehler (`Subnetz`, `HealthAntwort`, `Sonde`, `Scanner` unbekannt).

- [ ] **Step 3: Implementierung**

`Subnetz.kt`:
```kotlin
package de.badhub.btslight.tablet.suche

/** Kandidaten für den Subnetz-Scan — wie `scan_subnet` im Pi-Skript: das eigene /24. */
object Subnetz {
    fun kandidaten(eigeneIp: String?): List<String> {
        val teile = eigeneIp?.trim()?.split(".") ?: return emptyList()
        if (teile.size != 4) return emptyList()
        if (teile.any { it.toIntOrNull() !in 0..255 }) return emptyList()
        val praefix = teile.take(3).joinToString(".")
        return (1..254).map { "$praefix.$it" }
    }
}
```

`HealthAntwort.kt`:
```kotlin
package de.badhub.btslight.tablet.suche

/**
 * Prüft, ob eine Antwort von `:8088/health` wirklich von bts-light stammt.
 * Der Tablet-Server antwortet mit einem JSON-Objekt; ein fremder Dienst auf
 * demselben Port liefert HTML oder einen Fehlercode. Bewusst nur ein
 * struktureller Blick (kein Parser), damit die Klasse ohne Android läuft.
 */
object HealthAntwort {
    fun istTreffer(status: Int, body: String): Boolean {
        if (status != 200) return false
        val t = body.trim()
        return t.startsWith("{") && t.endsWith("}")
    }
}
```

`Sonde.kt`:
```kotlin
package de.badhub.btslight.tablet.suche

/** Fragt eine Adresse, ob dort bts-light antwortet. Austauschbar für Tests. */
fun interface Sonde {
    suspend fun antwortet(ip: String): Boolean
}
```

`Scanner.kt`:
```kotlin
package de.badhub.btslight.tablet.suche

import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope

/**
 * Parallel-Scan in Blöcken, Abbruch beim ersten Treffer — wie das Pi-Skript.
 * Blockgröße 30: genug Parallelität für 254 Adressen in wenigen Sekunden,
 * ohne den WLAN-Stack des Tablets mit 254 offenen Sockets zu überfahren.
 */
class Scanner(private val sonde: Sonde, private val blockgroesse: Int = 30) {
    suspend fun finde(kandidaten: List<String>): String? {
        for (block in kandidaten.chunked(blockgroesse)) {
            val treffer = coroutineScope {
                block.map { ip -> async { if (sonde.antwortet(ip)) ip else null } }.awaitAll()
            }.firstOrNull { it != null }
            if (treffer != null) return treffer
        }
        return null
    }
}
```

- [ ] **Step 4: Tests grün**

Run: `cd android && ./gradlew --no-daemon :app:testDebugUnitTest`
Erwartet: `BUILD SUCCESSFUL`, 8 Tests.

- [ ] **Step 5: Commit**

```bash
git add android/app/src
git commit -m "android: Subnetz-Kandidaten, Health-Prüfung und Block-Scanner (rein, getestet)"
```

---

### Task 3: Suchreihenfolge `ServerSuche`

**Files:**
- Create: `android/app/src/main/kotlin/de/badhub/btslight/tablet/suche/ServerSuche.kt`
- Test: `android/app/src/test/kotlin/de/badhub/btslight/tablet/suche/ServerSucheTest.kt`

**Interfaces:**
- Consumes: `Sonde`, `Scanner`, `Subnetz` (Task 2).
- Produces: `enum class Quelle { GEMERKT, SCAN, MDNS }`; `sealed class Suchergebnis { data class Treffer(val ip: String, val quelle: Quelle); object Nichts }`; `class ServerSuche(sonde: Sonde, scanner: Scanner, mdns: suspend () -> String?) { suspend fun ausfuehren(gemerkteIp: String?, eigeneIp: String?): Suchergebnis }`.

- [ ] **Step 1: Failing Tests**

```kotlin
package de.badhub.btslight.tablet.suche

import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import java.util.Collections

class ServerSucheTest {
    private fun sonde(vararg antwortende: String, gefragt: MutableList<String> = mutableListOf()) =
        Sonde { ip -> gefragt.add(ip); ip in antwortende }

    @Test
    fun gemerkte_ip_zuerst_ohne_scan() = runTest {
        val gefragt = Collections.synchronizedList(mutableListOf<String>())
        val s = ServerSuche(sonde("192.168.16.100", gefragt = gefragt), Scanner(sonde("192.168.16.100", gefragt = gefragt))) { error("mDNS darf nicht laufen") }
        val e = s.ausfuehren(gemerkteIp = "192.168.16.100", eigeneIp = "192.168.16.42")
        assertEquals(Suchergebnis.Treffer("192.168.16.100", Quelle.GEMERKT), e)
        assertEquals(listOf("192.168.16.100"), gefragt)
    }

    @Test
    fun scan_findet_neue_ip_wenn_gemerkte_tot_ist() = runTest {
        val s = ServerSuche(sonde("192.168.16.7"), Scanner(sonde("192.168.16.7"))) { null }
        val e = s.ausfuehren(gemerkteIp = "192.168.16.100", eigeneIp = "192.168.16.42")
        assertEquals(Suchergebnis.Treffer("192.168.16.7", Quelle.SCAN), e)
    }

    @Test
    fun mdns_ist_der_letzte_rueckfall_und_wird_gegengeprueft() = runTest {
        // mDNS liefert eine Adresse aus einem anderen Subnetz (Bridge) —
        // gilt nur, wenn die Sonde sie bestätigt.
        val s = ServerSuche(sonde("10.0.5.5"), Scanner(sonde("10.0.5.5"))) { "10.0.5.5" }
        assertEquals(Suchergebnis.Treffer("10.0.5.5", Quelle.MDNS), s.ausfuehren(null, "192.168.16.42"))
    }

    @Test
    fun mdns_treffer_ohne_antwort_zaehlt_nicht() = runTest {
        val s = ServerSuche(sonde(), Scanner(sonde())) { "10.0.5.5" }
        assertEquals(Suchergebnis.Nichts, s.ausfuehren(null, "192.168.16.42"))
    }

    @Test
    fun ohne_eigene_ip_bleibt_nur_gemerkt_und_mdns() = runTest {
        val gefragt = Collections.synchronizedList(mutableListOf<String>())
        val s = ServerSuche(sonde(gefragt = gefragt), Scanner(sonde(gefragt = gefragt))) { null }
        assertEquals(Suchergebnis.Nichts, s.ausfuehren("192.168.16.100", null))
        assertEquals(listOf("192.168.16.100"), gefragt)
    }
}
```

- [ ] **Step 2: Fehlschlag sehen** — Run wie oben, erwartet Kompilierfehler `ServerSuche`.

- [ ] **Step 3: Implementierung**

```kotlin
package de.badhub.btslight.tablet.suche

/** Woher der Treffer stammt — landet im Log, damit man im Feld sieht, welcher Weg trägt. */
enum class Quelle { GEMERKT, SCAN, MDNS }

sealed class Suchergebnis {
    data class Treffer(val ip: String, val quelle: Quelle) : Suchergebnis()
    object Nichts : Suchergebnis() {
        override fun toString() = "Nichts"
    }
}

/**
 * Reihenfolge nach Zuverlässigkeit, identisch zum Pi-Skript `btslight_ip()`:
 * 1) gemerkte IP (sofort), 2) Subnetz-Scan (zuverlässig), 3) mDNS (über WLAN
 * das schwächste Glied, deshalb zuletzt und nur gegengeprüft).
 */
class ServerSuche(
    private val sonde: Sonde,
    private val scanner: Scanner,
    private val mdns: suspend () -> String?,
) {
    suspend fun ausfuehren(gemerkteIp: String?, eigeneIp: String?): Suchergebnis {
        if (gemerkteIp != null && sonde.antwortet(gemerkteIp)) {
            return Suchergebnis.Treffer(gemerkteIp, Quelle.GEMERKT)
        }
        scanner.finde(Subnetz.kandidaten(eigeneIp))?.let {
            return Suchergebnis.Treffer(it, Quelle.SCAN)
        }
        val m = mdns()
        if (m != null && sonde.antwortet(m)) return Suchergebnis.Treffer(m, Quelle.MDNS)
        return Suchergebnis.Nichts
    }
}
```

- [ ] **Step 4: Tests grün** — Run wie oben, erwartet 13 Tests.

- [ ] **Step 5: Commit**

```bash
git add android/app/src
git commit -m "android: Suchreihenfolge gemerkt → Scan → mDNS nach dem Pi-Muster"
```

---

### Task 4: Zustandsmaschine der Hülle

**Files:**
- Create: `android/app/src/main/kotlin/de/badhub/btslight/tablet/kern/Huelle.kt`
- Test: `android/app/src/test/kotlin/de/badhub/btslight/tablet/kern/HuelleTest.kt`

**Interfaces:**
- Produces:
  ```kotlin
  sealed class Zustand { object Wartend; data class Suchen(val versuch: Int); data class Verbunden(val ip: String) }
  sealed class Ereignis { object Start; object WlanDa; object WlanWeg; data class Gefunden(val ip: String); object NichtsGefunden; object Ladefehler; object Handgriff; data class HandAdresse(val ip: String) }
  sealed class Wirkung { data class Suchen(val verzoegerungMs: Long); data class LadeLobby(val ip: String); object Wartekarte; data class Merke(val ip: String) }
  class Huelle(fehltoleranz: Int = 3, rundeMs: Long = 10_000) { val zustand: Zustand; fun verarbeite(e: Ereignis): List<Wirkung> }
  ```

- [ ] **Step 1: Failing Tests**

```kotlin
package de.badhub.btslight.tablet.kern

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class HuelleTest {
    private val ip = "192.168.16.100"

    @Test
    fun start_zeigt_wartekarte_und_sucht_sofort() {
        val h = Huelle()
        assertEquals(listOf(Wirkung.Wartekarte, Wirkung.Suchen(0)), h.verarbeite(Ereignis.Start))
        assertEquals(Zustand.Suchen(1), h.zustand)
    }

    @Test
    fun treffer_merkt_ip_und_laedt_die_lobby() {
        val h = Huelle().apply { verarbeite(Ereignis.Start) }
        assertEquals(listOf(Wirkung.Merke(ip), Wirkung.LadeLobby(ip)), h.verarbeite(Ereignis.Gefunden(ip)))
        assertEquals(Zustand.Verbunden(ip), h.zustand)
    }

    @Test
    fun gleicher_treffer_im_verbundenen_zustand_laedt_nicht_neu() {
        // Ein Suchlauf nach WLAN-Wackler bestätigt dieselbe IP: die laufende
        // Zählung darf nicht durch ein Neuladen unterbrochen werden.
        val h = Huelle().apply { verarbeite(Ereignis.Start); verarbeite(Ereignis.Gefunden(ip)) }
        assertTrue(h.verarbeite(Ereignis.Gefunden(ip)).isEmpty())
    }

    @Test
    fun andere_ip_laedt_die_lobby_neu() {
        val h = Huelle().apply { verarbeite(Ereignis.Start); verarbeite(Ereignis.Gefunden(ip)) }
        assertEquals(listOf(Wirkung.Merke("192.168.16.7"), Wirkung.LadeLobby("192.168.16.7")), h.verarbeite(Ereignis.Gefunden("192.168.16.7")))
    }

    @Test
    fun ohne_treffer_kommt_die_naechste_runde_verzoegert() {
        val h = Huelle(rundeMs = 10_000).apply { verarbeite(Ereignis.Start) }
        assertEquals(listOf(Wirkung.Suchen(10_000)), h.verarbeite(Ereignis.NichtsGefunden))
        assertEquals(Zustand.Suchen(2), h.zustand)
    }

    @Test
    fun verbunden_vertraegt_zwei_fehlschlaege_und_kippt_beim_dritten() {
        val h = Huelle(fehltoleranz = 3, rundeMs = 10_000).apply { verarbeite(Ereignis.Start); verarbeite(Ereignis.Gefunden(ip)) }
        h.verarbeite(Ereignis.Ladefehler)
        assertEquals(listOf(Wirkung.Suchen(10_000)), h.verarbeite(Ereignis.NichtsGefunden))
        assertEquals(listOf(Wirkung.Suchen(10_000)), h.verarbeite(Ereignis.NichtsGefunden))
        assertEquals(Zustand.Verbunden(ip), h.zustand)
        assertEquals(listOf(Wirkung.Wartekarte, Wirkung.Suchen(10_000)), h.verarbeite(Ereignis.NichtsGefunden))
        assertEquals(Zustand.Suchen(1), h.zustand)
    }

    @Test
    fun wlan_weg_wartet_ohne_zu_suchen_bis_wlan_da() {
        val h = Huelle().apply { verarbeite(Ereignis.Start); verarbeite(Ereignis.Gefunden(ip)) }
        assertEquals(listOf(Wirkung.Wartekarte), h.verarbeite(Ereignis.WlanWeg))
        assertEquals(Zustand.Wartend, h.zustand)
        assertTrue(h.verarbeite(Ereignis.NichtsGefunden).isEmpty())
        assertEquals(listOf(Wirkung.Suchen(0)), h.verarbeite(Ereignis.WlanDa))
        assertEquals(Zustand.Suchen(1), h.zustand)
    }

    @Test
    fun wlan_da_im_verbundenen_zustand_prueft_still_nach() {
        // Netz kurz weg und wieder da: kein Wartekarten-Flackern, nur ein
        // Suchlauf; bestätigt er dieselbe IP, passiert nichts.
        val h = Huelle().apply { verarbeite(Ereignis.Start); verarbeite(Ereignis.Gefunden(ip)) }
        assertEquals(listOf(Wirkung.Suchen(0)), h.verarbeite(Ereignis.WlanDa))
        assertTrue(h.verarbeite(Ereignis.Gefunden(ip)).isEmpty())
    }

    @Test
    fun ladefehler_sucht_sofort_und_laedt_bei_bestaetigung_neu() {
        val h = Huelle().apply { verarbeite(Ereignis.Start); verarbeite(Ereignis.Gefunden(ip)) }
        assertEquals(listOf(Wirkung.Suchen(0)), h.verarbeite(Ereignis.Ladefehler))
        assertEquals(listOf(Wirkung.Merke(ip), Wirkung.LadeLobby(ip)), h.verarbeite(Ereignis.Gefunden(ip)))
    }

    @Test
    fun handgriff_erzwingt_suche_und_neuladen() {
        val h = Huelle().apply { verarbeite(Ereignis.Start); verarbeite(Ereignis.Gefunden(ip)) }
        assertEquals(listOf(Wirkung.Suchen(0)), h.verarbeite(Ereignis.Handgriff))
        assertEquals(listOf(Wirkung.Merke(ip), Wirkung.LadeLobby(ip)), h.verarbeite(Ereignis.Gefunden(ip)))
    }

    @Test
    fun handgriff_im_wartenden_zustand_startet_die_suche() {
        val h = Huelle().apply { verarbeite(Ereignis.WlanWeg) }
        assertEquals(listOf(Wirkung.Suchen(0)), h.verarbeite(Ereignis.Handgriff))
        assertEquals(Zustand.Suchen(1), h.zustand)
    }

    @Test
    fun handadresse_laedt_direkt_und_merkt() {
        val h = Huelle().apply { verarbeite(Ereignis.Start) }
        assertEquals(listOf(Wirkung.Merke("10.1.1.1"), Wirkung.LadeLobby("10.1.1.1")), h.verarbeite(Ereignis.HandAdresse("10.1.1.1")))
        assertEquals(Zustand.Verbunden("10.1.1.1"), h.zustand)
    }
}
```

- [ ] **Step 2: Fehlschlag sehen** — Kompilierfehler `Huelle`/`Ereignis`/`Wirkung`.

- [ ] **Step 3: Implementierung**

```kotlin
package de.badhub.btslight.tablet.kern

sealed class Zustand {
    /** Kein WLAN — warten, nicht suchen. */
    object Wartend : Zustand() { override fun toString() = "Wartend" }
    data class Suchen(val versuch: Int) : Zustand()
    data class Verbunden(val ip: String) : Zustand()
}

sealed class Ereignis {
    object Start : Ereignis()
    object WlanDa : Ereignis()
    object WlanWeg : Ereignis()
    data class Gefunden(val ip: String) : Ereignis()
    object NichtsGefunden : Ereignis()
    /** Hauptrahmen der WebView konnte nicht laden. */
    object Ladefehler : Ereignis()
    /** „Turnier-PC neu suchen" aus Menü oder Wartekarte. */
    object Handgriff : Ereignis()
    /** „Adresse von Hand eingeben". */
    data class HandAdresse(val ip: String) : Ereignis()
}

sealed class Wirkung {
    /** Suchlauf starten, nach `verzoegerungMs` (0 = sofort). */
    data class Suchen(val verzoegerungMs: Long) : Wirkung()
    data class LadeLobby(val ip: String) : Wirkung()
    object Wartekarte : Wirkung() { override fun toString() = "Wartekarte" }
    data class Merke(val ip: String) : Wirkung()
}

/**
 * Zustandsmaschine der Hülle. Sie entscheidet, WANN gesucht wird — die Spec
 * verlangt „nur bei Bedarf": Start, WLAN-Ereignis, Ladefehler, Handgriff.
 * Im Zustand Verbunden gibt es ohne Ereignis keine Suche.
 *
 * Fehltoleranz wie beim Pi: ein Ausfall zählt erst nach `fehltoleranz`
 * erfolglosen Runden, damit ein WLAN-Wackler die Anzeige nicht wegwirft.
 */
class Huelle(private val fehltoleranz: Int = 3, private val rundeMs: Long = 10_000) {
    var zustand: Zustand = Zustand.Wartend
        private set
    private var fehlschlaege = 0
    /** Nach Ladefehler/Handgriff soll auch dieselbe IP neu geladen werden. */
    private var neuLaden = false

    fun verarbeite(e: Ereignis): List<Wirkung> = when (e) {
        Ereignis.Start -> {
            zustand = Zustand.Suchen(1)
            listOf(Wirkung.Wartekarte, Wirkung.Suchen(0))
        }
        Ereignis.WlanDa -> when (zustand) {
            is Zustand.Verbunden -> listOf(Wirkung.Suchen(0))
            else -> { zustand = Zustand.Suchen(1); listOf(Wirkung.Suchen(0)) }
        }
        Ereignis.WlanWeg -> {
            zustand = Zustand.Wartend
            fehlschlaege = 0
            listOf(Wirkung.Wartekarte)
        }
        is Ereignis.Gefunden -> {
            val vorher = zustand
            fehlschlaege = 0
            val gleich = vorher is Zustand.Verbunden && vorher.ip == e.ip
            zustand = Zustand.Verbunden(e.ip)
            if (gleich && !neuLaden) {
                emptyList()
            } else {
                neuLaden = false
                listOf(Wirkung.Merke(e.ip), Wirkung.LadeLobby(e.ip))
            }
        }
        Ereignis.NichtsGefunden -> when (val z = zustand) {
            Zustand.Wartend -> emptyList()
            is Zustand.Suchen -> {
                zustand = Zustand.Suchen(z.versuch + 1)
                listOf(Wirkung.Suchen(rundeMs))
            }
            is Zustand.Verbunden -> {
                fehlschlaege++
                if (fehlschlaege >= fehltoleranz) {
                    fehlschlaege = 0
                    zustand = Zustand.Suchen(1)
                    listOf(Wirkung.Wartekarte, Wirkung.Suchen(rundeMs))
                } else {
                    listOf(Wirkung.Suchen(rundeMs))
                }
            }
        }
        Ereignis.Ladefehler -> if (zustand is Zustand.Verbunden) {
            neuLaden = true
            listOf(Wirkung.Suchen(0))
        } else {
            emptyList()
        }
        Ereignis.Handgriff -> {
            neuLaden = true
            if (zustand !is Zustand.Verbunden) zustand = Zustand.Suchen(1)
            listOf(Wirkung.Suchen(0))
        }
        is Ereignis.HandAdresse -> {
            zustand = Zustand.Verbunden(e.ip)
            fehlschlaege = 0
            neuLaden = false
            listOf(Wirkung.Merke(e.ip), Wirkung.LadeLobby(e.ip))
        }
    }
}
```

- [ ] **Step 4: Tests grün** — erwartet 25 Tests.

- [ ] **Step 5: Commit**

```bash
git add android/app/src
git commit -m "android: Zustandsmaschine der Hülle — Suche nur bei Bedarf, Fehltoleranz 3"
```

---

### Task 5: Adressfilter, Log-Ringpuffer, PIN-Regel, Geräte-ID

**Files:**
- Create: `android/app/src/main/kotlin/de/badhub/btslight/tablet/kern/AdressFilter.kt`, `…/kern/LogPuffer.kt`, `…/kern/PinRegel.kt`, `…/kern/GeraeteId.kt`
- Test: `android/app/src/test/kotlin/de/badhub/btslight/tablet/kern/AdressFilterTest.kt`, `…/LogPufferTest.kt`, `…/PinRegelTest.kt`, `…/GeraeteIdTest.kt`

**Interfaces:**
- Produces: `class AdressFilter(host: String, port: Int = 8088) { fun erlaubt(url: String?): Boolean }`; `class LogPuffer(maxZeilen: Int = 800, maxZeichen: Int = 1000, uhr: () -> String) { fun schreibe(text: String); fun inhalt(): String }`; `object PinRegel { fun gueltig(pin: String?): Boolean }`; `object GeraeteId { fun aus(androidId: String?): String }`.

- [ ] **Step 1: Failing Tests**

`AdressFilterTest.kt`:
```kotlin
package de.badhub.btslight.tablet.kern

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AdressFilterTest {
    private val f = AdressFilter("192.168.16.100")

    @Test
    fun nur_der_gefundene_host_auf_8088_ist_erlaubt() {
        assertTrue(f.erlaubt("http://192.168.16.100:8088/felder"))
        assertTrue(f.erlaubt("http://192.168.16.100:8088/court/H1F3?x=1"))
        assertTrue(f.erlaubt("about:blank"))
    }

    @Test
    fun alles_andere_wird_verworfen() {
        // Kein Internet, kein TLS-Port, kein fremdes Schema — das ersetzt den
        // Web-Filter von Fully PLUS.
        assertFalse(f.erlaubt("https://badhub.de/spieler/123/live"))
        assertFalse(f.erlaubt("http://192.168.16.100:8443/felder"))
        assertFalse(f.erlaubt("https://192.168.16.100:8443/felder"))
        assertFalse(f.erlaubt("http://192.168.16.101:8088/felder"))
        assertFalse(f.erlaubt("http://192.168.16.100/felder"))
        assertFalse(f.erlaubt("intent://scan/#Intent;scheme=zxing;end"))
        assertFalse(f.erlaubt("javascript:alert(1)"))
        assertFalse(f.erlaubt(null))
        assertFalse(f.erlaubt("kein url"))
    }
}
```

`LogPufferTest.kt`:
```kotlin
package de.badhub.btslight.tablet.kern

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class LogPufferTest {
    @Test
    fun jede_zeile_traegt_einen_zeitstempel() {
        val p = LogPuffer(uhr = { "2026-09-06 10:00:00" })
        p.schreibe("Boot")
        assertEquals("2026-09-06 10:00:00 - Boot", p.inhalt())
    }

    @Test
    fun ringpuffer_behaelt_die_juengsten_800_zeilen() {
        val p = LogPuffer(maxZeilen = 800, uhr = { "t" })
        repeat(801) { p.schreibe("z$it") }
        val zeilen = p.inhalt().lines()
        assertEquals(800, zeilen.size)
        assertEquals("t - z1", zeilen.first())
        assertEquals("t - z800", zeilen.last())
    }

    @Test
    fun ueberlange_zeilen_werden_gekappt_damit_der_upload_unter_2_mb_bleibt() {
        val p = LogPuffer(maxZeichen = 1000, uhr = { "t" })
        p.schreibe("x".repeat(5000))
        assertTrue(p.inhalt().length <= 1000 + "t - ".length)
    }
}
```

`PinRegelTest.kt`:
```kotlin
package de.badhub.btslight.tablet.kern

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PinRegelTest {
    @Test
    fun vier_bis_acht_ziffern() {
        assertTrue(PinRegel.gueltig("0000"))
        assertTrue(PinRegel.gueltig("12345678"))
        assertFalse(PinRegel.gueltig("123"))
        assertFalse(PinRegel.gueltig("123456789"))
        assertFalse(PinRegel.gueltig("12a4"))
        assertFalse(PinRegel.gueltig(""))
        assertFalse(PinRegel.gueltig(null))
    }
}
```

`GeraeteIdTest.kt`:
```kotlin
package de.badhub.btslight.tablet.kern

import org.junit.Assert.assertEquals
import org.junit.Test

class GeraeteIdTest {
    @Test
    fun praefix_fire_und_nur_sichere_zeichen() {
        // /pi-log filtert selbst auf [A-Za-z0-9_-]; wir liefern das schon
        // sauber, damit die Datei beim PC den erwarteten Namen bekommt.
        assertEquals("fire-9774d56d682e549c", GeraeteId.aus("9774d56d682e549c"))
        assertEquals("fire-abc", GeraeteId.aus("a b/c"))
        assertEquals("fire-unbekannt", GeraeteId.aus(null))
        assertEquals("fire-unbekannt", GeraeteId.aus("///"))
    }
}
```

- [ ] **Step 2: Fehlschlag sehen** — Kompilierfehler für alle vier Klassen.

- [ ] **Step 3: Implementierung**

`AdressFilter.kt`:
```kotlin
package de.badhub.btslight.tablet.kern

import java.net.URI

/**
 * Die WebView folgt nur Adressen des gefundenen Turnier-PCs auf dem
 * Klartext-Port. Alles andere (Internet, TLS-Port, fremde Schemata) wird
 * verworfen — das Tablet soll im Kiosk nichts außer bts-light erreichen.
 */
class AdressFilter(private val host: String, private val port: Int = 8088) {
    fun erlaubt(url: String?): Boolean {
        if (url == null) return false
        if (url == "about:blank") return true
        val u = try { URI(url) } catch (e: Exception) { return false }
        return u.scheme == "http" && u.host == host && u.port == port
    }
}
```

`LogPuffer.kt`:
```kotlin
package de.badhub.btslight.tablet.kern

import java.time.LocalDateTime
import java.time.format.DateTimeFormatter

/** Ringpuffer für das Geräte-Log — Deckel wie `tail -n 800` beim Pi. */
class LogPuffer(
    private val maxZeilen: Int = 800,
    private val maxZeichen: Int = 1000,
    private val uhr: () -> String = { LocalDateTime.now().format(FORMAT) },
) {
    private val zeilen = ArrayDeque<String>()

    @Synchronized
    fun schreibe(text: String) {
        zeilen.addLast("${uhr()} - ${text.take(maxZeichen)}")
        while (zeilen.size > maxZeilen) zeilen.removeFirst()
    }

    @Synchronized
    fun inhalt(): String = zeilen.joinToString("\n")

    private companion object {
        val FORMAT: DateTimeFormatter = DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss")
    }
}
```

`PinRegel.kt`:
```kotlin
package de.badhub.btslight.tablet.kern

/** Kiosk-PIN: 4–8 Ziffern. Bedienschutz, keine Sicherheitsgrenze. */
object PinRegel {
    fun gueltig(pin: String?): Boolean =
        pin != null && pin.length in 4..8 && pin.all { it.isDigit() }
}
```

`GeraeteId.kt`:
```kotlin
package de.badhub.btslight.tablet.kern

/** Geräte-ID für /pi-log: `fire-<ANDROID_ID>`, stabil je Tablet über Turniere hinweg. */
object GeraeteId {
    fun aus(androidId: String?): String {
        val sauber = androidId?.filter { it.isLetterOrDigit() || it == '-' || it == '_' }?.take(32)
        return "fire-" + (sauber?.ifEmpty { null } ?: "unbekannt")
    }
}
```

- [ ] **Step 4: Tests grün** — erwartet 32 Tests.

- [ ] **Step 5: Commit**

```bash
git add android/app/src
git commit -m "android: Adressfilter, Log-Ringpuffer, PIN-Regel und Geräte-ID (rein, getestet)"
```

---

### Task 6: Android-Adapter für Netz und Suche

**Files:**
- Create: `android/app/src/main/kotlin/de/badhub/btslight/tablet/netz/AndroidSonde.kt`, `…/netz/MdnsSuche.kt`, `…/netz/NetzBeobachter.kt`, `…/netz/LogUpload.kt`, `…/netz/Einstellungen.kt`

**Interfaces:**
- Consumes: `Sonde`, `HealthAntwort` (Task 2), `LogPuffer` (Task 5).
- Produces: `class AndroidSonde(port: Int = 8088, timeoutMs: Int = 1000) : Sonde`; `class MdnsSuche(nsd: NsdManager) { suspend fun finde(timeoutMs: Long = 3000): String? }`; `class NetzBeobachter(ctx: Context, aufNetz: (da: Boolean) -> Unit) { fun start(); fun stop(); companion fun eigeneIpv4(ctx: Context): String? }`; `class LogUpload(puffer: LogPuffer) { suspend fun sende(ip: String, geraeteId: String): Boolean }`; `class Einstellungen(ctx: Context) { var gemerkteIp: String?; var pin: String? }`.

Diese Klassen sind Adapter ohne eigene Logik; sie werden nicht auf der JVM getestet (Feldtest-Checkliste in der Spec). Deshalb kein Failing-Test-Schritt, aber der Build muss kompilieren.

- [ ] **Step 1: Sonde und Log-Upload**

`AndroidSonde.kt`:
```kotlin
package de.badhub.btslight.tablet.netz

import de.badhub.btslight.tablet.suche.HealthAntwort
import de.badhub.btslight.tablet.suche.Sonde
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.HttpURLConnection
import java.net.URL

/** Klopft an `http://<ip>:8088/health`. Kurzes Timeout, damit 254 Sonden schnell durch sind. */
class AndroidSonde(private val port: Int = 8088, private val timeoutMs: Int = 1000) : Sonde {
    override suspend fun antwortet(ip: String): Boolean = withContext(Dispatchers.IO) {
        var c: HttpURLConnection? = null
        try {
            c = URL("http://$ip:$port/health").openConnection() as HttpURLConnection
            c.connectTimeout = timeoutMs
            c.readTimeout = timeoutMs * 2
            c.requestMethod = "GET"
            val status = c.responseCode
            val body = if (status == 200) c.inputStream.bufferedReader().use { it.readText() } else ""
            HealthAntwort.istTreffer(status, body)
        } catch (e: Exception) {
            false
        } finally {
            c?.disconnect()
        }
    }
}
```

`LogUpload.kt`:
```kotlin
package de.badhub.btslight.tablet.netz

import de.badhub.btslight.tablet.kern.LogPuffer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.HttpURLConnection
import java.net.URL

/**
 * Schickt das Geräte-Log an den Turnier-PC (`POST /pi-log`), der es lokal
 * ablegt und in die Cloud weiterreicht — derselbe Weg wie bei den Pis:
 * plain HTTP im LAN, keine Uhr, kein TLS nötig. Fehler sind still.
 */
class LogUpload(private val puffer: LogPuffer, private val port: Int = 8088) {
    suspend fun sende(ip: String, geraeteId: String): Boolean = withContext(Dispatchers.IO) {
        var c: HttpURLConnection? = null
        try {
            c = URL("http://$ip:$port/pi-log?device=$geraeteId").openConnection() as HttpURLConnection
            c.connectTimeout = 8000
            c.readTimeout = 8000
            c.requestMethod = "POST"
            c.doOutput = true
            c.setRequestProperty("Content-Type", "text/plain; charset=utf-8")
            c.outputStream.use { it.write(puffer.inhalt().toByteArray(Charsets.UTF_8)) }
            c.responseCode == 200
        } catch (e: Exception) {
            false
        } finally {
            c?.disconnect()
        }
    }
}
```

- [ ] **Step 2: mDNS-Rückfall**

`MdnsSuche.kt`:
```kotlin
package de.badhub.btslight.tablet.netz

import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeoutOrNull
import kotlin.coroutines.resume

/**
 * mDNS über NsdManager — nur Rückfall, hart begrenzt: über WLAN hing die
 * Auflösung bei den Pis minutenlang, deshalb darf sie hier NIE blockieren.
 */
class MdnsSuche(private val nsd: NsdManager) {
    suspend fun finde(timeoutMs: Long = 3000): String? {
        var listener: NsdManager.DiscoveryListener? = null
        try {
            return withTimeoutOrNull(timeoutMs) {
                suspendCancellableCoroutine { cont ->
                    val l = object : NsdManager.DiscoveryListener {
                        override fun onServiceFound(info: NsdServiceInfo) {
                            nsd.resolveService(info, object : NsdManager.ResolveListener {
                                override fun onServiceResolved(r: NsdServiceInfo) {
                                    val ip = r.host?.hostAddress
                                    if (ip != null && cont.isActive) cont.resume(ip)
                                }
                                override fun onResolveFailed(s: NsdServiceInfo, code: Int) {}
                            })
                        }
                        override fun onStartDiscoveryFailed(t: String, code: Int) { if (cont.isActive) cont.resume(null) }
                        override fun onStopDiscoveryFailed(t: String, code: Int) {}
                        override fun onDiscoveryStarted(t: String) {}
                        override fun onDiscoveryStopped(t: String) {}
                        override fun onServiceLost(info: NsdServiceInfo) {}
                    }
                    listener = l
                    nsd.discoverServices(DIENST, NsdManager.PROTOCOL_DNS_SD, l)
                }
            }
        } finally {
            listener?.let { runCatching { nsd.stopServiceDiscovery(it) } }
        }
    }

    private companion object {
        /** Muss zu `tablet/mdns.rs` passen. */
        const val DIENST = "_bts-light._tcp"
    }
}
```

- [ ] **Step 3: Netz-Beobachter und Einstellungen**

`NetzBeobachter.kt`:
```kotlin
package de.badhub.btslight.tablet.netz

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import java.net.Inet4Address

/** Meldet WLAN da/weg über den Standard-Netz-Callback — Auslöser für die Suche. */
class NetzBeobachter(ctx: Context, private val aufNetz: (da: Boolean) -> Unit) {
    private val cm = ctx.getSystemService(ConnectivityManager::class.java)
    private val callback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) = aufNetz(true)
        override fun onLost(network: Network) = aufNetz(false)
    }

    fun start() = cm.registerDefaultNetworkCallback(callback)
    fun stop() {
        runCatching { cm.unregisterNetworkCallback(callback) }
    }

    companion object {
        /** Eigene IPv4 des aktiven Netzes — Grundlage für den Subnetz-Scan. */
        fun eigeneIpv4(ctx: Context): String? {
            val cm = ctx.getSystemService(ConnectivityManager::class.java)
            val lp = cm.getLinkProperties(cm.activeNetwork ?: return null) ?: return null
            return lp.linkAddresses.map { it.address }.filterIsInstance<Inet4Address>()
                .firstOrNull()?.hostAddress
        }
    }
}
```

`Einstellungen.kt`:
```kotlin
package de.badhub.btslight.tablet.netz

import android.content.Context

/** Zwei Werte überleben den Neustart: die gemerkte IP und die Kiosk-PIN. Kein Feld. */
class Einstellungen(ctx: Context) {
    private val p = ctx.getSharedPreferences("huelle", Context.MODE_PRIVATE)

    var gemerkteIp: String?
        get() = p.getString("gemerkte_ip", null)
        set(v) = p.edit().putString("gemerkte_ip", v).apply()

    var pin: String?
        get() = p.getString("pin", null)
        set(v) = p.edit().putString("pin", v).apply()
}
```

- [ ] **Step 4: Bauen**

Run: `cd android && ./gradlew --no-daemon :app:compileDebugKotlin :app:testDebugUnitTest`
Erwartet: `BUILD SUCCESSFUL`, weiterhin 32 Tests.

- [ ] **Step 5: Commit**

```bash
git add android/app/src
git commit -m "android: Sonde, mDNS, Netz-Beobachter, Log-Upload und Einstellungen als Adapter"
```

---

### Task 7: JS-Brücke und Kiosk-Helfer

**Files:**
- Create: `android/app/src/main/kotlin/de/badhub/btslight/tablet/kiosk/FullyBruecke.kt`, `…/kiosk/Kiosk.kt`, `…/kiosk/KioskAdminReceiver.kt`, `android/app/src/main/res/xml/device_admin.xml`
- (Der `BootReceiver` braucht die Activity und entsteht in Task 8; sein Code steht hier zur Vollständigkeit.)

**Interfaces:**
- Produces: `class FullyBruecke(ctx: Context)` mit `@JavascriptInterface getBatteryLevel(): Int`, `isPlugged(): Boolean`; `object Kiosk { fun istBesitzer(ctx): Boolean; fun einrichten(a: Activity, log: (String) -> Unit); fun sperren(a: Activity, log); fun verlassen(a: Activity) }`.

- [ ] **Step 1: JS-Brücke**

`FullyBruecke.kt`:
```kotlin
package de.badhub.btslight.tablet.kiosk

import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.BatteryManager
import android.webkit.JavascriptInterface

/**
 * Gibt sich gegenüber tablet.html als `window.fully` aus — genau die zwei
 * Methoden, die die Seite bei Fully Kiosk für den Akku-Badge abfragt. So
 * bleibt die Seite unverändert; die Web-Battery-API gibt es über HTTP nicht.
 */
class FullyBruecke(private val ctx: Context) {
    @JavascriptInterface
    fun getBatteryLevel(): Int {
        val bm = ctx.getSystemService(BatteryManager::class.java)
        return bm.getIntProperty(BatteryManager.BATTERY_PROPERTY_CAPACITY)
    }

    @JavascriptInterface
    fun isPlugged(): Boolean {
        val i = ctx.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
        return (i?.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0) ?: 0) != 0
    }
}
```

- [ ] **Step 2: Kiosk-Helfer und Receiver**

`res/xml/device_admin.xml`:
```xml
<?xml version="1.0" encoding="utf-8"?>
<device-admin xmlns:android="http://schemas.android.com/apk/res/android">
    <uses-policies />
</device-admin>
```

`KioskAdminReceiver.kt`:
```kotlin
package de.badhub.btslight.tablet.kiosk

import android.app.admin.DeviceAdminReceiver

/** Ziel von `dpm set-device-owner …/.KioskAdminReceiver`. Keine eigene Logik. */
class KioskAdminReceiver : DeviceAdminReceiver()
```

`BootReceiver.kt` (**erst in Task 8 anlegen**):
```kotlin
package de.badhub.btslight.tablet.kiosk

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import de.badhub.btslight.tablet.KioskActivity

/**
 * Rückfall-Autostart. Der eigentliche Autostart ist der Home-Launcher
 * (Kiosk.einrichten); Fire OS trödelt bei BOOT_COMPLETED gelegentlich,
 * deshalb beides.
 */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED) return
        ctx.startActivity(
            Intent(ctx, KioskActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
    }
}
```

`Kiosk.kt`:
```kotlin
package de.badhub.btslight.tablet.kiosk

import android.app.Activity
import android.app.admin.DevicePolicyManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.provider.Settings
import android.view.WindowManager
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

/**
 * Harte Sperre als Gerätebesitzer (einmalig per ADB gesetzt); ohne
 * Besitzer weiche Anheft-Sperre — Android fragt dann einmal nach.
 */
object Kiosk {
    private fun dpm(ctx: Context) = ctx.getSystemService(DevicePolicyManager::class.java)
    private fun admin(ctx: Context) = ComponentName(ctx, KioskAdminReceiver::class.java)

    fun istBesitzer(ctx: Context): Boolean = dpm(ctx).isDeviceOwnerApp(ctx.packageName)

    /** Einmalige Gerätebesitzer-Einstellungen; idempotent, bei jedem Start. */
    fun einrichten(a: Activity, log: (String) -> Unit) {
        if (!istBesitzer(a)) {
            log("Kiosk: kein Gerätebesitzer — weiche Sperre")
            return
        }
        val d = dpm(a)
        val ad = admin(a)
        d.setLockTaskPackages(ad, arrayOf(a.packageName))
        d.setLockTaskFeatures(ad, DevicePolicyManager.LOCK_TASK_FEATURE_NONE)
        d.setKeyguardDisabled(ad, true)
        d.setStatusBarDisabled(ad, true)
        // 7 = AC | USB | Wireless: Bildschirm bleibt am Ladegerät immer an.
        d.setGlobalSetting(ad, Settings.Global.STAY_ON_WHILE_PLUGGED_IN, "7")
        // Die App wird Home-Launcher → Autostart nach jedem Boot.
        val home = IntentFilter(Intent.ACTION_MAIN).apply {
            addCategory(Intent.CATEGORY_HOME)
            addCategory(Intent.CATEGORY_DEFAULT)
        }
        d.addPersistentPreferredActivity(ad, home, ComponentName(a, a.javaClass))
        log("Kiosk: Gerätebesitzer eingerichtet")
    }

    /** Vollbild + Wachhalten + Lock-Task. */
    fun sperren(a: Activity, log: (String) -> Unit) {
        a.window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        WindowCompat.setDecorFitsSystemWindows(a.window, false)
        WindowInsetsControllerCompat(a.window, a.window.decorView).apply {
            hide(WindowInsetsCompat.Type.systemBars())
            systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        }
        runCatching { a.startLockTask() }
            .onSuccess { log("Kiosk: Lock-Task aktiv (Besitzer=${istBesitzer(a)})") }
            .onFailure { log("Kiosk: Lock-Task fehlgeschlagen: ${it.message}") }
    }

    fun verlassen(a: Activity) {
        runCatching { a.stopLockTask() }
        a.finishAffinity()
    }
}
```

- [ ] **Step 3: Bauen**

Run: `cd android && ./gradlew --no-daemon :app:compileDebugKotlin :app:testDebugUnitTest`
Erwartet: `BUILD SUCCESSFUL`, 32 Tests.

- [ ] **Step 4: Commit**

```bash
git add android/app/src/main/kotlin/de/badhub/btslight/tablet/kiosk/FullyBruecke.kt android/app/src/main/kotlin/de/badhub/btslight/tablet/kiosk/Kiosk.kt android/app/src/main/kotlin/de/badhub/btslight/tablet/kiosk/KioskAdminReceiver.kt android/app/src/main/res/xml/device_admin.xml
git commit -m "android: fully-kompatible JS-Brücke, Kiosk-Helfer (Gerätebesitzer, Lock-Task), Admin-Receiver"
```

---

### Task 8: `KioskActivity`, Layout, Manifest

**Files:**
- Create: `android/app/src/main/kotlin/de/badhub/btslight/tablet/KioskActivity.kt`, `android/app/src/main/res/layout/activity_kiosk.xml`, `android/app/src/main/kotlin/de/badhub/btslight/tablet/kiosk/BootReceiver.kt` (aus Task 7)
- Modify: `android/app/src/main/AndroidManifest.xml`, `android/app/src/main/res/values/strings.xml`

**Interfaces:**
- Consumes: alles aus Task 2–7.
- Produces: lauffähige App.

- [ ] **Step 1: Layout und Strings**

`res/layout/activity_kiosk.xml`:
```xml
<?xml version="1.0" encoding="utf-8"?>
<FrameLayout xmlns:android="http://schemas.android.com/apk/res/android"
    android:layout_width="match_parent"
    android:layout_height="match_parent"
    android:background="#111827">

    <WebView
        android:id="@+id/web"
        android:layout_width="match_parent"
        android:layout_height="match_parent" />

    <LinearLayout
        android:id="@+id/wartekarte"
        android:layout_width="match_parent"
        android:layout_height="match_parent"
        android:background="#111827"
        android:gravity="center"
        android:orientation="vertical"
        android:padding="32dp">

        <TextView
            android:layout_width="wrap_content"
            android:layout_height="wrap_content"
            android:text="@string/app_name"
            android:textColor="#9CA3AF"
            android:textSize="18sp" />

        <TextView
            android:id="@+id/wartekarte_titel"
            android:layout_width="wrap_content"
            android:layout_height="wrap_content"
            android:layout_marginTop="12dp"
            android:text="@string/suche_titel"
            android:textColor="#F9FAFB"
            android:textSize="28sp" />

        <TextView
            android:id="@+id/wartekarte_detail"
            android:layout_width="wrap_content"
            android:layout_height="wrap_content"
            android:layout_marginTop="16dp"
            android:gravity="center"
            android:textColor="#D1D5DB"
            android:textSize="16sp" />

        <Button
            android:id="@+id/erneut"
            android:layout_width="wrap_content"
            android:layout_height="wrap_content"
            android:layout_marginTop="28dp"
            android:text="@string/erneut_suchen" />

        <TextView
            android:id="@+id/wartekarte_hinweis"
            android:layout_width="wrap_content"
            android:layout_height="wrap_content"
            android:layout_marginTop="24dp"
            android:textColor="#F59E0B"
            android:textSize="13sp"
            android:visibility="gone" />
    </LinearLayout>

    <!-- Unsichtbare Ecke: 2 s Fingerdruck öffnet das Hüllen-Menü. -->
    <View
        android:id="@+id/ecke"
        android:layout_width="72dp"
        android:layout_height="72dp"
        android:layout_gravity="top|start" />
</FrameLayout>
```

`res/values/strings.xml` (ersetzen):
```xml
<?xml version="1.0" encoding="utf-8"?>
<resources>
    <string name="app_name">bts-light Tablet</string>
    <string name="suche_titel">Suche Turnier-PC im WLAN …</string>
    <string name="warte_titel">Kein WLAN</string>
    <string name="erneut_suchen">Erneut suchen</string>
    <string name="kein_besitzer">Nicht als Gerätebesitzer eingerichtet – Sperre nur weich.</string>
    <string name="pin_festlegen">Kiosk-PIN festlegen (4–8 Ziffern)</string>
    <string name="pin_eingeben">Kiosk-PIN</string>
    <string name="pin_falsch">PIN falsch</string>
    <string name="menu_titel">bts-light Tablet</string>
    <string name="menu_neu_suchen">Turnier-PC neu suchen</string>
    <string name="menu_adresse">Adresse von Hand eingeben</string>
    <string name="menu_pin">PIN ändern</string>
    <string name="menu_verlassen">Kiosk verlassen</string>
    <string name="adresse_titel">IP-Adresse des Turnier-PCs</string>
    <string name="ok">OK</string>
    <string name="abbrechen">Abbrechen</string>
</resources>
```

- [ ] **Step 2: Manifest**

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android">

    <uses-permission android:name="android.permission.INTERNET" />
    <uses-permission android:name="android.permission.ACCESS_NETWORK_STATE" />
    <uses-permission android:name="android.permission.ACCESS_WIFI_STATE" />
    <uses-permission android:name="android.permission.RECEIVE_BOOT_COMPLETED" />

    <application
        android:label="@string/app_name"
        android:allowBackup="false"
        android:usesCleartextTraffic="true"
        android:theme="@style/Theme.AppCompat.NoActionBar">

        <!-- Launcher-Eintrag UND Home-Kandidat: als Gerätebesitzer macht sich
             die App selbst zum festen Home → Autostart nach jedem Boot. -->
        <activity
            android:name=".KioskActivity"
            android:exported="true"
            android:launchMode="singleTask"
            android:lockTaskMode="if_whitelisted"
            android:configChanges="orientation|screenSize|keyboardHidden">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.HOME" />
                <category android:name="android.intent.category.DEFAULT" />
            </intent-filter>
        </activity>

        <receiver
            android:name=".kiosk.KioskAdminReceiver"
            android:exported="true"
            android:permission="android.permission.BIND_DEVICE_ADMIN">
            <meta-data
                android:name="android.app.device_admin"
                android:resource="@xml/device_admin" />
            <intent-filter>
                <action android:name="android.app.action.DEVICE_ADMIN_ENABLED" />
            </intent-filter>
        </receiver>

        <receiver
            android:name=".kiosk.BootReceiver"
            android:exported="true">
            <intent-filter>
                <action android:name="android.intent.action.BOOT_COMPLETED" />
            </intent-filter>
        </receiver>
    </application>
</manifest>
```

- [ ] **Step 3: Activity**

`KioskActivity.kt`:
```kotlin
package de.badhub.btslight.tablet

import android.annotation.SuppressLint
import android.net.nsd.NsdManager
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.text.InputType
import android.view.MotionEvent
import android.view.View
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.Button
import android.widget.EditText
import android.widget.TextView
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import de.badhub.btslight.tablet.kern.AdressFilter
import de.badhub.btslight.tablet.kern.Ereignis
import de.badhub.btslight.tablet.kern.GeraeteId
import de.badhub.btslight.tablet.kern.Huelle
import de.badhub.btslight.tablet.kern.LogPuffer
import de.badhub.btslight.tablet.kern.PinRegel
import de.badhub.btslight.tablet.kern.Wirkung
import de.badhub.btslight.tablet.kern.Zustand
import de.badhub.btslight.tablet.kiosk.FullyBruecke
import de.badhub.btslight.tablet.kiosk.Kiosk
import de.badhub.btslight.tablet.netz.AndroidSonde
import de.badhub.btslight.tablet.netz.Einstellungen
import de.badhub.btslight.tablet.netz.LogUpload
import de.badhub.btslight.tablet.netz.MdnsSuche
import de.badhub.btslight.tablet.netz.NetzBeobachter
import de.badhub.btslight.tablet.suche.Scanner
import de.badhub.btslight.tablet.suche.ServerSuche
import de.badhub.btslight.tablet.suche.Suchergebnis
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/**
 * Die Hülle: WebView + Wartekarte. Alle Entscheidungen trifft `Huelle`
 * (Zustandsmaschine, getestet); hier werden nur Ereignisse eingespeist und
 * Wirkungen ausgeführt.
 */
class KioskActivity : AppCompatActivity() {
    private lateinit var web: WebView
    private lateinit var wartekarte: View
    private lateinit var wartekarteTitel: TextView
    private lateinit var wartekarteDetail: TextView
    private lateinit var wartekarteHinweis: TextView
    private lateinit var einstellungen: Einstellungen
    private lateinit var suche: ServerSuche
    private lateinit var geraeteId: String

    private val huelle = Huelle()
    private val log = LogPuffer()
    private val upload = LogUpload(log)
    private var filter: AdressFilter? = null
    private var suchlauf: Job? = null
    private var uploadTakt: Job? = null
    private var netz: NetzBeobachter? = null
    private val handler = Handler(Looper.getMainLooper())

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_kiosk)
        einstellungen = Einstellungen(this)
        geraeteId = GeraeteId.aus(Settings.Secure.getString(contentResolver, Settings.Secure.ANDROID_ID))
        web = findViewById(R.id.web)
        wartekarte = findViewById(R.id.wartekarte)
        wartekarteTitel = findViewById(R.id.wartekarte_titel)
        wartekarteDetail = findViewById(R.id.wartekarte_detail)
        wartekarteHinweis = findViewById(R.id.wartekarte_hinweis)
        findViewById<Button>(R.id.erneut).setOnClickListener { verarbeite(Ereignis.Handgriff) }

        log.schreibe("Start $geraeteId, Version ${BuildConfig.VERSION_NAME}")
        Kiosk.einrichten(this, log::schreibe)
        Kiosk.sperren(this, log::schreibe)
        wartekarteHinweis.visibility = if (Kiosk.istBesitzer(this)) View.GONE else View.VISIBLE
        wartekarteHinweis.setText(R.string.kein_besitzer)

        webEinrichten()
        eckeEinrichten()

        val sonde = AndroidSonde()
        suche = ServerSuche(sonde, Scanner(sonde), MdnsSuche(getSystemService(NsdManager::class.java))::finde)
        netz = NetzBeobachter(this) { da ->
            log.schreibe(if (da) "WLAN da" else "WLAN weg")
            verarbeite(if (da) Ereignis.WlanDa else Ereignis.WlanWeg)
        }

        if (einstellungen.pin == null) pinFestlegen { verarbeite(Ereignis.Start) } else verarbeite(Ereignis.Start)
    }

    override fun onStart() {
        super.onStart()
        netz?.start()
    }

    override fun onStop() {
        netz?.stop()
        super.onStop()
    }

    // ---- Zustandsmaschine -------------------------------------------------

    private fun verarbeite(e: Ereignis) = runOnUiThread {
        for (w in huelle.verarbeite(e)) fuehreAus(w)
        wartekarteAktualisieren()
    }

    private fun fuehreAus(w: Wirkung) {
        when (w) {
            is Wirkung.Suchen -> starteSuche(w.verzoegerungMs)
            is Wirkung.LadeLobby -> {
                filter = AdressFilter(w.ip)
                log.schreibe("Lobby laden: ${w.ip}")
                web.loadUrl("http://${w.ip}:8088/felder")
                wartekarte.visibility = View.GONE
                uploadStarten(w.ip)
            }
            Wirkung.Wartekarte -> {
                uploadTakt?.cancel()
                wartekarte.visibility = View.VISIBLE
            }
            is Wirkung.Merke -> einstellungen.gemerkteIp = w.ip
        }
    }

    /** Ein Suchlauf zur Zeit; ein zweiter Auslöser während des Laufs verpufft. */
    private fun starteSuche(verzoegerungMs: Long) {
        if (suchlauf?.isActive == true) return
        suchlauf = lifecycleScope.launch {
            delay(verzoegerungMs)
            val t0 = System.currentTimeMillis()
            val erg = suche.ausfuehren(einstellungen.gemerkteIp, NetzBeobachter.eigeneIpv4(this@KioskActivity))
            log.schreibe("Suche: $erg (${System.currentTimeMillis() - t0} ms)")
            when (erg) {
                is Suchergebnis.Treffer -> verarbeite(Ereignis.Gefunden(erg.ip))
                Suchergebnis.Nichts -> verarbeite(Ereignis.NichtsGefunden)
            }
        }
    }

    private fun wartekarteAktualisieren() {
        when (val z = huelle.zustand) {
            Zustand.Wartend -> {
                wartekarteTitel.setText(R.string.warte_titel)
                wartekarteDetail.text = ""
            }
            is Zustand.Suchen -> {
                wartekarteTitel.setText(R.string.suche_titel)
                wartekarteDetail.text = "Versuch ${z.versuch} · eigene IP ${NetzBeobachter.eigeneIpv4(this) ?: "–"} · zuletzt ${einstellungen.gemerkteIp ?: "–"}"
            }
            is Zustand.Verbunden -> Unit
        }
    }

    /** Einmal nach jeder erfolgreichen Suche, dann alle 5 min — solange verbunden. */
    private fun uploadStarten(ip: String) {
        uploadTakt?.cancel()
        uploadTakt = lifecycleScope.launch {
            while (isActive) {
                upload.sende(ip, geraeteId)
                delay(5 * 60 * 1000L)
            }
        }
    }

    // ---- WebView ------------------------------------------------------------

    @SuppressLint("SetJavaScriptEnabled")
    private fun webEinrichten() {
        web.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true            // localStorage: Spielstand, Geräte-ID der Seite
            mediaPlaybackRequiresUserGesture = false // Gong ohne Fingertipp
            cacheMode = android.webkit.WebSettings.LOAD_DEFAULT
        }
        web.addJavascriptInterface(FullyBruecke(this), "fully")
        web.webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                val ok = filter?.erlaubt(request.url.toString()) ?: false
                if (!ok) log.schreibe("Adresse verworfen: ${request.url}")
                return !ok
            }

            override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceError) {
                if (!request.isForMainFrame) return
                log.schreibe("Ladefehler ${error.errorCode} ${request.url}")
                verarbeite(Ereignis.Ladefehler)
            }
        }
    }

    // ---- Hüllen-Menü ----------------------------------------------------------

    private fun eckeEinrichten() {
        val ecke = findViewById<View>(R.id.ecke)
        val oeffnen = Runnable { pinAbfragen { menuZeigen() } }
        ecke.setOnTouchListener { _, ev ->
            when (ev.actionMasked) {
                MotionEvent.ACTION_DOWN -> handler.postDelayed(oeffnen, 2000)
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> handler.removeCallbacks(oeffnen)
            }
            true
        }
    }

    private fun menuZeigen() {
        log.schreibe("Menü geöffnet")
        val eintraege = arrayOf(
            getString(R.string.menu_neu_suchen),
            getString(R.string.menu_adresse),
            getString(R.string.menu_pin),
            getString(R.string.menu_verlassen),
        )
        AlertDialog.Builder(this).setTitle(R.string.menu_titel).setItems(eintraege) { _, i ->
            when (i) {
                0 -> verarbeite(Ereignis.Handgriff)
                1 -> adresseAbfragen()
                2 -> pinFestlegen { }
                3 -> { log.schreibe("Kiosk verlassen"); Kiosk.verlassen(this) }
            }
        }.setNegativeButton(R.string.abbrechen, null).show()
    }

    private fun adresseAbfragen() {
        val feld = EditText(this).apply { inputType = InputType.TYPE_CLASS_TEXT; hint = "192.168.16.100" }
        AlertDialog.Builder(this).setTitle(R.string.adresse_titel).setView(feld)
            .setPositiveButton(R.string.ok) { _, _ ->
                val ip = feld.text.toString().trim()
                if (ip.isNotEmpty()) verarbeite(Ereignis.HandAdresse(ip))
            }
            .setNegativeButton(R.string.abbrechen, null).show()
    }

    private fun pinFestlegen(danach: () -> Unit) {
        val feld = EditText(this).apply { inputType = InputType.TYPE_CLASS_NUMBER or InputType.TYPE_NUMBER_VARIATION_PASSWORD }
        AlertDialog.Builder(this).setTitle(R.string.pin_festlegen).setView(feld).setCancelable(false)
            .setPositiveButton(R.string.ok) { _, _ ->
                val pin = feld.text.toString()
                if (PinRegel.gueltig(pin)) { einstellungen.pin = pin; danach() } else pinFestlegen(danach)
            }.show()
    }

    private fun pinAbfragen(danach: () -> Unit) {
        val feld = EditText(this).apply { inputType = InputType.TYPE_CLASS_NUMBER or InputType.TYPE_NUMBER_VARIATION_PASSWORD }
        AlertDialog.Builder(this).setTitle(R.string.pin_eingeben).setView(feld)
            .setPositiveButton(R.string.ok) { _, _ ->
                if (feld.text.toString() == einstellungen.pin) danach() else log.schreibe("PIN falsch")
            }
            .setNegativeButton(R.string.abbrechen, null).show()
    }

    /** Zurück-Taste im Kiosk ist tot; ohne Lock-Task würde sie die App beenden. */
    @Deprecated("Deprecated in Java")
    override fun onBackPressed() = Unit
}
```

`BootReceiver.kt` jetzt anlegen (Inhalt aus Task 7, Step 2).

- [ ] **Step 4: Bauen und Tests**

Run: `cd android && ./gradlew --no-daemon :app:assembleDebug :app:testDebugUnitTest`
Erwartet: `BUILD SUCCESSFUL`, `app/build/outputs/apk/debug/app-debug.apk` vorhanden, 32 Tests grün.

- [ ] **Step 5: code-reviewer-Subagent** über `android/` laufen lassen; Befunde einarbeiten.

- [ ] **Step 6: Commit**

```bash
git add android
git commit -m "android: KioskActivity — Wartekarte, WebView mit Adressfilter, Hüllen-Menü, Log-Takt, Boot-Receiver"
```

---

### Task 9: Einrichtungsskripte

**Files:**
- Create: `android/setup-tablet.ps1`, `android/setup-tablet.sh`

- [ ] **Step 1: PowerShell-Skript**

```powershell
# Einmalige Einrichtung eines Fire-/Android-Tablets als bts-light-Zähltablet.
# Voraussetzung: Tablet zurückgesetzt, Amazon-/Google-Anmeldung übersprungen,
# WLAN verbunden, ADB-Debugging an, USB angeschlossen, adb im PATH.
param([string]$Apk = "bts-light-tablet.apk")

$ErrorActionPreference = "Stop"
$Paket = "de.badhub.btslight.tablet"

Write-Host "Geräte:"; adb devices
$geraete = (adb devices | Select-String "device$").Count
if ($geraete -ne 1) { Write-Error "Genau ein Tablet per USB anschließen (gefunden: $geraete)."; exit 1 }

# Device Owner geht nur ohne eingerichtete Konten.
$konten = adb shell dumpsys account | Select-String "Account \{"
if ($konten) {
  Write-Error "Auf dem Tablet ist noch ein Konto eingerichtet (Amazon?). Tablet zurücksetzen, Anmeldung überspringen, erneut starten."
  exit 1
}

Write-Host "APK installieren: $Apk"
adb install -r $Apk

Write-Host "Gerätebesitzer setzen"
adb shell dpm set-device-owner "$Paket/.kiosk.KioskAdminReceiver"

Write-Host "WebView-Stand (für die Doku/Fehlersuche):"
foreach ($p in "com.amazon.webview.chromium", "com.google.android.webview", "com.android.webview") {
  $v = adb shell dumpsys package $p | Select-String "versionName"
  if ($v) { Write-Host "  $p $v" }
}

Write-Host "App starten"
adb shell am start -n "$Paket/.KioskActivity"
Write-Host "Fertig. Am Tablet jetzt die Kiosk-PIN vergeben."
```

- [ ] **Step 2: Bash-Variante**

```bash
#!/usr/bin/env bash
# Einmalige Einrichtung eines Fire-/Android-Tablets als bts-light-Zähltablet.
# Voraussetzung: Tablet zurückgesetzt, Anmeldung übersprungen, WLAN verbunden,
# ADB-Debugging an, USB angeschlossen, adb im PATH.
set -euo pipefail
APK="${1:-bts-light-tablet.apk}"
PAKET="de.badhub.btslight.tablet"

echo "Geräte:"; adb devices
n=$(adb devices | grep -c 'device$' || true)
[ "$n" -eq 1 ] || { echo "Genau ein Tablet per USB anschließen (gefunden: $n)." >&2; exit 1; }

if adb shell dumpsys account | grep -q 'Account {'; then
  echo "Auf dem Tablet ist noch ein Konto eingerichtet (Amazon?). Zurücksetzen, Anmeldung überspringen, erneut starten." >&2
  exit 1
fi

echo "APK installieren: $APK"; adb install -r "$APK"
echo "Gerätebesitzer setzen"; adb shell dpm set-device-owner "$PAKET/.kiosk.KioskAdminReceiver"
echo "WebView-Stand:"
for p in com.amazon.webview.chromium com.google.android.webview com.android.webview; do
  adb shell dumpsys package "$p" 2>/dev/null | grep versionName | sed "s/^/  $p /" || true
done
echo "App starten"; adb shell am start -n "$PAKET/.KioskActivity"
echo "Fertig. Am Tablet jetzt die Kiosk-PIN vergeben."
```

- [ ] **Step 3: Syntax prüfen**

Run: `bash -n android/setup-tablet.sh && powershell -NoProfile -Command "[scriptblock]::Create((Get-Content -Raw android/setup-tablet.ps1)) | Out-Null; 'ok'"`
Erwartet: `ok`, keine Fehler.

- [ ] **Step 4: Commit**

```bash
git add android/setup-tablet.ps1 android/setup-tablet.sh
git commit -m "android: Einrichtungsskripte (APK, Gerätebesitzer, WebView-Stand)"
```

---

### Task 10: CI und Release-Workflow

**Files:**
- Modify: `.github/workflows/ci.yml` (neuer Job `android` nach `build`), `.github/workflows/release.yml` (neuer Job `android`, `publish` erweitert)

- [ ] **Step 1: CI-Job**

An `ci.yml` unter `jobs:` anhängen (Einrückung wie `build`):
```yaml
  # Android-Kiosk-App: Unit-Tests auf der JVM + Debug-APK als Bau-Probe.
  # Kein Emulator; Android-Verhalten steht auf der Feldtest-Liste der Spec.
  android:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: actions/setup-java@v4
        with:
          distribution: temurin
          java-version: 17
      - uses: android-actions/setup-android@v3
      - uses: gradle/actions/setup-gradle@v4
      - name: Tests + Debug-APK
        working-directory: android
        run: |
          chmod +x gradlew
          ./gradlew --no-daemon :app:testDebugUnitTest :app:assembleDebug
```

- [ ] **Step 2: Release-Job**

In `release.yml` nach dem Job `build` einfügen:
```yaml
  # Tablet-APK. Mit Keystore-Secret signiert (Release), sonst Debug-APK —
  # dann müssen Tablets beim späteren Umstieg auf die signierte Variante
  # einmal deinstalliert werden (andere Signatur).
  android:
    needs: build
    if: startsWith(github.ref, 'refs/tags/')
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v7
      - uses: actions/setup-java@v4
        with:
          distribution: temurin
          java-version: 17
      - uses: android-actions/setup-android@v3
      - uses: gradle/actions/setup-gradle@v4
      - name: APK bauen
        working-directory: android
        env:
          KEYSTORE_B64: ${{ secrets.ANDROID_KEYSTORE_B64 }}
          ANDROID_KEYSTORE_PASS: ${{ secrets.ANDROID_KEYSTORE_PASS }}
        run: |
          set -euo pipefail
          chmod +x gradlew
          VERSION=$(jq -r .version ../package.json)
          if [ -n "${KEYSTORE_B64}" ]; then
            echo "${KEYSTORE_B64}" | base64 -d > release.keystore
            export ANDROID_KEYSTORE_PATH="$PWD/release.keystore"
            ./gradlew --no-daemon :app:assembleRelease
            cp app/build/outputs/apk/release/app-release.apk "bts-light-tablet-${VERSION}.apk"
          else
            echo "::warning::ANDROID_KEYSTORE_B64 fehlt — Debug-APK statt signierter Release-APK"
            ./gradlew --no-daemon :app:assembleDebug
            cp app/build/outputs/apk/debug/app-debug.apk "bts-light-tablet-${VERSION}-debug.apk"
          fi
          ls -la bts-light-tablet-*.apk
      - name: APK ans GitHub-Release hängen
        working-directory: android
        env:
          GH_TOKEN: ${{ github.token }}
        run: gh release upload "${GITHUB_REF_NAME}" bts-light-tablet-*.apk --clobber --repo "${GITHUB_REPOSITORY}"
```

`publish` anpassen: `needs: build` → `needs: [build, android]`. Im Schritt „Nach badhub.de hochladen" vor dem `rsync` die feste Kopie und die APK aufnehmen:
```bash
          APK=$(cd rel && ls bts-light-tablet-*.apk 2>/dev/null | head -1 || true)
          if [ -n "${APK}" ]; then cp "rel/${APK}" "rel/bts-light-tablet.apk"; fi
          rsync -av \
            rel/latest.json \
            rel/index.html \
            rel/*-setup.exe \
            $( [ -n "${APK}" ] && echo rel/*.apk ) \
            bts-deploy@178.104.221.177:/var/www/badhub/public/download/bts-light/
```
Der Schritt „Release-Assets holen" lädt mit `gh release download` bereits alle Assets, die APK ist also in `rel/`.

- [ ] **Step 3: YAML prüfen**

Run: `node -e "const y=require('js-yaml');for(const f of ['.github/workflows/ci.yml','.github/workflows/release.yml']){y.load(require('fs').readFileSync(f,'utf8'));console.log(f,'ok')}"` (falls `js-yaml` fehlt: `npx --yes js-yaml .github/workflows/ci.yml >/dev/null && echo ok`).
Erwartet: beide `ok`. **Kein Doppelpunkt in Schrittnamen** (Memory „Kombi-Ausrichtung": ein Doppelpunkt im Schrittnamen killt die CI).

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml .github/workflows/release.yml
git commit -m "ci/release: Android-Kiosk-App testen, bauen und als APK veröffentlichen"
```

---

### Task 11: Dokumentation

**Files:**
- Create: `docs/tablet-android-app.md`
- Modify: `docs/handbuch.json` (Gruppe „Am Feld"), `docs/tablet-kiosk.md`, `docs/tablet.md` (Abschnitt „Voraussetzungen"), `docs/logging.md` (Abschnitt „Geräte-Logs"), `docs/release.md` (Abschnitte „Benötigte GitHub-Secrets" und „Stabiler Download-Link"), `CLAUDE.md` (Tabelle), `docs/changelog.md`, `docs/roadmap.md`, `docs/features/tablet-android-kiosk-app.md` (Status)

- [ ] **Step 1: `docs/tablet-android-app.md`** anlegen mit diesen Abschnitten (Text ausformulieren, kein Stichwort-Skelett):

```markdown
# Zähl-Tablet als Kiosk-App (Android / Fire-Tablets)

> Spec: [features/tablet-android-kiosk-app.md](features/tablet-android-kiosk-app.md) · ADR 0058

Die App „bts-light Tablet" macht aus einem Fire-Tablet ein Zähl-Tablet, das
beim Einschalten von selbst den Turnier-PC im Hallen-WLAN findet und die
Felder-Lobby im Vollbild zeigt. Sie ersetzt Fully Kiosk für das Verleih-Set.

## Was die App tut
- Sucht den Turnier-PC wie die Pi-Monitore: gemerkte IP → Subnetz-Scan auf
  Port 8088 → mDNS. Nur beim Start, bei WLAN-Wechsel, bei Ladefehlern und
  auf Knopfdruck — nie im Hintergrund.
- Lädt `http://<PC-IP>:8088/felder`, die Felder-Lobby. Feld antippen, zählen.
- Sperrt das Tablet als Gerätebesitzer: keine Android-Tasten, keine
  Statusleiste, keine anderen Apps, kein Internet. Bildschirm bleibt an.
- Meldet den Akku wie Fully Kiosk und schickt ihr Log an den Turnier-PC
  (`pi-logs/fire-….log`, weiter in die Cloud).

## Einrichten (einmalig je Tablet, ca. 5 Minuten)
1. Tablet auf Werkseinstellungen zurücksetzen. Beim Einrichten die Amazon-
   Anmeldung **überspringen** — mit angemeldetem Konto verweigert Android den
   Gerätebesitzer-Schritt.
2. Hallen-WLAN verbinden. Entwickleroptionen freischalten (Einstellungen →
   Geräteoptionen → Seriennummer 7× tippen), **ADB-Debugging** einschalten.
3. Tablet per USB an den PC. `adb` muss installiert sein (Android Platform
   Tools). APK von <https://badhub.de/download/bts-light/bts-light-tablet.apk>
   laden.
4. Im Repo-Ordner `android/`: `.\setup-tablet.ps1 -Apk bts-light-tablet.apk`
   (Windows) bzw. `./setup-tablet.sh bts-light-tablet.apk`. Das Skript prüft
   „keine Konten", installiert, setzt den Gerätebesitzer, startet die App.
5. Am Tablet die **Kiosk-PIN** (4–8 Ziffern) vergeben. Fertig — ab jetzt
   startet das Tablet immer direkt in die App.

## Bedienung
- **Wartekarte** „Suche Turnier-PC im WLAN …" mit Versuchszähler und Knopf
  „Erneut suchen". Erscheint, solange kein PC gefunden ist oder das WLAN fehlt.
- **Hüllen-Menü:** 2 Sekunden Finger in die **linke obere Ecke** → PIN →
  „Turnier-PC neu suchen", „Adresse von Hand eingeben", „PIN ändern",
  „Kiosk verlassen".
- Die Einstellungs-PIN der Zähl-Seite (Zahnrad, `tablet_settings_pin`) ist
  davon getrennt — zwei Ebenen wie in [tablet-kiosk.md](tablet-kiosk.md).
- **Turnier-PC bekommt mitten im Turnier eine neue IP:** Die Seite zeigt
  „Verbindung verloren", die App merkt es nicht von selbst → Menü → „neu suchen".

## Update der App
Kiosk per PIN verlassen → im Silk-Browser die APK von badhub.de laden →
installieren → App-Symbol antippen. Oder per USB: `adb install -r <apk>`.
Die Signatur muss gleich bleiben; eine Debug-APK lässt sich nicht über eine
signierte installieren (und umgekehrt) — dann vorher deinstallieren und den
Gerätebesitzer neu setzen.

## Ohne Gerätebesitzer
Wurde der ADB-Schritt übersprungen oder verweigert, läuft die App mit
„Bildschirm anheften": Android fragt einmal nach, der Ausstieg geht über die
Android-Geste. Die Wartekarte zeigt den Hinweis „Nicht als Gerätebesitzer
eingerichtet".

## Fehlersuche
- Wartekarte bleibt: WLAN-Name prüfen, eigene IP auf der Karte (fehlt sie,
  ist das Tablet nicht im Netz). Turnier-PC: Übertragung läuft? Firewall?
- Geräte-Log beim PC: `pi-logs/fire-<id>.log` („Logs öffnen"), in der Cloud
  unter derselben ID. Jede Suche steht mit Weg und Dauer drin.
- WebView-Stand: gibt das Einrichtungsskript aus.
```

- [ ] **Step 2: Bestehende Dokus**

- `docs/tablet-kiosk.md`: Abschnitt 2 umbenennen in „2) Kiosk-Sperre: eigene App (empfohlen) oder Fully Kiosk" und oben einen Absatz einfügen: *Für das Verleih-Set ist die eigene Kiosk-App (seit September 2026) der empfohlene Weg — sie findet den Turnier-PC selbst, braucht keine Start-URL und keine PLUS-Lizenz: [tablet-android-app.md](tablet-android-app.md). Fully Kiosk bleibt für fremde Geräte und iPads die Alternative.*
- `docs/tablet.md`, „Voraussetzungen": beim Punkt „Bildschirm-Schlaf ausschalten" ergänzen: *Mit der Kiosk-App entfällt das — sie hält den Bildschirm selbst wach.*
- `docs/logging.md`, „Geräte-Logs": Satz ergänzen, dass Fire-Tablets mit der Kiosk-App unter `fire-<ANDROID_ID>.log` neben den `pi-…`-Dateien liegen, Upload nach jeder Suche und alle 5 min.
- `docs/release.md`: unter „Benötigte GitHub-Secrets" zwei Zeilen `ANDROID_KEYSTORE_B64` (Base64 des Keystores, Alias `bts-light-tablet`) und `ANDROID_KEYSTORE_PASS`; Hinweis „nie wechseln — Sideload-Updates verlangen dieselbe Signatur"; Erzeugung: `keytool -genkeypair -v -keystore bts-light-tablet.keystore -alias bts-light-tablet -keyalg RSA -keysize 2048 -validity 10000`, dann `base64 -w0 bts-light-tablet.keystore`. Unter „Stabiler Download-Link" den festen Link `https://badhub.de/download/bts-light/bts-light-tablet.apk` aufnehmen.
- `CLAUDE.md`, Tabelle: neue Zeile `| **Tablet-Kiosk-App (Android)** (`android/` — `suche/` Subnetz/Scanner/ServerSuche, `kern/` Huelle/AdressFilter/LogPuffer/PinRegel/GeraeteId, `netz/` Sonde/mDNS/NetzBeobachter/LogUpload, `kiosk/` Kiosk/FullyBruecke/Receiver, `KioskActivity`, `setup-tablet.*`, Jobs `android` in `ci.yml`/`release.yml`) | `docs/tablet-android-app.md` (Bedienung) · `docs/features/tablet-android-kiosk-app.md` (Spec) · `docs/tablet-kiosk.md` · ADR 0058 |` und im Stack-Absatz einen Punkt „`android/` — Kotlin-Kiosk-App für die Zähl-Tablets (eigenes Gradle-Projekt, nicht Teil des Cargo-Workspace)".
- `docs/changelog.md`: Eintrag unter der nächsten Version (Nummer erst beim Merge): „**Tablet-Kiosk-App für Android / Fire-Tablets.** Eigene App findet den Turnier-PC selbst (gemerkte IP → Subnetz-Scan → mDNS), zeigt die Felder-Lobby im Vollbild, sperrt als Gerätebesitzer, meldet den Akku und schickt ihr Log an `/pi-log`. APK: `badhub.de/download/bts-light/bts-light-tablet.apk`. Spec `docs/features/tablet-android-kiosk-app.md`, ADR 0058."
- `docs/roadmap.md`: Eintrag von „Spezifiziert" nach „Umgesetzt, aber noch nicht abgenommen" verschieben, mit der Feldtest-Liste aus der Spec (Device Owner auf Fire OS, WebView-Stand, Lock-Task, Boot-Zeit, Scan-Dauer, Akku-Badge, Turniertag). Roadmap-Punkt „Anstoß der Suche aus `tablet.html` über die JS-Brücke" unter „Geplant" ergänzen.
- `docs/features/tablet-android-kiosk-app.md`: Status auf „**umgesetzt**" mit dem Datum des Merges, „Feldtest offen".

- [ ] **Step 3: Handbuch-Manifest und -Bau**

In `docs/handbuch.json`, Gruppe „Am Feld", direkt hinter dem Eintrag `docs/tablet-kiosk.md` einfügen:
```json
        { "datei": "docs/tablet-android-app.md", "slug": "tablet-android-app",
          "titel": "Zähl-Tablet als Kiosk-App (Android)" },
```
Run: `node scripts/test-handbuch.mjs`
Erwartet: fehlerfrei (der Test verlangt, dass jede neue Anleitung im Manifest steht).

- [ ] **Step 4: Commit**

```bash
git add docs CLAUDE.md
git commit -m "docs: Tablet-Kiosk-App — Bedienung, Einrichtung, Handbuch, Logging, Release-Secrets, Changelog"
```

---

### Task 12: Abschluss

- [ ] **Step 1: Alles grün** — `cd android && ./gradlew --no-daemon :app:testDebugUnitTest :app:assembleDebug`; im Repo-Root `cargo test --workspace` (unverändert, muss grün bleiben) und `npm run build`.
- [ ] **Step 2: code-reviewer-Subagent** über den gesamten Branch-Diff; `security-reviewer` über `AdressFilter`, `FullyBruecke` (JS-Schnittstelle), `KioskActivity` (Nutzer-Eingaben Adresse/PIN) und die Workflows.
- [ ] **Step 3: Branch pushen, PR anlegen** — Titel ohne Versionsnummer, Body per `--body-file` (Memory „Commit-Messages per Datei"). Im Body: Spec-Link, Feldtest-Liste, Hinweis auf die zwei anzulegenden Secrets.
- [ ] **Step 4: Beim Merge** die drei Versionsdateien gemeinsam bumpen (nächste freie Version), `docs/changelog.md` mit der Nummer versehen.
