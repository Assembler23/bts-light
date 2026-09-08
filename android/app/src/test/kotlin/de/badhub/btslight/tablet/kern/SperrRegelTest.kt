package de.badhub.btslight.tablet.kern

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SperrRegelTest {
    @Test
    fun besitzer_heftet_immer_an() {
        assertTrue(SperrRegel.anheften(besitzer = true, hersteller = "Amazon"))
        assertTrue(SperrRegel.anheften(besitzer = true, hersteller = "samsung"))
        assertTrue(SperrRegel.anheften(besitzer = true, hersteller = null))
    }

    @Test
    fun ohne_besitzer_kein_anheften_auf_fire_os() {
        // Fire OS legt beim Anheften seinen „Toddler Mode" über die App und
        // schluckt jeden Touch (Feldtest 08.09.2026, zwei Fire HD 10).
        assertFalse(SperrRegel.anheften(besitzer = false, hersteller = "Amazon"))
        assertFalse(SperrRegel.anheften(besitzer = false, hersteller = "amazon"))
        assertFalse(SperrRegel.anheften(besitzer = false, hersteller = " AMAZON "))
    }

    @Test
    fun ohne_besitzer_anheften_auf_anderem_android() {
        assertTrue(SperrRegel.anheften(besitzer = false, hersteller = "samsung"))
        assertTrue(SperrRegel.anheften(besitzer = false, hersteller = "Google"))
        assertTrue(SperrRegel.anheften(besitzer = false, hersteller = ""))
        assertTrue(SperrRegel.anheften(besitzer = false, hersteller = null))
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
