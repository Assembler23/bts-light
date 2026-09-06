package de.badhub.btslight.tablet.suche

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class HealthAntwortTest {
    @Test
    fun nur_200_mit_json_objekt_ist_bts_light() {
        assertTrue(HealthAntwort.istTreffer(200, """{"courts":[]}"""))
        assertTrue(HealthAntwort.istTreffer(200, "  {\"ok\":true}\n"))
    }

    @Test
    fun fremde_dienste_auf_8088_fallen_durch() {
        // Ein Router-Webinterface oder ein anderer Dienst antwortet mit HTML
        // oder einem Fehlercode — beides darf nicht als Turnier-PC gelten.
        assertFalse(HealthAntwort.istTreffer(200, "<html><body>hi</body></html>"))
        assertFalse(HealthAntwort.istTreffer(200, ""))
        assertFalse(HealthAntwort.istTreffer(404, "{}"))
        assertFalse(HealthAntwort.istTreffer(200, "[1,2]"))
    }
}
