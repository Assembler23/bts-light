package de.badhub.btslight.tablet.kern

import java.time.LocalDateTime
import java.time.format.DateTimeFormatter

/** Ringpuffer für das Geräte-Log — Deckel wie `tail -n 800` beim Pi. */
class LogPuffer(
    private val maxZeilen: Int = 800,
    private val maxZeichen: Int = 1000,
    private val uhr: () -> String = { LocalDateTime.now().format(FORMAT) },
) {
    private val zeilen = ArrayDeque<String>()

    @Synchronized
    fun schreibe(text: String) {
        zeilen.addLast("${uhr()} - ${text.take(maxZeichen)}")
        while (zeilen.size > maxZeilen) zeilen.removeFirst()
    }

    @Synchronized
    fun inhalt(): String = zeilen.joinToString("\n")

    private companion object {
        val FORMAT: DateTimeFormatter = DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss")
    }
}
