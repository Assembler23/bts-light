package de.badhub.btslight.tablet.kern

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class EntschlackungTest {
    @Test
    fun liste_ist_gefuellt_und_ohne_doppelte() {
        assertTrue(Entschlackung.ALLE.size >= 100)
        assertEquals(Entschlackung.ALLE.size, Entschlackung.ALLE.toSet().size)
        assertEquals(
            Entschlackung.ALEXA.size + Entschlackung.INHALTE.size + Entschlackung.HINTERGRUND.size + Entschlackung.UPDATES.size,
            Entschlackung.ALLE.size,
        )
    }

    @Test
    fun ota_ist_dabei() {
        assertTrue(Entschlackung.UPDATES.contains("com.amazon.device.software.ota"))
    }

    @Test
    fun tabu_pakete_stehen_nie_auf_der_liste() {
        // Browser, Appstore, WebView, Launcher, Konto-/Gerätedienste und die
        // eigene App dürfen nie versteckt werden — sonst ist das Tablet tot.
        for (t in Entschlackung.TABU) {
            assertFalse("Tabu-Paket $t steht auf der Liste", Entschlackung.ALLE.contains(t))
        }
    }

    @Test
    fun nur_plausible_paketnamen() {
        val muster = Regex("^[a-z][a-z0-9_]*(\\.[a-zA-Z][a-zA-Z0-9_]*)+$")
        for (p in Entschlackung.ALLE) assertTrue("kein Paketname: $p", muster.matches(p))
    }
}
