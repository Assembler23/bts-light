package de.badhub.btslight.tablet.kern

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AdressFilterTest {
    private val f = AdressFilter("192.168.16.100")

    @Test
    fun nur_der_gefundene_host_auf_8088_ist_erlaubt() {
        assertTrue(f.erlaubt("http://192.168.16.100:8088/felder"))
        assertTrue(f.erlaubt("http://192.168.16.100:8088/court/H1F3?x=1"))
        assertTrue(f.erlaubt("about:blank"))
    }

    @Test
    fun alles_andere_wird_verworfen() {
        // Kein Internet, kein TLS-Port, kein fremdes Schema — das ersetzt den
        // Web-Filter von Fully PLUS.
        assertFalse(f.erlaubt("https://badhub.de/spieler/123/live"))
        assertFalse(f.erlaubt("http://192.168.16.100:8443/felder"))
        assertFalse(f.erlaubt("https://192.168.16.100:8443/felder"))
        assertFalse(f.erlaubt("http://192.168.16.101:8088/felder"))
        assertFalse(f.erlaubt("http://192.168.16.100/felder"))
        assertFalse(f.erlaubt("intent://scan/#Intent;scheme=zxing;end"))
        assertFalse(f.erlaubt("javascript:alert(1)"))
        assertFalse(f.erlaubt(null))
        assertFalse(f.erlaubt("kein url"))
    }
}
