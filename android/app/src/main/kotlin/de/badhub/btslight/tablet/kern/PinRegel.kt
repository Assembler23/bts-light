package de.badhub.btslight.tablet.kern

/** Kiosk-PIN: 4–8 Ziffern. Bedienschutz, keine Sicherheitsgrenze. */
object PinRegel {
    fun gueltig(pin: String?): Boolean =
        pin != null && pin.length in 4..8 && pin.all { it.isDigit() }
}
