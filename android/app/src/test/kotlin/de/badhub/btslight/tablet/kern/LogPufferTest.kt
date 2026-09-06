package de.badhub.btslight.tablet.kern

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class LogPufferTest {
    @Test
    fun jede_zeile_traegt_einen_zeitstempel() {
        val p = LogPuffer(uhr = { "2026-09-06 10:00:00" })
        p.schreibe("Boot")
        assertEquals("2026-09-06 10:00:00 - Boot", p.inhalt())
    }

    @Test
    fun ringpuffer_behaelt_die_juengsten_800_zeilen() {
        val p = LogPuffer(maxZeilen = 800, uhr = { "t" })
        repeat(801) { p.schreibe("z$it") }
        val zeilen = p.inhalt().lines()
        assertEquals(800, zeilen.size)
        assertEquals("t - z1", zeilen.first())
        assertEquals("t - z800", zeilen.last())
    }

    @Test
    fun ueberlange_zeilen_werden_gekappt_damit_der_upload_unter_2_mb_bleibt() {
        val p = LogPuffer(maxZeichen = 1000, uhr = { "t" })
        p.schreibe("x".repeat(5000))
        assertTrue(p.inhalt().length <= 1000 + "t - ".length)
    }
}
