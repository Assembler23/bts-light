package de.badhub.btslight.tablet.kern

/**
 * Erkennung des Android-Dialogs „App ist auf dem Bildschirm fixiert" (SystemUI
 * `ScreenPinningRequest`), den Android ohne Gerätebesitzer bei **jedem**
 * Fixieren zeigt — nach jedem Einschalten also ein Fingertipp „Verstanden",
 * bis der Bedienungshilfe-Dienst der App ihn übernimmt (Feldtest 08.09.2026:
 * es gibt kein stilles Fixieren ohne Besitzer, das ist AOSP-Verhalten).
 *
 * Reine Regel über die sichtbaren Texte eines Fensters: Nur wenn das Fenster
 * eindeutig der Fixier-Dialog ist, wird der Bestätigungsknopf benannt — nie
 * ein beliebiges „OK" in SystemUI. Amazons Zusatzkästchen „Touch-Funktion …
 * deaktivieren" bleibt unangetastet (es würde den Toddler Mode einschalten).
 */
object FixierDialog {
    /** Paket, dem der Dialog gehört. */
    const val SYSTEMUI = "com.android.systemui"

    /** Wortstücke, die den Fixier-Dialog kennzeichnen (DE/EN). */
    private val KENNZEICHEN = listOf("fixiert", "angeheftet", "pinned", "bildschirm fixieren", "screen pinning")

    /** AOSP-Kennung des Bestätigungsknopfs — sprachunabhängig, erstes Kriterium. */
    const val KNOPF_ID = "com.android.systemui:id/screen_pinning_ok_button"

    /** Beschriftungen des Bestätigungsknopfs (DE/EN), kleingeschrieben — Rückfall, falls Amazon die ID umbenennt. */
    private val BESTAETIGEN = listOf("verstanden", "got it")

    /** Beschriftungen, die NIE angetippt werden dürfen. */
    private val TABU = listOf("nein danke", "no thanks", "touch", "deaktivieren", "disable")

    private fun norm(t: CharSequence?): String = t?.toString()?.trim()?.lowercase() ?: ""

    fun istSystemUi(paket: CharSequence?): Boolean = paket?.toString() == SYSTEMUI

    /** Ist das Fenster mit diesen sichtbaren Texten der Fixier-Dialog? */
    fun istFixierDialog(texte: List<CharSequence?>): Boolean {
        val alle = texte.joinToString(" ") { norm(it) }
        return KENNZEICHEN.any { it in alle } && texte.any { istBestaetigung(it) }
    }

    /** Ist dieser Text die Beschriftung des Bestätigungsknopfs? */
    fun istBestaetigung(text: CharSequence?): Boolean {
        val n = norm(text)
        return n in BESTAETIGEN && TABU.none { it in n }
    }

    /** Ist das die View-Kennung des Bestätigungsknopfs? */
    fun istBestaetigungsId(id: CharSequence?): Boolean = id?.toString() == KNOPF_ID
}
