package de.badhub.btslight.tablet.kern

/**
 * Ob die Hülle den Bildschirm anheften darf (`startLockTask`).
 *
 * Als Gerätebesitzer immer — das ist die harte Kiosk-Sperre. Ohne Besitzer
 * bleibt Android das schwächere „Bildschirm anheften" — auch auf Fire OS,
 * **außer** die Kindersicherung hat „App fixieren → Touch-Funktion
 * deaktivieren" an (secure `toddler_mode_default_value = 1`): Dann legt
 * Fire OS beim Anheften seinen „Toddler Mode" über die App, ein Vollbild-
 * Fenster, das jeden Touch schluckt — die App wäre unbedienbar, auch die
 * Ecken-Geste käme nie an (Feldtest 08.09.2026, zwei Fire HD 10, in drei
 * Durchläufen eingegrenzt). Dort also lieber keine Sperre als eine, die das
 * Zählen verhindert. Auf anderen Herstellern hat der Wert keine Bedeutung.
 * Reine Regel ohne Android-Abhängigkeit; Hersteller (`Build.MANUFACTURER`)
 * und Schalter (`Settings.Secure`) liefert der Aufrufer.
 */
object SperrRegel {
    /** Name des Fire-OS-Schalters „Touch-Funktion deaktivieren" in `Settings.Secure`. */
    const val TODDLER_TOUCH_SCHALTER = "toddler_mode_default_value"

    fun istAmazon(hersteller: String?): Boolean =
        hersteller?.trim()?.equals("amazon", ignoreCase = true) == true

    fun anheften(besitzer: Boolean, hersteller: String?, touchGesperrt: Boolean): Boolean =
        besitzer || !istAmazon(hersteller) || !touchGesperrt
}
