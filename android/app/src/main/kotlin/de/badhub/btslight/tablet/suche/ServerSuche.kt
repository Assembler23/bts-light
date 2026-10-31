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
