package de.badhub.btslight.tablet

import android.annotation.SuppressLint
import android.net.nsd.NsdManager
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.provider.Settings
import android.text.InputType
import android.view.MotionEvent
import android.view.View
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.Button
import android.widget.EditText
import android.widget.TextView
import android.widget.Toast
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import de.badhub.btslight.tablet.kern.AdressFilter
import de.badhub.btslight.tablet.kern.Ereignis
import de.badhub.btslight.tablet.kern.GeraeteId
import de.badhub.btslight.tablet.kern.Huelle
import de.badhub.btslight.tablet.kern.LogPuffer
import de.badhub.btslight.tablet.kern.PinRegel
import de.badhub.btslight.tablet.kern.Wirkung
import de.badhub.btslight.tablet.kern.Zustand
import de.badhub.btslight.tablet.kiosk.FullyBruecke
import de.badhub.btslight.tablet.kiosk.Kiosk
import de.badhub.btslight.tablet.netz.AndroidSonde
import de.badhub.btslight.tablet.netz.Einstellungen
import de.badhub.btslight.tablet.netz.LogUpload
import de.badhub.btslight.tablet.netz.MdnsSuche
import de.badhub.btslight.tablet.netz.NetzBeobachter
import de.badhub.btslight.tablet.suche.Scanner
import de.badhub.btslight.tablet.suche.ServerSuche
import de.badhub.btslight.tablet.suche.Suchergebnis
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Die Hülle: WebView + Wartekarte. Alle Entscheidungen trifft `Huelle`
 * (Zustandsmaschine, getestet); hier werden nur Ereignisse eingespeist und
 * Wirkungen ausgeführt.
 */
class KioskActivity : AppCompatActivity() {
    private lateinit var web: WebView
    private lateinit var wartekarte: View
    private lateinit var wartekarteTitel: TextView
    private lateinit var wartekarteDetail: TextView
    private lateinit var wartekarteHinweis: TextView
    private lateinit var einstellungen: Einstellungen
    private lateinit var suche: ServerSuche
    private lateinit var geraeteId: String

    private val huelle = Huelle()
    private val log = LogPuffer()
    private val upload = LogUpload(log)
    private var filter: AdressFilter? = null
    private var suchlauf: Job? = null
    private var uploadTakt: Job? = null
    private var netz: NetzBeobachter? = null
    private val handler = Handler(Looper.getMainLooper())

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_kiosk)
        einstellungen = Einstellungen(this)
        geraeteId = GeraeteId.aus(Settings.Secure.getString(contentResolver, Settings.Secure.ANDROID_ID))
        web = findViewById(R.id.web)
        wartekarte = findViewById(R.id.wartekarte)
        wartekarteTitel = findViewById(R.id.wartekarte_titel)
        wartekarteDetail = findViewById(R.id.wartekarte_detail)
        wartekarteHinweis = findViewById(R.id.wartekarte_hinweis)
        findViewById<Button>(R.id.erneut).setOnClickListener { verarbeite(Ereignis.Handgriff) }

        log.schreibe("Start $geraeteId, Version ${BuildConfig.VERSION_NAME}")
        Kiosk.einrichten(this, log::schreibe)
        val sperre = Kiosk.sperren(this, log::schreibe)
        wartekarteHinweis.visibility = if (Kiosk.istBesitzer(this)) View.GONE else View.VISIBLE
        // Ohne Anheften ehrlich sagen, dass gar keine Sperre wirkt — und bei
        // gesperrtem Touch gleich den Schalter nennen, der es behebt.
        wartekarteHinweis.setText(when (sperre) {
            Kiosk.Sperre.Angeheftet -> R.string.kein_besitzer
            Kiosk.Sperre.TouchGesperrt -> R.string.kein_besitzer_touch_gesperrt
            Kiosk.Sperre.Fehlgeschlagen -> R.string.kein_besitzer_ohne_sperre
        })

        webEinrichten()

        val sonde = AndroidSonde()
        // Eine gebundene Referenz auf `MdnsSuche::finde` (Default-Parameter)
        // passt nicht auf `suspend () -> String?` (KSuspendFunction1<Long, ...>
        // statt KSuspendFunction0) — deshalb ein Lambda statt Methodenreferenz.
        val mdns = MdnsSuche(getSystemService(NsdManager::class.java))
        suche = ServerSuche(sonde, Scanner(sonde)) { mdns.finde() }
        netz = NetzBeobachter(this) { da ->
            log.schreibe(if (da) "WLAN da" else "WLAN weg")
            verarbeite(if (da) Ereignis.WlanDa else Ereignis.WlanWeg)
        }

        if (einstellungen.pin == null) pinFestlegen(erstStart = true) { verarbeite(Ereignis.Start) } else verarbeite(Ereignis.Start)
    }

    override fun onStart() {
        super.onStart()
        netz?.start()
    }

    override fun onStop() {
        netz?.stop()
        super.onStop()
    }

    // ---- Zustandsmaschine -------------------------------------------------

    /** Immer asynchron auf dem UI-Thread — nie inline, sonst sieht der Such-Wächter sich selbst. */
    private fun verarbeite(e: Ereignis) {
        handler.post {
            for (w in huelle.verarbeite(e)) fuehreAus(w)
            wartekarteAktualisieren()
        }
    }

    private fun fuehreAus(w: Wirkung) {
        when (w) {
            is Wirkung.Suchen -> starteSuche(w.verzoegerungMs)
            is Wirkung.LadeLobby -> {
                filter = AdressFilter(w.ip)
                log.schreibe("Lobby laden: ${w.ip}")
                web.loadUrl("http://${w.ip}:8088/felder")
                wartekarte.visibility = View.GONE
                uploadStarten(w.ip)
            }
            Wirkung.Wartekarte -> {
                uploadTakt?.cancel()
                wartekarte.visibility = View.VISIBLE
                // Die versteckte Seite darf keinen Gong spielen und keine Tipps bekommen.
                web.loadUrl("about:blank")
            }
            is Wirkung.Merke -> einstellungen.gemerkteIp = w.ip
        }
    }

    /** Läuft gerade wirklich eine Suche (nicht nur die Wartezeit davor)? */
    private var sucheAktiv = false

    /**
     * Ein echter Suchlauf zur Zeit. Ein sofortiger Anstoß (Ladefehler, Handgriff)
     * bricht eine noch schlafende Runde ab, statt zu verpuffen — sonst wirkt
     * „Erneut suchen" bis zu 10 s lang tot.
     */
    private fun starteSuche(verzoegerungMs: Long) {
        if (sucheAktiv) { log.schreibe("Suche: Anstoß verworfen, Lauf aktiv"); return }
        if (verzoegerungMs == 0L) suchlauf?.cancel()
        else if (suchlauf?.isActive == true) return
        suchlauf = lifecycleScope.launch {
            delay(verzoegerungMs)
            sucheAktiv = true
            val t0 = System.currentTimeMillis()
            val erg = try {
                withContext(Dispatchers.Default) {
                    suche.ausfuehren(einstellungen.gemerkteIp, NetzBeobachter.eigeneIpv4(this@KioskActivity))
                }
            } finally {
                sucheAktiv = false
            }
            log.schreibe("Suche: $erg (${System.currentTimeMillis() - t0} ms)")
            when (erg) {
                is Suchergebnis.Treffer -> verarbeite(Ereignis.Gefunden(erg.ip))
                Suchergebnis.Nichts -> verarbeite(Ereignis.NichtsGefunden)
            }
        }
    }

    private fun wartekarteAktualisieren() {
        when (val z = huelle.zustand) {
            Zustand.Wartend -> {
                wartekarteTitel.setText(R.string.warte_titel)
                wartekarteDetail.text = ""
            }
            is Zustand.Suchen -> {
                wartekarteTitel.setText(R.string.suche_titel)
                wartekarteDetail.text = "Versuch ${z.versuch} · eigene IP ${NetzBeobachter.eigeneIpv4(this) ?: "–"} · zuletzt ${einstellungen.gemerkteIp ?: "–"}"
            }
            is Zustand.Verbunden -> Unit
        }
    }

    /** Einmal nach jeder erfolgreichen Suche, dann alle 5 min — solange verbunden. */
    private fun uploadStarten(ip: String) {
        uploadTakt?.cancel()
        uploadTakt = lifecycleScope.launch {
            while (isActive) {
                upload.sende(ip, geraeteId)
                delay(5 * 60 * 1000L)
            }
        }
    }

    // ---- WebView ------------------------------------------------------------

    @SuppressLint("SetJavaScriptEnabled")
    private fun webEinrichten() {
        web.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true            // localStorage: Spielstand, Geräte-ID der Seite
            mediaPlaybackRequiresUserGesture = false // Gong ohne Fingertipp
            cacheMode = android.webkit.WebSettings.LOAD_DEFAULT
            // Die Seite kommt immer von http:// vom Tablet-Server — Datei- und
            // Content-Provider-Zugriff braucht sie nie (kleinere Angriffsfläche).
            allowFileAccess = false
            allowContentAccess = false
        }
        web.addJavascriptInterface(FullyBruecke(this), "fully")
        web.webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                val ok = filter?.erlaubt(request.url.toString()) ?: false
                if (!ok) log.schreibe("Adresse verworfen: ${request.url}")
                return !ok
            }

            override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceError) {
                if (!request.isForMainFrame) return
                log.schreibe("Ladefehler ${error.errorCode} ${request.url}")
                verarbeite(Ereignis.Ladefehler)
            }
        }
    }

    // ---- Hüllen-Menü ----------------------------------------------------------

    /**
     * Hüllen-Geste: 2 s Fingerdruck links oben. Wird beim Verteilen der
     * Ereignisse beobachtet und NICHT verschluckt — die Zählseite hat in
     * derselben Ecke ihren „Letzten Punkt zurück"-Knopf. Feuert der Timer,
     * bekommt die Seite ein ACTION_CANCEL, damit ihr Knopf nicht zusätzlich
     * auslöst. Rutscht der Finger aus der Ecke, ist die Geste vorbei.
     */
    override fun dispatchTouchEvent(ev: MotionEvent): Boolean {
        val ecke = (72 * resources.displayMetrics.density)
        val inEcke = ev.x < ecke && ev.y < ecke
        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN -> if (inEcke) {
                eckeGedrueckt = true
                handler.postDelayed(eckeAusloeser, 2000)
            }
            MotionEvent.ACTION_MOVE -> if (eckeGedrueckt && !inEcke) eckeAbbrechen()
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> eckeAbbrechen()
        }
        return super.dispatchTouchEvent(ev)
    }

    private var eckeGedrueckt = false
    private val eckeAusloeser = Runnable {
        eckeGedrueckt = false
        val jetzt = SystemClock.uptimeMillis()
        val cancel = MotionEvent.obtain(jetzt, jetzt, MotionEvent.ACTION_CANCEL, 0f, 0f, 0)
        web.dispatchTouchEvent(cancel)
        cancel.recycle()
        pinAbfragen { menuZeigen() }
    }

    private fun eckeAbbrechen() {
        eckeGedrueckt = false
        handler.removeCallbacks(eckeAusloeser)
    }

    private fun menuZeigen() {
        log.schreibe("Menü geöffnet")
        val eintraege = arrayOf(
            getString(R.string.menu_neu_suchen),
            getString(R.string.menu_adresse),
            getString(R.string.menu_pin),
            getString(R.string.menu_verlassen),
        )
        AlertDialog.Builder(this).setTitle(R.string.menu_titel).setItems(eintraege) { _, i ->
            when (i) {
                0 -> verarbeite(Ereignis.Handgriff)
                1 -> adresseAbfragen()
                2 -> pinFestlegen(erstStart = false) { }
                3 -> { log.schreibe("Kiosk verlassen"); Kiosk.verlassen(this, log::schreibe) }
            }
        }.setNegativeButton(R.string.abbrechen, null).show()
    }

    private fun adresseAbfragen() {
        val feld = EditText(this).apply { inputType = InputType.TYPE_CLASS_TEXT; hint = "192.168.16.100" }
        AlertDialog.Builder(this).setTitle(R.string.adresse_titel).setView(feld)
            .setPositiveButton(R.string.ok) { _, _ ->
                val ip = feld.text.toString().trim()
                if (ip.isNotEmpty()) verarbeite(Ereignis.HandAdresse(ip))
            }
            .setNegativeButton(R.string.abbrechen, null).show()
    }

    /**
     * Beim allerersten Start ist eine PIN Pflicht (kein Abbrechen, ungültige
     * Eingabe fragt erneut). Beim späteren „PIN ändern" bleibt die alte PIN
     * gültig — abbrechbar, ungültige Eingabe zeigt nur einen Hinweis.
     */
    private fun pinFestlegen(erstStart: Boolean, danach: () -> Unit) {
        val feld = EditText(this).apply { inputType = InputType.TYPE_CLASS_NUMBER or InputType.TYPE_NUMBER_VARIATION_PASSWORD }
        val dialog = AlertDialog.Builder(this).setTitle(R.string.pin_festlegen).setView(feld).setCancelable(!erstStart)
            .setPositiveButton(R.string.ok) { _, _ ->
                val pin = feld.text.toString()
                when {
                    PinRegel.gueltig(pin) -> { einstellungen.pin = pin; danach() }
                    erstStart -> pinFestlegen(erstStart, danach)
                    else -> Toast.makeText(this, R.string.pin_ungueltig, Toast.LENGTH_SHORT).show()
                }
            }
        if (!erstStart) dialog.setNegativeButton(R.string.abbrechen, null)
        dialog.show()
    }

    private fun pinAbfragen(danach: () -> Unit) {
        val feld = EditText(this).apply { inputType = InputType.TYPE_CLASS_NUMBER or InputType.TYPE_NUMBER_VARIATION_PASSWORD }
        AlertDialog.Builder(this).setTitle(R.string.pin_eingeben).setView(feld)
            .setPositiveButton(R.string.ok) { _, _ ->
                if (feld.text.toString() == einstellungen.pin) {
                    danach()
                } else {
                    log.schreibe("PIN falsch")
                    Toast.makeText(this, R.string.pin_falsch, Toast.LENGTH_SHORT).show()
                }
            }
            .setNegativeButton(R.string.abbrechen, null).show()
    }

    /** Zurück-Taste im Kiosk ist tot; ohne Lock-Task würde sie die App beenden. */
    @Deprecated("Deprecated in Java")
    override fun onBackPressed() = Unit
}
