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
2. Hallen-WLAN verbinden. Entwickleroptionen freischalten (Einstellungen →
   Geräteoptionen → Seriennummer 7× tippen) und darin **ADB-Debugging**
   einschalten.
3. Tablet per USB an den Einrichtungs-PC anschließen. `adb` muss dort
   installiert sein (Android Platform Tools). Die APK von
   <https://badhub.de/download/bts-light/bts-light-tablet.apk> laden.
4. Im Repo-Ordner `android/`: `.\setup-tablet.ps1 -Apk bts-light-tablet.apk`
   unter Windows bzw. `./setup-tablet.sh bts-light-tablet.apk` unter Linux/
   macOS ausführen. Das Skript prüft zuerst, dass auf dem Tablet **kein
   Konto** eingerichtet ist, installiert dann die APK, setzt die App als
   Gerätebesitzer und startet sie. Schlägt `adb install` oder
   `dpm set-device-owner` fehl, bricht die PowerShell-Fassung mit einer
   Fehlermeldung samt Exit-Code ab (die Bash-Fassung ebenso, über `set -e`)
   — es geht also nie unbemerkt schief.
5. Am Tablet die **Kiosk-PIN** (4–8 Ziffern) vergeben; das fragt die App
   beim allerersten Start automatisch ab, ein Abbrechen ist an dieser
   Stelle nicht möglich. Fertig — ab jetzt startet das Tablet nach jedem
   Einschalten direkt in die App, sucht den Turnier-PC und zeigt die
   Felder-Lobby.

## Bedienung

Solange kein Turnier-PC gefunden ist, zeigt die App eine Wartekarte statt
der Webseite — die WebView selbst lädt dabei bewusst `about:blank`, damit
eine im Hintergrund unsichtbar weiterlaufende Lobby-Seite keinen Gong
abspielt und keine Fingertipps entgegennimmt. Fehlt das WLAN ganz, steht
dort nur „Kein WLAN"; ist ein Netz da und die Suche läuft, zeigt die Karte
„Suche Turnier-PC im WLAN …" mit Versuchszähler, der eigenen IP-Adresse und
der zuletzt bekannten Turnier-PC-Adresse, dazu den Knopf „Erneut suchen"
(ohne PIN nutzbar).

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
App mit der schwächeren Sperre „Bildschirm anheften": Android fragt beim
Start der App einmal nach, der Ausstieg geht über die normale
Android-Geste statt über die Kiosk-PIN. Die Wartekarte zeigt in diesem Fall
zusätzlich den kleinen Hinweis „Nicht als Gerätebesitzer eingerichtet –
Sperre nur weich."

## Fehlersuche

Bleibt die Wartekarte dauerhaft stehen, zunächst den WLAN-Namen und die
eigene IP-Adresse auf der Karte prüfen — fehlt die eigene IP, ist das
Tablet gar nicht im Netz. Auf der Turnier-PC-Seite lohnt sich der Blick, ob
die Übertragung läuft und ob die Windows-Firewall den Zugriff erlaubt.

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
Android-Abhängigkeit und ist mit 33 JVM-Unit-Tests abgedeckt; Activity,
Receiver und WebView sind nur dünne, ungetestete Adapter darüber.
