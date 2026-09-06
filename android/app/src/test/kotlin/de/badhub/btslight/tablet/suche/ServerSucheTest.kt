package de.badhub.btslight.tablet.suche

import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import java.util.Collections

class ServerSucheTest {
    private fun sonde(vararg antwortende: String, gefragt: MutableList<String> = mutableListOf()) =
        Sonde { ip -> gefragt.add(ip); ip in antwortende }

    @Test
    fun gemerkte_ip_zuerst_ohne_scan() = runTest {
        val gefragt = Collections.synchronizedList(mutableListOf<String>())
        val s = ServerSuche(sonde("192.168.16.100", gefragt = gefragt), Scanner(sonde("192.168.16.100", gefragt = gefragt))) { error("mDNS darf nicht laufen") }
        val e = s.ausfuehren(gemerkteIp = "192.168.16.100", eigeneIp = "192.168.16.42")
        assertEquals(Suchergebnis.Treffer("192.168.16.100", Quelle.GEMERKT), e)
        assertEquals(listOf("192.168.16.100"), gefragt)
    }

    @Test
    fun scan_findet_neue_ip_wenn_gemerkte_tot_ist() = runTest {
        val s = ServerSuche(sonde("192.168.16.7"), Scanner(sonde("192.168.16.7"))) { error("mDNS darf nach einem Scan-Treffer nicht laufen") }
        val e = s.ausfuehren(gemerkteIp = "192.168.16.100", eigeneIp = "192.168.16.42")
        assertEquals(Suchergebnis.Treffer("192.168.16.7", Quelle.SCAN), e)
    }

    @Test
    fun mdns_ist_der_letzte_rueckfall_und_wird_gegengeprueft() = runTest {
        // mDNS liefert eine Adresse aus einem anderen Subnetz (Bridge) —
        // gilt nur, wenn die Sonde sie bestätigt.
        val s = ServerSuche(sonde("10.0.5.5"), Scanner(sonde("10.0.5.5"))) { "10.0.5.5" }
        assertEquals(Suchergebnis.Treffer("10.0.5.5", Quelle.MDNS), s.ausfuehren(null, "192.168.16.42"))
    }

    @Test
    fun mdns_treffer_ohne_antwort_zaehlt_nicht() = runTest {
        val s = ServerSuche(sonde(), Scanner(sonde())) { "10.0.5.5" }
        assertEquals(Suchergebnis.Nichts, s.ausfuehren(null, "192.168.16.42"))
    }

    @Test
    fun ohne_eigene_ip_bleibt_nur_gemerkt_und_mdns() = runTest {
        val gefragt = Collections.synchronizedList(mutableListOf<String>())
        val s = ServerSuche(sonde(gefragt = gefragt), Scanner(sonde(gefragt = gefragt))) { null }
        assertEquals(Suchergebnis.Nichts, s.ausfuehren("192.168.16.100", null))
        assertEquals(listOf("192.168.16.100"), gefragt)
    }
}
