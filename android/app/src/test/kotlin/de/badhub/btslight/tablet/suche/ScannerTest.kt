package de.badhub.btslight.tablet.suche

import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Collections

class ScannerTest {
    private fun sondeMitTreffer(treffer: String?, gefragt: MutableList<String>) =
        Sonde { ip -> gefragt.add(ip); ip == treffer }

    @Test
    fun erster_treffer_gewinnt_und_beendet_den_scan() = runTest {
        val gefragt = Collections.synchronizedList(mutableListOf<String>())
        val scanner = Scanner(sondeMitTreffer("192.168.16.77", gefragt), blockgroesse = 30)
        val ip = scanner.finde(Subnetz.kandidaten("192.168.16.5"))
        assertEquals("192.168.16.77", ip)
        // .77 liegt im dritten Block (61–90); danach darf kein Block mehr laufen.
        assertTrue("zu viele Sonden: ${gefragt.size}", gefragt.size <= 90)
        assertTrue(gefragt.none { it.endsWith(".91") })
    }

    @Test
    fun ohne_treffer_werden_alle_kandidaten_gefragt_und_null_geliefert() = runTest {
        val gefragt = Collections.synchronizedList(mutableListOf<String>())
        val scanner = Scanner(sondeMitTreffer(null, gefragt))
        assertNull(scanner.finde(Subnetz.kandidaten("10.0.0.9")))
        assertEquals(254, gefragt.size)
    }

    @Test
    fun leere_kandidatenliste_ergibt_null() = runTest {
        assertNull(Scanner(Sonde { true }).finde(emptyList()))
    }
}
