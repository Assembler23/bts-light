# Tablet-Kiosk-App für Android / Fire-Tablets — Spezifikation

> Status: **umgesetzt 2026-09-06**, Feldtest offen.
> Quelle: Nutzer-Wunsch vom 06.09.2026 („Beim Anmachen des Tablets soll er die
> korrekte IP raussuchen, die Anwendung im Vollbildmodus starten und die
> Feldauswahl bekommen"). Betroffen: neuer Ordner `android/`, CI/Release,
> Doku. Der Rust-Kern bleibt unverändert. ADR:
> [0058](../adr/0058-eigene-kiosk-app-statt-fully-kiosk.md).

## Kontext / Problem

Die Zähl-Tablets (Amazon-Fire-Tablets aus dem Verleih-Set) laufen heute im
Fully Kiosk Browser mit einer **fest eingetragenen Start-URL**
`http://<PC-IP>:8088/felder` ([tablet-kiosk.md](../tablet-kiosk.md)). Der
Turnier-PC hat aber in jeder Halle eine andere IP. Vor jedem Turnier muss
deshalb jemand an jedem Tablet die Adresse anpassen oder den QR-Code
scannen — genau die Handarbeit, die bts-light abschaffen soll. Die
Kiosk-Sperre (Buttons sperren, kein Internet, Exit-PIN) gibt es zudem nur in
der bezahlten PLUS-Variante von Fully.

Die Court-Monitore auf Raspberry Pi haben dasselbe Problem längst gelöst
(`pi/shared-startbrowser.sh`): gemerkte IP zuerst, dann Subnetz-Scan auf
`:8088/health`, mDNS nur als Rückfall, Chromium im Kiosk. Dieses Muster
bekommt jetzt eine Android-App.

## Zielbild & Erfolgskriterien

1. **Einschalten genügt.** Tablet an → App startet von selbst, findet den
   Turnier-PC im Hallen-WLAN ohne Eingabe und zeigt die **Felder-Lobby**
   (`/felder`) im Vollbild. Der Helfer tippt sein Feld an und zählt.
2. **Kein Weg raus.** Home, Zurück, Zuletzt-Liste, Statusleiste und andere
   Apps sind gesperrt; kein Link führt ins Internet. Verlassen nur per PIN.
3. **Bildschirm bleibt an**, unabhängig vom Browser-Wake-Lock (der über
   plain HTTP nicht verfügbar ist).
4. **Akku-Badge** in der bts-light-Übersicht funktioniert wie bisher bei
   Fully — ohne Änderung an `tablet.html`.
5. **Ferndiagnose:** Das Geräte-Log landet wie bei den Pis beim Turnier-PC
   und in der Cloud.
6. **Die App bleibt dumm.** Alle Turnier-Logik kommt weiter als Webseite
   vom Turnier-PC. Eine neue APK gibt es nur, wenn sich die Hülle ändert.

## Nicht-Ziele

- **Cloud-Modus.** Die App sucht nur im LAN. Cloud-Tablets laufen weiter
  über QR im Browser. Ein Cloud-Rückfall braucht eine Ersteinrichtung je
  Gerät (Namespace) und ist bewusst ausgeklammert.
- **Feld merken.** Nach einem Neustart landet das Tablet immer in der
  Lobby. Ein still übernommenes, belegtes Feld (ADR 0017) wäre schlimmer
  als ein Tipp mehr.
- **Selbst-Update der App.** Siehe Ziel 6.
- **Hintergrund-Überwachung der Verbindung.** Sobald die Lobby geladen ist,
  endet die Suche (Abschnitt „Suche, nur wenn nötig").
- **Änderungen an `tablet.html`** oder am Tablet-Server.
- Andere Plattformen (iPad, Windows-Tablets).

## Entscheidungen

### Eigene Kotlin-App, kein Tauri-Android, kein Fully-Fernsteuern (ADR 0058)

Die App braucht Device-Owner-, Lock-Task-, Boot- und WebView-Schnittstellen —
alles reine Android-APIs, die man ohnehin in Kotlin schreibt. Tauri 2 könnte
zwar Android bauen, bräuchte aber dieselben Kotlin-Plugins plus eine
schwere Werkzeugkette (NDK, `cargo-ndk`); die Suchlogik ist rund hundert
Zeilen und rechtfertigt keine Rust-Wiederverwendung. Fully Kiosk per
REST/Intent auf die richtige URL zu schicken wurde verworfen, weil die
Sperre dann weiter PLUS-Lizenzen kostet und zwei Apps gepflegt werden
müssten.

### Suche nach dem Pi-Muster

Reihenfolge nach Zuverlässigkeit, identisch zu `shared-startbrowser.sh`:

1. **Gemerkte IP** (App-Einstellungen). Antwortet `http://<ip>:8088/health`
   mit 200 und JSON, ist die Suche fertig.
2. **Subnetz-Scan:** /24 aus der eigenen WLAN-Adresse, alle 254 Adressen in
   Blöcken von 30 parallel auf `:8088/health`, 1 s Verbindungs-Timeout je
   Adresse, Abbruch beim ersten Treffer. Treffer wird gemerkt.
3. **mDNS-Rückfall** über `NsdManager` auf `_bts-light._tcp`, hart auf 3 s
   begrenzt — mDNS über WLAN ist das schwächste Glied (Feld-Lehre der Pis).

Die Sonde gilt nur als Treffer, wenn die Antwort als JSON lesbar ist; ein
fremder Dienst auf Port 8088 fällt so durch.

### Suche, nur wenn nötig

Die Suche läuft **ausschließlich** bei:

- App-Start / Boot,
- WLAN-Ereignis (Netz weg → Wartekarte; neues Netz → Suche),
- Ladefehler der WebView im Hauptrahmen (Server antwortet nicht mehr),
- Handgriff im Hüllen-Menü oder Knopf auf der Wartekarte.

Sobald die Lobby geladen ist, gibt es keine Pings und keinen Takt mehr. Die
Verbindung überwacht die Tablet-Seite selbst (WebSocket-Reconnect,
Hinweisband). **Bewusst offen:** Bekommt der Turnier-PC mitten im Turnier
eine neue IP, sieht die Seite nur „Verbindung verloren" und die Hülle merkt
es nicht; dafür gibt es den Handgriff. Ein Anstoß aus der Seite über die
JS-Brücke wäre die saubere Lösung, bräuchte aber eine Änderung in
`tablet.html` — Roadmap.

Ein Ausfall wird erst nach **drei** erfolglosen Runden (≈ 30 s) als solcher
gewertet, damit ein WLAN-Wackler die Anzeige nicht auf die Wartekarte wirft.
Antwortet nach einer Suche eine **andere** IP als zuvor, lädt die App die
Lobby neu.

### Bildschirm-Zustände

| Zustand | Anzeige |
|---|---|
| Suchen | Wartekarte: „Suche Turnier-PC im WLAN …", eigene IP, zuletzt gemerkte IP, Versuchszähler, Knopf **„Erneut suchen"** (ohne PIN), klein: „nicht als Gerätebesitzer eingerichtet", falls zutreffend |
| Verbunden | WebView mit `http://<ip>:8088/felder`, danach navigiert der Helfer selbst (Feld, Zahnrad-Menü) |
| Verloren | wie Suchen, nach der Fehltoleranz |

Kein WLAN-Name (SSID) auf der Wartekarte — bewusst weggelassen: Ab
Android 9 ist die SSID nur mit erteilter Standortfreigabe und
eingeschalteter Ortung lesbar, für ein Kiosk-Gerät unangemessen.

### Hüllen-Menü

Zwei Sekunden Fingerdruck in der **linken oberen Ecke** → PIN-Abfrage →
Menü: **Turnier-PC neu suchen** · **Adresse von Hand eingeben** (fremdes
Subnetz) · **PIN ändern** · **Kiosk verlassen**. Die PIN ist die Kiosk-PIN
der App (4–8 Ziffern), vergeben beim ersten Start ohne gespeicherte PIN.
Sie ist **getrennt** von `tablet_settings_pin` der Tablet-Seite (zwei
Ebenen, siehe [tablet-kiosk.md](../tablet-kiosk.md)).

### Kiosk-Sperre als Gerätebesitzer

Die App wird beim Einrichten per ADB zum **Device Owner** gemacht und als
**Home-Launcher** festgeschrieben. Der Launcher ist der eigentliche
Autostart (Android startet nach dem Boot den Home-Bildschirm); ein
`BOOT_COMPLETED`-Receiver bleibt als Rückfall, weil Fire OS dort trödelt.

Beim Start als Gerätebesitzer:

- **Lock-Task** ohne Nachfrage (Home/Zurück/Zuletzt/Statusleiste/
  Benachrichtigungen gesperrt, keine anderen Apps).
- **Sperrbildschirm aus** (`setKeyguardDisabled`) — kein Fire-Werbe-Lockscreen.
- **Bildschirm wach** über `FLAG_KEEP_SCREEN_ON`; zusätzlich
  `STAY_ON_WHILE_PLUGGED_IN` als globale Einstellung.
- **Immersives Vollbild**, Navigationsleiste weg.
- **Adressfilter:** Die WebView folgt nur Adressen des gefundenen Hosts.
  Jeder andere Link (z. B. badhub-Spielerseite aus der Lobby) wird still
  verworfen. Ersetzt den Web-Filter von Fully PLUS.

**Ohne Gerätebesitzer** (ADB-Schritt fehlt oder von Fire OS verweigert)
fällt die App auf „Bildschirm anheften" zurück: Android fragt einmal nach,
Ausstieg über die Android-Geste. Die Wartekarte weist darauf hin.

**Nachlese 08.09.2026 (v0.9.284) — kein Anheften auf Fire OS ohne
Besitzer:** Feldtest auf zwei Fire HD 10: Nach `startLockTask()` ohne
Gerätebesitzer schaltet Fire OS seinen „Toddler Mode" ein
(`com.amazon.toddlermode.ToddlerModeService` in `system_server`) und legt
`ToddlerModeTransparentWindow` als Vollbild-Fenster über die App;
`dumpsys input` zeigt es als einziges Touch-Ziel, die App bekommt keinen
Touch mehr — auch die Ecken-Geste nie. Darum entscheidet
`kern/SperrRegel.anheften(besitzer, hersteller)` (Unit-Test): Besitzer →
immer; ohne Besitzer nur, wenn der Hersteller nicht „Amazon" ist. Auf
Fire-Tablets ohne Besitzer bleibt Vollbild + Bildschirm-an ohne Sperre, die
Wartekarte sagt herstellerneutral „ohne Sperre (Anheften nicht möglich)" —
denselben Text zeigt sie, wenn `startLockTask()` anderswo wirft (der Boolean
aus `sperren` unterschied die Ursache bewusst nicht; seit v0.9.285 ein
Enum, siehe unten). Ausweg an einem
bereits angehefteten Tablet: `adb shell am task lock stop`. Außerdem
gemessen: Der Gerätebesitzer scheitert auf Fire OS am **Profile Owner**
`com.amazon.parentalcontrols` (geschütztes Paket), nicht nur am Konto.
**Korrektur nach dem Reset-Test am selben Tag:** Der Profile Owner ist
auch direkt nach einem Werksreset mit übersprungener Anmeldung wieder
gesetzt (Fire OS 8.0, Tablet GN434J…), dazu drei `amazon.account`-Konten
ohne Login. Der Gerätebesitzer-Weg dieser Spec ist auf Fire OS 8 damit
**nicht gangbar**; Autostart und harte Sperre brauchen einen anderen
Mechanismus (offen, Roadmap: Accessibility-„Home-Hijack" wie
LauncherHijack/Fully). Die Spec bleibt für Android-Geräte anderer
Hersteller gültig.

**Korrektur v0.9.285 — Toddler Mode eingegrenzt:** Drei Durchläufe am
zurückgesetzten Tablet: (1) Kindersicherung aus → `startLockTask()` heftet
still an, kein Toddler-Fenster, Touch geht. (2) Kindersicherung an, „App
fixieren" an, „Touch-Funktion deaktivieren" an (secure
`toddler_mode_default_value=1`) → SystemUI-Dialog `ScreenPinningConfirmation`
bei jedem Anheften, nach Bestätigung zwei `ToddlerMode*`-Fenster, Touch tot.
(3) wie (2), aber Touch-Schalter aus (Wert 0) → Dialog, danach angeheftet
**mit** Touch. Die pauschale Regel „auf Amazon nie anheften" (v0.9.284) war
zu breit: `SperrRegel.anheften(besitzer, hersteller, touchGesperrt)` heftet
auf Amazon jetzt an, solange der Schalter nicht 1 ist; `Kiosk.sperren`
liefert `Sperre.{Angeheftet, TouchGesperrt, Fehlgeschlagen}` mit je eigenem
Wartekarten-Hinweis. Das Skript setzt `toddler_mode_default_value 0`,
`stay_on_while_plugged_in 7` und `locksettings set-disabled true` und
bricht bei fehlgeschlagenem `set-device-owner` nicht mehr ab (auf Fire OS 8
immer).

**Nachlese 08.09.2026 (v0.9.285) — Autostart, Entschlackung Stufe 2, Akku:**
Autostart ohne Besitzer über `device_config put activity_manager
default_background_activity_starts_enabled true` (gerätweit; hebt „Abort
background activity starts" für den `BootReceiver` auf; SYSTEM_ALERT_WINDOW
und `settings put global background_activity_starts_enabled 1` wirkten auf
Fire OS nicht; überlebt Neustarts). **Korrektur:** Ein stilles Fixieren ohne
Besitzer gibt es nicht — AOSP zeigt bei jedem `startLockTask()` einer nicht
freigegebenen App den SystemUI-Dialog `ScreenPinningRequest` („App ist auf
dem Bildschirm fixiert", Nein danke / Verstanden, bei Amazon plus Kästchen
„Touch-Funktion deaktivieren"); die vermeintlich stillen Fälle waren Tipps
des Nutzers bzw. `am task lock` (System-Aufrufer). Lösung:
Bedienungshilfe-Dienst `kiosk/BestaetigungsDienst` (Manifest `<service>`
mit `BIND_ACCESSIBILITY_SERVICE`, Konfiguration `res/xml/bestaetigung.xml`:
nur `com.android.systemui`, `flagRetrieveInteractiveWindows`), Regel
`kern/FixierDialog` (Kennzeichen „fixiert/pinned" UND Bestätigungsknopf;
Tabu „Nein danke"/„Touch"/„deaktivieren"; Unit-Test), Einschalten per
`settings put secure enabled_accessibility_services …` im Skript (bestehende
Dienste bleiben). Feldtest: nach Neustart fixiert ohne Tipp, Touch bei der
App. `Kiosk.nachheften` prüft nach 3/10/30/60/120 s und bei Fokus-Erhalt
nach (max. fünf Versuche je Episode, Mindestabstand 2 s, Zähler zurück bei
erkannter Fixierung; `onDestroy` räumt die Takte). Energiesparmodus über
`low_power_trigger_level 100` + `automatic_power_save_mode 0`, Nachtmodus
per `battery_saver_constants` aus und `FORCE_DARK_OFF` in der WebView.
Entschlackung nach Fire-Tools-Liste (114 Pakete, `Entschlackung.ALEXA/
INHALTE/HINTERGRUND/UPDATES`), je Paket `pm disable-user` **und** `pm
suspend` (Letzteres greift auch bei protected); TABU zusätzlich
`com.amazon.redstone` (Fire-Tastatur). Messung: 81 installiert, 57
deaktiviert, 81 angehalten, 0 verweigert (kompletter Skriptlauf), Prozesse
26 → 17, App/WLAN/Lobby in Ordnung;
`adep`/`storagemanager` kommen von selbst zurück, OTA/`kso` laufen
angehalten weiter als Prozess. Akku: Fensterhelligkeit 30 % in der App
(`Kiosk.HELLIGKEIT`), Energiesparmodus + sticky per Skript.

**Nachlese 08.09.2026 (v0.9.285) — Entschlacken (Stufe 1, überholt durch
Stufe 2 unten: Fire-Tools-Liste, `disable-user` + `suspend`, `tcomm` nicht
mehr tabu):** Amazon-Apps (Alexa,
Video, Music, Kindle, Audible, Photos, Wetter, Shopping, Kids, Hilfe,
Freevee, Silk Kids, Sonderangebote) und der OTA-Dienst werden bei der
Einrichtung stillgelegt. Zwei Wege, weil Fire OS 8 `pm uninstall -k
--user 0` durchgehend verweigert (DELETE_FAILED_INTERNAL_ERROR, gemessen
an der Wetter-App) und einige Pakete auch gegen `pm disable-user` als
„protected" schützt (OTA, `kindle.kso`): Das Skript deaktiviert per
`disable-user`, was geht; der Gerätebesitzer versteckt in
`Kiosk.einrichten` per `setApplicationHidden` dieselbe Liste
(`kern/Entschlackung.ALLE`, Quelle; Skripte tragen Kopien). Wächter-Test
`EntschlackungTest`: Tabu-Pakete (eigene App, Silk, Appstore, WebView,
Launcher, Kindersicherung, `dcp`/`imp`/`tcomm`, Einstellungen) dürfen nie
auf der Liste stehen. Ob `setApplicationHidden` die „protected" Pakete auf
Fire OS wirklich versteckt, zeigt erst der Lauf auf einem zurückgesetzten
Tablet — das Log meldet je Paket „verweigert".

„Kiosk verlassen" beendet Lock-Task und die App → normales Android. Ein
Antippen des App-Symbols sperrt wieder.

### Restrisiken

- **LAN-Vertrauen:** Jedes Gerät im Hallen-WLAN, das auf `:8088/health` mit
  einer JSON-artigen Antwort reagiert, wird für diese Sitzung der Server
  des Tablets — akzeptiert, weil das Hallen-Netz physisch kontrolliert ist.
- **Klartext-HTTP durchgehend** (Seite, Sonde, Log) — LAN-only, wie der
  Tablet-Server selbst.
- **PIN ist Bedienschutz, keine Sicherheitsgrenze** — Klartext gespeichert,
  keine Sperre nach Fehlversuchen.

### JS-Brücke: `fully`-kompatibel

Die App reicht der WebView ein Objekt `fully` mit genau `getBatteryLevel()`
und `isPlugged()` — das, was `tablet.html` heute bei Fully Kiosk abfragt.
Der Akku-Badge funktioniert damit ohne Seitenänderung. Nichts weiter wird
exponiert; die Brücke wird nur für den gefundenen Host eingeblendet.

### Logging nach Pi-Vorbild

Ringpuffer-Log (≈ 800 Zeilen) im privaten App-Speicher: Boot, WLAN-
Ereignisse, jeder Suchlauf mit Ergebnis und Dauer, Ladefehler, Lock-Task-
Zustand, Menüzugriffe. Upload per `POST /pi-log?device=fire-<ANDROID_ID>`
an den Turnier-PC (Endpunkt existiert, filtert die ID bereits), **einmal
nach jeder erfolgreichen Suche und dann alle 5 Minuten**, solange eine
Seite geladen ist — ein kleiner POST an die bekannte IP, kein Suchlauf.
Die Logs liegen beim Turnier-PC im Ordner `pi-logs/` neben denen der Pis
und gehen von dort in die Cloud.

### Projekt, Build, Verteilung

- Ordner `android/`, Gradle + Kotlin, Paket `de.badhub.btslight.tablet`,
  Name „bts-light Tablet", minSdk 28 (Android 9 / Fire OS 7).
- **Version** wird beim Bauen aus `package.json` gelesen — keine vierte
  Versionsdatei.
- `ci.yml`: Job auf Ubuntu (JDK 17, Android-SDK) → Unit-Tests + Debug-APK.
- `release.yml`: beim Tag signierte Release-APK
  `bts-light-tablet-<version>.apk` ans GitHub-Release und nach
  `badhub.de/download/bts-light/` neben den Installer. `latest.json` und
  Auto-Update der Desktop-App bleiben unberührt.
- **Signatur:** einmalig erzeugter Keystore als GitHub-Secret
  (`ANDROID_KEYSTORE_B64`, `ANDROID_KEYSTORE_PASS`). Sideload-Updates
  verlangen dieselbe Signatur → Keystore nie wechseln (wie der
  Updater-Schlüssel, Doku in `release.md`). Bis das Secret angelegt ist,
  baut die CI nur die Debug-APK.
- **Ersteinrichtung je Tablet** (`android/setup-tablet.ps1` + `.sh`):
  Tablet zurücksetzen, Amazon-Anmeldung überspringen, WLAN verbinden,
  ADB-Debugging an, USB → Skript: prüft „keine Konten", installiert APK,
  setzt Device Owner, macht die App zum Launcher, gibt die WebView-Version
  aus, startet die App. Am Tablet dann die Kiosk-PIN vergeben.
- **Update:** Kiosk per PIN verlassen → Silk → APK von badhub.de laden →
  installieren → App antippen. Oder `adb install -r`.

## Ablauf

```
Boot → Home = App → Lock-Task
  → Suche (gemerkte IP → Scan → mDNS)
      Treffer → WebView /felder → Suche AUS → Log-Upload, dann alle 5 min
      kein Treffer → Wartekarte, Runde alle 10 s bis Treffer
  WLAN weg → Wartekarte → neues Netz → Suche
  Ladefehler → 3 Runden Toleranz → Wartekarte → Suche
  Ecke 2 s → PIN → Menü (neu suchen / Adresse / PIN / verlassen)
```

## Tests

Die Logik liegt in reinen Kotlin-Klassen ohne Android-Abhängigkeit (JUnit,
TDD); Activity, Receiver und WebView sind dünne Adapter darüber.

- Subnetz aus eigener Adresse ableiten (`192.168.16.42/24` → 254 Kandidaten).
- Kandidatenreihenfolge: gemerkte IP zuerst, dann Scan, dann mDNS.
- Scanner mit austauschbarer Sonde: erster Treffer gewinnt, Blöcke von 30,
  Abbruch nach Treffer, kein Treffer → leer.
- Antwortprüfung: 200 + JSON = Treffer; 200 + HTML, 404, Timeout = kein Treffer.
- Zustandsmaschine Suchen/Verbunden/Verloren: Fehltoleranz 3 Runden,
  IP-Wechsel → Neuladen, keine Runde im Zustand Verbunden.
- Adressfilter: nur Host+Port des Treffers, alles andere verworfen (auch
  `http://<ip>:8443`, `https://badhub.de/...`, `intent:`-Schemata).
- Log-Ringpuffer: Deckel 800 Zeilen, Upload-Body ≤ 2 MB.
- PIN-Regeln: 4–8 Ziffern, sonst abgelehnt.

## Offen / Feldtest

Nur echte Hardware klärt:

1. **Device Owner auf Fire OS:** `dpm set-device-owner` mit frischem Tablet
   ohne Amazon-Konto. Wird es verweigert → weiche Sperre dokumentieren,
   Alternative prüfen.
2. **Amazon-WebView-Stand:** reicht die Chromium-Version für `tablet.html`
   (Module, `fetch`, Pointer-Events)? Ggf. Mindestversion in die Doku.
3. **Lock-Task auf Fire OS:** Statusleiste wirklich weg? Fire-Launcher
   unterdrückt? Sperrbildschirm aus?
4. **Boot-Zeit** bis zur Lobby, mit und ohne gemerkte IP.
5. **Scan-Dauer** im Hallen-WLAN (254 Adressen, Blöcke von 30).
6. **Akku-Badge** in der Übersicht sichtbar.
7. Ein Turniertag parallel zu einem Fully-Tablet.
