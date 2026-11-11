# badhub-Push entkoppeln — Spezifikation

> Status: **abgestimmt 2026-11-11** (freigegeben vom Nutzer; via /idee: Brief → Grill → How-To → Review).
> Quelle: Kommunikations-Audit vom 10.11.2026 (Paket 2, siehe [roadmap.md](../roadmap.md)).
> Betroffene Crates: src-tauri (Rust-Kern; im Frontend nur Kommentare in `src/types.ts`).
> ADR: [0086](../adr/0086-ein-geordneter-sender-fuer-die-badhub-strecke.md) ·
> [0087](../adr/0087-voller-tset-nur-bei-strukturaenderung.md).
> Geprüft gegen bts-light main `fdfd701` (v0.9.378) und badhub `ddc54531`. Reine Code-Lektüre —
> nichts gebaut, nichts gemessen außer dem einen Wert, der schon im Code steht (siehe „Messung").

## Kontext / Problem

Der Turnier-PC überträgt den Turnierstand an den badhub-Liveticker: voller Stand (`tset`),
Einzelpunkt (`tupdate_match`), Meldeliste für den Check-In, Spielplan (`sched`) und
Punktverlauf (`tverlauf`). Diese Strecke hängt heute am selben 5-s-Zyklus, der auch die Halle
steuert. Das Audit vom 10.11.2026 nannte sechs Befunde; der Grill hat sie am Code nachgeprüft
und teils korrigiert. Maßgeblich ist die korrigierte Fassung:

1. **Ein hängendes badhub verzögert den nächsten Zyklus.** `run_once` wartet vier Pushes
   nacheinander ab (`sync.rs:2943–2944`, `2950–2952`, `2958–2960`, `2963`). Der HTTP-Client
   hat nur eine Gesamtfrist von 15 s, keinen `connect_timeout` (`badhub/push.rs:12`,
   `67–72`). Der Loop ist „Lauf + 5 s Schlaf" (`commands.rs:2414`, `2448`).
   **Korrektur:** Feldvergabe, `set_snapshot`, Rotation, Autodruck und Erst-Ansage laufen im
   Zyklus *vor* den Pushes (`sync.rs:2829–2896`) — verzögert wird der *nächste* Zyklus.
   Bis zu 4 × 15 s nur beim „schwarzen Loch" (Captive Portal, versickernde Pakete); ein
   gezogenes Kabel oder ein DNS-Fehler scheitert sofort. Typisch 15–45 s.
2. **Der Fehlerpfad sendet die teuerste Nutzlast im schnellsten Takt.** `on_failure` setzt
   `last_pushed = None` (`sync.rs:3003–3006`) → nächster Zyklus voller `tset`, ohne Abstand.
   Das Logo reist mit, solange es nicht gestempelt oder die 10-min-Auffrischung fällig ist
   (`sync.rs:1808–1838`, `LOGO_AUFFRISCHUNG` `sync.rs:37`); gestempelt wird nur nach Erfolg —
   in einem Ausfall über 10 min also in jedem Versuch. Jedes Speichern der Einstellungen
   baut die Engine neu (`commands.rs:2365`) und schickt das Logo sofort wieder.
   **Zusätzlich gefunden:**
   - Die `ScorePushQueue` (`tablet/server.rs:168–255`) hat weder Backoff noch Auth-Sperre
     und schreibt je Fehlschlag eine Warnung (`server.rs:250–252`).
   - Dauerhafte Ablehnungen des `tset` (400/413/409) werden im 5-s-Takt wiederholt.
   - Die Meldeliste geht bei jedem Fehler außer 404/400 — auch bei 409 („another
     federation"), 401, 413 — alle 5 s vollständig hinaus, für immer (`sync.rs:861–865`).
   - Ein leerer Spielplan wird gesendet (`sync.rs:923–926`, die Anzahl wird nicht geprüft).
     badhub antwortet 400 „sched ohne matches"; bts-light deutet das als „kennt den Kanal
     nicht" und pausiert ihn 30 min.
   - Die Arbeiter der Queue sind abgelöste Tasks (`server.rs:228`). `stop_sync`
     (`commands.rs:2857`) bricht sie nicht ab: Ein laufender Arbeiter sendet nach dem
     Stoppen weiter, mit dem alten Passwort.
   - Ein Captive Portal, das 200 mit HTML liefert, gilt als Erfolg: Alle Sendefunktionen
     werten nur `status == 200` (`push.rs:98–100`).
3. **Im Normalbetrieb geht fast jeder Zyklus als Vollstand hinaus.** Zwei oder mehr
   geänderte Satzstände ergeben einen vollen `tset` (`badhub/diff.rs:73–74`). Tablet-Punkte
   gehen doppelt: einmal sofort über die Queue, einmal über den Loop. `sched` geht minütlich
   voll — eine dokumentierte Entscheidung (`sync.rs:3015–3020`); ob ein Inhaltsvergleich
   spart, hängt an der Stabilität von `predicted_start_ts` und ist unbelegt.
4. **Fehler des Loops stehen nicht im Diagnose-Log.** `BtpError` kehrt ohne Log-Zeile zurück
   (`sync.rs:2676–2685`), ein gescheiterter `tset` ebenso (`sync.rs:2980–2983`);
   `docs/logging.md` Zeile 26 behauptet das Gegenteil. Umgekehrt loggen die additiven Pushes
   und die Queue ungedrosselt je Zyklus (`sync.rs:864`, `944`, `1011`; `server.rs:251`).
5. **Überholen.** Ursache sind **zwei unabhängige Sender** (Loop und je Feld ein Arbeiter).
   badhub kennt keine Reihenfolge (`rid` wird nur zurückgespiegelt), und ein `tset` ersetzt
   den Snapshot vollständig. Bei badhub läuft `tupdate_match` außerdem ohne Transaktion und
   Sperre: Parallele Updates verschiedener Felder können sich Punkte wegschreiben — das
   passiert heute schon.
6. **Punktverlauf.** Die Hash-Last stimmt: Je Zyklus werden alle Einträge gebaut und über
   ihre volle Serialisierung gehasht (`sync.rs:955–1017`, `badhub/verlauf.rs:176–209`).
   **Korrektur:** Die „Blockade" ist überwiegend falsch. badhub verwirft je Eintrag still
   mit 200 und `verworfen: n`; bts-light liest den Antwortkörper nicht und stempelt alles
   als angenommen (`sync.rs:995–999`) → **stiller Verlust**. Verworfen wird unter anderem
   jedes Spiel, das badhub noch nicht aus `tset` oder `sched` kennt — und `tverlauf` geht
   heute auch nach einem gescheiterten `tset`. Eine echte Blockade gibt es nur bei
   Ganz-Ablehnung (500/413/409): Dann gehen immer wieder dieselben ersten 64 Einträge
   (`MAX_SPIELE_JE_NACHRICHT`, `verlauf.rs:33`, Auswahl `183–189`). 400/404 pausiert den
   Kanal **30 min** (`CHECKIN_UNSUPPORTED_RETRY`, `sync.rs:204` — nicht 10 min, wie im
   Audit und im Grill zunächst notiert).

Außerdem verliert jedes Speichern der Einstellungen den gesamten Sendezustand
(`SyncEngine::new`, `sync.rs:761`): Diff-Basis, `rid`, Logo-Stempel, Meldeliste, die drei
Kanal-Pausen, `sched`-Takt und Verlauf-Stand.

Betroffen sind die Turnierleitung (Halle ruckelt, wenn das Internet hängt; Statuszeile sagt
nicht, was los ist), die Zuschauer am Liveticker (Punkte überholen sich, Verläufe fehlen)
und wer nach einem Turnier das Diagnose-Log liest.

## Zielbild & Erfolgskriterien

1. Der lokale Betrieb (Feldvergabe, SR-Rotation, Autodruck, Erst-Ansage, Tablets, Monitore)
   läuft im 5-s-Takt, egal in welchem Zustand die Strecke zu badhub ist. Zielwert:
   Zykluszeit ≤ BTP-Zeit + 5 s + 1 s, auch bei totem badhub.
2. Im Fehlerfall wird mit wachsendem Abstand (5/10/20/40/60 s) und ab dem zweiten Versuch
   ohne Logo wiederholt. Bei 401/403 geht kein weiterer Request hinaus, bis die
   Einstellungen gespeichert oder die Übertragung neu gestartet wird.
3. Im Normalbetrieb geht ein voller `tset` nur bei Strukturänderung und sonst alle 60 s
   hinaus; Tablet-Punkte gehen genau einmal.
4. Zustandswechsel von BTP und der badhub-Strecke stehen flankengesteuert im Diagnose-Log.
5. Ein veralteter Vollstand überholt keinen frischeren Tablet-Punkt.
6. Der Punktverlauf arbeitet nur an geänderten Spielen, verliert nichts still und bleibt
   an einer abgelehnten Nachricht nicht hängen.
7. Im Gutfall ist die Strecke Punkt → badhub nicht langsamer als heute.
8. Die Statuszeile im Dashboard bleibt **eine** Zeile und sagt in Alltagssprache, was los
   ist; ein neuer Schalter im Setup entsteht nicht.

Gegenüber dem Brief entfallen: Komprimierung (badhub nimmt kein gzip an) und „`sched` nur bei
Änderung" als festes Ziel (nur nach Messung, PR 5).

## Nicht-Ziele

- Paket 3 des Audits (Relay härten) — eigener /idee-Lauf.
- gzip. badhub reicht den Körper roh an `json_decode`; gzip ergäbe 400 „Malformed JSON".
- Jede Änderung, die einen badhub-Deploy **voraussetzt**: kein neues Wire-Format, keine
  Sperre im `tupdate_match`-Zweig, keine Reihenfolge-Prüfung bei badhub (Folgepunkte, siehe
  unten).
- TP-Datei-Upload (eigener Takt, im Audit ohne Befund), HTTPS-Zwang und Redirect-Politik
  der badhub-URL, Sprachdienste, Log-Upload.
- Check-In-Lese-Weg (`CheckinLese`, `sync.rs:3057`) und Sponsor-/Logo-Endpunkt
  (`push_checkin_branding`): bleiben, wie sie sind.
- Ein periodischer Probe-Versuch im Halt.
- Ein neuer Schalter im Setup.
- Das Verhalten „totes Tablet mit altem Session-Stand überdeckt eine Handeingabe in BTP für
  das laufende Spiel" — heute schon so, auch auf den Monitoren.
- Den Verlauf-Stand über das Speichern der Einstellungen retten (siehe Annahmen).

## Verhalten der Gegenstelle badhub

Stand badhub `ddc54531` (`POST /api/live_update.php`, `lib/live_update_lib.php`,
`LiveUpdateController.php`, `lib/live_verlauf_lib.php`); nur gelesen, nichts geändert.

- **Reihenfolge im Controller:** 405 → Langzeit-Limit 429 (`Retry-After: 86400`) →
  Kurzzeit-Limit 429 (`Retry-After: 900`) → Auth 401 → Validierung. Die Auth-Sperre greift
  nach 5 Fehlversuchen in 15 min je IP bzw. 50 in 24 h. Ein 429 kommt **vor** der
  Passwortprüfung und zählt **nicht** als Fehlversuch.
- **Weitere Status:** 400 (ungültig), 413 (Körper > 5 MiB), 409 (fremder Check-In), 500.
- **Antwortform:** immer ein JSON-Objekt mit Feld `type`. Erfolg bei `tset`,
  `tupdate_match`, Meldeliste: `{"type":"answer","status":"ok",…}`; bei `sched`:
  `{"type":"ok","rows":n}`; bei `tverlauf`: `{"type":"ok","rows":n,"verworfen":n}`; Fehler:
  `{"type":"error","message":…}`.
- **`verworfen`** ist nur eine Zahl, keine Liste. „Bekannt" ist ein Spiel, wenn es im
  Snapshot oder im `sched` steht.
- **`tupdate_match` für ein unbekanntes Spiel** hängt einen Rumpf-Eintrag `{_id, s}` an —
  ein Punkt vor dem Struktur-`tset` erzeugt ein halbes Spiel.
- **Kein Throttle für Erfolge.** Je Request prüft badhub das Passwort gegen alle Zugänge
  (bcrypt, Kostenfaktor 12) — das bestimmt die Dauer eines `tupdate_match`.
- **Lebenszeichen:** `updated_at` gilt nach 10 min als veraltet, nach 30 min wird das
  Turnier ausgeblendet; auch `tupdate_match` frischt auf. Spieler-Index, Gäste (Fenster
  30 min) und das Aufräumen der Kind-Turniere laufen **nur** im `tset`-Zweig.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:** nur `src-tauri/`.
  Neu: `badhub/strecke.rs` (reine Logik), `badhub/sender.rs` (Postfach und Sender-Task),
  `badhub/testhilfe.rs` (nur Tests).
  Erweitert: `badhub/push.rs`, `badhub/diff.rs`, `badhub/verlauf.rs`, `sync.rs`,
  `commands.rs`, `tablet/server.rs`, `tablet/state.rs`, `tablet/timeline.rs`,
  `tablet/sheet.rs`. Frontend: nur der Kommentar zu `kind` in `src/types.ts:942–948`.
- **Architekturregeln:** R1 gewahrt (Status weiter über `get_status`). R2 gewahrt — der
  Push bildet nur ab; BTP bleibt die Wahrheit. R3/R4/R6 nicht berührt. R5 nicht berührt:
  `process_result` bleibt unverändert, betroffen ist nur der Live-Punkt (`handle_score`).
- **Konfiguration & Abwärtskompatibilität:** kein neues Feld in `config.rs`, keine neue
  Datei auf der Platte, kein geändertes gespeichertes Format, kein neues Wire-Format. Jede
  Stufe funktioniert mit dem heutigen badhub.
- **Datenschutz:** kein neues personenbezogenes Feld. Log-Zeilen der Strecke tragen nur
  Kanal, Klasse, HTTP-Status, Bytes und Dauer — keine Körper, keine Namen, kein Passwort.
  Der Antwortkörper von badhub wird begrenzt gelesen und nie protokolliert.
- **Abhängigkeiten:** keine neue Cargo-/npm-Dependency. `tokio` `test-util` ist schon
  Dev-Abhängigkeit (`src-tauri/Cargo.toml:96`). Kein Relay-Deploy, kein badhub-Deploy.

## Lösung

### L1 — Ein Sender, ein Postfach (ADR 0086)

Alles, was an den Liveticker-Endpunkt geht, läuft über **einen** Sender-Task. Er wartet jede
Antwort ab, bevor er den nächsten Auftrag wählt — nie zwei Sendungen gleichzeitig.

**Postfach** = ein Mutex plus `tokio::sync::Notify`, drei Schlitze, je „neuester gewinnt":

1. **Stand** (vom Loop, einmal je Zyklus am Ende von `run_once`): der fertige `BtpSnapshot`
   (wird heute schon in `on_success` verschoben — kein zusätzlicher Klon), Hand- und
   Auto-Hallen (heute schon geklonte Maps, `sync.rs:2924–2929`), die Menge der
   tablet-getriebenen Spiele (L9) und die Erzeugungszeit.
2. **Spielplan-Auftrag** (vom Loop, wenn der `SchedTakt` fällig ist): nur die Prognose-Map.
   Er bleibt liegen, bis er gesendet oder durch einen neueren ersetzt ist.
3. **Feld-Marken** (von `handle_score`): nur die CourtID „hier gibt es einen neuen Punkt",
   geordnet und ohne Dubletten. Keine Nutzlast.

Die Ablieferung ist synchron, sperrt nur kurz und wartet nie auf das Netz. Loop und
`ServerCtx` bekommen je einen `Option<Griff>` auf das Postfach.

**Die Nutzlast baut der Sender, zum Sendezeitpunkt.** Er hält `Arc<TabletState>` und eine
Wertkopie der Config (wie `sync_config` heute) und baut `LivetickerContext`
(`badhub/payload.rs:27`) selbst; der `QueueOrderStore` lebt im `TabletState`. Gründe:

- `diff` braucht den bestätigten Vorstand, den nur der Sender kennt.
- Unmittelbar vor `build_tset` legt der Sender die Tablet-Stände noch einmal über seine
  Kopie (`apply_tablet_scores`) — ein `tset` ist nie älter als der jüngste Punkt.
- Feld-Marken lösen sich zum Sendezeitpunkt gegen den Live-Stand auf (neue kleine
  Lesefunktion im `TabletState`: laufendes Spiel des Felds samt Tablet-Sätzen). Die
  Veraltet-Prüfung der heutigen Queue (`server.rs:239–249`) ergibt sich daraus von selbst.

### L2 — Reihenfolge (Auftragswahl)

Eine reine Funktion wählt nach **jeder** Sendung neu:

1. Strecke nicht frei → warten bis zur Frist.
2. Voller `tset` nötig → `tset`. Nötig heißt: keine bestätigte Basis, Strukturänderung
   (L10), letzter bestätigter voller `tset` ≥ 60 s alt, oder erster Versuch nach einer
   Störung.
3. Feld-Marken → je Feld ein `tupdate_match`, wenn der Live-Stand vom bestätigten Satzstand
   abweicht.
4. Punktänderungen aus dem Loop-Stand für **nicht** tablet-getriebene Spiele.
5. Meldeliste (bei Änderung).
6. Spielplan-Auftrag.
7. Punktverlauf (nur mit bestätigter Basis).
8. Sonst schlafen bis zum `Notify` oder zum nächsten Vollstand-Termin.

Folgen:

- Ein Punkt wartet höchstens **eine** laufende Anfrage ab, nie die ganze additive Kette.
- Trifft ein Punkt während eines `tset`-Uploads ein, liegt die Marke im Postfach und geht
  direkt danach — bei badhub in richtiger Reihenfolge, weil seriell.
- Ein `tset` wird nie als fertige Nachricht geparkt; „`tset` vor einem neueren Punkt
  gebaut" kann nicht mehr vorkommen.
- `tset` kommt vor Punkten, weil ein Punkt für ein badhub unbekanntes Spiel einen
  Rumpf-Eintrag erzeugt. Ohne bestätigte Basis gehen deshalb keine Punkte hinaus.
- Die serielle Strecke beseitigt das gegenseitige Wegschreiben paralleler `tupdate_match`
  bei badhub, ohne badhub-Deploy.

Schritte 3 und 4 sowie die Regeln „voller `tset` ≥ 60 s" und „Strukturänderung" gelten in
dieser Form ab PR 2b bzw. PR 3. In PR 2a bleibt das **Was** unverändert (`diff` wie heute,
Lebenszeichen wie heute); nur das **Wann und Wer** wandert in den Sender.

### L3 — Wem gehört welcher Zustand (Zielbild nach PR 3)

| Zustand | Heute | Ziel |
|---|---|---|
| Bestätigter Strukturstand (Diff-Basis) | `engine.last_pushed` (`sync.rs:95`) | Sender; gesetzt **nur** nach bestätigtem vollem `tset` |
| Bestätigter Satzstand je Spiel | implizit im Snapshot | Sender; fortgeschrieben nach `tset` und `tupdate_match` |
| `rid` | Engine und `ServerCtx` (zwei Zähler) | Sender, ein Zähler |
| Zeitpunkt des letzten **vollen** `tset` | `last_push_at` (jeder Push, `sync.rs:100`) | Sender |
| Meldeliste, Verlauf-Stand, Kanal-Pausen | Engine | Sender |
| Streckenzustand (frei / gestört / gedrosselt / angehalten) | — | Sender (in PR 1: geteiltes „Tor" zwischen Engine und Queue) |
| Logo-Stempel, „Strecke war zuletzt gestört", 429-Frist | Engine bzw. — | `AppState.push_merker` (nur RAM) |
| `sched`-Takt, Prognose | Engine | bleibt in der Engine |
| Anzeige-Lage der Strecke | `AppState.status` | neuer Schlitz `AppState.push_lage`; `get_status` setzt die eine Zeile zusammen |

**`AppState.push_merker`** (`Arc<Mutex<…>>`) überlebt den Sync-Neustart, nicht den
App-Neustart. Der **Logo-Stempel** und der Merker „war zuletzt gestört" sind an einen
Ziel-Schlüssel aus URL, Passwort-Hash und Turnier-GUID gebunden; ändert sich das Ziel,
gelten sie nicht mehr. Die **429-Frist** ist nur an die URL gebunden, nicht an das Passwort:
badhub sperrt je IP und vor der Passwortprüfung, und die Frist soll gerade das Speichern
eines korrigierten Passworts überleben (siehe „Zur Freigabe ausdrücklich vorgelegt").

Der **Halt** lebt bewusst nur im Sender (in PR 1: im Tor). `start_sync` baut beides neu —
„Speichern oder Neustart der Übertragung hebt den Halt auf" ergibt sich ohne Zusatzcode.

### L4 — Lebenszyklus

- **Start:** `start_sync` legt das Postfach **vor** dem Loop-Spawn und vor
  `ServerCtx::new` an (heutige Reihenfolge `commands.rs:2363` → `2464` wird umgestellt). Der
  Sender-Task startet **im** Loop-Task unter einem `TaktWaechter` (`commands.rs:31–37`), wie
  Flush-Takt (`2385`) und TP-Takt (`2399`).
- **Stop / Speichern:** `stop_sync` bricht den Loop-Task ab → der Wächter bricht den Sender
  ab, eine laufende Anfrage wird verworfen. `push_lage` wird in `stop_sync` neben dem Status
  zurückgesetzt (`commands.rs:2929`). Das Frontend macht beim Speichern
  `stopSync` → 400 ms → `startSync` (`src/pages/SetupWizard.tsx:1093–1095`).
- **Slave:** kein Postfach, kein Sender (`(!slave_mode).then(…)` wie beim TP-Takt);
  `run_once` kehrt vor der Ablieferung zurück (`sync.rs:2832–2838`).
- **Überlebt den Sync-Neustart:** Logo-Stempel, „war zuletzt gestört" (→ erste Sendung ohne
  Logo), 429-Frist. **Überlebt nicht:** Halt, Diff-Basis, Meldeliste, Verlauf-Stand,
  Kanal-Pausen.

### L5 — Fehlerklassen je Kanal

„Strecke" wirkt auf alle Kanäle, „Kanal" nur auf den betroffenen. „←" heißt: wie links.

| Ausgang | `tset` | `tupdate_match` | Meldeliste | `sched` | `tverlauf` |
|---|---|---|---|---|---|
| 200 + echte badhub-Antwort | Ok | Ok | Ok | Ok | Ok, `verworfen` auswerten |
| 200 ohne echte Antwort (HTML) | Strecke: Störung | ← | ← | ← | ← |
| Transport, Frist, DNS, TLS | Strecke: Störung | ← | ← | ← | ← |
| 3xx (nicht gefolgt), 408 | Strecke: Störung | ← | ← | ← | ← |
| 5xx | Strecke: Störung | Strecke: Störung | Kanal: Backoff | Kanal: Backoff | Kanal: Backoff + Auswahl rotieren |
| 429 | Strecke: gedrosselt | ← | ← | ← | ← |
| 401 mit `WWW-Authenticate: Basic` | Strecke: Halt (Wand) | ← | ← | ← | ← |
| 401 / 403 mit badhub-Antwort | Strecke: Halt (Passwort) | ← | ← | ← | ← |
| 401 / 403 ohne badhub-Antwort (aus dem Sicherheits-Review von PR 1: Filter oder Proxy im Hallen-WLAN) | Strecke: Störung | ← | ← | ← | ← |
| 400 | Kanal: abgelehnt | Punkt verwerfen | Kanal: 30 min | Kanal: 30 min | Kanal: 30 min |
| 404 | Strecke: Störung (falsche URL) | ← | Kanal: 30 min | Kanal: 30 min | Kanal: 30 min |
| 409 | Kanal: abgelehnt | Punkt verwerfen | Kanal: 30 min, eine Log-Zeile | Kanal: 30 min | Kanal: Backoff + rotieren |
| 413 | einmal sofort ohne Logo; sonst abgelehnt; Logo bis zur Markenänderung aussetzen | Punkt verwerfen | Kanal: 30 min | Kanal: 30 min | Kanal: Backoff + rotieren |
| übrige 4xx | Kanal: abgelehnt | Punkt verwerfen | Kanal: 30 min | Kanal: 30 min | Kanal: 30 min |

Erläuterungen:

- **„Kanal: abgelehnt"** beim `tset`: nächster Versuch nach 60 s, bei jeder weiteren
  Ablehnung in Folge verdoppelt, höchstens 10 min (60/120/240/480/600 s). Ein Erfolg setzt
  zurück. Solange eine bestätigte Basis besteht, laufen `tupdate_match` weiter.
- **„Kanal: Backoff"** bei den additiven Kanälen: dieselbe Stufenfolge wie die Strecke
  (L6), je Kanal gezählt. Der Liveticker (`tset`, `tupdate_match`) läuft dabei weiter.
- **„Kanal: 30 min"** ersetzt die drei `*_unsupported_since`-Felder (`sync.rs:178`, `190`,
  `197`); die Deutung 400/404 = „badhub kennt den Kanal noch nicht" bleibt für die drei
  additiven Kanäle bei 30 min (`sync.rs:204`).
- **5xx auf `tset`/`tupdate_match`** ist eine Strecken-Störung, weil der nächste Versuch
  dann ein voller `tset` ist — das heilt auch „Corrupt snapshot in DB" bei badhub.
- **„Punkt verwerfen"**: kein zweiter Versuch für diesen Punkt; der nächste Punkt oder der
  nächste volle `tset` trägt den Stand.
- Die Unterscheidung Wand / Passwort nutzt die vorhandene Auswertung `abgelehnt`
  (`push.rs:41–64`).
- **Ein leerer Spielplan wird gar nicht gesendet.**

**Echte badhub-Antwort:** höchstens 64 KiB lesen (Muster `koerper_begrenzt`,
`badhub/tp_upload.rs:362`), der Körper muss ein JSON-Objekt mit einem String-Feld `type`
sein. Bewusst locker — `answer` und `ok` gelten beide; eine gute Antwort für fremd zu halten
wäre schlimmer als ein unerkanntes Portal.

### L6 — Backoff, Halt, 429

- **Backoff der Strecke:** n-ter Fehlversuch in Folge → 5, 10, 20, 40, 60, 60 … s. Ein
  Erfolg setzt zurück. Kein Zufallsanteil (es gibt einen Client). Der erste Versuch nach
  einer Störung ist immer ein voller `tset`.
- **Streckenautomat:** `Frei → Gestoert{n, naechster}`, `→ Gedrosselt{bis}`,
  `→ Angehalten{grund}`. Aus `Angehalten` führt kein Übergang heraus außer dem Neubau.
  „Darf senden?" und „Ausgang melden" liefern einen optionalen Übergang für Log und Status.
- **Halt (401/403, Basic-Auth-Wand):** Nach der ersten Ablehnung geht kein Request mehr an
  den Liveticker-Endpunkt, auf keinem Kanal — bis die Einstellungen gespeichert werden oder
  die Übertragung neu gestartet wird. Kein Probe-Versuch.
- **429:** bis zum Ablauf der Frist kein Request. Die Frist kommt aus `Retry-After`, nur in
  der Sekundenform (wie `tp_upload.rs:464–468`); fehlt der Kopf oder ist er unlesbar, gelten
  60 s. Untergrenze 5 s, **Obergrenze 60 min** — dieselben Grenzen wie `MIN_RETRY_AFTER` /
  `MAX_RETRY_AFTER` in `badhub/tp_takt.rs:49–53`.
- **Fristen des Clients:** `connect_timeout` 5 s, Gesamtfrist 15 s; beide als Parameter
  injizierbar (Tests). Ab PR 3 richtet sich die Gesamtfrist nach der Körpergröße:
  15 s + Bytes / 16 KiB/s, höchstens 60 s — im Sender-Task stört das die Halle nicht mehr.

### L7 — Logo-Regel

Das Logo reist in einem vollen `tset` nur mit, wenn **alle** Bedingungen gelten:

- (a) es ist nach der heutigen Regel nötig (Marke weicht ab oder Stempel ≥ 10 min alt,
  `sync.rs:1814–1817`),
- (b) die Strecke ist frei,
- (c) der unmittelbar vorige `tset`-Versuch war erfolgreich **oder** es ist der allererste
  Versuch seit App-Start,
- (d) es läuft keine Logo-Sperre.

Daraus folgt:

- Der erste Versuch darf das Logo tragen, jeder weitere im Fehlerfall nicht.
- Nach einer Störung geht der erste `tset` ohne Logo; das Logo folgt mit dem nächsten
  vollen `tset` (ab PR 3 also binnen 60 s).
- Scheitert ein `tset` **mit** Logo, gilt 10 min Logo-Sperre (`LOGO_AUFFRISCHUNG`); nach
  413 bis zur Änderung der Logo-Marke.
- Speichern im Fehlerfall: Der Merker sagt „war gestört" → ohne Logo. Ein gültiger Stempel
  bleibt bei unverändertem Ziel erhalten.

### L8 — Statuszeile und Log

**Statuszeile.** Der Sender schreibt nur seine Lage nach `AppState.push_lage`:
`Unbekannt | Ok{letzter_erfolg} | Gestoert{naechster_versuch, seit, grund} |
Gedrosselt{bis} | Angehalten{grund} | Abgelehnt{status, naechster_versuch}`. Der Loop
schreibt nur noch die BTP-Lage. `get_status` ruft eine reine Funktion
`status_zeile(btp, push, jetzt)`; „N s" wird beim Abruf gerechnet (das Frontend fragt alle
2 s). `SyncOutcome` verliert `PushedFull`, `PushedUpdate`, `Idle` und `PushError`
(`sync.rs:73–90`) zugunsten eines „abgeliefert".

Vorrang: gestoppt → BTP-Fehler → leerer Snapshot verworfen → Slave → Push-Lage → ok.

| Lage | `kind` | Text |
|---|---|---|
| Ok | `ok` | „Verbunden – Liveticker aktuell" |
| Gestört | `warn` | „Internet-Strecke gestört — Halle läuft weiter, nächster Versuch in N s" |
| Gedrosselt (429) | `warn` | „badhub hat die Anmeldung vorübergehend gesperrt — nächster Versuch um HH:MM" |
| Halt Passwort | `push_error` | „Übertragung an badhub angehalten: Passwort prüfen" |
| Halt Wand | `push_error` | „Übertragung an badhub angehalten: Der Server verlangt eine eigene Anmeldung" |
| `tset` abgelehnt | `warn` | „badhub lehnt den Turnierstand ab (HTTP nnn) — nächster Versuch in N s" |

Diese Texte sind verbindlich und gelten wortgleich in Code, Tests, `docs/oberflaechen.md`
und `docs/liveticker-push.md`. Der Langtext zur Wand (`push.rs:20–22`) geht ins Log.
`warn` (orange) und `push_error` (rot) kennt das Frontend schon (`Dashboard.tsx:44–52`,
`AppShell.tsx:17–25`) — keine neue Farbe, keine neue Komponente.

Stufung: Die fünf Störungstexte kommen mit **PR 1** (dort als neue `SyncOutcome`-Varianten
in `status_from`, `commands.rs:129`). Der Ok-Text „Verbunden – Liveticker aktuell" ersetzt
die drei heutigen Ok-Texte (`commands.rs:131–133`) erst mit **PR 2a**, zusammen mit
`push_lage`, `status_zeile` und dem herunterzählenden N.

**Log-Flanken** (generisch, für BTP **und** die Strecke):

- je Zustandswechsel genau eine Zeile;
- solange ein Fehlerzustand anhält, höchstens alle 10 min eine Erinnerung mit der Zahl der
  Fehlversuche;
- bei der Rückkehr eine Zeile mit Dauer und Zahl der Versuche;
- Felder: Kanal, Klasse, HTTP-Status, Bytes, Dauer. Keine Körper, keine Namen, kein
  Passwort.

Die ungedrosselten Warnungen in `sync.rs:864`, `944`, `1011` und `server.rs:251` entfallen
zugunsten dieser Flanken.

### L9 — „Tablet-getriebenes Feld"

- **Definition:** `TabletState::apply_tablet_scores` (`tablet/state.rs:4703–4715`)
  überschreibt die Sätze eines Spiels genau dann, wenn die Feld-Session dasselbe Spiel trägt
  und Sätze hat. Solche Sätze entstehen nur über `handle_score` → `record_score` (einziger
  produktiver Aufrufer: `tablet/server.rs:4756`) oder beim Start aus `live-scores.json`.
- **Erkennung:** `apply_tablet_scores` gibt künftig die Menge der überschriebenen Match-IDs
  zurück; sie reist mit dem Stand ins Postfach.
- **Regel:** Für diese Spiele leitet der Sender aus dem Loop-Stand **keine** Punkt-Updates
  ab. Ihre Punkte kommen nur über die Feld-Marken; im vollen `tset` stehen sie mit dem Stand
  vom Sendezeitpunkt.
- **Feld ohne Tablet:** Der Stand kommt aus BTP in den Loop-Stand; weicht er vom bestätigten
  Satzstand ab, geht binnen eines Zyklus ein `tupdate_match`.
- **Kein Zurückdrehen durch den 60-s-Vollstand:** ein Sender, seriell; Tablet-Stände werden
  unmittelbar vor dem Bau erneut übergelegt; ein später eintreffender Punkt geht nach dem
  `tset`.
- **Nach App-Neustart** ohne verbundenes Tablet trägt der erste `tset` die
  wiederhergestellten Stände.

### L10 — Vollstand-Regel und Strukturmarke (ADR 0087)

- **Vollstand fällig**, wenn nie ein voller `tset` bestätigt wurde oder der letzte
  bestätigte ≥ 60 s alt ist. Ein `tupdate_match` verschiebt den Termin **nicht**. (Heute
  hängt das Lebenszeichen am letzten Push jeder Art, `sync.rs:1842–1845`, gestempelt in
  `2971–2973` — solange Einzel-Updates fließen, ist kein voller `tset` garantiert.)
- **Strukturänderung → voller `tset` im selben Zyklus.** Die Strukturmarke wird in
  `badhub/diff.rs` aus `diff` herausgelöst. Sie umfasst je Spiel: Status, Feld, Sieger,
  Sätze **beendeter** Spiele, Meldungs-IDs beider Seiten und den Vorbereitungs-Aufruf — also
  mindestens die heutigen Fingerabdrücke `court_assignment` (`diff.rs:108`) und
  `preparation_calls` (`diff.rs:119`). Satzstände **laufender** Spiele gehören nicht dazu.
- Hand-Hallen und manuelle Spielreihenfolge erreichen badhub weiterhin erst mit dem
  60-s-Vollstand (wie heute, `diff.rs:28–35`).
- **Punktänderungen:** laufende Spiele, deren Sätze vom bestätigten Satzstand abweichen,
  ohne die tablet-getriebenen. Je Spiel ein `tupdate_match`. **Deckel N = 4** (vorläufig):
  Bei mehr als N Punktänderungen in einem Stand geht stattdessen ein voller `tset`.

### L11 — Punktverlauf

- `tverlauf` geht nur, wenn eine bestätigte Basis besteht (badhub kennt die Spiele sonst
  nicht und verwirft still).
- Die Antwort wird gelesen. Bei `verworfen > 0` wird das Paket **nicht** als angenommen
  gestempelt. Weil die Antwort nur eine Zahl nennt, zählt ein Versuchszähler je Eintrag des
  Pakets; nach **3** Versuchen wird der Eintrag geparkt, bis er sich ändert oder ein neuer
  voller `tset` oder `sched` bestätigt wurde.
- Eine Ganz-Ablehnung (5xx, 413, 409) verschiebt den Startpunkt der 64er-Auswahl — der
  nächste Versuch trägt eine andere Auswahl.
- Änderungsmarke statt Voll-Hash: ein Zähler je Spiel in `TimelineStore` (Schreibpfade
  `apply_rally`, `ersetze`, `finalize`) und `SheetStore` (`apply_event`,
  `apply_event_sync`, `finalize`) plus der Pausenzustand. Ein voller Eintrag wird nur für
  Spiele mit abweichender Marke gebaut.
- Ein Turnierwechsel leert den Stand (wie heute).

## Zur Freigabe ausdrücklich vorgelegt

Diese Punkte wurden nach Empfehlung gesetzt, nicht vom Nutzer einzeln entschieden. Mit der
Freigabe der Spec gelten sie als bestätigt.

1. **`Retry-After` höchstens 60 min.** badhub sendet bei der Langzeit-Sperre 86400 s.
   Wörtlich eingehalten stünde der Liveticker einen Tag still, auch nach korrigiertem
   Passwort. Der Deckel folgt dem Hausmuster in `tp_takt.rs`; ein 429 zählt bei badhub nicht
   als Fehlversuch, ein Versuch je Stunde verlängert die Sperre also nicht.
2. **Die 429-Frist überlebt das Speichern der Einstellungen**, nicht den App-Neustart.
   Speichern ist damit kein Weg, die Drossel zu umgehen. Dafür ist die Frist nur an die URL
   gebunden, nicht an das Passwort (L3) — das How-To nennt einen gemeinsamen Ziel-Schlüssel
   für alle Merker; mit ihm verfiele die Frist gerade beim Speichern eines korrigierten
   Passworts.
3. **Deckel N = 4 Punktänderungen** je Stand, vorläufig bis zur Messung (Schritt A/B).
4. **Kein gzip** in diesem Lauf (badhub nimmt es nicht an) — Folgepunkt für badhub.
5. **`sched`-Inhaltsvergleich nur nach Messung** (PR 5); sonst bleibt der Minutentakt.

## Akzeptanzkriterien

In Klammern der PR, mit dem das Kriterium erfüllt ist. „Request" meint stets einen Request
an den Liveticker-Endpunkt.

**Gutfall**

- [ ] **AK1** (PR 2b) Lastmodell 20 zählende Felder, zusammen 2 Punkte/s, 300 ms Antwortzeit
  je Anfrage: Jeder Punkt ist spätestens 3 s nach `handle_score` gesendet. Bei einem
  einzelnen Feld wartet ein Punkt höchstens eine laufende Anfrage ab. `handle_score` kehrt
  zurück, ohne auf das Netz zu warten.
- [ ] **AK2** (PR 3) Ändern sich in einem Zyklus die Satzstände zweier laufender Spiele ohne
  Strukturänderung, gehen zwei `tupdate_match` und **kein** voller `tset` hinaus. Bei mehr
  als N = 4 Punktänderungen geht ein voller `tset`.
- [ ] **AK3** (PR 3) Für ein tablet-getriebenes Spiel geht je Punkt genau **eine** Sendung
  hinaus; der Loop-Stand erzeugt für dieses Spiel kein `tupdate_match`.
- [ ] **AK4** (PR 3) Ein Feld ohne Tablet: Ein in BTP geänderter Satzstand geht spätestens
  mit dem nächsten Zyklus als `tupdate_match` hinaus.
- [ ] **AK5** (PR 3) Spätestens 60 s nach dem letzten bestätigten vollen `tset` folgt der
  nächste, auch wenn laufend `tupdate_match` gehen.
- [ ] **AK6** (PR 3) Ändert sich Feldbelegung, Vorbereitungs-Aufruf, Ergebnis oder Paarung,
  geht im selben Zyklus ein voller `tset`. (negativ) Ein reiner Punkt in einem laufenden
  Spiel ändert die Strukturmarke nicht.
- [ ] **AK7** (PR 2b) Ein Punkt, der während eines `tset`-Uploads eintrifft, geht
  unmittelbar danach; die letzte Sendung trägt den neueren Stand. Ein voller `tset` trägt
  nie einen älteren Tablet-Stand als den zum Sendezeitpunkt bekannten. Zwei Punkte desselben
  Felds, die während einer laufenden Anfrage eintreffen, ergeben eine Sendung.
- [ ] **AK8** (PR 2b, negativ) Ein Punkt für ein inzwischen beendetes oder neu belegtes Feld
  wird nicht gesendet.

**Fehlerfälle**

- [ ] **AK9** (PR 1 / PR 2a) Schwarzes Loch (Verbindungsaufbau versickert): Versuche im
  Abstand 5/10/20/40/60/60 s. Mit PR 1 kostet jeder Versuch den Zyklus höchstens **eine**
  Frist (5 s Verbindungsaufbau, 15 s bei hängendem Server), und die additiven Pushes
  desselben Zyklus entfallen. Ab PR 2a gilt: Zykluszeit ≤ BTP-Zeit + 5 s + 1 s; `run_once`
  wartet nie auf badhub.
- [ ] **AK10** (PR 1) DNS-Fehler oder gezogenes Kabel: wie AK9, die Versuche scheitern
  sofort.
- [ ] **AK11** (PR 1) Captive Portal (200 mit HTML): gilt als Störung. Logo, Meldeliste und
  Verlauf werden nicht als angenommen gestempelt.
- [ ] **AK12** (PR 1) 5xx auf `tset`: Backoff; der nächste Versuch ist ein voller `tset`
  ohne Logo. (negativ) 5xx auf Meldeliste, `sched` oder `tverlauf` hält `tset` und
  `tupdate_match` nicht an.
- [ ] **AK13** (PR 1) 401/403: Nach der ersten Ablehnung geht kein Request mehr hinaus —
  auch kein Tablet-Punkt —, bis die Einstellungen gespeichert oder die Übertragung neu
  gestartet wird. Statuszeile rot: „Übertragung an badhub angehalten: Passwort prüfen". Bei
  401 mit `WWW-Authenticate: Basic` ebenfalls Halt, mit dem Text „Übertragung an badhub
  angehalten: Der Server verlangt eine eigene Anmeldung".
- [ ] **AK14** (PR 1) 429: bis zum Ablauf der Frist kein Request. Frist = `Retry-After` in
  Sekunden, mindestens 5 s, höchstens 60 min; fehlt der Kopf oder ist er unlesbar, 60 s. Die
  Statuszeile nennt die Uhrzeit. Die Frist überlebt das Speichern der Einstellungen, auch
  mit geändertem Passwort; nach einem App-Neustart gilt sie nicht mehr.
- [ ] **AK15** (PR 1) 400, 409 oder ein übriger 4xx auf `tset`: nächster Versuch nach 60 s,
  dann 120/240/480/600 s — nie im 5-s-Takt. 413 auf `tset`: genau ein sofortiger Versuch
  ohne Logo; scheitert auch er, wie 400; das Logo reist erst wieder nach einer Änderung der
  Logo-Marke. Statuszeile: „badhub lehnt den Turnierstand ab (HTTP nnn) — nächster Versuch
  in N s".
- [ ] **AK16** (PR 1) 409 auf der Meldeliste: genau eine Log-Zeile, nächster Versuch
  frühestens nach 30 min. Dasselbe gilt für 400/404 auf Meldeliste, `sched` und `tverlauf`.
- [ ] **AK17** (PR 1) Ein leerer Spielplan wird nicht gesendet und pausiert den Kanal nicht.
- [ ] **AK18** (PR 1 / PR 2b) Schmaler Uplink: Im Fehlerfall geht je Backoff-Stufe höchstens
  ein Versuch hinaus, ohne Logo; ein gescheiterter `tset` mit Logo wird frühestens nach
  10 min wieder mit Logo versucht. Ab PR 2b läuft nie mehr als eine Sendung gleichzeitig.
- [ ] **AK19** (PR 1) App-Neustart im Fehlerfall: Der erste Versuch darf das Logo tragen,
  der zweite nicht.
- [ ] **AK20** (PR 1) Speichern im Fehlerfall: Der Halt ist aufgehoben, es geht genau ein
  neuer Versuch hinaus, ohne Logo. Ein gültiger Logo-Stempel bleibt bei unverändertem Ziel
  (URL, Passwort, Turnier-GUID) erhalten — im Gutfall löst Speichern keinen Logo-Versand
  aus.
- [ ] **AK21** (PR 2a) Slave: kein Sender-Task, kein Request.
- [ ] **AK22** (PR 2a / PR 2b) Stoppen: Danach geht kein Request aus dem Sender mehr hinaus;
  eine laufende Anfrage wird abgebrochen. Ab PR 2b gilt das auch für Tablet-Punkte (es gibt
  keine abgelösten Arbeiter mehr).
- [ ] **AK23** (PR 1 / PR 2a) Statuszeile: Ein BTP-Fehler hat Vorrang vor jeder Push-Lage.
  Bei gestörter Strecke orange mit „Internet-Strecke gestört — Halle läuft weiter, nächster
  Versuch in N s". Ab PR 2a zählt N zwischen zwei Abrufen herunter, und der Gutfall zeigt
  „Verbunden – Liveticker aktuell"; `stop_sync` setzt die Push-Lage zurück.
- [ ] **AK24** (PR 1) Log: Je Zustandswechsel von BTP und Strecke genau eine Zeile; im
  Dauerfehler höchstens eine Erinnerung je 10 min; bei der Rückkehr eine Zeile mit Dauer und
  Zahl der Versuche. (negativ) Keine Zeile enthält Antwortkörper, Spielernamen oder das
  Passwort.
- [ ] **AK25** (PR 4) Punktverlauf: kein `tverlauf` ohne bestätigten vollen `tset`. Bei
  `verworfen > 0` wird nichts gestempelt; nach 3 Versuchen wird ein Eintrag geparkt und
  kommt nach dem nächsten bestätigten vollen `tset` wieder. Eine Ganz-Ablehnung (5xx, 413,
  409) sendet beim nächsten Mal eine andere Auswahl. Für ein unverändertes Spiel wird kein
  Eintrag gebaut.
- [ ] **AK26** (PR 2a) In jedem der Fälle AK9–AK18 laufen Feldvergabe, Rotation, Autodruck,
  Erst-Ansage und `set_snapshot` im 5-s-Takt weiter.
- [ ] **AK27** (PR 2a) Während ein Upload hängt, ersetzt ein neuer Stand den wartenden im
  Postfach; nach dem Upload wird der neueste Stand verarbeitet, kein Zwischenstand. Ein
  Spielplan-Auftrag bleibt liegen, bis er gesendet oder ersetzt ist.

## PR-Zerlegung und Tests

Je PR: ein Patch-Bump in `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` und
`package.json` (die Nummer wird erst beim Merge festgelegt), Eintrag in `docs/changelog.md`,
`code-reviewer`. TDD; grün sein müssen `cargo test` (Workspace),
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`,
`npm run build`, `node scripts/test-handbuch.mjs` und
`node scripts/test-handbuch-fakten.mjs`. Keiner der PRs ändert ein gespeichertes Format.

### PR 1 — Fristen, Fehlerklassen, Halt/429, Backoff ohne Logo, Log-Flanken (noch ohne Sender)

**Erfüllt:** AK9 (erster Teil), AK10–AK17, AK18 (erster Teil), AK19, AK20, AK23 (erster
Teil), AK24.

**Inhalt**

- `build_client` mit `connect_timeout` 5 s (Gesamtfrist 15 s bleibt); Fristen als Parameter.
- Eine Sendefunktion `sende(client, url, passwort, kanal, körper) → Ausgang` statt fünf
  Kopien (`push.rs:77–241`). `Ausgang` trägt Status, Wand-Merkmal, `Retry-After`, den
  begrenzt gelesenen Körper, Bytes und Dauer. `push_checkin_branding` bleibt unberührt.
- `badhub/strecke.rs` komplett: Klassifikation (L5), Backoff und Streckenautomat (L6),
  `Retry-After`, Antwortprüfung, Logo-Entscheidung (L7), Log-Flanken (L8).
- Ein geteiltes **Tor** (`Arc<Mutex<Strecke>>`), in `start_sync` neu angelegt, an Engine
  **und** `ServerCtx`:
  - Ist die Strecke nicht frei, sendet in diesem Zyklus kein Kanal.
  - Scheitert der `tset` an der Strecke, entfallen die additiven Pushes desselben Zyklus.
  - Die Queue verwirft Punkte bei gesperrter Strecke (der nächste Versuch ist ohnehin ein
    voller `tset`) und meldet ihre Ausgänge ans Tor.
- Kanal-Pausen nach Tabelle L5; leerer `sched` wird nicht gesendet.
- `AppState.push_merker` (Logo-Stempel, „war gestört", 429-Frist).
- Log-Flanken für die Strecke und für BTP-Fehler (im Loop, `commands.rs` 2414–2447).
- Neue `SyncOutcome`-Varianten für gestört, gedrosselt, angehalten und abgelehnt mit den
  Texten aus L8.
- Messtest (Schritt A) und Debug-Zeile je Sendung (Schritt B).

**Wirkung ohne Sender.** Ein totes badhub kostet den Zyklus je Backoff-Stufe höchstens
**eine** Frist statt bis zu 4 × 15 s; im Dauerausfall ist das ein Versuch je 60 s. Die
Entkopplung selbst (Zyklus wartet nie) kommt erst mit PR 2a. Bekannte Lücke bis PR 2b: Ein
Arbeiter der Queue, der beim Stoppen gerade läuft, hält noch das alte Tor und kann einen
wartenden Punkt mit dem alten Passwort senden.

**Dateien:** `badhub/strecke.rs` (neu), `badhub/testhilfe.rs` (neu, `#[cfg(test)]`),
`badhub/mod.rs`, `badhub/push.rs`, `sync.rs`, `tablet/server.rs`, `commands.rs`,
`src/types.ts` (nur Kommentar).

**Tests**

- `strecke.rs`: `klassifikation_je_kanal_folgt_der_tabelle` (tabellengetrieben, alle Zellen
  aus L5) · `basic_wand_ist_halt_aber_kein_passwortfehler` ·
  `backoff_5_10_20_40_60_und_bleibt_bei_60` · `erfolg_setzt_den_backoff_zurueck` ·
  `halt_laesst_nichts_mehr_durch` · `gedrosselt_haelt_retry_after_ein` ·
  `retry_after_sekunden_untergrenze_obergrenze_und_unlesbar` ·
  `html_mit_200_ist_keine_badhub_antwort` · `antwort_answer_und_ok_gelten_beide` ·
  `logo_nur_beim_ersten_versuch_oder_nach_einem_erfolg` ·
  `logo_sperre_nach_gescheitertem_logo_tset` · `logo_nach_413_erst_bei_neuer_marke` ·
  `flanke_eine_zeile_je_wechsel` · `erinnerung_hoechstens_alle_zehn_minuten` ·
  `rueckkehr_nennt_dauer_und_versuche`.
- `push.rs` (echtes HTTP, kurze injizierte Fristen):
  `haengender_server_laeuft_in_die_gesamtfrist` · `captive_portal_200_html_ist_stoerung` ·
  `status_429_liefert_retry_after` · `antwortkoerper_wird_begrenzt_gelesen` ·
  `grosser_koerper_kommt_vollstaendig_an`.
- `sync.rs`: `nach_transportfehler_wartet_der_naechste_zyklus` ·
  `additive_pushes_entfallen_wenn_der_tset_an_der_strecke_scheitert` ·
  `nach_401_geht_kein_request_mehr_hinaus` ·
  `meldeliste_409_wird_nicht_im_takt_wiederholt` · `leerer_spielplan_wird_nicht_gesendet` ·
  `zweiter_versuch_traegt_kein_logo` · `sync_neustart_im_fehler_sendet_kein_logo`.
- `tablet/server.rs`: `score_push_respektiert_das_tor` · `score_push_meldet_401_als_halt`.
- `commands.rs`: `status_texte_fuer_gestoert_gedrosselt_angehalten` (prüft auch den Text
  für „abgelehnt").

**Mock.** `badhub/testhilfe.rs`: Antwortskript je Anfrage, Zähler, aufgezeichnete
Körpergrößen, vollständiges Lesen des Körpers (wie der Mock in `tp_upload.rs`), Modus „nimmt
an, antwortet nie". Die vorhandenen Mocks reichen nicht (der in `push.rs` bedient eine
Anfrage ohne Zähler; der in `sync.rs` zählt, liest aber nur 64 KiB und kann nicht hängen).
Der `connect_timeout` selbst ist lokal nicht verlässlich testbar → Abnahme von Hand
(Firewall-Regel „verwerfen").

**Doku:** neue `docs/liveticker-push.md` (Kanäle, Tabelle, Backoff, Halt, Logo; in
`docs/handbuch.json` unter „intern" eintragen, sonst meldet `scripts/test-handbuch.mjs` die
Datei) · `docs/logging.md` (Zeile 26 korrigieren, Flanken-Zeilen beschreiben) ·
`docs/oberflaechen.md` (Statuszeile; je neuem Text ein
`<!-- pruef: "…" in src-tauri/src/commands.rs -->`) · `docs/spieler-check-in.md` (409,
Pausen) · neue Tabellenzeile in `CLAUDE.md` · `docs/roadmap.md` · `docs/changelog.md`.

**Reviews:** `code-reviewer`, `security-reviewer` (Auth-Verhalten, Auswertung fremder
Antworten, kein Geheimnis im Log).

### PR 2a — Sender-Task für den Loop-Stand, Status-Schlitz

**Erfüllt:** AK9 (zweiter Teil), AK21, AK22 (erster Teil), AK23 (zweiter Teil), AK26, AK27.

**Inhalt.** `badhub/sender.rs` (Postfach, Griff, Schleife, Auftragswahl). Der Transport
liegt hinter einer schmalen Schnittstelle, damit Tests mit pausierter Uhr ohne echtes Netz
laufen. `run_once` liefert ab und wartet nicht mehr; die Engine verliert die Push-Felder.
Die Was-Regeln bleiben unverändert (`diff` wie heute, Lebenszeichen wie heute).
`AppState.push_lage` und `status_zeile` in `get_status`; `stop_sync` setzt zurück. Die
Queue bleibt noch bestehen und teilt den Streckenzustand über das Tor aus PR 1.

**Dateien:** `badhub/sender.rs` (neu), `badhub/mod.rs`, `sync.rs`, `commands.rs`,
`src/types.ts` (Kommentar).

**Tests**

- Auftragswahl: `ohne_basis_kommt_zuerst_ein_voller_tset` ·
  `gesperrte_strecke_waehlt_warten` · `additive_kanaele_kommen_nach_dem_liveticker` ·
  `verlauf_erst_mit_basis`.
- Schleife mit Transport-Attrappe (`start_paused`):
  `neuester_stand_gewinnt_waehrend_der_upload_haengt` ·
  `ablieferung_kehrt_sofort_zurueck` · `nach_stoerung_folgt_ein_voller_tset_ohne_logo` ·
  `halt_endet_erst_mit_neuem_sender` · `abbruch_beendet_den_sender` ·
  `spielplan_auftrag_bleibt_bis_gesendet`.
- `sync.rs`: `run_once_wartet_nie_auf_badhub` (Attrappe blockiert, der Zyklus endet) ·
  `slave_liefert_nichts_ab`.
- `commands.rs`: `btp_fehler_hat_vorrang_vor_der_push_lage` ·
  `countdown_rechnet_zum_abrufzeitpunkt` · `stop_setzt_die_push_lage_zurueck`.

**Mock.** Die Attrappe. Echtes HTTP mit pausierter Uhr vermeiden — die Uhr springt vor,
während auf den Socket gewartet wird, und Fristen feuern zu früh.

**Doku:** `docs/liveticker-push.md` (Architektur) · `docs/oberflaechen.md` (Anker ziehen
mit dem Text um; neuer Ok-Text) · `CLAUDE.md`-Zeile · `docs/changelog.md`.

**Reviews:** `code-reviewer`, `security-reviewer` (Passwort in einem langlebigen Task,
Log-Inhalte).

### PR 2b — Tablet-Punkte in den Sender

**Erfüllt:** AK1, AK7, AK8, AK18 (zweiter Teil), AK22 (zweiter Teil).

**Voraussetzung:** Messwert „Dauer eines `tupdate_match` gegen Produktion" aus Schritt B
liegt vor (siehe „Messung").

**Inhalt.** `handle_score` legt nur noch die Feld-Marke ab. `ScorePushQueue`,
`queue_score_push` und der zweite `rid`-Zähler entfallen. Neue Lesefunktion „Live-Stand des
Felds" im `TabletState`. Das Tor aus PR 1 entfällt (der Streckenzustand lebt nur noch im
Sender).

**Dateien:** `tablet/server.rs`, `tablet/state.rs`, `badhub/sender.rs`, `commands.rs`.

**Tests:** `punkt_waehrend_tset_upload_geht_danach_und_gewinnt` ·
`zwei_punkte_desselben_felds_ergeben_eine_sendung` ·
`punkt_fuer_beendetes_oder_neu_belegtes_feld_wird_verworfen` (ersetzt
`score_push_queue_serialises_per_court_and_keeps_only_the_newest`, `server.rs:5451`) ·
`zwanzig_felder_bei_300_ms_je_anfrage_bleiben_unter_der_latenzgrenze` ·
`handle_score_wartet_nie`. Die vorhandenen `handle_score_*`-Tests bleiben grün.

**Doku:** `docs/tablet.md` (Punktweg) · `docs/liveticker-push.md` · `docs/changelog.md`.

**Reviews:** `code-reviewer`.

### PR 3 — Weniger Vollstände, Lebenszeichen-Vertrag, kein Doppelversand

**Erfüllt:** AK2–AK6.

**Inhalt.** Strukturmarke und Punktänderungen in `diff.rs` (L10); `apply_tablet_scores`
gibt die Menge zurück (L9); Vollstand-Regel „letzter **voller** `tset` ≥ 60 s"; Deckel N;
Diff-Basis nur noch nach bestätigtem vollem `tset`; Frist nach Körpergröße (L6).

**Dateien:** `badhub/diff.rs`, `badhub/sender.rs`, `tablet/state.rs`, `sync.rs`,
`badhub/push.rs`.

**Tests:** `zwei_geaenderte_spiele_ergeben_zwei_tupdates_keinen_tset` ·
`tablet_getriebene_spiele_erzeugen_keine_loop_punkte` ·
`feld_ohne_tablet_bekommt_seinen_btp_stand` ·
`strukturmarke_aendert_sich_bei_feld_paarung_ergebnis_aufruf` ·
`strukturmarke_bleibt_bei_reinem_punkt` · `vollstand_nach_60_s_auch_wenn_tupdates_fliessen` ·
`tupdate_verschiebt_den_vollstand_nicht` · `vollstand_traegt_den_stand_vom_sendezeitpunkt` ·
`mehr_als_n_punktaenderungen_ergeben_einen_tset`. Die vorhandenen `diff.rs`-Tests werden
angepasst.

**Doku:** `docs/liveticker-push.md` · `docs/preparation.md` (Aufruf-Fingerabdruck) ·
`docs/changelog.md`.

**Reviews:** `code-reviewer`.

### PR 4 — Punktverlauf

**Erfüllt:** AK25.

**Inhalt.** Regeln L11.

**Dateien:** `badhub/verlauf.rs`, `badhub/sender.rs`, `tablet/timeline.rs`,
`tablet/sheet.rs`, `badhub/push.rs`.

**Tests:** `verworfen_groesser_null_stempelt_nichts` · `nach_drei_versuchen_wird_geparkt` ·
`geparkter_eintrag_kommt_nach_neuem_tset_wieder` · `ganz_ablehnung_rotiert_die_auswahl` ·
`ohne_bestaetigten_tset_kein_verlauf` · `unveraenderte_spiele_werden_nicht_gebaut` (zählt
die Aufrufe) · `marke_steigt_bei_rally_sync_ereignis_und_pause` ·
`turnierwechsel_leert_den_stand`.

**Doku:** `docs/punktverlauf.md` · `docs/liveticker-push.md` · `docs/changelog.md`.

**Reviews:** `code-reviewer`.

### PR 5 (nur nach Messung) — Spielplan nur bei Änderung

Inhaltsmarke ohne `rid`; trotzdem mindestens alle 10 min. Test:
`unveraenderter_spielplan_geht_nicht_erneut`. **Entfällt**, wenn die Messung weniger als
50 % unveränderte Minuten zeigt. Doku: `docs/liveticker-push.md`, `docs/changelog.md`.
Review: `code-reviewer`.

### Reihenfolge

Bindend: PR 1 → PR 2a → PR 2b, und PR 2a → PR 3 → PR 4. PR 3 setzt PR 2b fachlich voraus
(„Tablet-Punkte nur über den Punktweg"); wird PR 2b wegen der Messung zurückgestellt, bleibt
PR 3 liegen. PR 5 ist unabhängig von 2b–4.

## Messung

Die Fixtures im Repo (10 und 36 Spiele) sind kein realistischer Stand. Belastbar ist bisher
nur ein Wert aus dem Code: Köpi-Cup 2026, `tset` 187 KiB bei 500 beendeten Spielen, rund
380 Byte je Eintrag (`badhub/payload.rs:66–76`); der Deckel von 1000 beendeten Spielen
ergibt etwa 370 KiB.

**Schritt A — lokal, mit PR 1.** Ein ignorierter Unit-Test im Crate (z. B. in
`badhub/payload.rs`; im Crate, weil `prognose_auffrischen` `pub(crate)` ist,
`tablet/tl.rs:4290`). Eingabe: eine Umgebungsvariable mit dem Pfad zu einem echten
`SENDTOURNAMENTINFO`-Mitschnitt. Ausgabe: Bytes von `tset` (ohne Logo), `tupdate_match`,
Meldeliste, `sched`, `tverlauf` (je Spiel und je 64er-Paket); Anzahl laufender, beendeter
und kommender Spiele; ob `sched(t)` und `sched(t + 60 s)` ohne `rid` byte-gleich sind.
Aufruf: `cargo test -p bts-light --lib messung_ -- --ignored --nocapture`.

**Schritt B — im Feld, mit PR 1.** Eine Debug-Zeile je Sendung (Kanal, Bytes, Dauer) und je
`sched` „Inhalt geändert ja/nein". Auswertung aus dem Diagnose-Log des nächsten Turniers.

| Messwert | Entscheidung | Vor welchem PR |
|---|---|---|
| Dauer eines `tupdate_match` gegen Produktion | Trägt die serielle Strecke 20 Felder? Richtwert: Median ≤ 400 ms genügt für rund 2 Punkte/s. Darüber kommt in PR 2b ein Sicherheitsventil dazu (ab K wartenden Feldern ein Sammel-`tset`; K wird aus dem Messwert festgelegt und vor dem Merge in dieser Spec nachgetragen) | PR 2b |
| Größe des vollen `tset`, Strukturänderungen je Minute | Deckel N (vorläufig 4); ob die erweiterte Strukturmarke zu viele Vollstände auslöst (dann zurück auf die zwei heutigen Fingerabdrücke plus Ergebnis) | PR 3 |
| Größe von `sched` und des größten `tverlauf`-Pakets | Abstand zur 5-MiB-Grenze; Parameter der Frist nach Körpergröße | PR 3 |
| Anteil unveränderter `sched`-Minuten | ≥ 50 % → PR 5 bauen; sonst bleibt der Minutentakt. Bei ausgeschalteter Prognose ist der Plan trivial stabil | PR 5 |

## Risiken & Rollback

**Risiken im laufenden Turnier**

- **Durchsatz der seriellen Strecke.** badhub prüft je Request das Passwort gegen alle
  Zugänge. Dauert ein `tupdate_match` länger als etwa 400 ms, stauen sich Punkte bei vielen
  Feldern. Absicherung: Messung B vor PR 2b, AK1, Sicherheitsventil, PR 2b einzeln
  zurückdrehbar.
- **Weniger Vollstände.** badhub pflegt Spieler-Index, Gäste und das Aufräumen der
  Kind-Turniere nur im `tset`-Zweig (Fenster 30 min). Der 60-s-Vollstand deckt das. Im Halt
  geht nichts hinaus — der Liveticker gilt nach 10 min als veraltet und ist nach 30 min
  ausgeblendet. Das ist gewollt; die Statuszeile ist rot.
- **Antwortprüfung zu streng.** Ein Empfänger mit anderem Antwortformat gälte dauerhaft als
  gestört. Deshalb die lockere Regel (JSON-Objekt mit `type`).
- **Großer `tset` auf schmalem Uplink** blockiert Punkte für die Dauer des Uploads (bis zur
  Frist). Absicherung: kein Logo im Fehlerfall, Logo-Sperre; die längere Frist nach
  Körpergröße kommt erst in PR 3.
- **Erweiterte Strukturmarke** kann bei Massenänderungen (Auslosung) Vollstände häufen — je
  Zyklus höchstens einer; die Messung prüft das.
- **`build_client` wird geteilt.** Derselbe Client-Bau dient `ServerCtx.http`
  (`commands.rs:2467`: Relay-Steuerung, Vereinslogos) und dem Log-Upload. Der
  `connect_timeout` wirkt dort mit — erwünscht, aber im Review von PR 1 zu prüfen.
- **Abgebrochene Anfrage, die badhub doch noch verarbeitet.** Nach jeder Störung folgt ein
  voller `tset`; der heilt einen dadurch veralteten Stand.

**Rollback.** Jeder PR ist einzeln per Revert zurückzunehmen: kein neues Config-Feld, keine
neue Datei auf der Platte, kein neues Wire-Format, kein badhub-Deploy. Ein Downgrade per
Auto-Update ist gefahrlos. Reihenfolge beim Zurücknehmen: 4 → 3 → 2b → 2a → 1.

## Bewusste Abwägungen

- **Halt ohne Probe-Versuch.** Ein falsches Passwort heilt nicht von selbst; jeder weitere
  Versuch zählt bei badhub auf die Auth-Sperre. Preis: Wird das Passwort bei badhub
  korrigiert statt in der App, bleibt die Übertragung stehen, bis jemand speichert oder neu
  startet.
- **Streng seriell statt begrenzt parallel.** Langsamer bei vielen Feldern, dafür richtige
  Reihenfolge und kein Wegschreiben bei badhub.
- **Punkte werden im Fehlerfall verworfen, nicht gepuffert.** Der nächste volle `tset` trägt
  den Stand; ein Puffer brächte nur Zwischenstände, die niemand mehr sehen will.
- **Lockere Antwortprüfung.** Ein Portal, das zufällig ein JSON-Objekt mit `type` liefert,
  bliebe unerkannt.
- **Hand-Hallen und manuelle Reihenfolge** erreichen badhub weiter mit bis zu 60 s Verzug.
- **Der Verlauf-Stand überlebt das Speichern nicht** — alle Verläufe gehen danach in
  64er-Paketen neu, wie heute.
- **Der Sendezustand liegt nur im RAM.** Nach einem App-Neustart beginnt alles mit einem
  vollen `tset`, und der erste Versuch darf das Logo tragen.
- **`tverlauf`-Regeln ohne eigenen ADR.** Sie stehen hier (L11) und in
  `docs/punktverlauf.md`; das How-To hielt einen dritten ADR für optional.

## Offene Fragen / Annahmen

Annahmen aus dem How-To, die nicht am Code belegt sind:

| Annahme | Zu prüfen in |
|---|---|
| Alle Empfänger (auch Test-Hosts) antworten mit einem JSON-Objekt mit Feld `type` | PR 1 (Probe gegen Produktion und Testsystem vor dem Merge) |
| Ein Mitschnitt eines großen Turniertags liegt vor oder lässt sich ziehen; Rückfall: der Zwei-Hallen-Mitschnitt, synthetisch auf rund 1000 Spiele vervielfacht | PR 1 (Schritt A) |
| Der Live-Stand eines Felds lässt sich aus `match_for_court` (`tablet/state.rs:2802`) und der Feld-Session leicht lesen | PR 2a (Entwurf), PR 2b (Umsetzung) |
| Die Transport-Schnittstelle nutzt `async fn` in Traits (Rust ≥ 1.75); im Repo ist keine Mindestversion festgeschrieben | PR 2a |
| Eine abgebrochene Anfrage, die badhub doch noch vollständig verarbeitet, ist vernachlässigbar | PR 2a (Review) |
| Der Neuversand aller Verläufe nach dem Speichern ist hinnehmbar; sonst gehört der Verlauf-Stand in den Merker | PR 4 |

Festlegungen dieser Spec, wo das How-To nur einen Rahmen nennt (bei der Freigabe mit
bestätigt):

- Wachstum der `tset`-Ablehnungspause: Verdopplung von 60 s bis 10 min (das How-To nennt nur
  Anfang und Deckel).
- Lastmodell von AK1: 20 Felder, zusammen 2 Punkte/s (aus dem Richtwert der Messung).
- Der Statustext für „`tset` abgelehnt" kommt schon mit PR 1 (das How-To zählt dort nur
  gestört, gedrosselt, angehalten auf; die Klassifikation „abgelehnt" entsteht aber in
  PR 1).
- `tupdate_match` läuft bei abgelehntem `tset` nur weiter, solange eine bestätigte Basis
  besteht.

Offen bleibt nur, was an Messwerten hängt (Tabelle „Messung").

## Folgepunkte für badhub (nicht in dieser Spec)

- **gzip annehmen** (`Content-Encoding: gzip` am Liveticker-Endpunkt). Erst danach kann
  bts-light komprimieren, mit Rückfall für ältere Stände.
- **Sperre oder Transaktion im `tupdate_match`-Zweig.** Parallele Updates können sich heute
  Punkte wegschreiben; clientseitig durch die serielle Strecke entschärft, für andere
  Sender nicht.
- **Reihenfolge-Prüfung** (z. B. über `rid`), damit ein älterer Stand einen neueren nicht
  ersetzt.

Alle drei sind in [roadmap.md](../roadmap.md) vermerkt.

## Betroffene Doku-Dateien

- **`docs/liveticker-push.md`** (neu, PR 1; fortgeschrieben in jedem PR) — die bisher
  fehlende Doku der Push-Strecke: Kanäle, Fehlerklassen, Backoff, Halt, Logo, Sender,
  Vollstand-Regel. Eintrag in **`docs/handbuch.json`** unter „intern".
- `docs/logging.md` — Zeile 26 („fehlgeschlagene Liveticker-Pushes") korrigieren;
  Flanken-Zeilen beschreiben (PR 1).
- `docs/oberflaechen.md` — Statuszeile des Dashboards mit `pruef:`-Anker je Text (PR 1,
  PR 2a).
- `docs/spieler-check-in.md` — Meldeliste: 409 und Pausen (PR 1).
- `docs/tablet.md` — Punktweg des Tablets (PR 2b).
- `docs/preparation.md` — Aufruf als Teil der Strukturmarke (PR 3).
- `docs/punktverlauf.md` — Antwort auswerten, Parken, Rotation, Änderungsmarke (PR 4).
- `docs/changelog.md` — je Version.
- `CLAUDE.md` — neue Zeile in der Doku-Tabelle („badhub-Push-Strecke" → diese Spec,
  `docs/liveticker-push.md`, ADR 0086/0087), angelegt in PR 1, ergänzt in PR 2a–4.
- `docs/roadmap.md` — Stand je PR.
- ADR [0086](../adr/0086-ein-geordneter-sender-fuer-die-badhub-strecke.md),
  [0087](../adr/0087-voller-tset-nur-bei-strukturaenderung.md).

## Umsetzungs-Hinweise

- Erst nach der Freigabe dieser Spec wird Produktivcode geschrieben.
- Muster im Haus: `CheckinLese` (`sync.rs:3057`; Zustand außerhalb der Engine,
  Flanken-Log), `badhub/tp_takt.rs` (reiner Zustandsautomat mit `einordnen`/`anwenden`,
  Grenzen für `Retry-After`), `badhub/tp_upload.rs` (`Retry-After` lesen, Körper begrenzt
  lesen), `#[tokio::test(start_paused = true)]` in `btp/write_gate.rs`.
- Nicht während eines laufenden Turniers mergen; PR 2b und PR 3 nur mit den zugehörigen
  Messwerten.
- Kommentare im Code auf Deutsch (was und warum).
