package de.badhub.btslight.tablet.kern

/**
 * Ob die Hülle den Bildschirm anheften darf (`startLockTask`).
 *
 * Als Gerätebesitzer immer — das ist die harte Kiosk-Sperre. Ohne Besitzer
 * bleibt Android das schwächere „Bildschirm anheften", **außer auf Fire OS**:
 * Amazon schaltet beim Anheften seinen „Toddler Mode" ein und legt ein
 * unsichtbares Vollbild-Fenster über die App, das jeden Touch schluckt — die
 * App ist dann unbedienbar, auch die Ecken-Geste kommt nie an (Feldtest
 * 08.09.2026 auf zwei Fire HD 10). Dort also lieber gar keine Sperre als eine,
 * die das Zählen verhindert. Reine Regel ohne Android-Abhängigkeit; den
 * Hersteller liefert der Aufrufer (`Build.MANUFACTURER`).
 */
object SperrRegel {
    fun istAmazon(hersteller: String?): Boolean =
        hersteller?.trim()?.equals("amazon", ignoreCase = true) == true

    fun anheften(besitzer: Boolean, hersteller: String?): Boolean =
        besitzer || !istAmazon(hersteller)
}
