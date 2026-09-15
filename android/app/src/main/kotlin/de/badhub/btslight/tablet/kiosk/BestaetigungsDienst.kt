package de.badhub.btslight.tablet.kiosk

import android.accessibilityservice.AccessibilityService
import android.os.SystemClock
import android.util.Log
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import de.badhub.btslight.tablet.kern.FixierDialog

/**
 * Bedienungshilfe-Dienst, der den Android-Dialog „App ist auf dem Bildschirm
 * fixiert" selbst mit „Verstanden" bestätigt. Ohne Gerätebesitzer zeigt
 * Android ihn bei jedem Fixieren — nach jedem Einschalten wäre sonst ein
 * Fingertipp nötig. Der Dienst sieht nur SystemUI-Fenster, entscheidet über
 * die reine Regel `FixierDialog` und tippt ausschließlich den Bestätigungs-
 * knopf; Amazons Kästchen „Touch-Funktion deaktivieren" rührt er nie an.
 *
 * Einschalten per adb (das Skript tut es): `settings put secure
 * enabled_accessibility_services de.badhub.btslight.tablet/.kiosk.BestaetigungsDienst`
 * + `settings put secure accessibility_enabled 1`.
 */
class BestaetigungsDienst : AccessibilityService() {
    private var zuletztGetipptMs = 0L
    private var zuletztFensterMs = 0L

    override fun onAccessibilityEvent(event: AccessibilityEvent) {
        if (!FixierDialog.istSystemUi(event.packageName)) return
        val jetzt = SystemClock.elapsedRealtime()
        // Der Dienst läuft im App-Prozess auf dem UI-Thread; SystemUI feuert
        // Inhaltsänderungen ständig (Uhr, Benachrichtigungen). Durchsucht wird
        // nur nach einem Fensterwechsel und kurz danach (Review-Befund).
        if (event.eventType == AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED) zuletztFensterMs = jetzt
        else if (jetzt - zuletztFensterMs > 3_000) return
        // Entprellen: Der Dialog feuert mehrere Ereignisse, ein Tipp reicht.
        if (jetzt - zuletztGetipptMs < 1_500) return
        for (fenster in windows) {
            val wurzel = fenster.root ?: continue
            if (!FixierDialog.istSystemUi(wurzel.packageName)) continue
            val texte = ArrayList<CharSequence?>()
            val knoepfe = ArrayList<AccessibilityNodeInfo>()
            sammle(wurzel, texte, knoepfe)
            if (!FixierDialog.istFixierDialog(texte) && knoepfe.none { FixierDialog.istBestaetigungsId(it.viewIdResourceName) }) continue
            // View-Kennung zuerst (sprachunabhängig), Text als Rückfall.
            val knopf = knoepfe.firstOrNull { FixierDialog.istBestaetigungsId(it.viewIdResourceName) } ?: knoepfe.firstOrNull() ?: continue
            val ziel = klickbar(knopf) ?: continue
            if (ziel.performAction(AccessibilityNodeInfo.ACTION_CLICK)) {
                zuletztGetipptMs = jetzt
                Log.i(TAG, "Fixier-Dialog bestätigt")
                return
            }
        }
    }

    /** Alle Texte des Fensters einsammeln, Bestätigungsknöpfe (per Kennung oder Text) merken. */
    private fun sammle(knoten: AccessibilityNodeInfo, texte: MutableList<CharSequence?>, knoepfe: MutableList<AccessibilityNodeInfo>) {
        val t = knoten.text ?: knoten.contentDescription
        if (t != null) texte.add(t)
        if (FixierDialog.istBestaetigungsId(knoten.viewIdResourceName) || (t != null && FixierDialog.istBestaetigung(t))) knoepfe.add(knoten)
        for (i in 0 until knoten.childCount) {
            val kind = knoten.getChild(i) ?: continue
            sammle(kind, texte, knoepfe)
        }
    }

    /** Der Text sitzt manchmal in einem Kind; nach oben bis zum klickbaren Knoten. */
    private fun klickbar(knoten: AccessibilityNodeInfo): AccessibilityNodeInfo? {
        var k: AccessibilityNodeInfo? = knoten
        var tiefe = 0
        while (k != null && tiefe < 5) {
            if (k.isClickable) return k
            k = k.parent
            tiefe++
        }
        return null
    }

    override fun onInterrupt() = Unit

    private companion object {
        const val TAG = "BestaetigungsDienst"
    }
}
