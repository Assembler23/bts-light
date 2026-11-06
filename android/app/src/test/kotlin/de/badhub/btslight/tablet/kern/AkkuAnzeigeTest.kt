package de.badhub.btslight.tablet.kern

import de.badhub.btslight.tablet.kern.AkkuAnzeige.Stufe
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AkkuAnzeigeTest {
    @Test
    fun prozent_aus_level_und_scale() {
        assertEquals(73, AkkuAnzeige.prozent(73, 100))
        assertEquals(50, AkkuAnzeige.prozent(128, 256))
        assertEquals(100, AkkuAnzeige.prozent(120, 100))
    }

    @Test
    fun unbekannte_werte_zeigen_nichts() {
        assertNull(AkkuAnzeige.prozent(-1, 100))
        assertNull(AkkuAnzeige.prozent(50, 0))
        assertNull(AkkuAnzeige.text(null, true))
        assertNull(AkkuAnzeige.stufe(null))
    }

    @Test
    fun text_wie_auf_den_tablet_seiten() {
        assertEquals("🔋 73 %", AkkuAnzeige.text(73, false))
        assertEquals("⚡ 73 %", AkkuAnzeige.text(73, true))
    }

    @Test
    fun schwellen_sind_exklusiv() {
        assertEquals(Stufe.Ok, AkkuAnzeige.stufe(20))
        assertEquals(Stufe.Niedrig, AkkuAnzeige.stufe(19))
        assertEquals(Stufe.Niedrig, AkkuAnzeige.stufe(10))
        assertEquals(Stufe.Kritisch, AkkuAnzeige.stufe(9))
        assertEquals(Stufe.Kritisch, AkkuAnzeige.stufe(0))
    }
}
