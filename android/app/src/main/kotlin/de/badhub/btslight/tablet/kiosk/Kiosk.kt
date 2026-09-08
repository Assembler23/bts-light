package de.badhub.btslight.tablet.kiosk

import android.app.Activity
import android.app.admin.DevicePolicyManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.os.Build
import android.provider.Settings
import android.view.WindowManager
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import de.badhub.btslight.tablet.kern.Entschlackung
import de.badhub.btslight.tablet.kern.SperrRegel

/**
 * Harte Sperre als Gerätebesitzer (einmalig per ADB gesetzt); ohne
 * Besitzer weiche Anheft-Sperre — Android fragt dann ggf. einmal nach.
 * Auf Fire OS entfällt das Anheften nur, wenn die Kindersicherung den
 * Touch angehefteter Apps sperrt (siehe `SperrRegel`).
 */
object Kiosk {
    private fun dpm(ctx: Context) = ctx.getSystemService(DevicePolicyManager::class.java)
    private fun admin(ctx: Context) = ComponentName(ctx, KioskAdminReceiver::class.java)

    fun istBesitzer(ctx: Context): Boolean = dpm(ctx).isDeviceOwnerApp(ctx.packageName)

    /** Einmalige Gerätebesitzer-Einstellungen; idempotent, bei jedem Start. */
    fun einrichten(a: Activity, log: (String) -> Unit) {
        if (!istBesitzer(a)) {
            // Welche Sperre es ohne Besitzer gibt, entscheidet erst `sperren`.
            log("Kiosk: kein Gerätebesitzer")
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
        // Im Hintergrund: Beim ersten Lauf schreibt jedes Verstecken synchron
        // die Paket-Einstellungen und schickt Broadcasts — zusammen bis ~1 s,
        // das soll den Aufbau der Wartekarte nicht verzögern. DPM-Aufrufe sind
        // thread-sicher, `LogPuffer.schreibe` ist synchronized.
        val pm = a.packageManager
        Thread({ entschlacken(pm, d, ad, log) }, "entschlacken").start()
    }

    /**
     * Amazon-Apps verstecken (`Entschlackung.ALLE`): Alexa & Co. kosten Akku,
     * der OTA-Dienst startet mitten im Turnier neu. Das Einrichtungsskript
     * schafft per `pm disable-user` nur die ungeschützten; ob der
     * Gerätebesitzer die „protected" Pakete (OTA, Sonderangebote) verstecken
     * darf, entscheidet Fire OS — AOSP prüft dieselbe Schutzliste auch beim
     * Verstecken, das Log sagt es je Paket. Idempotent bei jedem Start; ein
     * verweigertes Paket bricht nichts ab. Braucht `QUERY_ALL_PACKAGES`,
     * sonst sieht die App ab Android 11 die fremden Pakete gar nicht und
     * zählt alle als „nicht vorhanden".
     */
    private fun entschlacken(pm: PackageManager, d: DevicePolicyManager, ad: ComponentName, log: (String) -> Unit) {
        var versteckt = 0; var verweigert = 0; var fehlt = 0
        for (p in Entschlackung.ALLE) {
            // MATCH_UNINSTALLED_PACKAGES: ein schon verstecktes Paket gilt sonst
            // als nicht vorhanden — und `isApplicationHidden` sagt für ein
            // fehlendes Paket „versteckt", deshalb zuerst die Existenz prüfen.
            val da = runCatching { pm.getPackageInfo(p, PackageManager.MATCH_UNINSTALLED_PACKAGES) }.isSuccess
            if (!da) { fehlt++; continue }
            if (runCatching { d.isApplicationHidden(ad, p) }.getOrDefault(false)) { versteckt++; continue }
            val ok = runCatching { d.setApplicationHidden(ad, p, true) }.getOrDefault(false)
            if (ok) versteckt++ else { verweigert++; log("Entschlacken: verweigert $p") }
        }
        log("Entschlacken: $versteckt versteckt, $verweigert verweigert, $fehlt nicht vorhanden")
    }

    /** Ergebnis von [sperren] — die Wartekarte formuliert je Fall einen anderen Hinweis. */
    enum class Sperre { Angeheftet, TouchGesperrt, Fehlgeschlagen }

    /**
     * Vollbild + Wachhalten + Lock-Task. Ohne Gerätebesitzer auf Fire OS wird
     * NICHT angeheftet, wenn die Kindersicherung „Touch-Funktion deaktivieren"
     * an hat (`SperrRegel`): Amazons „Toddler Mode" würde sonst jeden Touch
     * schlucken. Der Schalter ist ein Secure-Setting, ohne Berechtigung lesbar.
     */
    fun sperren(a: Activity, log: (String) -> Unit): Sperre {
        a.window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        WindowCompat.setDecorFitsSystemWindows(a.window, false)
        WindowInsetsControllerCompat(a.window, a.window.decorView).apply {
            hide(WindowInsetsCompat.Type.systemBars())
            systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        }
        val besitzer = istBesitzer(a)
        val touchGesperrt = runCatching {
            Settings.Secure.getInt(a.contentResolver, SperrRegel.TODDLER_TOUCH_SCHALTER, 0) == 1
        }.getOrDefault(false)
        if (!SperrRegel.anheften(besitzer, Build.MANUFACTURER, touchGesperrt)) {
            log("Kiosk: kein Anheften — Kindersicherung 'Touch-Funktion deaktivieren' ist an (${SperrRegel.TODDLER_TOUCH_SCHALTER}=1)")
            return Sperre.TouchGesperrt
        }
        return runCatching { a.startLockTask() }
            .onSuccess { log("Kiosk: Lock-Task aktiv (Besitzer=$besitzer, Hersteller=${Build.MANUFACTURER})") }
            .onFailure { log("Kiosk: Lock-Task fehlgeschlagen: ${it.message}") }
            .fold({ Sperre.Angeheftet }, { Sperre.Fehlgeschlagen })
    }

    fun verlassen(a: Activity, log: (String) -> Unit) {
        // Sonst würde Home unsere App als bevorzugte Home-Activity sofort wieder
        // starten; `einrichten` setzt die Vorgabe beim nächsten Start neu.
        val besitzer = istBesitzer(a)
        var homeVorgabeGeraeumt = false
        if (besitzer) {
            dpm(a).clearPackagePersistentPreferredActivities(admin(a), a.packageName)
            homeVorgabeGeraeumt = true
        }
        log("Kiosk: verlassen (Besitzer=$besitzer, Home-Vorgabe geräumt=$homeVorgabeGeraeumt)")
        runCatching { a.stopLockTask() }
            .onFailure { log("Kiosk: stopLockTask fehlgeschlagen: ${it.message}") }
        a.finishAffinity()
    }
}
