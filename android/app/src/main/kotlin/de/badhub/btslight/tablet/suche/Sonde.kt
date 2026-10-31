package de.badhub.btslight.tablet.suche

/** Fragt eine Adresse, ob dort bts-light antwortet. Austauschbar für Tests. */
fun interface Sonde {
    suspend fun antwortet(ip: String): Boolean
}
