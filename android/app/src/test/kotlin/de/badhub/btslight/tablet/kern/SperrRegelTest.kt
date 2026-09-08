package de.badhub.btslight.tablet.kern

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SperrRegelTest {
    @Test
    fun besitzer_heftet_immer_an() {
        assertTrue(SperrRegel.anheften(besitzer = true, hersteller = "Amazon", touchGesperrt = true))
        assertTrue(SperrRegel.anheften(besitzer = true, hersteller = "samsung", touchGesperrt = false))
        assertTrue(SperrRegel.anheften(besitzer = true, hersteller = null, touchGesperrt = true))
    }

    @Test
    fun fire_os_heftet_an_solange_touch_nicht_gesperrt() {
        // Feldtest 08.09.2026: Auf einem sauberen Fire-Tablet heftet die App
        // an und der Touch geht. Nur die Kindersicherung „Touch-Funktion
        // deaktivieren" (secure toddler_mode_default_value=1) legt beim
        // Anheften den Toddler Mode darüber, der jeden Touch schluckt.
        assertTrue(SperrRegel.anheften(besitzer = false, hersteller = "Amazon", touchGesperrt = false))
        assertFalse(SperrRegel.anheften(besitzer = false, hersteller = "Amazon", touchGesperrt = true))
        assertFalse(SperrRegel.anheften(besitzer = false, hersteller = " AMAZON ", touchGesperrt = true))
    }

    @Test
    fun anderes_android_heftet_ohne_besitzer_immer_an() {
        // Der Toddler-Schalter ist Fire-OS-eigen; anderswo hat der Wert keine
        // Bedeutung und darf das Anheften nicht verhindern.
        assertTrue(SperrRegel.anheften(besitzer = false, hersteller = "samsung", touchGesperrt = true))
        assertTrue(SperrRegel.anheften(besitzer = false, hersteller = "Google", touchGesperrt = false))
        assertTrue(SperrRegel.anheften(besitzer = false, hersteller = "", touchGesperrt = true))
        assertTrue(SperrRegel.anheften(besitzer = false, hersteller = null, touchGesperrt = true))
    }

    @Test
    fun amazon_erkennung() {
        assertTrue(SperrRegel.istAmazon("Amazon"))
        assertTrue(SperrRegel.istAmazon("amazon"))
        assertFalse(SperrRegel.istAmazon("Amazonas Tablets GmbH"))
        assertFalse(SperrRegel.istAmazon(null))
        assertFalse(SperrRegel.istAmazon(""))
    }
}
