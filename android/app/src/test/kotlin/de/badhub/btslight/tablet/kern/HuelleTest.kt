package de.badhub.btslight.tablet.kern

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class HuelleTest {
    private val ip = "192.168.16.100"

    @Test
    fun start_zeigt_wartekarte_und_sucht_sofort() {
        val h = Huelle()
        assertEquals(listOf(Wirkung.Wartekarte, Wirkung.Suchen(0)), h.verarbeite(Ereignis.Start))
        assertEquals(Zustand.Suchen(1), h.zustand)
    }

    @Test
    fun treffer_merkt_ip_und_laedt_die_lobby() {
        val h = Huelle().apply { verarbeite(Ereignis.Start) }
        assertEquals(listOf(Wirkung.Merke(ip), Wirkung.LadeLobby(ip)), h.verarbeite(Ereignis.Gefunden(ip)))
        assertEquals(Zustand.Verbunden(ip), h.zustand)
    }

    @Test
    fun gleicher_treffer_im_verbundenen_zustand_laedt_nicht_neu() {
        // Ein Suchlauf nach WLAN-Wackler bestätigt dieselbe IP: die laufende
        // Zählung darf nicht durch ein Neuladen unterbrochen werden.
        val h = Huelle().apply { verarbeite(Ereignis.Start); verarbeite(Ereignis.Gefunden(ip)) }
        assertTrue(h.verarbeite(Ereignis.Gefunden(ip)).isEmpty())
    }

    @Test
    fun andere_ip_laedt_die_lobby_neu() {
        val h = Huelle().apply { verarbeite(Ereignis.Start); verarbeite(Ereignis.Gefunden(ip)) }
        assertEquals(listOf(Wirkung.Merke("192.168.16.7"), Wirkung.LadeLobby("192.168.16.7")), h.verarbeite(Ereignis.Gefunden("192.168.16.7")))
    }

    @Test
    fun ohne_treffer_kommt_die_naechste_runde_verzoegert() {
        val h = Huelle(rundeMs = 10_000).apply { verarbeite(Ereignis.Start) }
        assertEquals(listOf(Wirkung.Suchen(10_000)), h.verarbeite(Ereignis.NichtsGefunden))
        assertEquals(Zustand.Suchen(2), h.zustand)
    }

    @Test
    fun verbunden_vertraegt_zwei_fehlschlaege_und_kippt_beim_dritten() {
        val h = Huelle(fehltoleranz = 3, rundeMs = 10_000).apply { verarbeite(Ereignis.Start); verarbeite(Ereignis.Gefunden(ip)) }
        h.verarbeite(Ereignis.Ladefehler)
        assertEquals(listOf(Wirkung.Suchen(10_000)), h.verarbeite(Ereignis.NichtsGefunden))
        assertEquals(listOf(Wirkung.Suchen(10_000)), h.verarbeite(Ereignis.NichtsGefunden))
        assertEquals(Zustand.Verbunden(ip), h.zustand)
        assertEquals(listOf(Wirkung.Wartekarte, Wirkung.Suchen(10_000)), h.verarbeite(Ereignis.NichtsGefunden))
        assertEquals(Zustand.Suchen(1), h.zustand)
    }

    @Test
    fun wlan_weg_wartet_ohne_zu_suchen_bis_wlan_da() {
        val h = Huelle().apply { verarbeite(Ereignis.Start); verarbeite(Ereignis.Gefunden(ip)) }
        assertEquals(listOf(Wirkung.Wartekarte), h.verarbeite(Ereignis.WlanWeg))
        assertEquals(Zustand.Wartend, h.zustand)
        assertTrue(h.verarbeite(Ereignis.NichtsGefunden).isEmpty())
        assertEquals(listOf(Wirkung.Suchen(0)), h.verarbeite(Ereignis.WlanDa))
        assertEquals(Zustand.Suchen(1), h.zustand)
    }

    @Test
    fun wlan_da_im_verbundenen_zustand_prueft_still_nach() {
        // Netz kurz weg und wieder da: kein Wartekarten-Flackern, nur ein
        // Suchlauf; bestätigt er dieselbe IP, passiert nichts.
        val h = Huelle().apply { verarbeite(Ereignis.Start); verarbeite(Ereignis.Gefunden(ip)) }
        assertEquals(listOf(Wirkung.Suchen(0)), h.verarbeite(Ereignis.WlanDa))
        assertTrue(h.verarbeite(Ereignis.Gefunden(ip)).isEmpty())
    }

    @Test
    fun ladefehler_sucht_sofort_und_laedt_bei_bestaetigung_neu() {
        val h = Huelle().apply { verarbeite(Ereignis.Start); verarbeite(Ereignis.Gefunden(ip)) }
        assertEquals(listOf(Wirkung.Suchen(0)), h.verarbeite(Ereignis.Ladefehler))
        assertEquals(listOf(Wirkung.Merke(ip), Wirkung.LadeLobby(ip)), h.verarbeite(Ereignis.Gefunden(ip)))
    }

    @Test
    fun handgriff_erzwingt_suche_und_neuladen() {
        val h = Huelle().apply { verarbeite(Ereignis.Start); verarbeite(Ereignis.Gefunden(ip)) }
        assertEquals(listOf(Wirkung.Suchen(0)), h.verarbeite(Ereignis.Handgriff))
        assertEquals(listOf(Wirkung.Merke(ip), Wirkung.LadeLobby(ip)), h.verarbeite(Ereignis.Gefunden(ip)))
    }

    @Test
    fun handgriff_im_wartenden_zustand_startet_die_suche() {
        val h = Huelle().apply { verarbeite(Ereignis.WlanWeg) }
        assertEquals(listOf(Wirkung.Suchen(0)), h.verarbeite(Ereignis.Handgriff))
        assertEquals(Zustand.Suchen(1), h.zustand)
    }

    @Test
    fun handadresse_laedt_direkt_und_merkt() {
        val h = Huelle().apply { verarbeite(Ereignis.Start) }
        assertEquals(listOf(Wirkung.Merke("10.1.1.1"), Wirkung.LadeLobby("10.1.1.1")), h.verarbeite(Ereignis.HandAdresse("10.1.1.1")))
        assertEquals(Zustand.Verbunden("10.1.1.1"), h.zustand)
    }

    @Test
    fun verspaeteter_treffer_nach_wlan_verlust_wird_verworfen() {
        // Ein Suchlauf lief noch, als das WLAN wegging: sein Ergebnis darf
        // die Wartekarte nicht verdrängen — WlanDa startet ohnehin neu.
        val h = Huelle().apply { verarbeite(Ereignis.Start); verarbeite(Ereignis.WlanWeg) }
        assertTrue(h.verarbeite(Ereignis.Gefunden(ip)).isEmpty())
        assertEquals(Zustand.Wartend, h.zustand)
    }
}
