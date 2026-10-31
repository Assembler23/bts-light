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
