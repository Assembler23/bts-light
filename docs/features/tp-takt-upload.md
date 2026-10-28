# Takt-Upload der TP-Datei (Stufe 2) — Spezifikation

> Status: **Freigegeben 28.10.2026** (Spec-Pipeline von Hand: Brief → Grill
> → How-To → Review, siehe
> [spec-pipeline-konzept.md](../spec-pipeline-konzept.md)). **Umgesetzt in
> v0.9.301** (PR L2; Handbuch [tp-datei-upload.md](../tp-datei-upload.md),
> Abschnitt „Automatisch senden“).
> Baut auf [tp-datei-upload.md](tp-datei-upload.md) (Stufe 1, umgesetzt in
> v0.9.300 und badhub #1045–#1079) auf und ersetzt dort das Nicht-Ziel „Kein
> Takt in Stufe 1“. Betroffenes Repo: **nur bts-light**. badhub bleibt
> unverändert (Cooldown, SHA-Dedup und Deckel je Zugang sind da).
> ADR: [0061](../adr/0061-takt-upload-lebenszyklus-schnellhash-anhalteregel.md).
> Messung 28.10.2026: ungegateter SHA-256 der offenen 3-MB-Datei in 41 ms;
> Datei ohne Eingaben über sieben Minuten byte- und mtime-stabil.

## Kontext / Problem

Stufe 1 schickt die TP-Datei nur auf Knopfdruck. Am Testturnier 5007 stand
am 28.10.2026 eine Datei von 00:14 Uhr neben einem Liveticker-Stand von
05:48 Uhr — niemand drückt den Knopf, während das Turnier läuft. Seit
badhub #1075 zeigt die Turnierseite außerdem Meldungen, Setzplätze und
Platzhalter im Baum; jede Auslosung, Nachmeldung oder Feldänderung in BTP
erreicht badhub nur mit einem neuen Upload.

Vorhanden (Stufe 1): Dateiwahl mit GUID-Bindung, Schreib-Gate (ADR 0060),
stabiles Lesen, `POST /api/tp-upload` mit Antwort-Abbildung, Status-Merker in
`AppState`, Handbuch. Serverseitig: gleiche Bytes → `unchanged`/`rejected`
ohne Cooldown, neue Bytes frühestens alle 60 s je Turnier, höchstens fünf
Annahmen je Minute und Zugang, Worker minütlich, Turnierseite 60 s Cache.

## Zielbild & Erfolgskriterien

Einmal eingeschaltet, prüft bts-light **solange der Liveticker läuft** in
einem einstellbaren Takt (Standard 15 Minuten), ob sich die TP-Datei seit dem
letzten dem Server bekannten Stand geändert hat, und sendet sie nur dann —
über denselben Weg wie der Knopf. Der Turnierleiter sieht im Status-Fenster,
wann zuletzt geprüft und gesendet wurde, wann der nächste Termin ist und ob
die Automatik angehalten wurde und warum.

Erfolgskriterien:

1. Nach einer Änderung in BTP (Auslosung, Ergebnis von Hand, Feld) ist der
   neue Stand spätestens nach Takt + 60 s Seitencache auf badhub sichtbar,
   ohne dass jemand klickt.
2. Eine unveränderte Datei kostet je Takt weder das Schreib-Gate noch einen
   Netzzugriff noch eine Log-Zeile.
3. Eine Antwort, die ein neuer Upload nicht heilen kann (Endimport,
   falscher Verband, falsches Passwort, Datei gehört nicht zur Kennung,
   Datei weg), hält die Automatik an; der Grund steht im Status-Fenster, und
   eine passende Korrektur im Setup setzt sie wieder in Gang.
4. BTP-Writes warten je geänderter Datei höchstens so lange wie beim Knopf
   (typisch 5 s, ADR 0060); unveränderte Takte kosten BTP nichts.

## Nicht-Ziele

- **Keine badhub-Änderung.** Kein neuer Endpunkt, keine Push-Benachrichtigung,
  kein kürzerer Cooldown.
- **Kein Takt ohne laufenden Liveticker.** Vorabend-Upload nach der Auslosung
  bleibt Knopfsache (F3: der Knopf ist die Freigabe). Kein Takt im
  Slave-Modus, aus dem TL-Web oder vom Tablet.
- **Kein Dateiwächter** („bei jeder Änderung sofort“), kein Takt unter
  5 Minuten.
- **Kein mtime-Vorfilter.** Die Änderungserkennung hasht die Datei; das
  mtime-Verhalten der offenen Jet-Datei bleibt ungemessen und ungenutzt.
- **Keine Änderung an den Gate-Konstanten** `RUHE`/`MAX_WARTEZEIT` (hängen an
  der Jet-Nachlauf-Messung, Roadmap).
- **Kein automatisches Finden der Datei** über den BTP-Prozess.
- **Keine Änderung der Knopf-Texte** (Handbuch-Anker aus Stufe 1 bleiben).

## Entscheidungen (Grill 28.10.2026)

| Frage | Entscheidung |
|---|---|
| T1 Änderungserkennung | **Schnell-Hash ohne Gate:** Datei mit derselben `lesen`-Funktion (Jet-Kopf, 32 MB) ohne Schreibsperre lesen, SHA-256 (`ring::digest`), gegen den Merker des letzten dem Server bekannten Stands vergleichen. Nur bei Abweichung Gate + stabiles Lesen + Senden. |
| T2 Laufzeit | **Nur solange der Liveticker läuft.** Takt-Task lebt im Sync-Task (`TaktWaechter`), Zustand in `AppState`. Erster Lauf frühestens 60 s nach Start; der Sync-Neustart beim Speichern verschiebt keinen Termin. |
| T3 Halt und Wiederanlauf | Fatal = 409, 403, 401, 413, `guid_mismatch`, `final_imported` (aus dem Status), keine TP-Datei, zu groß, verletzte Vorbedingung; „nicht lesbar“ erst ab dem dritten Mal in Folge (vorübergehende Sperren durch Virenscanner oder Netzlaufwerk). Wiederanlauf durch Speichern mit geänderter Datei, Turnier-Kennung, Liveticker-Adresse (`badhub.live_url`, Quelle des Kürzels), badhub-URL oder badhub-Passwort **oder** durch einen erfolgreichen Knopf-Upload. `parse_error`/`implausible` sind **kein** Halt (Verfeinerung gegenüber dem Grill, s. u.). |
| T4 Nachversuch | Vorübergehende Fehler (BTP beschäftigt, Datei unruhig, nicht erreichbar, 5xx, 415): Nachversuch nach 90 s, höchstens zwei in Folge, dann regulärer Takt. 429/503 mit `Retry-After`: nach dieser Zeit (mindestens 5 s). Takt-Texte ohne „bitte gleich noch einmal“. |
| T5 Status | Ein Statusobjekt (`quelle`, `geprueft_ms`, `naechster_ms`, `angehalten`); Dashboard pollt alle 30 s, solange Automatik an und Liveticker läuft. |
| T6 Voraussetzungen | Merge unabhängig von Jet-Nachlauf, Messung A2 und Lizenzfrage; Standard aus. Die drei bleiben Roadmap-Punkte **vor dem Feldtest**. |
| T7 Takt | Zahlenfeld in Minuten, Minimum 5 (geklemmt), Standard 15. |
| T8 Hash | `ring` direkt einbinden (schon transitiv im Baum). |

## Betroffene Komponenten / Architekturregeln / Daten

| Komponente | Änderung |
|---|---|
| `config.rs` `TpUploadConfig` | `auto_enabled: bool` (Standard `false`), `auto_minutes: u16` (Standard 15), `takt() -> Option<Duration>` mit Klemmung ≥ 5, Konstanten `TP_TAKT_STANDARD_MINUTEN = 15`, `TP_TAKT_MIN_MINUTEN = 5`. Struct behält `#[serde(default)]`; eigener `Default`. |
| `src-tauri/Cargo.toml` | `ring = "0.17"` (transitiv vorhanden über `rcgen`/`tokio-rustls`). |
| `badhub/tp_upload.rs` | `lesen` wird `pub(crate)`; `sha256(&[u8]) -> [u8; 32]`; `UploadAntwort::Ueberlastet { retry_after_s }` für 503 mit `Retry-After` (Text „badhub bittet um Geduld, in N s wieder möglich“); `UploadAntwort::Serverfehler { code: u16, message }` (bisher ohne Code — 413 und 415 müssen unterscheidbar sein). |
| **neu** `badhub/tp_takt.rs` | Reine Logik: `SendeMerker { pfad, guid, sha }`, `Einordnung`, `einordnen(&UploadAusgang)`, `naechster_termin(...)`, `takt_neu_bewerten(alt, neu)`, Takt-Texte, Konstanten `NACHVERSUCH = 90 s`, `MAX_NACHVERSUCHE = 2`, `MAX_NICHT_LESBAR = 3`, `PAUSE_NICHT_VERFUEGBAR = 30 min`, `STARTVERZUG = 60 s`, `PRUEFTAKT = 5 s`. |
| `commands.rs` | `status_pollen` liefert `Option<StatusAntwort>` statt Text; `fuehre_upload_aus` liefert `UploadAusgang` (unten), der Knopf-Pfad bildet daraus den Text wie heute (Texte unverändert); `TpUploadStatus` + Felder, interner Takt-Zustand `TpTaktZustand` mit `#[serde(skip)]` (Merker, Nachversuchszähler, Zähler „nicht lesbar“, „Status nachholen“); `tp_takt_schritt` (frei, testbar) und `tp_takt_schleife` unter `TaktWaechter` in `start_sync` (nur `!slave_mode`), Config-Zugriff über das geteilte `Arc` (`app_handle.state::<AppState>().config.clone()`, **nicht** die Wertkopie `sync_config`); Startregel; Wiederanlauf in `save_config`; Knopf-Erfolg hebt Halt. |

`UploadAusgang` (Rückgabe von `fuehre_upload_aus`, `sha` = SHA-256 der tatsächlich gesendeten Bytes aus dem stabilen Lesen):

```rust
pub(crate) enum UploadAusgang {
    Beschaeftigt,                                   // Gate: MAX_WARTEZEIT überschritten
    Lesefehler(LeseFehler),                         // Unruhig | KeineTpDatei | ZuGross | NichtLesbar
    Antwort { antwort: UploadAntwort, sha: [u8; 32] }, // alles außer 202
    Verarbeitet { status: StatusAntwort, sha: [u8; 32] }, // 202 + result aus dem Poll
    DauertNoch { sha: [u8; 32] },                   // 202, Frist ohne result
}
```
| `src/types.ts`, `src/api.ts` | Felder nachziehen; keine neuen Commands. |
| `pages/SetupWizard.tsx` | Häkchen **„Automatisch senden, solange der Liveticker läuft“**, Feld **„alle … Minuten“** (`min=5`), Satz zur Freigabe. |
| `pages/Dashboard.tsx` | Poll 30 s bei aktiver Automatik; Zeile mit Takt, nächster Prüfung, letztem Ergebnis samt Quelle; Halt-Hinweis mit Sprung ins Setup. |
| Handbuch | `docs/tp-datei-upload.md` Abschnitt „Automatisch senden“, `docs/einstellungen.md` zwei Zeilen; `pruef`-Anker. |

Regeln: **R1** (alles im Rust-Kern, Frontend schaltet per `save_config` und
liest `tp_upload_status`), **R2** unberührt, **ADR 0060** (Gate unverändert,
Schnell-Lesung bewusst außerhalb). Lehre „Sync-Neustart bei jedem Speichern“:
Zustand liegt in `AppState`, nie im Task oder in der `SyncEngine`.

Datenschutz: Die Schnell-Lesung hält die Datei nur für die Dauer des Hashens
im Speicher, schreibt nichts auf Platte, loggt keinen Inhalt. Der SHA im
Speicher ist kein personenbezogenes Datum. Je Turniertag entstehen höchstens
so viele Uploads, wie sich die Datei ändert, mindestens 5 Minuten auseinander.

## Verhalten im Detail

### Schalter und Takt (Setup)

Im Abschnitt „TP-Datei (Turnierdatenbank)“, unter der Dateiwahl, nur wenn ein
Pfad gewählt ist:

- Häkchen **„Automatisch senden, solange der Liveticker läuft“** (Standard aus).
- Feld **„alle … Minuten“**, Zahlenfeld mit `min=5`, Standard 15. Die
  Wahrheit ist `takt()`: Sie klemmt jeden gespeicherten Wert unter 5 auf 5;
  der gespeicherte Wert selbst wird nicht umgeschrieben.
- Satz darunter: „Damit sendet bts-light jede Änderung der Datei von selbst —
  auch neue Auslosungen, sobald sie in BTP stehen. Ohne laufenden Liveticker
  passiert nichts.“
- **Entfernen** (Pfad löschen) setzt auch `auto_enabled = false` — sonst
  hielte die Automatik beim nächsten Schritt mit „keine TP-Datei gewählt“ an.

Im Slave-Modus gibt es den Abschnitt weiterhin nicht.

### Lebenszyklus

1. `start_sync` startet (nur `!slave_mode`) den Takt-Task neben dem
   Sekundentakt, als `TaktWaechter` im Sync-Task. `stop_sync` beendet ihn mit.
2. Beim Start gilt `naechster_ms = max(naechster_ms, jetzt + 60 s)`; ein
   Termin aus einem früheren Lauf (Speichern = Stopp + Start) bleibt also
   stehen, ein leerer wird auf +60 s gesetzt.
3. Der Task prüft alle 5 s (`MissedTickBehavior::Skip`), ob `naechster_ms`
   fällig ist. Dazwischen tut er nichts.
4. Ist er fällig, läuft **ein Takt-Schritt** (unten). Er liest die
   **aktuelle** Konfiguration aus dem geteilten `AppState.config` — der
   Schalter wirkt sofort. Ein geänderter Minutenwert wirkt ab dem nächsten
   Termin; damit niemand nach „30 → 5“ eine halbe Stunde wartet, setzt
   `save_config` bei geändertem `auto_minutes` `naechster_ms = min(naechster,
   jetzt + 60 s)`.
5. Schalter aus (oder Slave-Modus): `naechster_ms = None`, kein Lesen, kein
   Senden; ein Halt bleibt **gespeichert** (die Dashboard-Zeile zeigt ihn erst
   wieder, wenn der Schalter an ist). Schalter wieder an: `naechster_ms =
   jetzt + 60 s`; Aus/An löst den Halt **nicht** (steht so im Handbuch).
6. Der Upload selbst läuft als eigener Task (wie beim Knopf), der
   Takt-Schritt wartet auf sein `JoinHandle`. Bricht das Speichern im Setup
   den Sync-Task — und damit den Takt-Task — mitten im Upload ab, läuft der
   Upload zu Ende und schreibt seinen Text; nur die Einordnung geht verloren.
   Der nächste Schritt findet keinen Merker, sendet dieselben Bytes, bekommt
   `unchanged` und setzt ihn — ein Gate-Lauf, kein Schaden.
7. Beim Beenden der App endet alles hart; ein halber Upload ist für den
   Server ein ungültiger Body (415) und richtet nichts an. Nach einem
   Neustart (auch Update-Wiederanlauf) beginnt der Zustand leer: kein Merker,
   kein Halt, Termin +60 s nach dem nächsten Start.

### Takt-Schritt

```
1  config lesen; auto_enabled && !slave_mode? sonst naechster = None, Ende
2  angehalten? → naechster = jetzt + takt (nur fürs Dashboard), Ende
3  pruefe_upload_bedingungen(config) → Err(text) → HALT(text), Ende
3a war der letzte Ausgang „dauert noch": einmal GET status nachholen;
   result guid_mismatch/final_imported → HALT; sonst Text aktualisieren
4  lesen(pfad) ohne Gate → Err KeineTpDatei/ZuGross → HALT(Knopf-Text);
   Err NichtLesbar → Nachversuch (90 s), ab dem dritten in Folge HALT
   (Unruhig gibt es hier nicht: eine Lesung)
5  sha = sha256(bytes); merker passt (pfad, guid) && sha == merker.sha
   → geprueft_ms = jetzt, naechster = jetzt + takt, Ende (keine Log-Zeile)
6  upload_starten → Err (Knopf läuft) → naechster = jetzt + 90 s, Ende
7  ausgang = fuehre_upload_aus(...)  (Gate, zweimal lesen, senden, Poll)
8  einordnen(ausgang) → siehe Tabelle; Status, Merker, Termin schreiben
```

Der Merker speichert den SHA der **gesendeten** Bytes (aus dem stabilen Lesen
unter dem Gate, Feld `sha` im `UploadAusgang`), nicht den aus Schritt 4 — hat
sich die Datei zwischen 4 und 7 geändert, ist das richtig so.

### Einordnung der Ausgänge

| Ausgang | Einordnung | Wirkung |
|---|---|---|
| `Verarbeitet` mit `result = ok` | Gesendet | Merker = SHA, Nachversuche 0, `naechster = jetzt + takt`, Text wie Knopf |
| `DauertNoch` | Gesendet | wie oben; zusätzlich Marke „Status nachholen“ (Schritt 3a des nächsten Schritts) |
| `Verarbeitet` mit `parse_error` / `implausible` | Gesendet (weich) | wie oben; Text = Server-Meldung. Dieselben Bytes werden nicht erneut gesendet; die nächste geänderte Datei bekommt eine neue Chance |
| `Verarbeitet` mit `guid_mismatch` / `final_imported` | **Halt** | Text = Server-Meldung |
| `Antwort` 200 `Unveraendert`, 200 `Abgelehnt` | Gesendet (Bytes bekannt) | Merker = SHA, Text wie Knopf |
| `Antwort` 409 `BereitsEndgueltig`, 403 `VerbandFalsch`, 401 `Unauthorized`, `Serverfehler{413}` | **Halt** | Text wie Knopf |
| `Antwort` `NochNichtVerfuegbar` (400/404/Basic-Wand) | Pause | `naechster = jetzt + 30 min`, Text „badhub kennt den Upload noch nicht — nächster Versuch hh:mm“ |
| `Antwort` 429 `ZuSchnell{s}`, 503 `Ueberlastet{s}` | Nachversuch(s) | `naechster = jetzt + clamp(s, 5 s, 1 h)`, zählt als Nachversuch; `jetzt` ist das **Ende** des Uploads |
| `Beschaeftigt`, `Lesefehler(Unruhig)`, `Antwort` `NichtErreichbar` / `Status{_}` (jeder sonstige Code) / `Serverfehler{415}` | Nachversuch | `naechster = jetzt + 90 s`; ab dem dritten in Folge `jetzt + takt`, und zwar für **jeden** weiteren Fehler in Folge — der Zähler fällt erst mit Erfolg, Pause oder Wiederanlauf |
| `Lesefehler(NichtLesbar)` | Nachversuch, ab dem dritten in Folge **Halt** | eigener Zähler; Text beim Halt wie Knopf |
| `Lesefehler(KeineTpDatei / ZuGross)` | **Halt** | Text wie Knopf |

„Zerrissen“ kann eine gesendete Datei nur durch Eingaben der Turnierleitung
direkt in BTP sein — die eigenen Writes hält das Gate zurück, und gesendet
wird nur, was zweimal gleich gelesen wurde. Deshalb genügt bei
`implausible`/`parse_error`, auf die nächste Änderung zu warten.

Takt-Texte für Nachversuche nennen keinen Handlungsaufruf: „BTP beschäftigt —
nächster Versuch hh:mm“, „Datei in Bewegung — nächster Versuch hh:mm“, „badhub
nicht erreichbar — nächster Versuch hh:mm“, „badhub bittet um Geduld — nächster
Versuch hh:mm“. Nach dem zweiten Nachversuch heißt es „… — nächste Prüfung
hh:mm“ mit dem regulären Termin.

**Halt:** `angehalten = Some(text)`, `naechster_ms` läuft weiter (Dashboard
zeigt den Termin durchgestrichen bzw. „angehalten“), eine Log-Zeile
„TP-Takt angehalten: <text>“. Kein weiterer Lese- oder Netzzugriff bis zum
Wiederanlauf.

**Wiederanlauf:** `save_config` vergleicht alte und neue Konfiguration in
**sechs** Feldern: `tp_upload.path`, `tp_upload.tournament_uuid`,
`tournament_uuid`, `badhub.live_url` (Quelle des Verbandskürzels — der
403-Text schickt den Nutzer genau dorthin), `badhub.url`, `badhub.password`.
Bei Änderung: `angehalten = None`, Merker gelöscht, alle Zähler 0,
`naechster_ms = jetzt + 60 s`. Zusätzlich hebt jeder Knopf-Upload mit
Einordnung „Gesendet“ den Halt auf und setzt den Merker auf die gesendeten
Bytes (die Automatik sendet dieselben nicht gleich noch einmal).

### Gemeinsamer Merker mit dem Knopf

Knopf und Takt teilen `laeuft` (`upload_starten`). Läuft ein Knopf-Upload,
überspringt der Takt (Nachversuch ohne Zähler); läuft ein Takt-Upload, ist der
Knopf wie heute gesperrt und zeigt „läuft …“. `quelle` sagt, wer `ergebnis`
zuletzt geschrieben hat.

### Status-Fenster

Zeile unter dem Knopf, nur wenn `auto_enabled`:

- aktiv: **„Automatik: alle 15 min · nächste Prüfung 14:47 · zuletzt 14:32
  (automatisch): übernommen, 312 Spiele“**. Ohne bisherigen Upload: „… ·
  noch nichts gesendet“. Bei Knopf-Ergebnis: „(von Hand)“.
- Liveticker gestoppt: **„Automatik startet mit dem Liveticker“**.
- Halt: **„Automatik angehalten: Turnier bereits endgültig importiert“** in
  Warnfarbe, daneben der Sprung ins Setup wie bei `tpHindernis`.

Das Dashboard pollt `tp_upload_status` alle 30 s, solange `auto_enabled` und
der Liveticker läuft, zusätzlich alle 5 s solange `laeuft` (wie heute).

### Log

Je gesendetem Upload eine Zeile (wie Stufe 1, mit Zusatz „Takt“ bzw.
„Knopf“); je Halt und je Pause eine Zeile; **keine** Zeile für „unverändert“
und keine für Nachversuche (die stehen im Status).

### Rückwärtskompatibilität

Alte Configs ohne die Felder laden mit Automatik aus und 15 Minuten. Eine
ältere App-Version verwirft die Felder beim nächsten Speichern — die
Automatik ist dann still aus; ein erneutes Update stellt den Standard (aus)
her, nicht den vorherigen Wert.

## Akzeptanzkriterien

1. `TpUploadConfig` ohne neue Felder lädt mit `auto_enabled = false`,
   `auto_minutes = 15`; `auto_minutes = 1` ergibt `takt() = 5 min`;
   `auto_enabled = false` ergibt `takt() = None`.
2. Bei unveränderter Datei (gleicher SHA wie der Merker) löst ein fälliger
   Takt-Schritt keinen HTTP-Request aus, nimmt das Schreib-Gate nicht und
   schreibt keine Log-Zeile; `geprueft_ms` und `naechster_ms` sind
   fortgeschrieben.
3. Bei geänderter Datei sendet der Takt-Schritt genau einen Upload über
   `fuehre_upload_aus`; danach steht der Merker auf dem SHA der gesendeten
   Bytes und `quelle = Takt`.
4. Die Einordnungstabelle gilt Zeile für Zeile (Unit-Test je Zeile):
   Gesendet, Gesendet (weich), Halt, Pause, Nachversuch mit und ohne
   `Retry-After`.
5. Nach zwei Nachversuchen in Folge (90 s, 90 s) ist der dritte Termin der
   reguläre Takt; ein Erfolg setzt den Zähler zurück.
6. `Retry-After: 7` ergibt einen Termin in 7 s, `Retry-After: 2` in 5 s.
7. Nach einem 409 ist `angehalten` gesetzt; ein weiterer fälliger Schritt
   liest und sendet nicht.
8. Speichern mit geänderter GUID (oder Pfad, Liveticker-Adresse, badhub-URL,
   Passwort) hebt den Halt auf, löscht den Merker und setzt den Termin auf
   +60 s; Speichern ohne Änderung dieser sechs Felder ändert nichts am
   Takt-Zustand. Speichern mit geändertem `auto_minutes` zieht einen fernen
   Termin auf höchstens +60 s vor.
8a. „Nicht lesbar“ in der Schnell-Lesung ergibt zweimal einen Nachversuch
   (90 s) und beim dritten Mal in Folge den Halt; eine erfolgreiche Lesung
   dazwischen setzt den Zähler zurück.
8b. Nach „dauert noch“ holt der nächste Schritt genau einen Status-Abruf
   nach; `guid_mismatch` daraus ergibt den Halt, `ok` den Text „übernommen,
   N Spiele“.
9. Ein erfolgreicher Knopf-Upload hebt den Halt auf und setzt den Merker;
   der nächste Takt-Schritt sendet dieselben Bytes nicht erneut.
10. Läuft ein Knopf-Upload, sendet der Takt nicht und setzt den Termin auf
    +90 s ohne den Nachversuchszähler zu erhöhen.
11. Schalter aus → `naechster_ms = None`; Schalter an → `jetzt + 60 s`;
    Slave-Modus → wie aus.
12. Ein Sync-Neustart (stop/start) mit einem Termin in 10 Minuten lässt
    diesen Termin stehen; ohne Termin ergibt der Start +60 s.
13. 503 mit `Retry-After` wird als `Ueberlastet { retry_after_s }` abgebildet
    und im Knopf-Text als „badhub bittet um Geduld, in N s wieder möglich“
    gezeigt; 413 und 415 tragen ihren Code in `Serverfehler { code, .. }`.
14. Die Takt-Texte enthalten nicht „bitte gleich noch einmal“; die
    Knopf-Texte und ihre Handbuch-Anker bleiben unverändert.
15. Setup zeigt Häkchen und Minutenfeld nur bei gewähltem Pfad; das Feld
    nimmt keine Werte unter 5 an; `npm run build` ist fehlerfrei.
16. Dashboard: bei aktiver Automatik und laufendem Liveticker ein Poll alle
    30 s; Zeile nennt Takt, nächsten Termin, letztes Ergebnis mit Quelle;
    bei Halt den Grund und den Sprung ins Setup.
17. Handbuch-Anker bestehen `scripts/test-handbuch-fakten.mjs`:
    `"Automatisch senden, solange der Liveticker läuft" in
    src/pages/SetupWizard.tsx`, `/TP_TAKT_STANDARD_MINUTEN: u16 = 15/` und
    `/TP_TAKT_MIN_MINUTEN: u16 = 5/ in src-tauri/src/config.rs`,
    `"Automatik angehalten"` und `"Automatik startet mit dem Liveticker" in
    src/pages/Dashboard.tsx`, `"badhub bittet um Geduld" in
    src-tauri/src/badhub/tp_upload.rs`, `"nächster Versuch" in
    src-tauri/src/badhub/tp_takt.rs` (keine inneren Anführungszeichen im
    verankerten Text). Der Stufe-1-Anker „Datei ändert sich gerade, bitte
    gleich noch einmal“ bleibt stehen (AK 14).
17a. „Entfernen“ im Setup setzt `auto_enabled = false`.
18. `cargo test`, `cargo clippy --workspace --all-targets -D warnings`,
    `cargo fmt --check` grün.

## Tests

- **Unit (`badhub/tp_takt.rs`):** `einordnen` je Tabellenzeile;
  `naechster_termin` (90 s × 2 → Takt; `Retry-After`; Pause 30 min; Halt
  lässt Termin stehen; „nicht lesbar“ 2× Nachversuch, 3. = Halt);
  `takt_neu_bewerten` nur bei den sechs Feldern, Minutenänderung zieht den
  Termin auf +60 s vor; Texte ohne Handlungsaufruf.
- **Unit (`config.rs`):** Defaults, Klemmung, `takt()`.
- **Unit (`tp_upload.rs`):** `sha256` bekannter Vektor; 503-Abbildung mit und
  ohne `Retry-After`.
- **Integration (`commands.rs`, Mock-Server aus Stufe 1):** AK 2, 3, 7, 8, 9,
  10 mit temporärer Datei (Jet-Kopf + Füllbytes) und Zähler der Requests;
  pausierte Tokio-Zeit für die Termine.
- **Frontend:** `tsc`/`vite build`; Handbuch-Faktentest.
- **Manuell vor dem Feldtest:** Testturnier mit BTP: Ergebnis von Hand
  eintragen → nach Takt + Cache auf badhub sichtbar; Schalter während des
  Laufs aus/an; Datei umbenennen → Halt sichtbar, neu wählen → läuft wieder.

## Risiken & Rollback

- **Ungegatete Lesung trifft eine halb geschriebene Seite** → falscher SHA →
  ein überflüssiger Gate-Lauf; der stabile Leser und der Server fangen den
  Rest. Kein Datenverlust.
- **Jet-Nachlauf länger als `RUHE`** (ungemessen): Der Takt vervielfacht die
  Kopien. Gegenmittel sind dieselben wie in Stufe 1 (Byte-Vergleich,
  Plausibilität am Server); die Messung bleibt Pflicht vor dem Feldtest.
- **Gate-Kosten in heißen Phasen:** je geänderter Datei ~5 s Wartezeit für
  BTP-Writes, höchstens alle 5 Minuten. Nachversuche nach „beschäftigt“
  sind auf zwei begrenzt.
- **Zwei Master mit derselben GUID:** Server-Cooldown → jeder zweite Lauf
  429 → Nachversuch mit `Retry-After`; kein Schaden.
- **Rollback:** Schalter aus (sofort wirksam) oder ältere Version (Felder
  verfallen, Takt still aus). Keine Daten auf dem Server zu bereinigen.

## Offene Fragen / Annahmen

- Jet-Nachlauf, Messung A2, Lizenzfrage (badhub-ADR 0047): **vor dem
  Feldtest**, nicht vor dem Merge (T6). Unverändert aus Stufe 1.
- mtime-Verhalten der offenen Datei: ungemessen, für diese Spec unerheblich.
- Annahme: 15 Minuten sind „oft genug“; der Wert ist einstellbar.

## Betroffene Doku-Dateien

`docs/tp-datei-upload.md` (Abschnitt „Automatisch senden“: Schalter, Takt,
was ein Halt ist und wie er endet — Aus/An löst ihn nicht —, Neustart-
Verhalten), `docs/einstellungen.md` (zwei Zeilen), diese Spec,
`docs/features/tp-datei-upload.md` (Nicht-Ziel „Kein Takt“ → Verweis,
Status-Kopf, **Umsetzungs-Hinweis 6 „L2“** auf die Entscheidungen T1/T7
umschreiben — dort stehen noch `auto_minutes: Option<u16>` und mtime+Größe),
`docs/btp_protocol.md` (Abschnitt Schreib-Gate: zweiter Leser außerhalb des
Gates), `docs/multi-hall.md` (Slave: kein Takt), `docs/adr/0061-…md`,
`docs/adr/README.md`, `CLAUDE.md` (Zeile „TP-Datei an badhub“ um
`badhub/tp_takt.rs`, `tp_takt_schritt`, Statusfelder), `docs/roadmap.md`
(Stufe 2 → umgesetzt), `docs/changelog.md`.

## Umsetzungs-Hinweise (aus dem How-To)

Reihenfolge PR L2, jeweils Test zuerst: `config.rs` → `Cargo.toml` +
`tp_upload.rs` (`lesen` pub(crate), `sha256`, `Ueberlastet`) →
`badhub/tp_takt.rs` (reine Logik) → `commands.rs` (`UploadAusgang`,
Status-Felder, `tp_takt_schritt`, Schleife in `start_sync`, Wiederanlauf in
`save_config`, Knopf-Erfolg) → Frontend → Handbuch, Spec-Status, ADR, CLAUDE.md,
Roadmap, Changelog, Version 0.9.301. code-reviewer nach jedem Schritt;
**security-reviewer** für die ungegatete Lesung und den Takt-Schritt (Deckel,
kein Inhalt im Log, nie häufiger als 5 min bzw. `Retry-After`, Halt-Texte sind
bereinigte Server-Meldungen).

## Ergänzung 28.10.2026: Turnier-Kennung aus der Datei (v0.9.302)

**Anlass.** Die Automatik sendet, was unter dem gespeicherten Pfad liegt. Öffnet
die Turnierleitung in BTP am Turniertag eine andere Datei (zweiter Tag,
Vorjahr, Sicherung), merkte das bisher erst der Server (`guid_mismatch`,
ein Upload von 0,4 MB umsonst). Außerdem tippte der Turnierleiter die
Turnier-GUID ab, obwohl BTP sie in die Datei schreibt.

**Befund** (Messung 28.10.2026, 30 TP-Dateien, `btp_protocol.md`): Die
`Settings`-Tabelle ist name/value, Jet legt die Werte als komprimierten
Unicode ab — `FF FE LinkCode FF FE <GUID>` steht genau einmal je Datei. Ein
Jet-Parser ist dafür nicht nötig (und hätte die Lizenzfrage aus ADR 0047).

**Entscheidungen.**

| Frage | Entscheidung |
|---|---|
| G1 Lesen | `badhub/tp_guid.rs` `guid_aus_bytes(&[u8]) -> GuidBefund` (`Gefunden` kanonisch / `Leer` / `Mehrdeutig` / `KeinAnker`), komprimierte Jet-Textform, UCS-2 nur als Rückfall ohne komprimierten Treffer (3,6 MB in 30 ms Debug). **Mehrere Anker mit unterschiedlichem Inhalt** (zwei GUIDs, oder GUID neben leerem Anker) → `Mehrdeutig`: Welche Zeile die aktuelle ist, sagt die Datei nicht — eine entlinkte Datei mit alter GUID im freien Seitenplatz sähe aus wie eine frisch verknüpfte mit leerer Altfassung. Nur die GUID — kein weiteres Feld aus der Datei. `fremde_guid`/`kurz` liegen im selben Modul. |
| G2 Dateiwahl | `tp_file_check` liest die Datei ganz (Command jetzt `async` + `spawn_blocking`) und liefert `guid: Option<String>` (`None` für Leer/Mehrdeutig/KeinAnker). Wizard: Feld „Turnier bei turnier.de“ **leer** → Übernahme aus der Datei (Hinweis „Turnier-Kennung aus der Datei übernommen“), Wähl-Knopf nicht mehr gesperrt; Feld mit Text, der keine Kennung ist → Datei abgelehnt („Feld leeren oder Adresse einfügen“), nichts wird überschrieben; Kennung oben **anders** → Datei abgelehnt, nichts gespeichert („gehört zu Turnier …“). Keine stille Übernahme einer anderen Kennung: Sie steuert auch den Liveticker (ADR 0054). Datei ohne eindeutige Kennung → nur mit eingetragener Kennung wählbar, Server prüft. Vergleiche im Wizard kanonisch (`extractTournamentGuid`). |
| G3 Vor dem Senden | In `fuehre_upload_aus` nach dem stabilen Lesen: `Gefunden(g)` mit `g ≠ vorgaben.guid` → `UploadAusgang::GuidFremd { datei }`, kein Netzzugriff, Knopf-Text „„<Datei>“ gehört zu Turnier <8 Zeichen>…, eingestellt ist <8 Zeichen>… — Kennung unter „1 · Liveticker-Ziel“ prüfen oder andere Datei wählen“ (derselbe Rat im Setup-Hinweis). `einordnen` → **Halt** (wie `guid_mismatch`). Die Schnell-Lesung des Takts prüft **nicht** — sie ist ungegated, ein Halt aus einer halb geschriebenen Seite wäre zu teuer; der Gate-Lauf danach fängt es. |
| G4 Wiederanlauf | Unverändert: Speichern mit neuer Datei/Kennung oder ein gelungener Knopf-Upload. |

**Akzeptanzkriterien.** (19) `guid_aus_bytes`: komprimiert und UCS-2 gefunden
und kanonisiert; ohne Anker `KeinAnker` trotz fremder GUIDs; Anker ohne Wert
`Leer`; Anker am Dateiende ohne Panik; zwei gleiche Anker eindeutig; zwei
verschiedene `Mehrdeutig` sortiert. (20) `tp_file_check` liefert die GUID
aus einer Datei mit Anker, sonst `None`. (21) `fuehre_upload_aus` mit fremder
GUID: `GuidFremd`, 0 Requests, Text nennt Dateiname und beide Kennungen
gekürzt; mit passender GUID wird gesendet. (22) `einordnen(GuidFremd)` =
Halt. (23) Handbuch-Anker „Turnier-Kennung aus der Datei übernommen“,
„Turnier-Kennung passt“, „gehört zu Turnier“ (Wizard und `commands.rs`).
Sonde: `BTS_TP_DATEI=<Pfad> cargo test guid_aus_echter_datei -- --ignored`.

**Nicht-Ziel.** Automatisches Finden der Datei über den BTP-Prozess bleibt
Roadmap; `WebID` wird nicht gelesen.
