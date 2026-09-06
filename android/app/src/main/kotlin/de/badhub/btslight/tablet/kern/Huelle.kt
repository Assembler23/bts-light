package de.badhub.btslight.tablet.kern

sealed class Zustand {
    /** Kein WLAN — warten, nicht suchen. */
    object Wartend : Zustand() { override fun toString() = "Wartend" }
    data class Suchen(val versuch: Int) : Zustand()
    data class Verbunden(val ip: String) : Zustand()
}

sealed class Ereignis {
    object Start : Ereignis()
    object WlanDa : Ereignis()
    object WlanWeg : Ereignis()
    data class Gefunden(val ip: String) : Ereignis()
    object NichtsGefunden : Ereignis()
    /** Hauptrahmen der WebView konnte nicht laden. */
    object Ladefehler : Ereignis()
    /** „Turnier-PC neu suchen" aus Menü oder Wartekarte. */
    object Handgriff : Ereignis()
    /** „Adresse von Hand eingeben". */
    data class HandAdresse(val ip: String) : Ereignis()
}

sealed class Wirkung {
    /** Suchlauf starten, nach `verzoegerungMs` (0 = sofort). */
    data class Suchen(val verzoegerungMs: Long) : Wirkung()
    data class LadeLobby(val ip: String) : Wirkung()
    object Wartekarte : Wirkung() { override fun toString() = "Wartekarte" }
    data class Merke(val ip: String) : Wirkung()
}

/**
 * Zustandsmaschine der Hülle. Sie entscheidet, WANN gesucht wird — die Spec
 * verlangt „nur bei Bedarf": Start, WLAN-Ereignis, Ladefehler, Handgriff.
 * Im Zustand Verbunden gibt es ohne Ereignis keine Suche.
 *
 * Fehltoleranz wie beim Pi: ein Ausfall zählt erst nach `fehltoleranz`
 * erfolglosen Runden, damit ein WLAN-Wackler die Anzeige nicht wegwirft.
 */
class Huelle(private val fehltoleranz: Int = 3, private val rundeMs: Long = 10_000) {
    var zustand: Zustand = Zustand.Wartend
        private set
    private var fehlschlaege = 0
    /** Nach Ladefehler/Handgriff soll auch dieselbe IP neu geladen werden. */
    private var neuLaden = false

    fun verarbeite(e: Ereignis): List<Wirkung> = when (e) {
        Ereignis.Start -> {
            zustand = Zustand.Suchen(1)
            listOf(Wirkung.Wartekarte, Wirkung.Suchen(0))
        }
        Ereignis.WlanDa -> when (zustand) {
            is Zustand.Verbunden -> listOf(Wirkung.Suchen(0))
            else -> { zustand = Zustand.Suchen(1); listOf(Wirkung.Suchen(0)) }
        }
        Ereignis.WlanWeg -> {
            zustand = Zustand.Wartend
            fehlschlaege = 0
            listOf(Wirkung.Wartekarte)
        }
        is Ereignis.Gefunden -> {
            val vorher = zustand
            // Verspäteter Treffer nach WLAN-Verlust: WlanDa löst ohnehin eine neue Suche aus.
            if (vorher is Zustand.Wartend) {
                emptyList()
            } else {
                fehlschlaege = 0
                val gleich = vorher is Zustand.Verbunden && vorher.ip == e.ip
                zustand = Zustand.Verbunden(e.ip)
                if (gleich && !neuLaden) {
                    emptyList()
                } else {
                    neuLaden = false
                    listOf(Wirkung.Merke(e.ip), Wirkung.LadeLobby(e.ip))
                }
            }
        }
        Ereignis.NichtsGefunden -> when (val z = zustand) {
            Zustand.Wartend -> emptyList()
            is Zustand.Suchen -> {
                zustand = Zustand.Suchen(z.versuch + 1)
                listOf(Wirkung.Suchen(rundeMs))
            }
            is Zustand.Verbunden -> {
                fehlschlaege++
                if (fehlschlaege >= fehltoleranz) {
                    fehlschlaege = 0
                    zustand = Zustand.Suchen(1)
                    listOf(Wirkung.Wartekarte, Wirkung.Suchen(rundeMs))
                } else {
                    listOf(Wirkung.Suchen(rundeMs))
                }
            }
        }
        Ereignis.Ladefehler -> if (zustand is Zustand.Verbunden) {
            neuLaden = true
            listOf(Wirkung.Suchen(0))
        } else {
            emptyList()
        }
        Ereignis.Handgriff -> {
            neuLaden = true
            if (zustand !is Zustand.Verbunden) zustand = Zustand.Suchen(1)
            listOf(Wirkung.Suchen(0))
        }
        is Ereignis.HandAdresse -> {
            zustand = Zustand.Verbunden(e.ip)
            fehlschlaege = 0
            neuLaden = false
            listOf(Wirkung.Merke(e.ip), Wirkung.LadeLobby(e.ip))
        }
    }
}
