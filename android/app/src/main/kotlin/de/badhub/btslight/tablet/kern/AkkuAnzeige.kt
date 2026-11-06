package de.badhub.btslight.tablet.kern

/**
 * Akkustand auf der Wartekarte (Spec `kiosk-akku-und-ausgang`): dieselben
 * Zeichen und Schwellen wie die Tablet-Seiten (`src/io/akku.mjs`) — sonst
 * sähe derselbe Akku am selben Gerät je nach Ansicht anders aus.
 * `scripts/test-kiosk-akku.mjs` prüft die Schwellen gegen das JS-Modul.
 */
object AkkuAnzeige {
    /** Unter diesem Wert orange … */
    const val NIEDRIG = 20
    /** … und darunter rot. Beides exklusiv: 20 % ist noch „ok". */
    const val KRITISCH = 10

    enum class Stufe { Ok, Niedrig, Kritisch }

    /** Prozent aus `EXTRA_LEVEL`/`EXTRA_SCALE`; `null`, wenn Android nichts Brauchbares meldet. */
    fun prozent(level: Int, scale: Int): Int? {
        if (level < 0 || scale <= 0) return null
        return Math.round(level * 100f / scale).coerceIn(0, 100)
    }

    /** „🔋 73 %" bzw. „⚡ 73 %" am Netz; `null` ohne Wert. */
    fun text(prozent: Int?, amNetz: Boolean): String? =
        prozent?.let { (if (amNetz) "⚡ " else "🔋 ") + it + " %" }

    fun stufe(prozent: Int?): Stufe? = when {
        prozent == null -> null
        prozent < KRITISCH -> Stufe.Kritisch
        prozent < NIEDRIG -> Stufe.Niedrig
        else -> Stufe.Ok
    }
}
