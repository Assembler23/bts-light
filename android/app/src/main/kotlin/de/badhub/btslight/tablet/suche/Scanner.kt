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
