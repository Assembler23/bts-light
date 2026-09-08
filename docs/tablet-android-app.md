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

1. Tablet auf Werkseinstellungen zurücksetzen. Beim Einrichten die
   Amazon-Anmeldung **überspringen** — mit einem angemeldeten Konto
   verweigert Android den Gerätebesitzer-Schritt.
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
   installiert die APK, setzt die App als Gerätebesitzer, **entschlackt**
   das Tablet (siehe unten) und startet die App. Schlägt `adb install` oder `dpm set-device-owner` fehl, bricht die
   PowerShell-Fassung mit einer Fehlermeldung samt Exit-Code ab (die
   Bash-Fassung ebenso, über `set -e`) — es geht also nie unbemerkt schief.
   Vorher lohnt sich **einmal ein Fire-OS-Update von Hand** (Einstellungen →
   Geräteoptionen → Systemupdates), denn danach schaltet die Einrichtung
   Amazons Update-Dienst ab.

**Entschlacken (seit v0.9.285):** Alexa, Prime Video, Amazon Music, Kindle,
Audible, Photos, Wetter, Shopping, Amazon Kids, Hilfe, Freevee, Silk Kids
und die Sonderangebote kosten auf einem Zähl-Tablet nur Akku und
Hintergrund; der OTA-Dienst würde mitten im Turnier ein Fire-OS-Update
einspielen und neu starten. Das Skript deaktiviert diese Pakete für den
Benutzer (`pm disable-user --user 0`; ein echtes `pm uninstall` verweigert
Fire OS 8 sogar für die Wetter-App) und meldet je Paket „deaktiviert" oder
„verweigert". Verweigert werden die Pakete, die Fire OS als „protected"
führt (OTA-Dienst, Sonderangebote). **Die App versucht zusätzlich beim
Start als Gerätebesitzer, dieselbe Liste zu verstecken**
(`setApplicationHidden`, Liste aus `kern/Entschlackung.kt`); das
Geräte-Log zeigt „Entschlacken: n versteckt, m verweigert, k nicht
vorhanden". Ob Fire OS dem Gerätebesitzer die geschützten Pakete
freigibt, ist offen — Android prüft beim Verstecken dieselbe Schutzliste
wie beim Deaktivieren. Steht der OTA-Dienst im Log als „verweigert", bleibt
Amazons Update-Dienst aktiv; dann hilft nur, Updates in den Einstellungen
zu meiden. Silk, Appstore, WebView, Launcher, Kindersicherung und die
Konto-/Gerätedienste bleiben bewusst unangetastet. Wer die Amazon-Apps
behalten will: `-OhneEntschlacken` (PowerShell) bzw. `behalten` als zweites
Argument (Bash). **Zurück holen** (beides nötig, weil die App auch die
vom Skript deaktivierten Pakete versteckt):

```
adb shell pm unhide --user 0 <paket>
adb shell pm enable --user 0 <paket>
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

- **Fire-Tablets (Hersteller „Amazon"):** Die App heftet den Bildschirm
  **bewusst nicht** an. Fire OS schaltet beim Anheften ohne Gerätebesitzer
  seinen „Toddler Mode" ein und legt ein unsichtbares Vollbild-Fenster über
  die App, das **jeden Touch schluckt** — die App wäre unbedienbar, auch die
  Ecken-Geste käme nie an (Feldtest 08.09.2026 auf zwei Fire HD 10, im
  Logcat „User is currently in toddler mode, touch outside pinned app is
  prohibited"). Es bleiben Vollbild, Bildschirm-an und die Server-Suche;
  Home führt aus der App heraus, Zurück bleibt wie im Kiosk ohne Wirkung.
  Die Wartekarte zeigt „Nicht als Gerätebesitzer eingerichtet – ohne Sperre
  (Anheften nicht möglich)." Regel: `kern/SperrRegel.kt` (Unit-Test
  `SperrRegelTest`); derselbe Hinweis erscheint auch, wenn das Anheften auf
  einem anderen Gerät scheitert.
- **Andere Android-Geräte:** die schwächere Sperre „Bildschirm anheften".
  Android fragt beim Start der App einmal nach, der Ausstieg geht über die
  normale Android-Geste statt über die Kiosk-PIN. Die Wartekarte zeigt
  „Nicht als Gerätebesitzer eingerichtet – Sperre nur weich."

**Autostart ohne Gerätebesitzer:** Der Home-Launcher-Autostart gehört zum
Gerätebesitzer-Schritt. Ohne ihn bleibt nur der Rückfall-Empfänger für
`BOOT_COMPLETED`, den Android ab Version 10 beim Starten von Activities aus
dem Hintergrund ausbremst — auf die App nach dem Einschalten ist also kein
Verlass. Wer trotzdem Autostart braucht, kann die App mit dem Werkzeug
„Custom Launcher" von Fire Toolbox als Startbildschirm setzen; dann öffnet
Fire OS sie nach jedem Boot wie einen Launcher.

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
APK installieren, Amazon-Apps stilllegen, Bildschirm-an und Sperrbildschirm
per adb setzen, App starten — genau das tut das Skript bis auf den
Gerätebesitzer-Schritt, den es dort mit Fehler abbricht (siehe
[roadmap.md](roadmap.md): Kiosk-Verhalten ohne Gerätebesitzer).

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
