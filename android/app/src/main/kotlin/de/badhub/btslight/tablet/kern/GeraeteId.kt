package de.badhub.btslight.tablet.kern

/** Geräte-ID für /pi-log: `fire-<ANDROID_ID>`, stabil je Tablet über Turniere hinweg. */
object GeraeteId {
    fun aus(androidId: String?): String {
        val sauber = androidId?.filter { it.isLetterOrDigit() || it == '-' || it == '_' }?.take(32)
        return "fire-" + (sauber?.ifEmpty { null } ?: "unbekannt")
    }
}
