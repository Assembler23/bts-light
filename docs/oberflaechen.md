# Welcher Bildschirm zeigt was

BTS Light hat mehr Oberflächen, als man auf den ersten Blick vermutet: das
Programm auf dem Turnier-PC, die Seiten auf den Geräten in der Halle und die
öffentlichen Seiten auf badhub.de. Diese Übersicht sagt, **wo du was findest**
und **welches Gerät welche Seite anzeigt**.

Es sind drei Welten:

| Welt | Wer bedient sie | Wo |
|---|---|---|
| **Die Windows-App** | Turnierleitung | am Turnier-PC |
| **Die Seiten in der Halle** | Schiedsrichter, Hallenaufbau | Tablets, Fernseher, Raspberry Pi |
| **Die öffentlichen Seiten** | Spieler und Zuschauer | eigenes Telefon, Hallen-TV |

## Die Windows-App

Die linke Leiste, von oben nach unten. Ein Menüpunkt kann **ausgegraut** sein,
solange die zugehörige Funktion abgeschaltet ist — ein Klick darauf springt
dann in die Einstellungen an die richtige Stelle, statt eine leere Seite zu
zeigen.

| Menüpunkt | Wofür | Ausgegraut, wenn |
|---|---|---|
| **Status** | Die Startseite: Läuft die Übertragung? Wie weit ist das Turnier? Sind alle Felder mit Geräten versorgt? Von hier aus lassen sich die Anzeigen im Browser öffnen, der Aushang drucken und die [Turnierdatei an badhub senden](tp-datei-upload.md). | nie |
| **Spielübersicht** | Das Arbeitspferd der Feldvergabe: oben die spielbereiten Spiele, darunter die Felder mit Ampel — grün frei, gelb belegt, **rot gesperrt**. Spiele zuweisen, Felder freigeben und sperren. Oben rechts öffnet **Turnierleitung öffnen** die [Turnierleitungs-Oberfläche](turnierleitung-web.md#am-turnier-pc-selbst-ein-klick-seit-v09430) im Browser dieses PCs. | nie |
| **Tablets** | Drei Reiter: **Übersicht** (welches Tablet hängt an welchem Feld, mit Akkustand), **In Vorbereitung** (gerufene Spiele), **QR-Codes** zum Einrichten der Geräte — die Adresse daneben ist ein Link und öffnet den Spielzettel im Browser des Turnier-PCs; bei eingeschaltetem Meetingpoint steht dort auch die Adresse der [Meetingpoint-Seite](meetingpoint.md) je Halle. | nie |
| **Turnierleitung** | Hier entstehen die Zugänge für die Turnierleitungs-Oberfläche: Gerät koppeln, QR-Code zeigen, Zugang wieder entziehen. Für den Turnier-PC selbst braucht es das nicht — dafür gibt es den Knopf in der Kopfzeile. | nie — hier schaltet man sie schließlich ein |
| **Check-In** | Wer ist da, wer fehlt — je Klasse, gesetzte Meldungen mit ihrem Setzplatz (`[1]`). Von Hand einchecken oder zurücksetzen, Fehlende ausrufen lassen, Hallen-Ansicht, Zeitplan, Check-In-Seite und QR-Aushang öffnen. | Check-In aus **oder** Turnier-Kennung fehlt |
| **Schiedsrichter** | Einsatzplanung: Reihenfolge (bei „Getrennte Listen" je eine für SR und AR), Rolle je Person („SR und AR", „nur SR", „nur AR"), Vorschau „als Nächstes SR/AR", Pausen, Sperrlisten, feldweise Schalter. Die Stammliste kommt aus BTP. | „Mit Schiedsrichtern spielen" aus |
| **Ansagen** | Feld von Hand ansagen, Freitext sprechen, gespeicherte Ansagen, Verlauf mit „Erneut abspielen" — und alle Detail-Einstellungen zu Stimme und Tempo. Unter dem Test-Knopf steht, worüber die Ansage läuft („Ausgabe: Windows-Standard (…)"), und **Ansage über:** wählt ein festes Gerät für diesen PC, siehe [Ausgabegerät](announcements.md#ausgabegerät-seit-v09318). | Ansagen aus |
| **Monitore** | Die Geräteverwaltung für Fernseher und Pis: **welches Gerät zeigt was**. Siehe unten. | Court-Monitor aus |
| **Siegerehrung** | Steuert, welche Disziplin der Sieger-Monitor gerade zeigt. Bewusst ohne Rotation, damit das Publikum das Podium fotografieren kann. | Court-Monitor aus |
| **Einstellungen** | Derselbe Bildschirm wie der Einrichtungs-Assistent. Siehe [Einstellungen](einstellungen.md). | nie |
| **Wartung** | Update prüfen, Logs öffnen, Version und Seiten-Stand ablesen, Geräte-Seiten zurücksetzen, zur vorigen App-Fassung zurückwechseln, Identität umziehen. | nie |

Über allem liegt eine **Kopfzeile** mit dem Start-/Stopp-Knopf und vier
Statusanzeigen:

- **Liveticker** — läuft die Übertragung gerade, oder ist sie gestoppt?
- **Hallennetz** — hängt der PC im WLAN der Halle?
- **Internet** beziehungsweise Cloud-Verbindung
- **Ferne Halle** — nur, wenn eine gekoppelt ist

Links neben dem Start-/Stopp-Knopf steht **Turnierleitung** (seit v0.9.430):
ein Klick öffnet die
[Turnierleitungs-Oberfläche](turnierleitung-web.md#am-turnier-pc-selbst-ein-klick-seit-v09430)
im Browser dieses PCs — ohne Koppeln. Ein zweiter Klick macht keinen zweiten
Tab auf. Am PC einer fernen Halle fehlt der Knopf.
<!-- pruef: /label="Turnierleitung"/ in src/components/AppShell.tsx -->
<!-- pruef: "Turnierleitung öffnen" in src/pages/FieldOverviewPage.tsx -->

Läuft alles, steht dort **„Verbunden – Liveticker aktuell"** (seit v0.9.382).
<!-- pruef: "Verbunden – Liveticker aktuell" in src-tauri/src/commands.rs -->

Stockt die Übertragung an badhub, sagt die Liveticker-Anzeige, woran es liegt
(seit v0.9.381). Die Halle läuft in allen Fällen weiter — Tablets, Monitore
und die Feldvergabe hängen nicht am Internet.

| Anzeige | Bedeutung | Was tun |
|---|---|---|
| orange: **„Internet-Strecke gestört — Halle läuft weiter, nächster Versuch in … s"** | badhub ist gerade nicht erreichbar. bts-light versucht es in wachsenden Abständen (5 bis 60 Sekunden) erneut. | nichts; Internet der Halle prüfen, wenn es anhält |
| orange: **„badhub hat die Anmeldung vorübergehend gesperrt — nächster Versuch um …"** | badhub hat nach mehreren Fehlversuchen eine Sperre verhängt. | abwarten |
| rot: **„Übertragung an badhub angehalten: Passwort prüfen"** | badhub lehnt das Passwort ab. bts-light sendet nichts mehr, damit badhub die Halle nicht sperrt. | Passwort in den Einstellungen berichtigen und speichern |
| rot: **„Übertragung an badhub angehalten: Der Server verlangt eine eigene Anmeldung"** | Vor dem Server liegt eine zusätzliche Anmeldung; das Liveticker-Passwort ist nicht die Ursache. | den Betreiber des Servers ansprechen |
| orange: **„badhub lehnt den Turnierstand ab (HTTP …) — nächster Versuch in … s"** | badhub nimmt den Stand nicht an. | wenn es anhält: Diagnose-Log senden |

| rot: **„Übertragung an badhub ausgefallen — bitte stoppen und neu starten"** | Der Teil der App, der an badhub sendet, ist abgestürzt (seit v0.9.382). Die Halle läuft weiter. | Übertragung stoppen und neu starten; Diagnose-Log senden |

<!-- pruef: "Übertragung an badhub ausgefallen" in src-tauri/src/commands.rs -->
<!-- pruef: "Internet-Strecke gestört — Halle läuft weiter" in src-tauri/src/commands.rs -->

Steht oben in der App ein roter Balken mit **Ein anderer PC ist mit diesem
Turnier verbunden**, läuft die Übertragung desselben Turniers gerade auf
einem zweiten PC. Die Cloud lässt nur einen zu: Tablets und Monitore, die
über die Cloud laufen, erreichen dann den anderen PC, nicht diesen. Die
Übertragung am anderen PC stoppen — der Balken verschwindet nach wenigen
Sekunden von selbst. Er erscheint erst, wenn die Lage länger als 30 Sekunden
anhält.
<!-- pruef: "Ein anderer PC ist mit diesem Turnier verbunden" in src-tauri/src/tablet/ws_verbindung.rs -->

Derselbe rote Balken meldet seit v0.9.434, wenn bts-light den Anschluss für
die Geräte im Hallennetz nicht öffnen konnte: **Der Tablet-Server konnte Port
8088 nicht öffnen**. Dann erreichen Tablets und Monitore im Hallennetz diesen
PC nicht, obwohl die Übertragung läuft. Meist läuft bts-light doppelt oder ein
anderes Programm belegt den Anschluss: das andere Programm schließen, dann die
Übertragung stoppen und neu starten. Bisher stand das nur in der Logdatei.

<!-- pruef: "Der Tablet-Server konnte Port {} nicht öffnen" in src-tauri/src/commands.rs -->
<!-- pruef: "pub const TABLET_PORT: u16 = 8088" in src-tauri/src/tablet/server.rs -->
<!-- pruef: "badhub hat die Anmeldung vorübergehend gesperrt" in src-tauri/src/commands.rs -->
<!-- pruef: "Übertragung an badhub angehalten: Passwort prüfen" in src-tauri/src/commands.rs -->
<!-- pruef: "Der Server verlangt eine eigene Anmeldung" in src-tauri/src/commands.rs -->
<!-- pruef: "badhub lehnt den Turnierstand ab" in src-tauri/src/commands.rs -->

Ist die automatische Feldvergabe eingeschaltet, aber in der
[Turnierleitungs-Oberfläche](turnierleitung-web.md) pausiert, steht oben im
Fenster, auf jeder Seite, ein orangefarbener Balken **„Automatik pausiert"** mit dem
Knopf **Fortsetzen** (seit v0.9.326). Ein Klick darauf lässt die Automatik
wieder freie Felder belegen — wie „Automatik läuft" im Browser.
<!-- pruef: "Automatik pausiert" in src/components/AutoAssignPauseBanner.tsx -->
<!-- pruef: "Fortsetzen" in src/components/AutoAssignPauseBanner.tsx -->

Ist in BTP eine **andere Turnierdatei** geöffnet als die eingetragene, steht
dort ein orangefarbener Balken mit der Frage, ob Datei und Turnier-Kennung
übernommen werden sollen — mit den Knöpfen **Ja, übernehmen** und **Nein**
(seit v0.9.424). bts-light ändert das Turnier nie von allein; was die
Antworten bewirken, steht unter
[Die Suche läuft mit](tp-datei-upload.md#die-suche-läuft-mit).
<!-- pruef: "Ja, übernehmen" in src/components/TpFundBanner.tsx -->

Spricht statt der Cloud-Stimme die Ersatzstimme, steht oben ein
orangefarbener Balken **„Hochwertige Cloud-Ansage (Azure/ElevenLabs)
fehlgeschlagen — die Ersatzstimme übernimmt."** In Klammern dahinter steht der
Grund (seit v0.9.443) — ohne Adresse und ohne Antworttext des Dienstes, zum
Beispiel „keine Verbindung zum Sprachdienst", „keine Antwort innerhalb der
Frist" oder, solange der Dienst nach einem Ausfall gesperrt ist, „zurzeit
nicht erreichbar, die Ersatzstimme spricht, bis der Dienst wieder antwortet".
Was dahintersteckt, steht unter
[Ansage ohne Internet](announcements.md#ansage-ohne-internet-seit-v09443).
<!-- pruef: "die Ersatzstimme übernimmt" in src/components/AzureFallbackBanner.tsx -->
<!-- pruef: "keine Verbindung zum Sprachdienst" in src-tauri/src/tts_text.rs -->
<!-- pruef: "keine Antwort innerhalb der Frist" in src-tauri/src/tts_text.rs -->
<!-- pruef: "zurzeit nicht erreichbar, die Ersatzstimme spricht, bis der Dienst" in src-tauri/src/tts_text.rs -->

### badhub-Abgleich (badhub-Turniere, seit v0.9.450)

Steuert bts-light ein [badhub-Turnier](einstellungen.md), steht auf der Seite
**Status** bei laufender Übertragung ein Kasten mit dem Abgleich:

| Anzeige | Bedeutung |
|---|---|
| **„badhub: aktuell"** | Alles, was in der Halle eingegeben wurde, ist bei badhub angekommen. |
| **„5 Ereignisse warten, letzter Kontakt 10:47"** | Ohne Internet: Die Halle läuft normal weiter, bts-light lädt nach, sobald Netz da ist. |
| **„badhub: noch keine Steuerung — …"** | bts-light hat die Steuerung noch nicht übernommen (meist: beim allerersten Start kein Internet, oder ein anderes Gerät steuert). Eingaben sind solange gesperrt. |
| rot: **„Die Steuerung wurde in badhub zurückgeholt. Eingaben am Turnier-PC gehen nicht mehr an badhub."** | Die Turnierleitung hat in badhub übernommen. Eingaben am Turnier-PC sind gesperrt. |
<!-- pruef: "badhub: aktuell" in src-tauri/src/quelle/badhub/mod.rs -->
<!-- pruef: "letzter Kontakt" in src-tauri/src/quelle/badhub/mod.rs -->
<!-- pruef: "badhub: noch keine Steuerung" in src-tauri/src/quelle/badhub/mod.rs -->
<!-- pruef: "Die Steuerung wurde in badhub zurückgeholt" in src-tauri/src/quelle/badhub/mod.rs -->

Darunter zwei Knöpfe:

- **Turnier beenden / Steuerung an badhub zurückgeben** — erst möglich, wenn
  „badhub: aktuell" steht (braucht Internet). Danach sind Eingaben nur noch in
  badhub möglich, und die Namen der Spielerinnen und Spieler werden von diesem
  PC gelöscht — sonst spätestens 14 Tage nach dem letzten Turniertag.
  <!-- pruef: "Turnier beenden / Steuerung an badhub zurückgeben" in src/components/BadhubAbgleichPanel.tsx -->
  <!-- pruef: /FRIST_TAGE: i64 = 14/ in src-tauri/src/quelle/badhub/mod.rs -->
- **Steuerung wieder übernehmen** — nach der Rückgabe oder wenn badhub die
  Steuerung zurückgeholt hat. bts-light lädt den Stand aus badhub; Eingaben
  dieses PCs, die nicht in badhub angekommen waren, werden verworfen (sie
  bleiben in einer Datei im Turnierordner liegen).
  <!-- pruef: "Steuerung wieder übernehmen" in src/components/BadhubAbgleichPanel.tsx -->

Darunter steht der **Slave-Schlüssel** für einen Ansage-Slave im Hallennetz
(siehe [Einstellungen](einstellungen.md)).
<!-- pruef: "Slave-Schlüssel für einen Ansage-Slave im Hallennetz" in src/components/BadhubAbgleichPanel.tsx -->

Bei einem badhub-Turnier sagt die Statuszeile nie „BTP nicht erreichbar“,
sondern **„badhub-Turnier: …“** mit dem Grund.
<!-- pruef: "badhub-Turnier: {e}" in src-tauri/src/commands.rs -->

### Blaues Band: Verbesserung für Tablets und Anzeigen (seit v0.9.433)

Manchmal lässt sich ein Fehler an einer Geräte-Seite — Zähl-Tablet, Monitor,
Turnierleitungs-Oberfläche — beheben, **ohne bts-light neu zu starten**. Dann
erscheint oben ein blaues Band: „Verbesserung für Tablets und Anzeigen liegt
bereit (Seiten-Stand 2) – ohne Unterbrechung." bts-light hat das Paket schon geladen; es
wechselt aber nichts, bevor jemand am Turnier-PC klickt.

- **Jetzt übernehmen** tauscht die Seiten sofort. Keine Verbindung reißt ab,
  die Übertragung läuft weiter.
- Danach sagt das Band, was mit den Geräten passiert: Ein Tablet **ohne**
  laufendes Spiel lädt von selbst neu. Ein Tablet **im Spiel** zeigt nur einen
  Hinweis und zählt weiter. Monitore lädt man in der Monitor-Verwaltung über
  „Neu laden", die Turnierleitungs-Oberfläche im Browser von Hand.
- **Zurück zum vorigen Stand** nimmt die Änderung wieder weg — ebenfalls ohne
  Unterbrechung. Der zurückgenommene Stand wird danach nicht noch einmal
  angeboten, ein späterer schon. Denselben Knopf gibt es dauerhaft unter
  **Wartung**; dort steht auch der aktuelle Stand („Seiten-Stand 0" heißt:
  die Seiten, die mit der App ausgeliefert wurden).

<!-- pruef: "Jetzt übernehmen" in src/io/updateBanner.mjs -->
<!-- pruef: "Zurück zum vorigen Stand" in src/io/updateBanner.mjs -->
<!-- pruef: "Verbesserung für Tablets und Anzeigen liegt bereit" in src/io/updateBanner.mjs -->
<!-- pruef: "Seiten-Stand" in src/io/updateBanner.mjs -->

**Wen der Tausch erreicht:** alle Geräte, die ihre Seite von **diesem PC**
bekommen — also die im Hallennetz. Geräte, die über die Cloud verbunden sind,
bekommen ihre Seiten vom Server; für sie ändert der Klick nichts, und im
reinen Cloud-Betrieb erscheint das Band gar nicht. Läuft in einer anderen
Halle ein zweiter bts-light-PC, wird er dort einzeln aktualisiert.

bts-light sucht beim Start und danach etwa alle zwanzig Minuten nach einem
Paket. Ohne Internet passiert nichts — ein schon geladenes Paket bleibt
bereit.

## Die Geräte in der Halle

Alle diese Seiten kommen vom Turnier-PC. Die Adresse beginnt mit
`http://bts-light.local:8088` oder der IP-Adresse des PCs im Hallennetz.

### Zum Zählen

| Seite | Gerät | Was drauf ist |
|---|---|---|
| **Felder-Lobby** `/felder` | Zähl-Tablet | Die Startseite eines Tablets: eine Kachel je Feld mit Paarung und Zustand. Antippen führt zum Feld. Ist der Meetingpoint an, steht darunter die Kachel **Meetingpoint** (mit PIN), die das Tablet zur Meetingpoint-Anzeige macht. |
| **Spielzettel** `/court/<Feld>` | Zähl-Tablet | Der eigentliche Zählbildschirm. Siehe [Das Zähltablett bedienen](tablet-bedienen.md). |
| **Demofeld** `/court/demo` | jedes Zähl-Tablet | Übungsspiel mit ausgedachten Namen (seit v0.9.416). Belegt kein Feld und überträgt nichts — gefahrlos mitten im Turnier. Erreichbar über die Kachel in der Felder-Lobby und den Feldwechsel im Mehr-Menü. Siehe [Üben am Demofeld](tablet-bedienen.md#üben-am-demofeld). |
| **Zähltafel** `/court/<Feld>/tafel` | Tablet am Netz oder TV | Nur zwei große Punktzahlen und der Satzstand — wie eine klassische Tafel, ohne Namen. |
| **Anzeige-Hülle** `/anzeige` | Tablet, das nur anzeigen soll | Zeigt eine der Anzeigen bildschirmfüllend, mit Zahnrad zum Umschalten. Praktisch, wenn ein Tablet nicht zählt, sondern nur zeigen soll. |

> **Steht das Tablet hinter dem Feld?** Zähltafel und Spielzettel können die
> Punkte **übereinander** statt nebeneinander anordnen — vorn und hinten statt
> links und rechts. Von allein richtet sich das nach der Geräteausrichtung:
> Hochformat ergibt übereinander.
>
> Fest einstellen lässt sich die **Ansicht** (Seiten und Anordnung in einem
> Zug, seit v0.9.292): in der Anzeige-Hülle genügt ein **Tipp auf die
> Punktzahlen**, ohne PIN — der Zyklus läuft automatisch → links/rechts →
> rechts/links → oben/unten → unten/oben. Dieselbe Wahl steht als Eintrag
> „Ansicht: …" im Zahnrad-Menü der Hülle. Am Zähl-Tablet heißt der Eintrag
> „Anordnung: …" (nur die Anordnung; die Seiten stellt dort „⇄" um) und
> liegt unter „⋯ Mehr → Feld wechseln · Anzeige …". Ein fest zugewiesener
> Fernseher richtet sich **immer** nach seiner Ausrichtung — dort gibt es
> keine feste Einstellung.

### Zum Anzeigen

| Seite | Gerät | Was drauf ist |
|---|---|---|
| **Court-Monitor** `/court/<Feld>/display` | TV am Spielfeld | Namen, Sätze, Punkte, Aufschlag, Aufruf-Uhr. Läuft kein Spiel: Werbung oder Leerlaufbild. |
| **Kombi-Anzeige** `/combo` | ein TV für zwei bis drei Felder | Mehrere Felder als Bänder auf einem Bildschirm. |
| **Hallen-Übersicht** `/info/overview` | großer TV oder Beamer | **Alle** Felder — auch die freien. Bei mehreren Hallen zeigt der Bildschirm eine Halle nach der anderen im Vollbild, statt alle zusammenzuquetschen. Ein Zusatz in der Adresse bindet ihn fest an eine Halle. |
| **In Vorbereitung** `/info/preparation` | TV am Meeting Point | Die als Nächstes anstehenden Spiele, gerufene zuerst. |
| **Siegerehrung** `/info/winners` | TV am Podest | Das Podium der gewählten Disziplin. Ein Bildschirm kann auch **nur einen Platz** zeigen — gedacht für je einen Monitor vor jeder Podeststufe. **Achtung beim dritten Platz:** Im Badminton gibt es **zwei** Dritte, und der Platz-3-Bildschirm zeigt beide untereinander — im Doppel also bis zu vier Namen. |
| **Werbung** `/info/ad` | Werbe-TV | Sponsorenbilder, rotierend oder als Einzelbild. |
| **Stream-Einblendung** `/stream?courts=3` | OBS-Browserquelle beim Livestream | Durchsichtige Score-Leiste für ein bis drei Felder. Adresse nur ans eigene Stream-Team geben — sie enthält in der Cloud die Kennung des Turniers. |
| **Stream-Werbung** `/stream/werbung` | OBS-Browserquelle beim Livestream | Durchsichtige Sponsoren-Ebene: rotierend, nebeneinander oder als Laufband. Zeigt nur Werbebilder mit dem Häkchen „Stream". Adresse aus dem Abschnitt „Stream-Werbung (OBS)" im Panel Monitore. |
<!-- pruef: "/stream/werbung" in src-tauri/src/tablet/server.rs -->

### Zum Steuern

| Seite | Gerät | Was drauf ist |
|---|---|---|
| **Turnierleitungs-Oberfläche** `/tl` | Tablet oder Telefon | Dieselben Entscheidungen wie am PC, aber unterwegs in der Halle: Felder, Aufgaben, Zähltafel-Warteschlange, Schiedsrichter, Spiele, beendete Spiele, Spielzeiten, Anfangszeiten — dazu oben ein Suchfeld, das Spieler und Schiedsrichter im ganzen Turnier findet. Welche dieser Panels ein Gerät zeigt, lässt sich je Gerät einstellen. Siehe [Turnierleitungs-Oberfläche im Browser](turnierleitung-web.md). |

### Zum Ansagen

| Seite | Gerät | Was drauf ist |
|---|---|---|
| **Ansage-Seite** `/ansage` | Tablet, Handy, Pi mit Lautsprecher | Spricht die Ansagen einer Halle, die der Turnier-PC fertig vertont. Halle wählen, einmal **Ton freischalten** — fertig. Die letzte Ansage steht groß zum Mitlesen, darunter die fünf letzten mit ↻ zum Wiederholen auf diesem Gerät (hochkant eingeklappt, ↻ der letzten direkt an der großen Ansage). Nur, wenn am PC „Audio für Ansage-Seiten erzeugen" an ist. Siehe [Ansage-Seite im Browser](announcements.md#ansage-seite-im-browser-seit-v09311). |
| **Ansage am TV** `…?ansage=<Halle>` | Pi am Fernseher | Kein eigenes Ziel, sondern ein Zusatz an der Adresse von Einzelfeld, Feld-Übersicht oder Kombi: Der TV zeigt wie immer und spricht zusätzlich die Ansagen seiner Halle — ohne Knopf. Siehe [Ansage-Modus](court-monitor.md#ansage-modus-der-tv-spricht-mit-seit-v09313). |
<!-- pruef: "/ansage" in src-tauri/src/tablet/server.rs -->
<!-- pruef: "ansageModusAusQuery(welt.location && welt.location.search)" in src/io/ansageKern.mjs -->

### Meetingpoint (seit v0.9.330)

Tablet oder Touch-Bildschirm am Meetingpoint einer Halle:
`/meetingpoint?halle=<Halle>` (LAN, Cloud, Slave-PC; auch als Ansicht der
Anzeige-Hülle). Gerufene Partien, Namen antippen = einchecken. Siehe
[meetingpoint.md](meetingpoint.md).

**Wenn der Turnier-PC weg ist (Cloud):** Monitore und Zähltafeln, die über
die Cloud laufen, zeigen 20 Sekunden lang weiter den letzten Stand und danach
**Keine Verbindung zum Turnier-PC** — der Stand wird abgedunkelt. Sobald der
Turnier-PC wieder verbunden ist, läuft die Anzeige von selbst weiter. Steht
dort **Verbindung unterbrochen**, liegt es an der Leitung des Geräts selbst.
<!-- pruef: "Keine Verbindung zum Turnier-PC" in src-tauri/assets/monitor.html -->
<!-- pruef: "Keine Verbindung zum Turnier-PC" in src-tauri/assets/tafel.html -->
<!-- pruef: "Verbindung unterbrochen" in src-tauri/assets/tafel.html -->

## Wie ein Gerät zu seiner Anzeige kommt

Ein frisch aufgestellter Fernseher zeigt **nicht** sofort ein Feld, sondern
nur das **Logo** — er wartet auf seine Zuweisung. Er meldet sich dabei von
selbst in der App: unter **Monitore** taucht er in der Liste auf und bekommt
dort sein Ziel:

- ein **Feld** (Court-Monitor),
- eine **Zähltafel** für ein Feld,
- eine **Information**: Hallen-Übersicht, In Vorbereitung, Siegerehrung,
- **Werbung**, rotierend oder ein bestimmtes Bild,
- eine **Kombi-Anzeige** aus mehreren Feldern.

Erst danach schaltet das Gerät selbständig auf die richtige Seite um. Du musst
an ihm nichts eintippen — das ist der ganze Sinn der Zuweisung.

**Stream-Einblendung für OBS:** Unter **Monitore** baut der Abschnitt
**Stream-Einblendung (OBS)** die Adresse für die OBS-Browserquelle: bis zu drei
Felder antippen, Flagge, Verein und Turnierlogo bei Bedarf abwählen, fertige
Adresse kopieren. Details: [Stream-Einblendung](court-monitor.md).
<!-- pruef: "Stream-Einblendung (OBS)" in src/pages/CourtMonitorPanel.tsx -->

**Welches Gerät ist welches?** Bei mehreren gleich aussehenden Fernsehern hilft
die Funktion **Identifizieren**: Der angeklickte Bildschirm blendet daraufhin
seinen Gerätecode ein, sodass du ihn in der Liste sicher zuordnen kannst.

**Fernseher schalten.** Hängt ein Monitor an einem Raspberry Pi mit aktueller
SD-Karte, zeigt seine Zeile einen Knopf **TV an** / **TV aus**; über der
Liste stehen **Alle TVs an** und **Alle TVs aus**. So gehen abends alle
Fernseher mit einem Klick aus. Details:
[Fernseher an- und ausschalten](court-monitor.md).
<!-- pruef: "Alle TVs an" in src/pages/CourtMonitorPanel.tsx -->

## Die öffentlichen Seiten auf badhub.de

Diese Seiten gehören nicht zu BTS Light, sondern zu badhub — BTS Light füttert
sie nur. Alle erreicht man ohne Zugangsdaten — nur der Turnierleitungs-Modus
der Check-In-Hallen-Ansicht will das Liveticker-Passwort.

| Seite | Wer schaut drauf |
|---|---|
| **Liveticker** | Zuschauer, Spieler, Eltern zu Hause: alle Felder Punkt für Punkt. |
| **Hallen-Monitor** | Die Online-Fassung der Court-Übersicht, für einen Hallen-TV mit Internet. Lässt sich fest an eine Halle binden. |
| **Nächste Spiele** | Die Online-Fassung der Vorbereitungs-Anzeige. |
| **Check-In-Seite** | Spieler auf dem eigenen Telefon beim Ankommen. |
| **Check-In-Hallen-Ansicht** | Ein Tablet oder Monitor am Eingang der Halle: eine Klasse im Vollbild mit allen Gemeldeten und Namenssuche, dazu ein Absprung zum Zeitplan. Mit dem Liveticker-Passwort schaltet die Turnierleitung dort ihren Modus frei. |
| **Zeitplan** | Alle: die Anfangszeiten je Tag, Disziplin und Klasse. |
| **Teilnehmerliste → eigene Spielerseite** | Ein Spieler sieht, in welcher Halle er spielt, wie viele Spiele noch vor ihm liegen und wann er etwa dran ist. |

Am schnellsten kommst du an diese Adressen über den **Aushang**: Ein A4-Blatt
mit zwei QR-Codes — Teilnehmerliste und Liveticker —, gedruckt über
**Status → Aushang drucken**. Siehe [Aushang für die Halle](aushang.md).

Aus der App heraus:

| Seite | Wo der Knopf sitzt |
|---|---|
| Liveticker, Hallen-Monitor, Nächste Spiele | **Status → Anzeigen im Browser öffnen** |
| Check-In-Hallen-Ansicht (Tablet/Monitor in der Halle) | **Check-In → Hallen-Ansicht** |
| Zeitplan mit allen Anfangszeiten | **Check-In → Zeitplan** |
| Check-In-Seite | **Check-In → Check-In-Seite öffnen** |
| Teilnehmerliste | kein Knopf — sie steht als QR auf dem Aushang |

> Der Block „Anzeigen im Browser öffnen" erscheint nur, wenn eine öffentliche
> Live-Adresse eingetragen ist, und die drei Knöpfe sind grau, solange der
> Liveticker nicht läuft.

## Bildschirme, die man leicht übersieht

- **Der TV-Starter** `/` oder `/tv` — ein Kachel-Menü, das sich mit den
  Pfeiltasten einer Fernbedienung bedienen lässt. Zum schnellen Durchprobieren,
  welche Anzeige auf einen Fernseher soll. Dazu die Kurzadressen `/alle`
  (Hallen-Übersicht), `/next` (nächste Spiele) und `/h/1`, `/h/2` für die
  einzelnen Hallen — kurz genug, um sie an einer Fernbedienung einzutippen.
- **Die Statusseite** `/status` — eine schlichte Liste aller Felder mit ihren
  Adressen und QR-Links. Steht in keinem Menü, ist aber beim Aufbau der
  schnellste Weg.
- **Die Felder-Lobby** `/felder` — wer nur QR-Codes kennt, übersieht sie.
- **Die Anzeige-Hülle** `/anzeige` — erreichbar über das Mehr-Menü des
  Zähl-Tabletts und über „Nur Spielstand anzeigen", wenn ein Feld schon
  besetzt ist.
- **Die Android-Kiosk-App** — sie sucht den Turnier-PC selbst und öffnet die
  Felder-Lobby im Vollbild. Siehe
  [Zähl-Tablet als Kiosk-App (Android)](tablet-android-app.md).
- **Das Geräte-Panel des Slaves** — läuft dieser PC als Slave einer fernen
  Halle, zeigt sein Status-Bildschirm die QR-Codes und Monitor-Links **seiner**
  Halle. Die Crew dort muss nicht auf den Master-Bildschirm schauen. Siehe
  [Master und Slave](master-slave.md).

## Welcher Bildschirm für welche Rolle

**Turnierleitung.** Am PC ist die **Spielübersicht** das Arbeitspferd,
**Status** die Kontrollanzeige. Unterwegs in der Halle die
**Turnierleitungs-Oberfläche** auf Tablet oder Telefon — dieselben
Entscheidungen, ohne am PC zu stehen.

**Am Zähltablett.** Nur zwei Seiten: **Felder-Lobby**, dann der
**Spielzettel**. Alles Weitere passiert dort.

**Hallenaufbau.** **Monitore** in der App ist die zentrale Seite — dort bekommt
jedes Gerät sein Ziel. Am Gerät selbst hilft `/status` für die Adressen und
**Tablets → QR-Codes** für die Feld-Codes.

**Spieler.** Sehen von BTS Light selbst nichts. Für sie zählen die
**öffentlichen Seiten** und in der Halle die **Vorbereitungs-Anzeige** und die
**Hallen-Übersicht**.

## Weiterlesen

- [Installation und erste Schritte](erste-schritte.md)
- [Einstellungen — was jede Option bewirkt](einstellungen.md)
- [Court-Monitor — TV-Anzeige am Spielfeld](court-monitor.md)
- [Turnierleitungs-Oberfläche im Browser](turnierleitung-web.md)
