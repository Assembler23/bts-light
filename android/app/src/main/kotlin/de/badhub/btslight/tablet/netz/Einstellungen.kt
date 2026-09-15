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
