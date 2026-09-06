package de.badhub.btslight.tablet.kern

/** Geräte-ID für /pi-log: `fire-<ANDROID_ID>`, stabil je Tablet über Turniere hinweg. Nur ASCII-Zeichen ([A-Za-z0-9_-]), genau wie der Server-Filter. */
object GeraeteId {
    fun aus(androidId: String?): String {
        val sauber = androidId?.filter { it in 'A'..'Z' || it in 'a'..'z' || it in '0'..'9' || it == '-' || it == '_' }?.take(32)
        return "fire-" + (sauber?.ifEmpty { null } ?: "unbekannt")
    }
}
