# Ansage ohne Internet: Ausfall erkennen, Offline-Stimme, Ansagen vorab laden — Spezifikation

> Status: **Freigegeben 2026-11-16** (via /idee: Brief → Grill → How-To → Review). E0 abgeschlossen (Messung, ADR 0105);
> **E1 umgesetzt in v0.9.443**; E2–E5 offen.
> Quelle: Idee vom 16.11.2026. Betroffene Crates: `src-tauri`, `src`, `relay-proto` (nur Lesart eines
> Wertes), `relay` unberührt.
> ADR: [0104](../adr/0104-offline-stimme-engine-und-verpackung.md) (Engine und Verpackung) ·
> [0105](../adr/0105-ansage-ausfall-erkennung-und-rueckfall-im-kern.md) (Ausfall-Erkennung, Rückfall) ·
> [0106](../adr/0106-ansagen-vorab-laden-und-plattencache.md) (Vorab laden, Plattencache).

## Kontext / Problem

Die Ansage kommt von einem Cloud-Sprachdienst (Azure, ElevenLabs, OpenAI oder Gemini). Fällt in der
Halle das Internet aus, passiert heute zweierlei:

1. **Die Ansage hängt.** Es gibt keine Ausfall-Erkennung. Jede Anfrage wartet ihre volle Frist ab
   (10 s, Gemini 20 s). Scheitert die Vertonung in Teilen, folgt ein zweiter Versuch für die ganze
   Ansage — erst dann springt die Ersatzstimme ein. Abgeleitet aus dem Code, nicht gemessen:
   20–30 s Stille nach dem Gong, bei Gemini 40–60 s. Alle folgenden Ansagen stauen sich dahinter,
   und jede wartet wieder genauso lange.
2. **Die Ersatzstimme ist ein hörbarer Bruch.** Es spricht die Windows-Stimme (Katja/Hedda) —
   verständlich, aber deutlich schlechter als die Cloud-Stimme.

Betroffen ist die Turnierleitung, die mitten im Turnier ohne Ansage dasteht, und die Halle, die
Aufrufe verpasst. Hallen-WLAN mit wackligem Internet ist der Normalfall, nicht die Ausnahme.

Was schon trägt: Liegen alle Teile einer Ansage im Cache, geht keine Anfrage ins Netz. Das nutzt
heute nur niemand gezielt, weil „Ansage in Teilen" standardmäßig aus ist und nichts im Voraus
geladen wird.

## Zielbild & Erfolgskriterien

Drei Bausteine, in dieser Reihenfolge gebaut:

- **Ausfall erkennen.** Eine harte Frist beendet das Warten. Nach dem ersten erkannten Ausfall
  wartet keine Ansage mehr auf das Netz; im Hintergrund prüft bts-light, wann der Dienst wieder da
  ist.
- **Offline-Stimme.** Eine freie neuronale deutsche Stimme rechnet direkt in bts-light. Sie ersetzt
  die Windows-Stimme als erste Ersatzstufe und ist zusätzlich als Sprachdienst
  **„Lokal (ohne Internet)"** wählbar. Einmaliger Download von etwa 75 MB, nichts zu installieren.
- **Ansagen vorab laden.** Solange Netz da ist, lädt bts-light die Teile kommender Ansagen mit der
  gewählten Cloud-Stimme auf die Platte — laufend für die nächsten Spiele und auf Knopfdruck für
  das ganze Turnier.

Die Ersatz-Reihenfolge danach: Cloud-Stimme → Offline-Stimme → Windows-Stimme → Browser-Stimme.

**Messbare Erfolgskriterien** (Feldmessung: Internet tot, WLAN weiter verbunden; gemessen wird die
Stille zwischen dem Ende des Gongs — ohne Gong: dem Auslösen — und dem ersten gesprochenen Wort):

| Fall | Zielwert |
|---|---|
| Ansage liegt vollständig vorab geladen bereit | ≤ 3 s, klingt wie immer |
| Erste Ansage nach dem Ausfall, nicht vollständig bereit | ≤ 5 s, Offline-Stimme. Ausnahme: Hält die App beim Ausfall noch eine offene Verbindung zum Dienst, wartet diese eine Ansage die harte Frist (4 s) ab und danach die Rechenzeit der Offline-Stimme |
| Jede weitere Ansage während des Ausfalls | kein Netzversuch; nur noch Rechenzeit der Offline-Stimme. Ausnahme: höchstens einmal je 10 Minuten wartet eine Ansage erneut die harte Frist ab (Ablauf des Merkers, siehe unten) |
| Netz wieder da | eine Ansage, die später als einen Prüftakt (15 s) nach der Rückkehr ausgelöst wird, kommt wieder von der Cloud-Stimme |

Die Messung der Ausfall-Erkennung ist auf einem Windows-Turnier-PC gelaufen (17.11.2026); die
Ausnahme in der zweiten Zeile stammt daher. Die Rechenzeit der Offline-Stimme ist noch nicht
gemessen (zweites Messwerkzeug, vor E2); weicht sie ab, werden die Zielwerte hier und in ADR 0105
korrigiert.

Bedienbarkeit: Einschalten ist je Baustein **ein Schalter**. Der Download startet von selbst und
zeigt seinen Fortschritt. Ohne Eingriff bleibt alles wie heute.

## Nicht-Ziele

- **Stimmen mischen.** Fehlt bei einem Ausfall auch nur ein Teil einer Ansage, spricht die
  Ersatzstimme die **ganze** Ansage. Nie Cloud-Stimme und Ersatzstimme in einer Ansage.
- **Vorab laden an Slaves.** Slaves bekommen nur die Offline-Stimme, je PC selbst einschaltbar.
- **Englische Offline-Stimme.** Englische Ansagen spricht die Windows-Stimme.
- **Kandidaten offener Paarungen vorab laden.** Nur Spiele mit feststehender Paarung.
- **Spontane Freitexte und „fehlende Spieler" am Meetingpoint vorab laden** — sie entstehen erst im
  Moment.
- **„gegen" als eigener Teil.** Der Schnitt der Ansage bleibt, wie er ist; kein Klang-Umbau.
- **Python-Sprachserver, großes Sprachmodell, Grafikkarte.**
- **GPL-Code in der Programmdatei, mitgelieferte DLL** (ADR 0100, ADR 0104).
- **Cloud-Anfrage schon während des Gongs starten.** Wäre eine eigene Beschleunigung; hier nicht
  enthalten.
- **Eigene Stimmen-Auswahl für die Offline-Stimme.** Es gibt genau eine.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - `src-tauri/src/tts_netzlage.rs` (neu): Merker „Dienst ausgefallen" als reiner Zustandsautomat.
  - `src-tauri/src/tts_text.rs`: geteilter HTTP-Client mit Verbindungsfrist, Fehlerklasse
    `TtsFehler`, atomarer Cache, Verfall.
  - `src-tauri/src/{azure_tts,elevenlabs_tts,openai_tts,gemini_tts}.rs`: Fehlerklasse an der Quelle,
    geteilter Cache-Schlüssel.
  - `src-tauri/src/offline_stimme/` (neu): `mod.rs` (Schnittstelle, Attrappe), `modell.rs`
    (Katalog, Download, Prüfsumme), `lage.rs` (Zustand für die Oberfläche), Engine nur unter
    `cfg(windows)`.
  - `src-tauri/src/commands.rs`: `azure_tts_speak`, `seiten_cloud_audio`, `seiten_windows_stimme`,
    `stimme_spielen_auf`, neue Commands (siehe Umsetzungs-Hinweise), Schutz des je-PC-Feldes.
  - `src-tauri/src/config.rs`: `OfflineStimmeConfig`, `DienstArt::Offline`, `AzureTtsConfig.vorab`.
  - `src-tauri/src/tts_verbrauch.rs`: Zählerspalte „vorab".
  - `src-tauri/src/tablet/relay_client.rs`: `azure_share_aus` gibt bei `lokal` nichts weiter.
  - `src/io/announcer.ts`, `src/io/tonWeg.mjs`, `src/io/sprachdienst.mjs`,
    `src/components/AnnounceSettings.tsx`, `src/components/AnsageVorabLader.tsx` (neu), Banner.
- **Architekturregeln (CLAUDE.md R1–R6):** R1 eingehalten — das Frontend baut wie bisher den
  Ansagetext und spricht den Kern nur über Commands an; Ton, Netz, Cache und Stimme liegen im Kern.
  R2, R4, R5, R6 unberührt (keine BTP-Writes, kein neuer Wire-Typ). R3: keine neue Strecke; die
  Ansage-Seiten und der TV-Ansage-Modus laufen über den vorhandenen Seiten-Weg mit, in LAN und
  Cloud gleich.
- **Konfiguration & Abwärtskompatibilität:**
  - `AppConfig.offline_stimme { enabled: bool }`, Standard `false`, `#[serde(default)]`. **Je PC:**
    geschützt wie `announce.output_device` in `keep_host_managed_fields`, `identity_bundle` (wird
    geleert) und `apply_imported_identity` (Stand dieses PCs bleibt). Gesetzt nur über Commands.
  - `AzureTtsConfig.dienst` bekommt den Wert `"lokal"`. **Schon die Version von E2 liest ihn als
    „kein Cloud-Dienst"**, obwohl er erst ab E3 wählbar ist. Grund: Ein unbekannter Dienst gilt
    heute als Azure; eine Vorversion mit hinterlegtem Azure-Schlüssel schickte nach einem Rückweg
    Namen an Azure, obwohl „ohne Internet" gewählt ist. E3 setzt `rueckwaerts_lesbar_ab` in
    `release/datenformat.json` auf die Version von E2.
  - `AzureTtsConfig.vorab: bool`, Standard `false`, `#[serde(default)]`. Geht **nicht** in die
    Share an Slaves.
  - Zeichenzähler `tts-zeichen.json`: neue Spalte `vorab` mit `#[serde(default)]`.
  - Jede dieser Änderungen ändert den Abdruck in `release/datenformat.json` (ADR 0101); außer in
    E3 bleibt die Grenze stehen, weil ältere Versionen die neuen Felder überlesen.
  - Der Ordner `tts-cache/` behält sein Format. Die Stimmdateien liegen in einem eigenen Ordner
    `stimmen/` im App-Datenverzeichnis; sie sind kein eigenes Datenformat von bts-light.
  - `identifier` und Updater-Pfad bleiben unangetastet.
- **Datenschutz:**
  - Die Offline-Stimme rechnet auf dem PC; Namen verlassen ihn dabei nicht.
  - **Vorab laden schickt Namen früher und breiter an den gewählten Anbieter:** vor dem Aufruf,
    und beim Knopf „ganzes Turnier" auch von Spielen, die später nie gerufen werden. Zweck bleibt
    derselbe (die Ansage dieser Personen). Deshalb: Standard aus, der Knopf zeigt vor dem Start den
    Anbieter und den Umfang und verlangt eine Bestätigung.
  - **Ton mit Namen liegt auf der Platte des Turnier-PCs**, jetzt auch der für Ansage-Seiten
    (bisher nur Arbeitsspeicher — Kriterium 10 der Spec `ansage-im-browser` wird bewusst
    aufgehoben). Dafür bekommt der Speicher erstmals einen Verfall: Was 60 Tage nicht benutzt
    wurde, wird gelöscht; dazu der Knopf **„Ansage-Speicher leeren"**. Der Roadmap-Befund „Azure-Audio
    mit Spielernamen dauerhaft auf der Platte" ist damit erledigt.
  - Kein Geburtsjahr, keine Lizenznummer im Ansagetext — unverändert. Der Text einer Ansage wird
    weiterhin nicht protokolliert.
  - CLAUDE.md bekommt mit E4 einen Absatz „Ansagen vorab laden" im Abschnitt Datenschutz.
- **Abhängigkeiten:**
  - Neue Cargo-Abhängigkeit `sherpa-onnx` (Apache-2.0, bringt ONNX Runtime/MIT mit), nur unter
    `[target.'cfg(windows)'.dependencies]`, statisch gelinkt. Die CI lädt das Engine-Archiv selbst
    und prüft es gegen eine SHA-256 im Repo (ADR 0104).
  - Stimme `de_DE-thorsten-medium` (CC0) und ein Aussprache-Lexikon (CC-BY-SA 4.0, Quellenangabe
    in der Doku) als Download von `https://badhub.de/download/bts-light/stimmen/`. Immer vom
    Produktiv-Host, auch im Testsystem-Betrieb (das Testsystem verlangt ein Passwort).
  - Keine neue npm-Abhängigkeit. Kein Relay-Deploy nötig.

## Verhalten im Einzelnen

### Ablauf einer Ansage (PC-Weg und Seiten-Weg gleich)

1. Liegt die Ansage **vollständig** im Cache (alle Teile bzw. die ganze Ansage), wird sie von dort
   gespielt — ohne Netz, egal ob der Merker gesetzt ist.
2. Sonst, wenn der Merker „Dienst ausgefallen" **nicht** gesetzt ist: Vertonung beim Cloud-Dienst
   unter einer harten Frist. Vorhandene Teile kommen aus dem Cache, fehlende aus dem Netz.
3. Sonst, oder wenn Schritt 2 scheitert: die Ersatzstimme spricht die ganze Ansage.

### Fehlerklassen

| Klasse | Auslöser | Merker setzen | Zweiter Versuch „als Ganzes" |
|---|---|---|---|
| Netz | Verbindungsaufbau, Namensauflösung, Zeitüberschreitung, harte Frist, HTTP 5xx | ja | **nein** |
| Drossel | HTTP 429 | nein | nein |
| Zugang | HTTP 401/403 | nein | nein |
| Inhalt | übrige HTTP-4xx, unlesbare Antwort | nein | ja (wie heute) |

Der Merker gilt je Sprachdienst und lebt in `AppState` (nicht in der `SyncEngine` — die wird bei
jedem Speichern neu gestartet). Solange er gesetzt ist, prüft der Kern im Hintergrund durch einen
kostenlosen Verbindungsaufbau (TLS) zum Host des Dienstes, ob er wieder erreichbar ist; eine
gelungene Prüfung löscht ihn. Ohne Prüf-Erfolg läuft er nach einer Höchstdauer ab, damit ein
Fehler in der Prüfung die Cloud-Stimme nicht dauerhaft abschaltet. Bei einem langen Ausfall heißt
das: Nach jedem Ablauf versucht die nächste nicht vollständig gecachte Ansage einmal das Netz,
wartet höchstens die harte Frist und setzt den Merker neu.

Konstanten: harte Frist 4 s · Verbindungsfrist 1,5 s (beide nach E0 festgelegt) · Prüftakt 15 s ·
Höchstdauer des Merkers 10 Minuten (Ausgangswerte, am Bau von E1 zu prüfen).

> **Ergebnis der Messung (17.11.2026):** Der Ausgangswert der harten Frist (2,5 s) hielt am
> Windows-Turnier-PC nicht — zwei von fünf echten Vertonungen brauchten bei gutem Netz länger,
> die längste 3,0 s. Die harte Frist ist deshalb auf 4 s angehoben. Die Verbindungsfrist von
> 1,5 s hat Abstand. Die 4 s stützen sich auf fünf Vertonungen an einem Dienst; E1 schreibt die
> Dauer jeder Vertonung ins Log, damit der Wert am ersten Turnier nachgeprüft werden kann.
> Zahlen und verworfene Wege:
> [ADR 0105](../adr/0105-ansage-ausfall-erkennung-und-rueckfall-im-kern.md), „Ergebnis der
> Messung".

### Ersatzstimme

Der Kern entscheidet an einer Stelle, wer spricht — für den PC und für die Ansage-Seiten:

1. **Offline-Stimme**, wenn auf diesem PC eingeschaltet, geladen und geprüft, und die Ansage
   deutsch ist.
2. sonst **Windows-Stimme** (wie heute).
3. sonst am PC die Browser-Stimme, auf Ansage-Seiten die Stimme des Geräts (wie heute).

Die Offline-Stimme bekommt denselben Text wie die Windows-Stimme: die Sprech-Abschnitte mit
angewandtem Basis- und Community-Wörterbuch, nie SSML. Ein Banner sagt, welche Stimme gerade
spricht (z. B. „Cloud-Stimme nicht erreichbar – es spricht die Offline-Stimme.").

Ist der Schalter aus, ruft der Kern die Offline-Stimme nie auf — die Kette ist dann exakt die
heutige.

### Offline-Stimme einschalten

Im Block der Ansage-Einstellungen: Schalter **„Offline-Stimme verwenden, wenn die Cloud-Stimme
ausfällt"**. Beim ersten Einschalten lädt bts-light Stimme und Lexikon (etwa 75 MB), zeigt den
Fortschritt und prüft die Prüfsumme. Zustände: *nicht geladen* · *wird geladen (n %)* · *bereit* ·
*Download fehlgeschlagen* (Knopf **„Erneut laden"**) · *auf diesem System nicht verfügbar*
(Nicht-Windows). Dazu **„Probe hören"** und **„Stimme entfernen"**. Bis die Stimme bereit ist,
spricht die Windows-Stimme.

Die Prüfsumme wird nach dem Download und einmal je Programmstart geprüft. Passt sie nicht, wird
die Datei gelöscht und der Zustand ist *nicht geladen*. Ist der Schalter an, lädt der Kern das
Modell beim Programmstart im Hintergrund in den Arbeitsspeicher und hält es dort.

### Sprachdienst „Lokal (ohne Internet)"

Eigener Eintrag in der Sprachdienst-Auswahl. Dann geht nichts ins Netz; die Offline-Stimme spricht
jede deutsche Ansage. Die Wahl stößt den Download an, falls die Stimme fehlt. Fehlt sie (noch),
spricht die Windows-Stimme, und ein Banner bietet das Laden an. „Ansage in Teilen" und „Vorab
laden" sind bei dieser Wahl ohne Wirkung und ausgegraut. Ansage-Seiten bekommen das Audio der
Offline-Stimme vom Turnier-PC. An Slaves wird bei dieser Wahl **nichts** vererbt; ein Slave nutzt
seine eigene Einstellung.

### Ansagen vorab laden

- Schalter **„Ansagen der nächsten Spiele vorab laden"** (Standard aus). Er schaltet „Ansage in
  Teilen vertonen" zwingend mit ein; die Oberfläche sagt dazu, dass die Ansage dadurch an den
  Pausen minimal anders klingen kann. Wird „Ansage in Teilen" ausgeschaltet, geht auch dieser
  Schalter aus.
- **Laufend:** Ein Lader im Fenster des Turnier-PCs baut für die nächsten 20 Spiele der Warteliste
  (angesetzt, beide Seiten bekannt; Reihenfolge wie im TL-Web) den Ansagetext mit **denselben**
  Funktionen wie die echte Ansage und gibt ihn dem Kern zum Vertonen ohne Wiedergabe. Dazu der
  Grundstock: Felder, Disziplinen, Klassen, Runden, Hallen, Stufenwörter, „Bitte mit dem Spielen
  beginnen".
- **Knopf „Ansagen vorab laden"** (ganzes Turnier): alle angesetzten Spiele mit bekannten Spielern
  auf beiden Seiten, je Seite in beiden Formen („A." und „gegen A."), dazu gespeicherte
  Freitext-Blöcke. Vor dem Start zeigt ein Dialog Anbieter, Zahl der Anfragen und Zeichen und
  verlangt eine Bestätigung. Der Lauf ist abbrechbar und setzt beim nächsten Start dort fort, wo er
  aufgehört hat — der Cache ist der Fortschritt.
- **Vorrang:** Eine echte Ansage geht immer vor. Vorab laden schickt höchstens eine Anfrage zur
  Zeit und wartet, solange eine echte Ansage vertont wird. Über alle Wege zusammen gehen nie mehr
  als drei Anfragen gleichzeitig an einen Dienst.
- **Drossel des Anbieters (429):** Vorab laden wartet (nach `Retry-After`, sonst wachsend) und macht
  weiter. Trifft eine 429 eine **echte** Ansage, bricht der Vorab-Lauf ab und meldet das.
- **Netzfehler beim Vorab laden** setzt den Merker — so ist ein Ausfall oft schon erkannt, bevor
  eine Ansage ansteht. Solange der Merker gesetzt ist, ruht das Vorab laden.
- **Nach einer Änderung** von Stimme, Tempo, Sprachdienst oder Wörterbuch passen alte Teile nicht
  mehr. Der laufende Lader lädt die nächsten 20 Spiele von selbst neu; der Bestand des Knopfes wird
  nicht von selbst neu geladen — die Anzeige zeigt den gesunkenen Stand.
- **Anzeige:** „Nächste Spiele: 18 von 20 geladen · Grundstock vollständig · Speicher 42 MB".
  Darunter der Hinweis: Ist die Ansage von Tabletbedienung oder Schiedsrichtern eingeschaltet,
  gehört dieser Teil zur Ansage; fehlt er bei einem Ausfall, spricht die Ersatzstimme die ganze
  Ansage.
- **Speicher:** Schreiben immer in eine Nebendatei, dann umbenennen. Ein Teil, der sich nicht als
  Ton lesen lässt, gilt als nicht vorhanden und wird gelöscht. Verfall: 60 Tage unbenutzt
  (Änderungsdatum als „zuletzt benutzt", bei einem Treffer höchstens einmal täglich erneuert);
  geräumt wird beim Programmstart und einmal täglich. **„Ansage-Speicher leeren"** löscht alles.
- **Ansage-Seiten und TV-Ansage-Modus** lesen und schreiben denselben Plattencache.

### Welche Ansage ist vorab ladbar?

| Ansageart | Vorab ladbar? | Anmerkung |
|---|---|---|
| Erst-Ansage aufs Feld | Ja, außer Besetzung | „Feld X.", Disziplin, Runde aus dem Grundstock; „A." und „gegen B." je Spiel. Das Feld muss nicht bekannt sein. |
| 2./3. Aufruf aufs Feld | Ja | Nur das Stufenwort ist neu (Grundstock). |
| Vorbereitung, Erstaufruf | Ja | Namensteile textgleich zur Erst-Ansage. |
| Vorbereitung, 2./3. Aufruf je Partei | Laufend nur Seite A; Seite B allein nur über den Knopf | „B." ohne „gegen" ist ein eigener Teil. |
| Meetingpoint-Aufruf | Ja | Zusatz aus dem Grundstock. |
| Meetingpoint „fehlende Spieler" | Nein | Die Namensliste entsteht erst im Moment. |
| Freitext | Nur gespeicherte Blöcke, nur über den Knopf | Spontane Texte nie. |
| Schiedsrichter/Aufschlagrichter | Nein | Die Besetzung steht selten vorab fest. |
| Tabletbedienung | Nein | Ein Teil samt Namen, erst bei der Feldvergabe bekannt. |
| „Bitte mit dem Spielen beginnen" | Ja | Grundstock. |
| Hallen-Zusatz | Ja | Grundstock. |

**Ehrliche Folge von „nie mischen":** Ansagen mit Tabletbedienung oder Schiedsrichtern (beide
Schalter sind standardmäßig an) sind bei einem Ausfall fast nie vollständig bereit — dann spricht
die Offline-Stimme die ganze Ansage. Vorab laden trägt bei einem Ausfall also vor allem für
Vorbereitungs- und Meetingpoint-Aufrufe und für Turniere, die Bedienung und Schiedsrichter nicht
ansagen. Bei laufendem Netz spart es in jedem Fall die Wartezeit auf den Dienst.

## Akzeptanzkriterien

### E1 — Ausfall erkennen

- [ ] **K1** Antwortet der Cloud-Dienst nicht, endet die Vertonung nach der harten Frist mit der
  Fehlerklasse „Netz" — bei Azure, ElevenLabs, OpenAI und Gemini, im PC-Weg und im Seiten-Weg.
- [ ] **K2** Nach einem Fehler der Klasse „Netz" gibt es **keinen** zweiten Versuch „als Ganzes" —
  im PC-Weg und im Seiten-Weg. Nach einem Fehler der Klasse „Inhalt" gibt es ihn weiterhin.
- [ ] **K3** Solange der Merker gesetzt ist, löst eine nicht vollständig gecachte Ansage **keine**
  Anfrage an den Dienst aus und geht sofort an die Ersatzstimme.
- [ ] **K4** Eine vollständig gecachte Ansage wird auch bei gesetztem Merker aus dem Cache gespielt.
- [ ] **K5** Fehler der Klassen „Drossel", „Zugang" und „Inhalt" setzen den Merker nicht.
- [ ] **K6** Eine gelungene Hintergrundprüfung löscht den Merker; die nächste Ansage geht wieder an
  den Dienst. Ohne Prüf-Erfolg läuft der Merker nach der Höchstdauer ab; scheitert die nächste
  Ansage dann wieder mit „Netz", steht er erneut für die volle Höchstdauer.
- [ ] **K7** Die Hintergrundprüfung verbraucht keine Zeichen (der Zähler bleibt unverändert) und
  läuft nur, solange der Merker gesetzt ist.
- [ ] **K8** Der Merker übersteht das Speichern der Einstellungen (Neustart der Übertragung).
- [ ] **K9** Der Merker gilt je Dienst: Ein Ausfall von Azure sperrt nach einem Dienstwechsel nicht
  ElevenLabs.
- [ ] **K10** Das Frontend wartet auf `azure_tts_speak` höchstens harte Frist + 0,5 s und geht dann
  an die Ersatzstimme, auch wenn der Kern nicht antwortet.
- [ ] **K11** Ohne Ausfall ändert sich nichts Hörbares: gleiche Stimme, gleicher Text, gleiche
  Reihenfolge der Stufen; der Rückfall-Banner erscheint wie bisher.

### E2 — Offline-Stimme als Ersatz

- [ ] **K12** Schalter aus (Auslieferungszustand): Die Offline-Stimme wird nie aufgerufen; die
  Kette ist die heutige (Test mit Attrappe: kein Aufruf).
- [ ] **K13** Schalter an, Stimme bereit, deutsche Ansage, Cloud-Dienst ausgefallen: Es spricht die
  Offline-Stimme über das gewählte Ausgabegerät; der Banner nennt sie.
- [ ] **K14** Englische Ansage: Es spricht die Windows-Stimme, nie die deutsche Offline-Stimme.
- [ ] **K15** Stimme fehlt, Download läuft noch, Prüfsumme falsch oder Engine meldet einen Fehler:
  Es spricht die Windows-Stimme; die Ansage fällt nicht aus.
- [ ] **K16** Download: Eine Datei mit falscher Prüfsumme oder falscher Größe wird verworfen und
  nie geladen; ein abgebrochener Download hinterlässt keine als fertig geltende Datei; „Erneut
  laden" führt zum Zustand *bereit*.
- [ ] **K17** Der Download schreibt nur in `stimmen/` im App-Datenverzeichnis unter festen
  Dateinamen und lädt nur von der fest eingebauten Adresse; nichts aus der Antwort bestimmt Pfad
  oder Ziel.
- [ ] **K18** Ein Wort, das nicht im Lexikon steht (z. B. ein seltener Familienname), wird
  gesprochen und nicht ausgelassen.
- [ ] **K19** Ansage-Seiten und TV-Ansage-Modus bekommen bei einem Ausfall das Audio der
  Offline-Stimme vom Turnier-PC (statt der Windows-Stimme).
- [ ] **K20** „Abbrechen" einer Ansage beendet auch eine laufende Berechnung der Offline-Stimme:
  Es kommt kein Ton mehr nach.
- [ ] **K21** Der Schalter gilt je PC: Er reist nicht im Identitäts-Bündel, wird beim Import nicht
  überschrieben und nicht vom Host-Abgleich zurückgesetzt.
- [ ] **K22** Eine `config.json` ohne das neue Feld lädt mit „aus". Der Linux-Build ist grün; dort
  meldet die Oberfläche *auf diesem System nicht verfügbar*.
- [ ] **K23** `dienst = "lokal"` in der Konfiguration führt schon in dieser Version zu **keiner**
  Anfrage an einen Cloud-Dienst — auch wenn ein Azure-Schlüssel hinterlegt ist.
- [ ] **K24** Die Programmdatei enthält keinen GPL-Code; neben ihr liegt keine zusätzliche Datei
  (`--selbsttest` und Kernwechsel laufen unverändert).

### E3 — Sprachdienst „Lokal (ohne Internet)"

- [ ] **K25** Die Auswahl zeigt „Lokal (ohne Internet)"; nach der Wahl spricht jede deutsche Ansage
  mit der Offline-Stimme, ohne Netzanfrage und ohne Zeichenverbrauch.
- [ ] **K26** Stimme fehlt bei dieser Wahl (z. B. nach Import auf einem anderen PC): Es spricht die
  Windows-Stimme, ein Banner bietet das Laden an.
- [ ] **K27** Bei dieser Wahl geht **keine** Stimm-Konfiguration an Slaves (`azure_share_aus`
  liefert nichts) — auch kein hinterlegter Schlüssel eines anderen Dienstes.
- [ ] **K28** `scripts/test-sprachdienst.mjs` hält Frontend und Kern für den neuen Wert im Gleichlauf.
- [ ] **K29** `release/datenformat.json` nennt als `rueckwaerts_lesbar_ab` die Version von E2.

### E4 — Vorab laden, laufend

- [ ] **K30** Einschalten setzt „Ansage in Teilen vertonen" mit; Ausschalten von „Ansage in Teilen"
  nimmt „Vorab laden" mit. Der Kern behandelt „vorab an, Teile aus" wie „Teile an".
- [ ] **K31** Vorab laden löst **nie** eine Wiedergabe aus und legt nichts in den Ansage-Verlauf.
- [ ] **K32** Ein vorab geladener Namens-Teil hat denselben Cache-Schlüssel wie derselbe Teil der
  echten Ansage — geprüft je Ansageart der Tabelle, auch bei anderem Feld.
- [ ] **K33** Sind alle Teile einer Ansage vorab geladen, spielt sie bei totem Internet mit der
  Cloud-Stimme, ohne Netzanfrage.
- [ ] **K34** Fehlt bei gesetztem Merker ein Teil, spricht die Ersatzstimme die **ganze** Ansage;
  kein Stück der Cloud-Stimme ist zu hören.
- [ ] **K35** Während eine echte Ansage vertont wird, startet Vorab laden keine neue Anfrage; über
  alle Wege gehen nie mehr als drei Anfragen gleichzeitig an einen Dienst.
- [ ] **K36** Ein Netzfehler beim Vorab laden setzt den Merker; solange er gesetzt ist, ruht das
  Vorab laden.
- [ ] **K37** Vorab geladene Zeichen erscheinen im Zeichenzähler in einer eigenen Spalte „vorab";
  ein bereits vorhandener Teil kostet nichts.
- [ ] **K38** Eine abgebrochene Schreiboperation hinterlässt keinen als gültig geltenden Eintrag;
  ein unlesbarer Eintrag gilt als nicht vorhanden und wird gelöscht.
- [ ] **K39** Ein Eintrag, der 60 Tage nicht benutzt wurde, ist nach dem nächsten Räumen weg; ein
  benutzter bleibt. „Ansage-Speicher leeren" löscht alle Einträge und nichts außerhalb von
  `tts-cache/`.
- [ ] **K40** Ansage-Seiten nutzen einen vorab geladenen Teil vom Plattencache (keine zweite
  Anfrage an den Dienst).
- [ ] **K41** Nach Änderung von Tempo, Stimme oder Wörterbuch zeigt die Anzeige die betroffenen
  Spiele als nicht geladen; der Lader holt die nächsten 20 neu.
- [ ] **K42** Spiele mit offener Paarung werden nicht vorab geladen. Am Slave läuft der Lader nie.
- [ ] **K43** Bei Sprachdienst „Lokal", bei ausgeschalteter Cloud-Stimme und bei ausgeschalteten
  Ansagen lädt nichts vorab.

### E5 — Knopf „Ansagen vorab laden"

- [ ] **K44** Vor dem Start nennt der Dialog Anbieter, Zahl der Anfragen und Zeichen; ohne
  Bestätigung geht keine Anfrage hinaus.
- [ ] **K45** Abbrechen stoppt nach der laufenden Anfrage; ein neuer Start lädt nur, was fehlt.
- [ ] **K46** Antwortet der Dienst mit 429, wartet der Lauf und setzt fort; die Wartezeit folgt
  `Retry-After`, wenn vorhanden.
- [ ] **K47** Trifft eine 429 eine echte Ansage, bricht der Vorab-Lauf ab und meldet das; die echte
  Ansage geht an die Ersatzstimme.
- [ ] **K48** Fällt das Netz mitten im Lauf aus, pausiert er und macht nach Rückkehr des Dienstes
  weiter; schon Geladenes bleibt.
- [ ] **K49** Gespeicherte Freitext-Blöcke werden mitgeladen; spontane Freitexte nie.

### Feldmessung (manuell, vor der Freigabe von E2 und E4 zum Release)

- [ ] **K50** Turnier-PC im Hallen-WLAN, Internet am Router getrennt: Die Zielwerte der Tabelle
  oben werden mit einer Stoppuhr an je zehn Ansagen eingehalten.

## Tests

TDD; `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings` und `npm run build` grün.

**Rust-Unit-Tests**

- `tts_netzlage`: Zustandsautomat mit gereichter Uhr — setzen, je Dienst getrennt, Prüf-Erfolg
  löscht, Ablauf nach Höchstdauer, kein Setzen bei Drossel/Zugang/Inhalt (K3, K5, K6, K9).
- Fehlerklasse je Dienst: Abbildung von Verbindungsfehler, Zeitüberschreitung und HTTP-Codes (K1, K5).
- Harte Frist mit `tokio::time::pause`: hängende Vertonung endet als „Netz" (K1).
- `teile_zusammenbauen` und Seiten-Weg mit Attrappen-Vertoner: kein Ganz-Versuch nach „Netz",
  Ganz-Versuch nach „Inhalt"; bei gesetztem Merker null Aufrufe; voller Cache spielt trotz Merker
  (K2, K3, K4).
- Reihenfolge der Ersatzstimme mit Attrappe: Schalter aus → nie gerufen; Fehler der Attrappe →
  Windows-Weg; Englisch → Windows-Weg (K12, K14, K15).
- `modell`: Prüfsumme und Größe falsch → verworfen; Abbruch → keine fertige Datei; fester Zielpfad
  (K16, K17).
- Phonem-Weg: unbekanntes Wort liefert eine nicht leere Lautfolge; Ausnahmeliste hat Vorrang (K18).
- Config: alte Datei lädt; Schutz des je-PC-Feldes in `keep_host_managed_fields`,
  `identity_bundle`, `apply_imported_identity` (K21, K22).
- `dienst_art("lokal")`, `vollstaendig()` mit Lage der Offline-Stimme, `azure_share_aus` leer
  (K23, K25, K27).
- Cache: atomar schreiben, unlesbarer Eintrag = Fehltreffer + gelöscht, Verfall mit gesetztem
  Änderungsdatum, Leeren bleibt im Ordner (K38, K39).
- Geteilter Cache-Schlüssel: „bereit" genau dann, wenn die Vertonung aus dem Cache käme (K32, K33).
- Vorrang und Höchstzahl gleichzeitiger Anfragen mit Zähl-Attrappe (K35); 429-Wartezeit mit und
  ohne `Retry-After` (K46); Abbruch bei Live-429 (K47).
- Zeichenzähler: alte Datei lädt, Spalte „vorab" zählt, Treffer kostet nichts (K37).
- `datenformat_abdruck` mit neuem Abdruck je Etappe (K29).

**Pflicht-Test „Vorab-Teil = Live-Teil"** (K32): Ein Node-Skript erzeugt mit den echten
`prepare*`-Funktionen für dieselbe Paarung den Text der echten Ansage und den des Vorab-Laders
(anderes Feld) als Fixture; ein Rust-Test schneidet beide und verlangt gleiche Namens-Teile und
gleiche Schlüssel — je Ansageart der Tabelle. Ein Drift-Test hält die Fixture aktuell.

**Node-Tests**

- `scripts/test-ton-weg.mjs`: Frist um die Vertonung (K10).
- `scripts/test-sprachdienst.mjs`: Wert `lokal`, Drift gegen `dienst_art` (K28).
- `scripts/test-ansage-vorab.mjs` (neu): Auswahl der nächsten 20, offene Paarungen bleiben draußen,
  Kopplung der beiden Schalter, Anzeigetext (K30, K41, K42).

**Messwerkzeuge** (`--ignored`, laufen nicht in der CI)

- `tests/tts_ausfall_probe.rs`: Zeit bis zum Fehler je Ausfallart (Kabel, Namensauflösung, stumme
  Gegenstelle, Anmeldeseite eines Hotspots), mit und ohne Verbindungsfrist. **Gebaut (16.11.2026).**
  Aufruf: `cargo test -p bts-light --test tts_ausfall_probe -- --ignored --nocapture`, die Lage
  benennt `BTS_PROBE_LAGE`. Welche Lagen am Windows-PC zu messen sind, steht im Kopf der Datei;
  die Ergebnisse stehen in ADR 0105 („Ergebnis der Messung").
- `tests/offline_stimme_probe.rs`: Ladezeit, Rechenzeit, Arbeitsspeicher, Zeit bis zum ersten Ton,
  2.000 Ansagen am Stück ohne Absturz; schreibt die Hörproben als WAV.

**Manuell:** K50; Hörvergleich der 20 Probe-Ansagen (E0); Download auf einem frischen PC.

## Risiken & Rollback

| Risiko | Folge | Gegenmittel |
|---|---|---|
| Harte Frist zu knapp | Langsames, aber lebendes WLAN löst die Ersatzstimme aus | Nach E0 von 2,5 s auf 4 s angehoben; Dauer jeder Vertonung im Log; Konstante, per Hotfix änderbar; Merker löscht sich nach gelungener Prüfung |
| Hintergrundprüfung meldet „erreichbar", die Vertonung scheitert trotzdem | Jede Ansage wartet wieder die Frist ab | Die Frist begrenzt den Schaden auf 4 s; der nächste Netzfehler setzt den Merker erneut |
| Absturz der Engine im Prozess | bts-light stürzt mitten im Turnier ab | Dauerlauf in E0 (2.000 Ansagen). Zeigt er einen Absturz, rechnet die Stimme in einem Kindprozess derselben Programmdatei (Sonderaufruf wie `--selbsttest`) — vor der Auslieferung von E2. Schalter aus = Engine wird nie berührt |
| Offline-Stimme betont Namen falsch | Schlechter verständlich als erhofft | Hörvergleich in E0 entscheidet; Ausnahmeliste; Wörterbuch wirkt wie bei der Windows-Stimme |
| Phonem-Weg V1 trägt nicht | E2/E3 nicht baubar | ADR 0104: V2 nur nach ausdrücklicher Freigabe; sonst entfallen E2/E3, E1/E4/E5 bleiben |
| Programmdatei wächst stark | Größere Updates | E0 misst den Zuwachs; über 40 MB wird vor E2 neu entschieden |
| Rechenlast bremst Sync oder Tablets | Verzögerte Ergebnisse | Berechnung außerhalb der Async-Laufzeit, eine zur Zeit, begrenzte Thread-Zahl; E0 misst auf einem schwachen PC |
| Vorab laden kostet unerwartet | Zeichenverbrauch | Standard aus; Schätzung + Bestätigung; eigene Zählerspalte; laufend nur 20 Spiele |
| 429 trifft die echte Ansage | Ersatzstimme trotz Netz | eine Anfrage zur Zeit; Abbruch des Laufs bei der ersten Live-429 |
| Vorab-Teil passt nicht zum Live-Teil | Vorab laden wirkungslos, Zeichen doppelt bezahlt | Pflicht-Test je Ansageart |
| Erster Programmstart räumt einen alten, großen Cache | kurze Plattenlast | Räumen im Hintergrund, nicht im Startpfad |

**Rollback:** Jede Etappe hat einen Schalter, dessen Aus-Stellung das heutige Verhalten ist
(Offline-Stimme aus, Vorab laden aus). E1 hat keinen Schalter und kein gespeichertes Feld; der
Rückweg auf die Vorversion (ADR 0101) ist jederzeit möglich. E2, E4, E5: Die Vorversion überliest
die neuen Felder; Stimmdateien und Cache bleiben liegen und stören nicht. E3: Rückweg nur bis zur
Version von E2 (siehe oben).

## Offene Fragen / Annahmen

**Entscheidet der Nutzer mit der Freigabe:**

1. **Stimme vom badhub-Spiegel.** Stimme und Lexikon liegen auf `badhub.de/download/bts-light/stimmen/`;
   Prüfsummen stehen in der Programmdatei. Eine neue Stimme kommt mit einer neuen App-Version.
2. **Nur Deutsch.** Englische Ansagen spricht die Windows-Stimme.
3. **Folge von „nie mischen"** (siehe Tabelle): Mit Tabletbedienung/Schiedsrichtern in der Ansage
   trägt bei einem Ausfall fast immer die Offline-Stimme, nicht der Vorab-Bestand.
4. **Messgröße:** Stille nach dem Gong, nicht Zeit ab dem Auslösen.
5. **Ablauf des Merkers.** „Kein Warten mehr nach dem ersten erkannten Ausfall" gilt mit einer
   Ausnahme: Bei einem langen Ausfall wartet höchstens einmal je 10 Minuten eine Ansage erneut die
   harte Frist ab. Ohne diesen Ablauf könnte ein Fehler in der Hintergrundprüfung die Cloud-Stimme
   bis zum Programmende abschalten.

**Entscheidet die Messung E0, danach der Nutzer:**

6. **Weg vom Text zum Laut.** Bevorzugt: freies Lexikon + eigener Schätzer (V1). Trägt er im
   Hörvergleich nicht, steht ein kleines GPL-Hilfsprogramm im Stimmpaket (V2) zur Wahl — nur mit
   ausdrücklicher Freigabe.

**Annahmen (ungemessen, E0 prüft):**

- Rechenzeit 0,2–0,6 s für einen Satz von 4 s auf einem i5 der 8. Generation; Arbeitsspeicher
  150–250 MB bei geladenem Modell; Ladezeit des Modells unter 2 s.
- Zuwachs der Programmdatei etwa 20 MB.
- Die Stimme `thorsten-medium` in ihrer aktuellen Fassung läuft in sherpa-onnx 2.x.
- Die Sprache einer Namens-Zeile hängt nur an der eigenen Seite, nicht am Gegner. Wäre es anders,
  fehlten nach dem Knopf Teile späterer Runden — sie kosteten dann eine normale Anfrage.
- „Die nächsten 20 Spiele" ist ein fester Wert ohne Einstellfeld.
- Der Download der Stimme geht immer an den Produktiv-Host, auch im Testsystem-Betrieb.
- Über 40 MB Zuwachs der Programmdatei wird vor E2 neu entschieden.
- Ein Gratis-Schlüssel bei Gemini ist nicht erkennbar; der Bestätigungsdialog weist darauf hin,
  dass der Anbieter das Laden ausbremsen kann.

**Bekannte Grenze:** Eine Meldung, die noch in keinem Spiel mit bekanntem Gegner steht (Freilos in
der ersten Runde), fehlt nach dem Knopf, bis ihre erste Paarung feststeht; der laufende Lader holt
sie dann.

## Betroffene Doku-Dateien

| Etappe | Dateien |
|---|---|
| alle | `docs/changelog.md` · `docs/roadmap.md` · diese Spec (Stand, Messergebnisse) |
| E0 | ADR 0104 und 0105 („Ergebnis der Messung") |
| E1 | `docs/announcements.md` (neuer Abschnitt „Ansage ohne Internet", Korrektur „gegen" an zwei Stellen) · `docs/oberflaechen.md` (Banner-Texte, mit `pruef`-Ankern) · `docs/logging.md` (neue Log-Zeilen) |
| E2 | `docs/announcements.md` · `docs/einstellungen.md` (Schalter, Standard aus, `pruef`-Anker) · `docs/oberflaechen.md` · `docs/release.md` (Engine-Archiv in der CI, Stimmpaket auf den Spiegel legen, Quellenangabe des Lexikons) · `docs/multi-hall.md` (je PC, Slaves) · CLAUDE.md (neue Zeile in der Doku-Tabelle) · `release/datenformat.json` |
| E3 | `docs/announcements.md` · `docs/einstellungen.md` · `docs/cloud-relay.md` (keine Share bei `lokal`) · `docs/release.md` (Rückweg-Grenze) · `release/datenformat.json` |
| E4 | `docs/announcements.md` (Vorab laden, Tabelle, Speicher) · `docs/einstellungen.md` · `docs/features/ansage-im-browser.md` (Nachtrag zu Kriterium 10) · `docs/features/ansage-in-teilen.md` (Nachtrag zu T5) · CLAUDE.md (Abschnitt Datenschutz: „Ansagen vorab laden") · `release/datenformat.json` |
| E5 | `docs/announcements.md` · `docs/einstellungen.md` · `docs/oberflaechen.md` · `release/datenformat.json` |

`docs/einstellungen.md` und `docs/oberflaechen.md` stehen im öffentlichen Handbuch; neue
Beschriftungen und Standardwerte bekommen `pruef`-Anker.

## Umsetzungs-Hinweise

Jede Etappe ist ein eigener PR mit eigener Version (Nummer erst beim Merge festlegen; gemeinsam in
`src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `package.json`). `code-reviewer` nach jeder
Änderung. Kein Relay-Deploy nötig.

**E0 — Messen (zwei Messwerkzeuge, kein Produktivverhalten)**
`tts_ausfall_probe` vor E1, `offline_stimme_probe` vor E2. Dazu: Zuwachs der Programmdatei, Link
gegen das Archiv `win-x64-static-MD`, Hörvergleich derselben 20 Ansagen (deutsche, polnische,
vietnamesische, englische Namen) für V1, espeak-Phoneme und Supertonic 3. Ergebnisse in ADR 0104
und 0105; Konstanten und Zielwerte hier nachtragen. Danach Entscheidung zu Punkt 6.

**E1 — Ausfall erkennen**
- `tts_text.rs`: ein geteilter Client mit `connect_timeout`; `TtsFehler { Netz, Drossel, Zugang,
  Inhalt }`, erzeugt an der Quelle in den vier Dienst-Modulen.
- `tts_netzlage.rs`: Merker je Dienst, Uhr wird gereicht; Feld `netzlage` in `AppState`;
  Hintergrundprüfung als Task, der nur bei gesetztem Merker läuft.
- `commands.rs`: Frist um die ganze Vertonung in `azure_tts_speak` (1194–1236) und
  `seiten_cloud_audio` (5713 ff.); Ganz-Versuch nur noch bei „Inhalt"; bei gesetztem Merker erst
  Cache prüfen, dann sofort Fehler „Netz".
- `announcer.ts` `playPrepared` und `tonWeg.mjs`: `mitFrist` um die Vertonung.
- Vorsicht: `tts_dienste_pruefen` bucht Zeichen — nicht als Hintergrundprüfung verwenden.
- Log: je Vertonung Dienst, Dauer in Millisekunden, Zeichenzahl und Ausgang (gelungen,
  Fehlerklasse, harte Frist) — **nie** der Ansagetext. Damit lässt sich die harte Frist von 4 s
  am ersten Turnier nachprüfen.

**E2 — Offline-Stimme als Ersatz** (`security-reviewer` Pflicht: Download, Dateipfade)
- `offline_stimme/mod.rs`: Schnittstelle „Abschnitte hinein, PCM heraus" nach dem Vorbild
  `winrt_stimme::pcm_fuer`; Zusammenfügen über `ansage_ton::stimme::fuege_segmente` (gleicht
  Abtastraten an). Attrappe für Tests. Engine nur `cfg(windows)`.
- `modell.rs`: Katalog-Konstanten (Name, Größe, SHA-256), Download in Nebendatei → prüfen →
  umbenennen.
- `commands.rs`: eine Funktion `ersatz_pcm` (Offline, dann Windows), benutzt von
  `stimme_spielen_auf` und `seiten_windows_stimme`; der Abspielbericht nennt die Stufe. Commands
  `offline_stimme_lage`, `_laden`, `_abbrechen`, `_setzen`, `_probe`, `_entfernen` (Muster
  `ansage_ausgabe_setzen`). Berechnung in `spawn_blocking`; die Generation aus `ansage_ton`
  verwirft Ergebnisse abgebrochener Ansagen.
- `config.rs`: `OfflineStimmeConfig`; Lesart `lokal` = kein Cloud-Dienst.
- Nie in `zusammensetzen` mischen (lehnt verschiedene Abtastraten ohnehin ab).
- CI: Schritt „Engine-Archiv laden, prüfen, cachen" im Windows-Job; Linux-Job ohne Engine.

**E3 — Sprachdienst „Lokal"**
`DienstArt::Offline` (im Code „Offline" nennen — `tts_ziel_lokal` heißt schon „eigene Konfig"),
`DIENSTE`, `vollstaendig(…, lage)`, `sprachdienst.mjs` + Drift-Test, `azure_share_aus`,
Ausgrauen der beiden Schalter, Banner „Stimme laden".

**E4 — Vorab laden, laufend**
- Geteilter `TtsZiel::cache_schluessel(teil, format)`, den Vertonung und Abfrage nutzen.
- Commands `ansage_vorab_laden(ssmls)`, `ansage_teile_bereit(ssmls)`, `ansage_speicher_lage`,
  `ansage_speicher_leeren`; Zustand in `AppState.vorab`.
- `AnsageVorabLader.tsx` neben `AnsageVertoner`, nur am Master; Quelle `preparationCandidates()`;
  neu rechnen nach Speichern und nach dem Nachladen des Wörterbuchs.
- Atomarer Cache und Verfall in `tts_text.rs`; `seiten_cloud_audio` bekommt den Plattencache.
- Globale Begrenzung auf drei Anfragen je Dienst; Live setzt einen Zähler mit Wächter, Vorab
  wartet davor.

**E5 — Knopf „ganzes Turnier"**
`ansage_vorab_schaetzen`, Start/Abbruch/Status, Wartelogik bei 429, Spalte `vorab` in
`tts_verbrauch.rs`, Bestätigungsdialog in `AnnounceSettings.tsx`.
