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
    // Nachfolger (resolveService mit Executor, hostAddresses) brauchen API 34 — minSdk ist 28.
    @Suppress("DEPRECATION")
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
                                    // Wettlauf mit dem Timeout: cont.isActive kann zwischen der Prüfung
                                    // und resume() durch Cancellation kippen. Ein zweites resume() nach
                                    // der Timeout-Cancellation wirft dann IllegalStateException — runCatching
                                    // schluckt das, das Ergebnis wird einfach verworfen, wie gewollt.
                                    if (ip != null && cont.isActive) runCatching { cont.resume(ip) }
                                }
                                override fun onResolveFailed(s: NsdServiceInfo, code: Int) {}
                            })
                        }
                        override fun onStartDiscoveryFailed(t: String, code: Int) {
                            if (cont.isActive) runCatching { cont.resume(null) }
                        }
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
