package de.badhub.btslight.tablet.kern

/**
 * Geheime Geste fürs Hüllen-Menü (Spec `kiosk-akku-und-ausgang`): genau drei
 * Finger 2 s halten. Die Ecke links oben ist auf der Zählseite oft von
 * Bedienelementen belegt; drei Finger gehen überall, und die Zählseite kennt
 * keine Mehrfinger-Bedienung.
 *
 * Rein (ohne Android), damit testbar: Die Activity meldet nach jedem
 * Touch-Ereignis die Zahl der aufliegenden Finger, die Regel sagt, ob der
 * 2-s-Timer zu starten oder abzubrechen ist. Fingerbewegung zählt nicht —
 * drei Finger halten nie ganz still.
 */
class DreiFinger {
    enum class Aktion { Starten, Abbrechen, Nichts }

    private var laeuft = false
    /** Nach dem Auslösen erst wieder scharf, wenn alle Finger abgehoben sind —
     *  sonst öffnete ein weiter liegender Dreigriff das Menü ein zweites Mal. */
    private var verbraucht = false

    /** `finger` = aufliegende Finger NACH dem Ereignis (0 nach UP/CANCEL). */
    fun beruehrung(finger: Int): Aktion {
        if (finger == 0) verbraucht = false
        return when {
            finger == FINGER && !laeuft && !verbraucht -> { laeuft = true; Aktion.Starten }
            finger != FINGER && laeuft -> { laeuft = false; Aktion.Abbrechen }
            else -> Aktion.Nichts
        }
    }

    /** Der Timer hat ausgelöst. */
    fun ausgeloest() {
        laeuft = false
        verbraucht = true
    }

    companion object {
        const val FINGER = 3
        const val HALTEN_MS = 2000L
    }
}
