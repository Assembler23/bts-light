package de.badhub.btslight.tablet.suche

/** Kandidaten für den Subnetz-Scan — wie `scan_subnet` im Pi-Skript: das eigene /24. */
object Subnetz {
    fun kandidaten(eigeneIp: String?): List<String> {
        val teile = eigeneIp?.trim()?.split(".") ?: return emptyList()
        if (teile.size != 4) return emptyList()
        if (teile.any { it.toIntOrNull() !in 0..255 }) return emptyList()
        val praefix = teile.take(3).joinToString(".")
        return (1..254).map { "$praefix.$it" }
    }
}
