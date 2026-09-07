package de.badhub.btslight.tablet.kiosk

import android.app.Activity
import android.app.admin.DevicePolicyManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Build
import android.provider.Settings
import android.view.WindowManager
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import de.badhub.btslight.tablet.kern.SperrRegel

/**
 * Harte Sperre als Gerätebesitzer (einmalig per ADB gesetzt); ohne
 * Besitzer weiche Anheft-Sperre — Android fragt dann einmal nach — außer
 * auf Fire OS, wo gar nicht angeheftet wird (siehe `SperrRegel`).
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
    }

    /**
     * Vollbild + Wachhalten + Lock-Task. Liefert `true`, wenn angeheftet
     * wurde. Ohne Gerätebesitzer auf Fire OS wird bewusst NICHT angeheftet
     * (`SperrRegel`): Amazons „Toddler Mode" würde sonst jeden Touch schlucken.
     */
    fun sperren(a: Activity, log: (String) -> Unit): Boolean {
        a.window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        WindowCompat.setDecorFitsSystemWindows(a.window, false)
        WindowInsetsControllerCompat(a.window, a.window.decorView).apply {
            hide(WindowInsetsCompat.Type.systemBars())
            systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        }
        val besitzer = istBesitzer(a)
        if (!SperrRegel.anheften(besitzer, Build.MANUFACTURER)) {
            log("Kiosk: kein Anheften ohne Besitzer auf ${Build.MANUFACTURER} (Toddler Mode)")
            return false
        }
        return runCatching { a.startLockTask() }
            .onSuccess { log("Kiosk: Lock-Task aktiv (Besitzer=$besitzer)") }
            .onFailure { log("Kiosk: Lock-Task fehlgeschlagen: ${it.message}") }
            .isSuccess
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
