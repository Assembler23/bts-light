package de.badhub.btslight.tablet.kern

import de.badhub.btslight.tablet.kern.DreiFinger.Aktion
import org.junit.Assert.assertEquals
import org.junit.Test

class DreiFingerTest {
    @Test
    fun drei_finger_starten_den_timer_genau_einmal() {
        val g = DreiFinger()
        assertEquals(Aktion.Nichts, g.beruehrung(1))
        assertEquals(Aktion.Nichts, g.beruehrung(2))
        assertEquals(Aktion.Starten, g.beruehrung(3))
        // Bewegung: weiter drei Finger — kein zweiter Start.
        assertEquals(Aktion.Nichts, g.beruehrung(3))
    }

    @Test
    fun ein_abhebender_finger_bricht_ab() {
        val g = DreiFinger()
        g.beruehrung(3)
        assertEquals(Aktion.Abbrechen, g.beruehrung(2))
    }

    @Test
    fun ein_vierter_finger_bricht_ab() {
        val g = DreiFinger()
        g.beruehrung(3)
        assertEquals(Aktion.Abbrechen, g.beruehrung(4))
        // Zurück auf drei startet neu.
        assertEquals(Aktion.Starten, g.beruehrung(3))
    }

    @Test
    fun alle_finger_weg_bricht_ab() {
        val g = DreiFinger()
        g.beruehrung(3)
        assertEquals(Aktion.Abbrechen, g.beruehrung(0))
    }

    @Test
    fun nach_dem_ausloesen_erst_wieder_nach_dem_abheben() {
        val g = DreiFinger()
        g.beruehrung(3)
        g.ausgeloest()
        assertEquals(Aktion.Nichts, g.beruehrung(3))
        assertEquals(Aktion.Nichts, g.beruehrung(2))
        assertEquals(Aktion.Nichts, g.beruehrung(3))
        g.beruehrung(0)
        assertEquals(Aktion.Starten, g.beruehrung(3))
    }
}
