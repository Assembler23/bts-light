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
        // Userinfo hat in unseren Adressen nichts verloren — der Turnier-PC
        // kennt keine Anmeldung, ein `user@host` wäre nur ein Verwirr- oder
        // Umgehungsversuch (manche URL-Parser lesen den Host danach anders).
        if (u.userInfo != null) return false
        return u.scheme == "http" && u.host == host && u.port == port
    }
}
