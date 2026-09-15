package de.badhub.btslight.tablet.suche

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SubnetzTest {
    @Test
    fun liefert_alle_254_hosts_des_eigenen_24er_netzes() {
        val k = Subnetz.kandidaten("192.168.16.42")
        assertEquals(254, k.size)
        assertEquals("192.168.16.1", k.first())
        assertEquals("192.168.16.254", k.last())
    }

    @Test
    fun unbrauchbare_adresse_ergibt_keine_kandidaten() {
        // Ohne WLAN-Adresse darf der Scanner nicht ins Leere laufen.
        assertTrue(Subnetz.kandidaten(null).isEmpty())
        assertTrue(Subnetz.kandidaten("").isEmpty())
        assertTrue(Subnetz.kandidaten("fe80::1").isEmpty())
        assertTrue(Subnetz.kandidaten("192.168.300.1").isEmpty())
    }
}
