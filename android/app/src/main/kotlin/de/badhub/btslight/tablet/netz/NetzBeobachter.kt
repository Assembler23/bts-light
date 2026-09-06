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
