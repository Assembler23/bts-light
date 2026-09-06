package de.badhub.btslight.tablet.kern

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PinRegelTest {
    @Test
    fun vier_bis_acht_ziffern() {
        assertTrue(PinRegel.gueltig("0000"))
        assertTrue(PinRegel.gueltig("12345678"))
        assertFalse(PinRegel.gueltig("123"))
        assertFalse(PinRegel.gueltig("123456789"))
        assertFalse(PinRegel.gueltig("12a4"))
        assertFalse(PinRegel.gueltig(""))
        assertFalse(PinRegel.gueltig(null))
    }
}
