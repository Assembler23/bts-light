package de.badhub.btslight.tablet.kern

import org.junit.Assert.assertEquals
import org.junit.Test

class GeraeteIdTest {
    @Test
    fun praefix_fire_und_nur_sichere_zeichen() {
        // /pi-log filtert selbst auf [A-Za-z0-9_-]; wir liefern das schon
        // sauber, damit die Datei beim PC den erwarteten Namen bekommt.
        assertEquals("fire-9774d56d682e549c", GeraeteId.aus("9774d56d682e549c"))
        assertEquals("fire-abc", GeraeteId.aus("a b/c"))
        assertEquals("fire-unbekannt", GeraeteId.aus(null))
        assertEquals("fire-unbekannt", GeraeteId.aus("///"))
        // Nur ASCII-Buchstaben, keine Unicode-Zeichen
        assertEquals("fire-abc", GeraeteId.aus("Äaбbc"))
    }
}
