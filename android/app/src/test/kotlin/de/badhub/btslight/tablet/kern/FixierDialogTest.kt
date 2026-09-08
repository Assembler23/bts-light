package de.badhub.btslight.tablet.kern

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class FixierDialogTest {
    private val fireOsDialog = listOf<CharSequence?>(
        "App ist auf dem Bildschirm fixiert",
        "Die App bleibt so lange auf dem Bildschirm fixiert, bis du die Fixierung aufhebst.",
        "Touch-Funktion für die fixierte App deaktivieren",
        "NEIN DANKE",
        "VERSTANDEN",
    )

    @Test
    fun erkennt_den_fire_os_dialog() {
        assertTrue(FixierDialog.istFixierDialog(fireOsDialog))
        assertTrue(FixierDialog.istFixierDialog(listOf("App is pinned", "Got it", "No thanks")))
    }

    @Test
    fun erkennt_fremde_dialoge_nicht() {
        // Ein beliebiges OK in SystemUI (z. B. USB-Debugging-Hinweis) ist kein Fixier-Dialog.
        assertFalse(FixierDialog.istFixierDialog(listOf("USB-Debugging zulassen?", "OK", "Abbrechen")))
        assertFalse(FixierDialog.istFixierDialog(listOf("App ist auf dem Bildschirm fixiert"))) // ohne Knopf
        assertFalse(FixierDialog.istFixierDialog(emptyList()))
        assertFalse(FixierDialog.istFixierDialog(listOf(null, "")))
    }

    @Test
    fun bestaetigungsknopf() {
        assertTrue(FixierDialog.istBestaetigung("VERSTANDEN"))
        assertTrue(FixierDialog.istBestaetigung(" Got it "))
        // „OK" bewusst nicht: zu breit für ein beliebiges SystemUI-Fenster (Review-Befund).
        assertFalse(FixierDialog.istBestaetigung("OK"))
        assertFalse(FixierDialog.istBestaetigung("NEIN DANKE"))
        assertFalse(FixierDialog.istBestaetigung("No thanks"))
        // Amazons Kästchen darf nie angetippt werden — es schaltet den Toddler Mode ein.
        assertFalse(FixierDialog.istBestaetigung("Touch-Funktion für die fixierte App deaktivieren"))
        assertFalse(FixierDialog.istBestaetigung(null))
        assertFalse(FixierDialog.istBestaetigung(""))
    }

    @Test
    fun view_kennung_des_knopfs() {
        assertTrue(FixierDialog.istBestaetigungsId("com.android.systemui:id/screen_pinning_ok_button"))
        assertFalse(FixierDialog.istBestaetigungsId("com.android.systemui:id/screen_pinning_cancel_button"))
        assertFalse(FixierDialog.istBestaetigungsId(null))
    }

    @Test
    fun nur_systemui() {
        assertTrue(FixierDialog.istSystemUi("com.android.systemui"))
        assertFalse(FixierDialog.istSystemUi("de.badhub.btslight.tablet"))
        assertFalse(FixierDialog.istSystemUi(null))
    }
}
