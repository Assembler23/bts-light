# Zähl-Tablet als Kiosk-App (Android / Fire-Tablets)

> Spec: [features/tablet-android-kiosk-app.md](features/tablet-android-kiosk-app.md) · ADR 0058

Die App „bts-light Tablet" macht aus einem Fire-Tablet ein Zähl-Tablet, das
beim Einschalten von selbst den Turnier-PC im Hallen-WLAN findet und die
Felder-Lobby im Vollbild zeigt. Sie ersetzt Fully Kiosk für das Verleih-Set:
keine Start-URL je Tablet, keine PLUS-Lizenz für die Sperre, und niemand
muss vor dem Turnier an jedem Gerät die IP-Adresse nachtragen.

## Was die App tut

Beim Start sucht die App den Turnier-PC genau wie die Court-Monitore auf dem
Raspberry Pi: zuerst die zuletzt gemerkte IP-Adresse, dann ein Scan des
eigenen /24-Subnetzes auf Port 8088, und erst als letzter Rückfall mDNS. Die
Suche läuft ausschließlich beim App-Start, bei einem WLAN-Wechsel, nach
einem Ladefehler der Webseite und auf Knopfdruck — nie als Hintergrund-Takt,
solange die Felder-Lobby einmal geladen ist. Läuft gerade schon ein
Suchdurchlauf, wird ein weiterer Anstoß verworfen und nur im Log vermerkt
(„Anstoß verworfen, Lauf aktiv") — sonst würden zwei Scans gleichzeitig im
selben Subnetz laufen. Ein Anstoß während der zehnsekündigen Wartezeit
*zwischen* zwei erfolglosen Runden wirkt dagegen sofort: Er bricht die
Wartezeit ab und startet die nächste Suche ohne Verzögerung, statt bis zum
Rundenende zu warten.

Hat die Suche den Turnier-PC gefunden, lädt die App
`http://<PC-IP>:8088/felder`, die bekannte Felder-Lobby. Von dort an
übernimmt die Weboberfläche wie gewohnt: Feld antippen, zählen, Ergebnis
eintragen. Die App selbst trifft keine Turnier-Entscheidungen — sie ist
bewusst dumm und bleibt bei der Kiosk-Hülle.

Als Gerätebesitzer sperrt die App das Tablet hart: keine Android-Buttons
(Home, Zurück, Zuletzt), keine Statusleiste, keine anderen Apps, kein
Zugriff auf fremde Adressen aus der geladenen Seite heraus. Der Bildschirm
bleibt an, auch ohne Ladekabel-Wake-Lock des Browsers. Der Akku wird wie
bei Fully Kiosk über ein eingeblendetes `fully`-Objekt gemeldet
(`getBatteryLevel()`, `isPlugged()`) — der Akku-Badge in der
bts-light-Übersicht funktioniert damit ohne jede Änderung an `tablet.html`.
Das Geräte-Log der App landet wie bei den Pi-Monitoren beim Turnier-PC
(`pi-logs/fire-<ANDROID_ID>.log`) und von dort in der Cloud.

## Einrichten (einmalig je Tablet, ca. 5 Minuten)

1. Nur auf Android-Geräten anderer Hersteller: Tablet auf Werkseinstellungen
   zurücksetzen und die Anmeldung **überspringen** — mit einem angemeldeten
   Konto verweigert Android den Gerätebesitzer-Schritt. Auf **Fire-Tablets
   bringt der Reset nichts** (siehe „Fire OS 8: Gerätebesitzer nicht
   möglich" unten); dort reicht: kein Amazon-Konto angemeldet,
   Kindersicherung aus.
2. Entwickleroptionen freischalten (Einstellungen → Geräteoptionen →
   Seriennummer 7× tippen) und darin **ADB-Debugging** einschalten. Das
   Hallen-WLAN kann das Skript in Schritt 4 selbst verbinden
   (`-Wlan <SSID> -WlanPasswort <Passwort>` bzw. `WLAN_SSID=… WLAN_PASSWORT=…`
   vor dem Bash-Aufruf, WPA2); ohne diese Angabe vorher von Hand verbinden.
3. Tablet per USB an den Einrichtungs-PC anschließen. `adb` muss dort
   installiert sein (Android Platform Tools). Die APK von
   <https://badhub.de/download/bts-light/bts-light-tablet.apk> laden.
4. Im Repo-Ordner `android/`: `.\setup-tablet.ps1 -Apk bts-light-tablet.apk
   -Wlan Hallen-WLAN -WlanPasswort geheim` unter Windows bzw.
   `WLAN_SSID=Hallen-WLAN WLAN_PASSWORT=geheim ./setup-tablet.sh
   bts-light-tablet.apk` unter Linux/macOS ausführen. Das Skript verbindet
   zuerst das WLAN (falls angegeben; per `cmd wifi connect-network`, wartet
   bis zu 30 s auf eine IP-Adresse und läuft bei Misserfolg mit Warnung
   weiter), prüft dann, dass auf dem Tablet **kein Konto** eingerichtet ist,
   installiert die APK, setzt die App als Gerätebesitzer, setzt Bildschirm-,
   Autostart- und Akku-Einstellungen, schaltet den Bestätigungsdienst ein,
   **entschlackt** das Tablet (siehe unten) und startet die App. Schlägt `adb install` fehl,
   bricht das Skript mit Fehlermeldung ab; gefundene Konten und ein
   gescheiterter `dpm set-device-owner` sind nur Warnungen, weil beides auf
   Fire OS 8 immer eintritt (interne Amazon-Konten, Kindersicherung als
   Profile Owner) — die Einrichtung läuft dann ohne Gerätebesitzer weiter.
   Vorher lohnt sich **einmal ein Fire-OS-Update von Hand** (Einstellungen →
   Geräteoptionen → Systemupdates), denn danach schaltet die Einrichtung
   Amazons Update-Dienst ab.

**Entschlacken (seit v0.9.285):** Grundlage ist die Debloat-Liste von
[Fire-Tools](https://github.com/mrhaydendp/Fire-Tools) (114 Pakete: Alexa,
Video, Musik, Bücher, Fotos, Shopping, Kids, Karten, Metriken, Werbe-IDs,
Sync, Push, Fernwartung, Amazons OTA-Update-Dienst …), community-erprobt und
in `kern/Entschlackung.kt` mit Beschreibung je Paket gepflegt; die Skripte
tragen Kopien, `scripts/test-entschlackung-liste.mjs` hält sie gleich. Je
installiertem Paket macht das Skript zwei Griffe: `pm disable-user --user 0`
(schaltet ab; verweigert Fire OS 8 bei „protected" Paketen wie OTA-Dienst
und Sonderangeboten) **und** `pm suspend` (hält an: keine Oberfläche, keine
Benachrichtigungen — geht auch bei protected). Ein echtes `pm uninstall`
verweigert Fire OS 8 für alle Systempakete. Die Ausgabe nennt je Paket
„aus+angehalten", „angehalten", „aus" oder „verweigert" und am Ende die
Summen. Als Gerätebesitzer versteckt die App dieselbe Liste zusätzlich per
`setApplicationHidden` (Log „Entschlacken: n versteckt, m verweigert, k
nicht vorhanden") — auf Fire OS 8 kommt es dazu nie, siehe unten.

Feldtest 08.09.2026 (Fire HD 10, Fire OS 8, kompletter Skriptlauf): 81 der
Pakete installiert, 57 deaktiviert, alle 81 angehalten, 0 verweigert; nach
dem Neustart starteten App und
WLAN, die Lobby lud, die Amazon-Prozesse sanken von 26 auf 17. Fire OS
schaltet den Speicher-Manager und ein Diagnose-Paket von selbst wieder frei
(harmlos); Alexa bleibt aus, sobald sie auch angehalten ist. **Ehrlich
dazu:** OTA-Dienst und Sonderangebote laufen trotz Anhalten als Prozess
weiter; ob der OTA-Dienst so noch ein Update einspielt, zeigt erst der
Dauerbetrieb — deshalb vorher einmal von Hand aktualisieren.

Bewusst **nicht** auf der Liste: WebView (die App selbst), Silk (Notausgang
zur Fehlersuche), Appstore, Launcher, Kindersicherung, die Middleware
`dcp`/`imp`, die Fire-Tastatur (`com.amazon.redstone` — ohne sie lässt sich
die Kiosk-PIN nicht tippen) und die Einstellungen. Wer die Amazon-Apps
behalten will: `-OhneEntschlacken` (PowerShell) bzw. `behalten` als zweites
Argument (Bash). **Zurück holen** je Paket:

```
adb shell pm unsuspend <paket>
adb shell pm enable --user 0 <paket>
adb shell pm unhide --user 0 <paket>      # nur falls die App es als Besitzer versteckt hat
```

**Warum kein Abbild für 20 Tablets:** Fire-Tablets haben einen gesperrten
Bootloader; ohne Root gibt es kein Voll-Abbild, und `adb backup` überträgt
weder den Gerätebesitzer noch Systemeinstellungen. Der Aufwand je Tablet
bleibt deshalb: Reset und Anmeldung überspringen (etwa 3 Minuten von Hand),
dann das Skript (unter einer Minute). Mit einem USB-Hub lassen sich mehrere
Tablets nacheinander abarbeiten; das Skript verlangt je Lauf genau ein
angeschlossenes Gerät.
5. Am Tablet die **Kiosk-PIN** (4–8 Ziffern) vergeben; das fragt die App
   beim allerersten Start automatisch ab, ein Abbrechen ist an dieser
   Stelle nicht möglich. Fertig — ab jetzt startet das Tablet nach jedem
   Einschalten direkt in die App, sucht den Turnier-PC und zeigt die
   Felder-Lobby.

## Bedienung

Solange kein Turnier-PC gefunden ist, zeigt die App eine Wartekarte statt
der Webseite — die WebView selbst lädt dabei bewusst `about:blank`, damit
eine im Hintergrund unsichtbar weiterlaufende Lobby-Seite keinen Gong
abspielt und keine Fingertipps entgegennimmt. Bricht das WLAN während des
Betriebs weg, steht dort „Kein WLAN", bis das Netz zurück ist. Startet das
Tablet dagegen ganz ohne Netz (z. B. beim ersten Einschalten in der Halle),
bleibt es bei „Suche Turnier-PC im WLAN …" mit eigener IP „–" und sucht alle
10 Sekunden weiter — dann zuerst das WLAN am Tablet prüfen. Ist ein Netz da
und die Suche läuft, zeigt die Karte „Suche Turnier-PC im WLAN …" mit
Versuchszähler, der eigenen IP-Adresse und der zuletzt bekannten
Turnier-PC-Adresse, dazu den Knopf „Erneut suchen" (ohne PIN nutzbar).

Das **Hüllen-Menü** öffnet sich, wenn man zwei Sekunden lang den Finger in
die linke obere Ecke des Bildschirms hält, gefolgt von der Kiosk-PIN. Es
bietet vier Punkte:

- **„Turnier-PC neu suchen"** stößt sofort einen neuen Suchlauf an.
- **„Adresse von Hand eingeben"** fragt eine IP-Adresse ab, für den Fall
  eines fremden Subnetzes, das der Scan nicht findet.
- **„PIN ändern"** fragt die neue PIN ab; anders als bei der ersten Vergabe
  ist dieser Dialog abbrechbar. Eine ungültige Eingabe setzt die PIN nicht,
  sondern zeigt nur den Hinweis „PIN muss 4–8 Ziffern haben" — man landet
  wieder im Menü, statt in einer Endlosschleife zu hängen.
- **„Kiosk verlassen"** beendet die Sperre und die App. Als Gerätebesitzer
  löscht dieser Schritt zusätzlich die Home-Vorgabe der App (sonst würde
  Android sie als bevorzugten Home-Bildschirm sofort wieder in den
  Vordergrund holen); danach verhält sich das Tablet wie ein normales
  Android-Gerät. Ein Antippen des App-Symbols genügt aber, um alles wieder
  scharf zu schalten — die Einrichtung läuft bei jedem Start der App erneut
  (idempotent), setzt also Lock-Task, Home-Vorgabe und Vollbild erneut.

Eine falsch eingegebene PIN öffnet nichts; die App zeigt kurz den Hinweis
„PIN falsch" und bleibt in der aktuellen Ansicht.

Die Einstellungs-PIN der Zähl-Seite (Zahnrad-Symbol, `tablet_settings_pin`)
ist von der Kiosk-PIN vollständig getrennt — zwei Ebenen wie in
[tablet-kiosk.md](tablet-kiosk.md) beschrieben. Wer beide auf denselben Wert
setzt, merkt den Unterschied im Alltag nicht.

**Bekommt der Turnier-PC mitten im Turnier eine neue IP-Adresse**, merkt die
App das nicht von selbst — die Suche läuft ja bewusst nicht im Hintergrund,
sobald die Lobby einmal geladen ist. Die Webseite selbst zeigt in diesem
Fall „Verbindung verloren"; der Weg zurück führt über das Hüllen-Menü und
„Turnier-PC neu suchen".

## Update der App

Kiosk-App per PIN verlassen, im Silk-Browser die APK von badhub.de laden,
installieren, App-Symbol antippen — das genügt für ein Update. Wer das
Tablet per USB am PC hat, kann stattdessen `adb install -r <apk>` nutzen.

Wichtig ist dabei die **Signatur**: Eine Debug-APK lässt sich nicht über
eine signierte installieren und umgekehrt — Android verweigert das mit
einem Signaturfehler. In diesem Fall muss die App erst deinstalliert und
danach der Gerätebesitzer-Schritt (`setup-tablet.ps1`/`.sh`) erneut
durchlaufen werden. Das betrifft insbesondere den Übergang vom ersten
Release, das mangels Signatur-Secrets noch als Debug-APK ausgeliefert
wurde, zur später signierten Fassung: Der Release-Workflow baut die
Tablet-APK in einem eigenen Job, der einen fehlgeschlagenen Build bewusst
nicht den Windows-Installer oder `latest.json` blockieren lässt
(`continue-on-error`). Solange die beiden Signatur-Secrets
(`ANDROID_KEYSTORE_B64`, `ANDROID_KEYSTORE_PASS`) noch nicht hinterlegt
sind, lädt dieser Job stattdessen eine unsignierte
`bts-light-tablet-<version>-debug.apk` hoch, und der feste Downloadlink
`bts-light-tablet.apk` zeigt in dieser Zeit auf genau diese Debug-Version.
Tablets, die zu diesem Zeitpunkt eingerichtet wurden, müssen beim späteren
Umstieg auf die signierte APK also einmal deinstalliert und neu als
Gerätebesitzer eingerichtet werden — siehe auch
[release.md](release.md).

## Ohne Gerätebesitzer

Wurde der ADB-Schritt übersprungen oder von Fire OS verweigert, läuft die
App **ohne harte Sperre**. Was dann passiert, hängt vom Gerät ab:

- **Fire-Tablets (Hersteller „Amazon"):** Die App heftet den Bildschirm an
  wie auf jedem Android — **außer** die Kindersicherung hat „App fixieren →
  Touch-Funktion deaktivieren" eingeschaltet. Dann legt Fire OS beim
  Fixieren seinen „Toddler Mode" über die App, ein unsichtbares
  Vollbild-Fenster, das **jeden Touch schluckt** — die App wäre unbedienbar,
  auch die Ecken-Geste käme nie an (Feldtest 08.09.2026 auf zwei Fire HD 10,
  in drei Durchläufen eingegrenzt: ohne Kindersicherung fixiert die App
  still und Touch geht; mit „App fixieren" fragt Fire OS bei jedem Start
  einmal nach; erst „Touch-Funktion deaktivieren" bringt das Toddler-
  Fenster). Der Schalter ist das Secure-Setting `toddler_mode_default_value`;
  das Einrichtungsskript setzt ihn auf 0, die App liest ihn beim Start und
  fixiert bei 1 **nicht** — die Wartekarte sagt dann „Nicht fixiert: In der
  Kindersicherung „App fixieren → Touch-Funktion deaktivieren" ausschalten".
  Achtung: Die Kindersicherungs-Oberfläche schreibt den Wert bei jedem
  Umschalten neu — wer dort „Touch-Funktion deaktivieren" wieder einschaltet,
  überstimmt das Skript (beobachtet 08.09.2026). Den Fixier-Dialog selbst
  zeigt Android ohnehin bei jedem Start; ihn bestätigt der
  Bedienungshilfe-Dienst der App (siehe „Autostart").
  Empfehlung: kein Amazon-Konto, Kindersicherung aus. Regel:
  `kern/SperrRegel.kt` (Unit-Test `SperrRegelTest`). Scheitert das Fixieren
  aus anderem Grund, steht „ohne Sperre (Anheften nicht möglich)" auf der
  Karte; Home führt dann aus der App heraus, Zurück bleibt ohne Wirkung.
- **Andere Android-Geräte:** die schwächere Sperre „Bildschirm anheften".
  Android fragt beim Start der App einmal nach, der Ausstieg geht über die
  normale Android-Geste statt über die Kiosk-PIN. Die Wartekarte zeigt
  „Nicht als Gerätebesitzer eingerichtet – Sperre nur weich."

**Autostart ohne Gerätebesitzer (seit v0.9.285):** Der Home-Launcher-
Autostart gehört zum Gerätebesitzer-Schritt. Ohne ihn bleibt der
Rückfall-Empfänger für `BOOT_COMPLETED`, den Android ab Version 10 beim
Starten von Activities aus dem Hintergrund abbricht (Logcat: „Abort
background activity starts"). Das Skript hebt diese Sperre gerätweit auf:

```
adb shell device_config put activity_manager default_background_activity_starts_enabled true
```

Feldtest 08.09.2026: Danach lag der Fokus nach jedem Neustart auf der App,
der Wert überlebte mehrere Neustarts; die Berechtigung „über anderen Apps
anzeigen" und der alte Entwickler-Schalter `background_activity_starts_enabled`
halfen auf Fire OS dagegen nicht. **Der Fixier-Dialog:** Ohne
Gerätebesitzer zeigt Android bei **jedem** Fixieren den Dialog „App ist auf
dem Bildschirm fixiert" mit „Nein danke / Verstanden" (AOSP-Verhalten, keine
Fire-OS-Eigenheit — ein stilles Fixieren gibt es ohne Besitzer nicht; nur
`adb shell am task lock` als System-Aufrufer fixiert ohne Dialog). Damit
nach dem Einschalten niemand tippen muss, bringt die App den
Bedienungshilfe-Dienst **„Fixieren bestätigen"** (`kiosk/BestaetigungsDienst`)
mit: Er sieht nur SystemUI-Fenster, erkennt den Dialog über die reine Regel
`kern/FixierDialog` (Unit-Test: Kennwort „fixiert/pinned" plus Knopf
„Verstanden/Got it/OK", nie „Nein danke", nie das Kästchen) und tippt den
Bestätigungsknopf. Amazons Kästchen „Touch-Funktion … deaktivieren" bleibt
unangetastet — angehakt schaltet es den Toddler Mode ein. Das Skript
schaltet den Dienst per adb ein (`enabled_accessibility_services`,
`accessibility_enabled 1`; auf Fire OS 8 per adb schreibbar, geprüft
08.09.2026). Feldtest: Nach dem Neustart fixiert, Dialog geschlossen, kein
Toddler-Fenster, Touch bei der App. Zusätzlich prüft die App nach dem Start
mehrfach (3 s … 120 s) und beim Fokus-Erhalt, ob sie fixiert ist, und
heftet sonst nach (`Kiosk.nachheften`, höchstens fünfmal je Episode; ist sie
wieder fixiert, beginnt die Zählung von vorn); nach fünf vergeblichen
Versuchen sagt die Wartekarte „ohne Sperre". Fire Toolbox „Custom
Launcher" ist damit nicht mehr nötig. Sicherung des Konfigurationswerts
gegen Androids RescueParty (setzt `device_config` nach wiederholten
System-Abstürzen zurück): `adb shell device_config set_sync_disabled_for_tests
persistent` — bisher nicht nötig.

**Akku (seit v0.9.285):** Die App setzt die Helligkeit ihres Fensters auf
30 % (`Kiosk.HELLIGKEIT`, ohne Berechtigung, wirkt solange die App vorn ist
— im Kiosk immer). Das Skript hält den Bildschirm am Ladekabel wach und
schaltet den Energiesparmodus über die **Automatik-Schwelle 100 %** ein:
Fire OS setzt den direkten Schalter beim Boot am Kabel zurück, die Schwelle
bleibt und greift, sobald das Kabel ab ist (simuliert geprüft 08.09.2026);
dazu „sticky" und der Prozent-Modus. Der Sparmodus würde den Nachtmodus
mitschalten — das Skript sperrt das (`battery_saver_constants`), und die
WebView färbt die Zählseite nicht algorithmisch um (`FORCE_DARK_OFF`).

**Fire OS 8: Gerätebesitzer nicht möglich, auch nicht nach Werksreset.**
Der Gerätebesitzer scheitert auf Fire-Tablets nicht am Amazon-Konto, sondern
daran, dass Amazons Kindersicherung (`com.amazon.parentalcontrols`) beim
ersten Start als **Profile Owner** von Nutzer 0 eingetragen wird — geprüft
am 08.09.2026 direkt nach einem Werksreset mit übersprungener Anmeldung:
Der Profile Owner ist sofort wieder da, dazu drei systeminterne Konten vom
Typ `amazon.account` ohne jede Anmeldung. `dpm set-device-owner` antwortet
„the user already has a profile owner", `dpm remove-active-admin` verweigert
(„non-test admin"), das Paket ist geschützt (kein `pm uninstall`, kein
`pm disable-user`). **Ein Werksreset bringt auf Fire OS 8 also nichts**; die
Schritte 1 und 4 oben (Reset, Gerätebesitzer) gelten nur für Android-Geräte
anderer Hersteller. Auf Fire-Tablets bleibt die Einrichtung ohne Besitzer:
APK installieren, Amazon-Apps stilllegen, Bildschirm-an, Sperrbildschirm
und den Toddler-Schalter per adb setzen, App starten — genau das tut das
Skript; den fehlgeschlagenen Gerätebesitzer-Schritt meldet es nur als
Warnung und läuft weiter. Was dann fehlt, ist allein der Autostart nach dem
Einschalten (siehe [roadmap.md](roadmap.md): Kiosk-Verhalten ohne
Gerätebesitzer).

## Fehlersuche

Bleibt die Wartekarte dauerhaft stehen, zunächst die eigene IP-Adresse auf
der Karte prüfen (einen WLAN-Namen zeigt die Karte bewusst nicht, siehe
unten) — fehlt die eigene IP, ist das Tablet gar nicht im Netz und das
WLAN muss in den Android-Einstellungen geprüft werden. Auf der
Turnier-PC-Seite lohnt sich der Blick, ob die Übertragung läuft und ob die
Windows-Firewall den Zugriff erlaubt.

Das Geräte-Log jedes Tablets liegt beim Turnier-PC unter
`pi-logs/fire-<ANDROID_ID>.log` (über „Logs öffnen" einsehbar) und in der
Cloud unter derselben Geräte-ID. Jeder Suchlauf steht dort mit dem
gefundenen Weg (gemerkte IP / Scan / mDNS) und seiner Dauer drin — der
schnellste Weg, einen zähen Boot- oder Such-Vorgang einzugrenzen. Den
WebView-Versionsstand des Tablets gibt das Einrichtungsskript beim Setup
aus; er hilft bei der Frage, ob die Amazon-WebView-Version für
`tablet.html` (Module, `fetch`, Pointer-Events) noch ausreicht.

Die Such- und Zustandslogik der App (Sonde, Scanner, Adressfilter,
PIN-Regeln, Zustandsmaschine) liegt in reinen Kotlin-Klassen ohne
Android-Abhängigkeit und ist mit 35 JVM-Unit-Tests abgedeckt; Activity,
Receiver und WebView sind nur dünne, ungetestete Adapter darüber.

## Was die App bewusst nicht kann / Restrisiken

- **LAN-Vertrauen:** Jedes Gerät im Hallen-WLAN, das auf `:8088/health` mit
  einer JSON-artigen Antwort reagiert, wird für diese Sitzung der Server
  des Tablets — akzeptiert, weil das Hallen-Netz physisch kontrolliert ist
  (Zutritt zur Halle statt Netzwerk-Zugangskontrolle).
- **Klartext-HTTP durchgehend:** Seite, Sonde und Log-Upload sprechen alle
  unverschlüsseltes HTTP — LAN-only, genau wie der Tablet-Server selbst.
- **PIN ist Bedienschutz, keine Sicherheitsgrenze:** Sie wird im Klartext
  gespeichert und sperrt nach Fehlversuchen nicht — sie soll nur
  versehentliches Verlassen des Kiosks verhindern, keinen Angreifer
  aufhalten.

Der feste Download-Name `bts-light-tablet.apk` zeigt außerdem **nur** auf
eine signiert gebaute APK; eine Debug-APK (ohne Keystore-Secret gebaut)
bleibt ausschließlich unter ihrem `-debug`-Namen erreichbar, damit kein
Tablet versehentlich eine nicht-signierte Version installiert, die später
kein signiertes Update mehr annimmt.
