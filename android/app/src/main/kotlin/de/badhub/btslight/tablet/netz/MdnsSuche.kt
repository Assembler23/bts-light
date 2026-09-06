package de.badhub.btslight.tablet.netz

import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import kotlinx.coroutines.InternalCoroutinesApi
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeoutOrNull

/**
 * mDNS über NsdManager — nur Rückfall, hart begrenzt: über WLAN hing die
 * Auflösung bei den Pis minutenlang, deshalb darf sie hier NIE blockieren.
 */
class MdnsSuche(private val nsd: NsdManager) {
    // Nachfolger (resolveService mit Executor, hostAddresses) brauchen API 34 — minSdk ist 28.
    // tryResume/completeResume sind kotlinx-internes API (kein anderer Weg, die
    // Wettlauf-Prüfung atomar zu machen) — bewusst geöffnet, kein Compiler-Fehler.
    @Suppress("DEPRECATION")
    @OptIn(InternalCoroutinesApi::class)
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
                                    // und resume() durch Cancellation kippen — tryResume ist atomar.
                                    if (ip != null) cont.tryResume(ip)?.let { cont.completeResume(it) }
                                }
                                override fun onResolveFailed(s: NsdServiceInfo, code: Int) {}
                            })
                        }
                        override fun onStartDiscoveryFailed(t: String, code: Int) {
                            cont.tryResume(null)?.let { cont.completeResume(it) }
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
