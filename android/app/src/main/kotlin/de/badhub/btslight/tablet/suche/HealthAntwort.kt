package de.badhub.btslight.tablet.suche

/**
 * Prüft, ob eine Antwort von `:8088/health` wirklich von bts-light stammt.
 * Der Tablet-Server antwortet mit einem JSON-Objekt; ein fremder Dienst auf
 * demselben Port liefert HTML oder einen Fehlercode. Bewusst nur ein
 * struktureller Blick (kein Parser), damit die Klasse ohne Android läuft.
 */
object HealthAntwort {
    fun istTreffer(status: Int, body: String): Boolean {
        if (status != 200) return false
        val t = body.trim()
        return t.startsWith("{") && t.endsWith("}")
    }
}
