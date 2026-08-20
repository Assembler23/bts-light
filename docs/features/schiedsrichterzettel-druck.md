# Ausgefüllte Schiedsrichterzettel drucken — Spezifikation

> Status: **abgestimmt 2026-08-19** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Idee vom 19.08.2026; Vorarbeit in `docs/roadmap.md` seit 11.08.2026.
> Betroffene Crates: `relay-proto`, `src-tauri`, `relay`, `src/`.
> ADR: [0037](../adr/0037-zettel-ereignisse-eigener-strom.md) ·
> [0038](../adr/0038-ereignisse-append-only.md) ·
> [0039](../adr/0039-zettel-html-im-webview.md)
>
> **Nachfolge-Spec:** [schiedsrichterzettel-autodruck.md](schiedsrichterzettel-autodruck.md)
> (abgestimmt 20.08.2026) nimmt zwei Festlegungen dieses Dokuments ausdrücklich zurück — das
> Nicht-Ziel „kein Zettel für Spiele ohne Tablet" und den Vermerk „Internes Turnier-Archiv —
> kein amtlicher Beleg" — und ersetzt das Raster durch den DBV-Bogen
> ([ADR 0043](../adr/0043-zettelblatt-nach-dbv-vorbild.md)). Beim Lesen dieses Dokuments beide
> Punkte mitdenken; wirksam werden sie mit der Umsetzung dort.

## Kontext / Problem

Ein Spiel, das in bts-light gezählt wurde, lässt sich heute nicht als Spielzettel ausdrucken.
Der Punktverlauf wird zwar erfasst (`tablet/timeline.rs`, Spec
[punktverlauf-graph](punktverlauf-graph.md)), und am Tablet existiert bereits ein
**PIN-geschützter Schiri-Modus** ([umpire-mode](../umpire-mode.md)) mit Karten in gelb, rot und
schwarz — aber die Karten liegen **nur im `localStorage` des Tablets**, erreichen den Host nie
und überleben keinen Gerätewechsel. Nach dem Turnier ist der Spielverlauf damit nicht
nachvollziehbar.

Als Referenz diente `phihag/bup` (`js/scoresheet.js`): dort ist der Zettel eine Projektion eines
vollständigen Ereignis-Logs auf ein Zellenraster (`row = 2*team + player`, `col` = Ballwechsel).
**Weder bup noch die genannten Forks haben eine Lizenz** — Code und SVG-Vorlagen werden deshalb
nicht übernommen, das Verfahren ist eigenständig nachgebaut.

## Zielbild & Erfolgskriterien

Die Turnierleitung kann für jedes mit Tablet gezählte Spiel einen ausgefüllten Zettel drucken:
Punktverlauf, Aufschlagfolge, Karten, Verletzungen mit Beginn und Ende, Unterbrechungen,
Überstimmungen und Zeiten. Der Schiedsrichter erfasst die Ereignisse während des Spiels am
Tablet, ohne dass das Zählen darunter leidet.

**Status des Dokuments: internes Turnier-Archiv.** Kein amtlicher Beleg, kein Protestverfahren,
keine Verbandsklärung. [umpire-mode](../umpire-mode.md) bleibt gültig, dass offizielle Turniere
über das Original-BTS laufen.

Erfolgskriterien:
- Für jedes tablet-gezählte Spiel eines Turniertages lässt sich ein Zettel drucken.
- In einer Stichprobe stimmen Punktfolge, Aufschlagfolge, Karten, Pausen und Zeiten mit einem
  parallel geführten handschriftlichen Bogen überein.
- Kein Schiedsrichter berichtet, dass die Erfassung das Zählen verzögert oder behindert hat.
- Ein Tabletwechsel mitten im Spiel kostet keine Ereignisse.

## Nicht-Ziele

- Übernahme von Code oder SVG aus bup.
- Amtlicher Status, digitale Signatur, rechtsverbindliche Unterschrift.
- Nachbildung eines konkreten Verbandsformulars — das Layout orientiert sich am üblichen Bogen.
- Weitere Formulare: 5×11-Zählweise, Mannschaftskampf-, OBL-, NLA-Bogen.
- Zettel für Spiele **ohne Tablet** (von Hand eingetragenes Ergebnis) — keine Datenbasis.
- Nachträgliches Bearbeiten eines abgeschlossenen Zettels.
- Übertragung der Ereignisse an badhub oder in den Liveticker.
- Änderung des Punktverlaufs: `MatchTimeline`, `TimelineSet`, `punktverlauf/<slug>.json`,
  die Graph-Route und `timelineSetSvg` bleiben **unverändert**.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:** `relay-proto/src/lib.rs` (neue Wire-Typen neben dem
  Punktverlauf-Block) · **neu** `src-tauri/src/tablet/sheet.rs` (`SheetStore`) · **neu**
  `src-tauri/src/tablet/scoresheet.rs` (Projektion + HTML) · Ingest in
  `tablet/server.rs` + `tablet/relay_client.rs` · `relay/src/main.rs` (Durchleitung + Route) ·
  `commands.rs` (`match_scoresheet_html`) · `src-tauri/assets/tablet.html` (Erfassung) ·
  **neu** `src/io/matchEvents.mjs` · `src-tauri/assets/tl.html` + `src/pages/FieldOverviewPage.tsx` (Knöpfe).
- **Architekturregeln** (feature-lokal, mit **S-** vorangestellt — es sind **nicht** die
  R1–R6 der `CLAUDE.md`, wo R1 „Frontend spricht den Kern nur über Tauri-Commands" und
  R2 „BTP ist die Wahrheit" heißt):
  **S-R1** — der Desktop holt den Zettel ausschließlich über den Tauri-Command
  `match_scoresheet_html`; kein `fetch` aus React auf `127.0.0.1:8088` (das bräche zusätzlich den
  Cloud-Only-Betrieb).
  **S-R2** — der Zettel ist reine Druckform; kein Ereignis erzeugt eine Court→Match-Zuordnung oder
  ein Ergebnis. Die schwarze Karte bleibt ohne Ergebnisweg, die Wertung läuft weiter über
  `disqualify_match`.
  **S-R3** — Ingest über **beide** WS-Wege; Leseroute unter **identischem Pfad** am eingebetteten
  Server und am Relay. In einer fernen Halle hinter `slave_bridge.rs` gilt nur der Cloud-Pfad;
  wie beim Punktverlauf gibt es **keine Slave-Persistenz**.
  **S-R4** — der Ereignis-Ingest benutzt denselben Halter-Filter wie `Rally`.
  **S-R5** — **kein neuer Schreibweg.** Ereignisse gehen nie nach BTP, `process_result` bleibt der
  einzige Ergebnispfad und wird nicht angefasst.
  **S-R6** — unberührt.
- **Konfiguration:** **keine neuen Felder.** Der Gate ist der bestehende PIN-Schiri-Modus
  (`STATE.umpireMode`, je Tablet im `localStorage`).
- **Datenschutz:** Der Zettel trägt Spielernamen und optional Verein — das ist sein Zweck.
  **Kein Geburtsjahr.** Karten sind personenbezogene Sanktionsdaten und erscheinen
  ausschließlich auf dem Zettel: nie im `TlState`, nie im Punktverlauf-Graph, nie in
  `punktverlauf/<slug>.json`, nie im badhub-Push, nie im Liveticker. In der Ereignisdatei stehen
  **keine Namen**, nur `team`/`player` (Regel aus ADR 0015); Namen kommen zur Laufzeit aus dem
  BTP-Snapshot. Die Trennung wird durch Wächter-Tests **erzwungen**, nicht zugesagt.
- **Abhängigkeiten:** **keine neue Cargo- oder npm-Abhängigkeit, keine neue Tauri-Permission**
  (`dialog:allow-save` bleibt aus). Gedruckt wird über den WebView.

## Datenmodell

`MatchEvent` mit `id` (12 Hex, am Tablet erzeugt, Dedupe-Schlüssel), `seq`, `set`, `after_n`
(Zahl der aufgezeichneten Ballwechsel im Satz, 0 = davor), `score_a`/`score_b` als
Plausibilitätsanker, `ts_ms`, `kind`, `team`, `player`, `phase`, `retracts`.

`EventKind` = `serve_start` · `card_yellow` · `card_red` · `card_black` · `injury_start` ·
`injury_end` · `suspension` · `overrule` · `referee_call` · `retired` · `disqualified` ·
`retract`.
`Phase` = `play` · `break_eleven` · `break_game` · `break_injury` · `pre_match` · `post_match`.

**Kein Freitextfeld** — Texte und Symbole entstehen beim Rendern aus `kind`. Das nimmt
Größen-Explosion, HTML-Injection und unkontrollierten Personenbezug in einem Zug weg.

**Die Aufschlagfolge ist ein Ereignis, kein Feld:** `serve_start` trägt Aufschläger und
Empfänger. `TimelineSet` wird **nicht** erweitert.

**Deckel:** `MAX_EVENTS_PER_MATCH = 64` · `MAX_SHEET_LEN = 16 KiB` (geprüft in Relay **und**
Host-Store) · `MAX_EVENT_ID_LEN = 32` mit Hex-Whitelist · `MAX_SHEETS_PER_DOC = 40`.
`MAX_TIMELINE_LEN` bleibt bei 8 KiB und wird nicht angefasst.

## Layout

A4 quer, Satzspiegel 281 × 194 mm.

- **Kopf:** Turnier, Disziplin · Runde · Gruppe, Spielnummer, Feld + Halle, Datum,
  Beginn/Ende/Dauer aus `match_times`, SR und Service-Richter aus `court_officials`. Rechts oben
  der Vermerk **„Internes Turnier-Archiv — kein amtlicher Beleg"**.
- **Teamspalte links:** vier Zeilen im Doppel, zwei im Einzel; Name, optional Verein/Nation.
- **Raster rechts:** je Satz ein Block, `row = 2*team + player`, `col` = Ballwechsel,
  Zellinhalt = neuer Punktstand des Aufschlägers. 60 Spalten je Block; Ballwechsel 61–120 als
  zweite Zeilengruppe („Fortsetzung") → deckungsgleich mit `MAX_RALLIES_PER_SET`. Ab Satz 4
  Seitenumbruch mit verkürzter Kopfzeile.
- **Marker in der Zelle**, druckbar in Schwarz-Weiß: `V` (Verwarnung), `F` (rote Karte),
  `D` (Disqualifikation). Ereignisse mit `phase ≠ play` erscheinen als Marker-Spalte am Blockrand.
- **Protokollzeile:** Nr. · Uhrzeit · Satz + Stand · Art im Klartext · Spieler · „zurückgenommen".
- **Fuß:** Endstand je Satz, Sieger, Ergebnisart, zwei Unterschriftsfelder (ausdrücklich ohne
  rechtliche Bedeutung), Erzeugungszeitpunkt + Version.

## Akzeptanzkriterien

**Wire und Grenzen**
- [ ] Serde-Roundtrip für alle neuen Frames; ein Frame ohne die neuen Felder bleibt lesbar.
- [ ] Ein unbekannter `kind` führt zu einem Fehler, nicht zu einem Panic und nicht zu einer Annahme.
- [ ] Ungültige Werte werden abgewiesen: `id` außerhalb der Hex-Whitelist oder länger als 32,
      `team`/`player` außerhalb {0,1}, `after_n > MAX_RALLIES_PER_SET`, `set > MAX_TIMELINE_SETS`.
- [ ] **Die Graph-Antwort ist byte-gleich zu heute** (Golden-String über `MatchTimeline`).

**Ereignis-Store**
- [ ] Dasselbe Ereignis zweimal empfangen ändert den Bestand nicht (Dedupe über `id`).
- [ ] **Ein Ersatz-Tablet löscht keine Ereignisse:** Gerät A legt drei an, Gerät B synct mit
      leerer Liste → alle drei bleiben.
- [ ] Eine Rücknahme ist ein zusätzlicher Eintrag; nichts wird gelöscht.
- [ ] Das 65. Ereignis wird verworfen, der Bestand bleibt unversehrt.
- [ ] Ereignisse für ein fremdes Match oder `match_id <= 0` werden verworfen.
- [ ] Bestand übersteht einen App-Neustart; ein Turnierwechsel öffnet eine neue Datei, die alte
      bleibt erhalten.
- [ ] Die Reihenfolge ist stabil `(set, after_n, seq)`, auch bei vertauschtem Eingang.
- [ ] **`punktverlauf/<slug>.json` enthält keinen Sanktionsdatensatz und keinen Namen**;
      `zettel/<slug>.json` enthält die Karte, aber ebenfalls keinen Namen.

**Ingest LAN und Cloud**
- [ ] Ereignisse werden nur vom **aktiven** Tablet des Feldes und nur für dessen Match angenommen.
- [ ] Der Relay reicht die Frames **verbatim** durch; ein überlanger Sync wird verworfen.
- [ ] **Der Relay hält keine Ereignisse vor:** nach Trennung des Hosts liefert ein Abruf 503 und
      kein zwischengespeichertes Dokument.
- [ ] Ein Tablet, das offline war, bringt seine Ereignisse beim Reconnect vollständig nach.

**Projektion**
- [ ] Einzel 21:19: Zeilenzuordnung und Zellwerte stimmen.
- [ ] Doppel mit `serve_start`: vier Zeilen, Partnerwechsel bei eigenem Punkt, Side-out wechselt
      das Team, Empfänger-Diagonale korrekt.
- [ ] Ohne `serve_start` (Altbestand): Degradation auf zwei Zeilen, Zellwerte korrekt, Hinweis
      „Aufschlagfolge nicht aufgezeichnet" gesetzt.
- [ ] Eine rote Karte belegt **genau eine** Zelle — die des von ihr erzeugten Ballwechsels.
- [ ] Die Zellenzahl entspricht immer `points.len()`, unabhängig von der Ereigniszahl.
- [ ] Umbruch in die zweite Zeilengruppe ab Ballwechsel 61.
- [ ] Ein `mid_game`-Satz beginnt bei `start_a`/`start_b`.
- [ ] Ein zurückgenommenes Ereignis erscheint **nicht** im Raster, aber durchgestrichen im Protokoll.

**Renderer und Ausgabe**
- [ ] Das Dokument enthält `@page` mit `A4 landscape`, **kein** `<script>` und keine externe URL.
- [x] **Das Blatt geht auf** (Nachtrag v0.9.246, Nutzer-Befund 20.08.2026): Raster +
      Namensspalte + Abstand passen in die bedruckbare Breite. Die Maße sind Konstanten
      (`SEITE_NUTZBAR_MM`, `ZELLE_BREITE_MM`, `NAMENSSPALTE_MM`, `RASTER_ABSTAND_MM`,
      `ZEILE_HOEHE_MM`, `NAME_PT`, `ZUSATZ_PT`) und kein CSS-Text mehr — vorher ergaben
      60 × 4,2 mm + 42 mm + 3 mm = **297 mm bei 281 mm Platz**, der Zettel lief also
      16 mm über das Blatt hinaus. Zugleich brauchten Name (10 pt) + Zusatz (7,5 pt)
      zusammen 7,7 mm in einer 7 mm hohen Zeile. Wächter: `raster_passt_auf_die_seite`,
      `namen_bleiben_in_ihrer_zeilenhoehe`.
- [x] **Ein langer Doppelname sprengt die Namensspalte nicht:** feste Breite
      (`flex: 0 0`), `white-space: nowrap` + `text-overflow: ellipsis` — er wird gekürzt,
      statt das Raster vom Blatt zu schieben. Wächter:
      `lange_namen_sprengen_die_spalte_nicht`.
- [ ] Ein Spielername mit `<script>` aus dem BTP-Snapshot wird escaped ausgegeben.
- [ ] Namen erscheinen (Zweck), ein Geburtsjahr nirgends.
- [ ] Abruf für ein Match ohne Aufzeichnung und für ein Match außerhalb des aktuellen
      Turnier-Snapshots liefert 404.
- [ ] Stapel: drei IDs ergeben drei Abschnitte mit Seitenumbruch; 41 IDs werden abgewiesen.
- [ ] Am Relay die Statuscodes des Punktverlaufs: **401 nur bei verbundenem Host mit unbekanntem
      Token**; ohne `tl_index`-Eintrag oder ohne Host bewusst **503** — „ein Netzwackler kostete
      sonst jedes Gerät seine Kopplung" (`relay/src/main.rs`, `tl_access_state`). Vorbild-Test:
      `timeline_without_recording_yields_404_and_foreign_token_is_rejected`.
- [ ] **`sanktionsdaten_erreichen_den_anzeige_zustand_nie`**: Der Wächter-Test weist zuerst nach,
      dass der Fixture wirklich Karten trägt, und dann, dass weder Text noch Struktur des
      `TlState` sie enthält.

**Erfassung am Tablet**
- [ ] Der Ereignis-Knopf erscheint nur bei eingeschaltetem Schiri-Modus.
- [ ] Ereignisse sind erfassbar vor dem ersten Ballwechsel, in der Satzpause, im 60-s-Intervall
      und in der Behandlungspause.
- [ ] **Die rote Karte ist nur wählbar, wenn sie auch zählen kann**; sonst ist sie deaktiviert
      mit Hinweis. Nie eine Karte, deren Punkt still verschluckt wird.
- [ ] Undo nimmt Ereignisse jenseits des neuen Schnitts mit und erzeugt dafür eine Rücknahme;
      Ereignisse davor bleiben unberührt.
- [ ] Vereinigung zweier Stände ist idempotent und reihenfolgeunabhängig.
- [ ] Ein Tabletwechsel mitten im Spiel überträgt die Ereignisse an das neue Gerät.
- [ ] Vorhandene Karten aus dem alten `localStorage`-Schlüssel werden einmalig übernommen.
- [ ] Das Ereignis-Modal ist **in der Pause bedienbar** und schließt sich beim Pausenende
      selbst. **Abweichung von der ursprünglichen Fassung (E6, 19.08.2026):** Dort stand
      „liegt unter dem Pausen-Overlay, damit es ‚Weiterspielen‘ nie verdeckt". Das ist mit
      „in Pausen erfassbar" unvereinbar — unter dem Overlay (z-index 15) ist das Modal
      vollständig verdeckt, jeder Tipp landet auf dem Overlay, und die Erfassung in der
      Pause wäre tot. Es liegt jetzt **darüber** (z-index 18); die ursprüngliche Absicht
      ist anders eingelöst: immer sichtbares „Abbrechen" **und** Selbstschluss in
      `endBreak` (den es vorher gar nicht gab).

**Verträglichkeit**
- [ ] Alte Tablet-Seite gegen neuen Host: keine Ereignisse, Zettel druckt trotzdem.
- [ ] Neue Tablet-Seite gegen alten Relay: Frames werden still verworfen, nach dem Relay-Update
      holt der nächste Sync alles nach.
- [ ] Alte `tl.html` und alte Desktop-App kennen den Knopf nicht und laufen unverändert.
- [ ] LAN und Cloud liefern bei gleichem Zustand denselben Zettel.

## Tests

**Rust-Unit-Tests** je Etappe wie in den Akzeptanzkriterien benannt; hervorzuheben:
`graph_dto_bleibt_byte_gleich` · `ersatz_tablet_loescht_keine_ereignisse` ·
`punktverlauf_datei_bleibt_frei_von_sanktionsdaten` · `relay_haelt_keine_ereignisse_vor` ·
`sanktionsdaten_erreichen_den_anzeige_zustand_nie`.

**Client-Tests** nach dem Haus-Muster (kanonische Fassung in `src/io/*.mjs`, `node:assert`-Test
unter `scripts/test-*.mjs`, CI-Schritt, markierte Inline-Kopie im Asset): neues
`src/io/matchEvents.mjs` mit `scripts/test-match-events.mjs`; `scripts/test-serving.mjs` wird um
die `serve_start`-Nutzlast erweitert. `scripts/check-asset-syntax.mjs` deckt die Inline-Kopie ab.

**Layout wird als Struktur geprüft, nicht als Pixel:** getestet wird die reine Funktion
`sheet_grid` (Zellenzahl, Zeilenzuordnung, Umbruch); fürs HTML nur Smoke-Asserts.

**Pflichtläufe:** `cargo test`, `cargo clippy --workspace --all-targets`, `cargo fmt --check`,
`npm run build`, die neuen `node`-Tests im CI. **Manuell:** Druck-Test unter Windows-WebView2
**und** Android-Chrome; Turnier-Feldtest mit parallel geführtem Papierbogen.

## Umsetzung in Etappen

Acht PRs, jeder eigenständig grün und einzeln rückbaubar. **Der Zettel-Knopf erscheint erst in
E7** — bis dahin ist alles unsichtbar additiv.

E1 Wire · E2 `SheetStore` · E3 Ingest LAN + Cloud + Relay · E4 Projektion · E5 Renderer +
Lesepfade · E6 Tablet-Erfassung · E7 Ausgabewege · E8 Advisories, Doku, ADRs, Version.

**Nachtrag v0.9.246:** In TL-Web ist der Zettel jetzt auch an jeder Zeile der
**Beendet-Liste** abrufbar — dort im ⋮-Menü zusammen mit „Ergebnis korrigieren" und
„Punktverlauf", also derselben Zusammenstellung wie im ⋮-Menü der Feldkachel
(Nutzer-Wunsch 20.08.2026). Die Beendet-Zeile hat dafür ihr `content-visibility`
verloren: Es zieht `contain: paint` nach sich und schnitte das `position: fixed`-Menü
an der Zeilenkante ab (dieselbe Falle wie bei der Spielzeile). Unkritisch, weil die
Liste durch `FINISHED_LIMIT` (30) ohnehin nicht wachsen kann.

**In E8 mitzuerledigen (Befunde aus dem Grill):** `confirm_walkover` ruft heute
`timeline_store().finalize()` **nicht**, der TL-Web-Weg schon — wird geradegezogen; die schwarze
Karte bleibt Protokollnotiz ohne Ergebnisweg; `SheetStore` bekommt wie `TimelineStore` nur am
Master ein Verzeichnis; `docs/adr/README.md` wird nachgezogen (führt den Index nur bis 0013).

**In E6 mitzuerledigen (Befunde aus dem E3/E4-Review, 19.08.2026):**

- **`after_n` MUSS nach dem Punkt gelesen werden.** `applyCard('red')` bucht erst den Punkt
  (`addPointOnSide`) und protokolliert dann die Karte. Nur in dieser Reihenfolge zeigt `after_n`
  auf genau den Ballwechsel, den die Karte erzeugt hat — die Projektion in E4 verlässt sich
  darauf. Ein Test in E6 muss die Reihenfolge festnageln, sonst kippt sie unbemerkt.
- **Kein End-zu-Ende-Test über einen echten Socket** deckt den Halter-/Match-Filter des Ingests
  ab. Das ist Parität zum `Rally`-Pfad, der ebenfalls keinen hat — kein neues Loch, aber auch
  keine Absicherung. Wer `handle_socket` testbar macht, sollte beide gleichzeitig mitnehmen.

**Offen, nicht E3 anzulasten (Security-Review E3, 19.08.2026):** Der Relay setzt **keinen
expliziten WebSocket-Nachrichten-Deckel**; es gilt der `tungstenite`-Default von 64 MiB. Ein
Angreifer kann den Relay damit zum Parsen einer sehr großen Nachricht zwingen, **bevor**
`match_events_valid` oder `MAX_SHEET_LEN` sie verwerfen. Das betrifft **alle vier Frame-Typen**
gleichermaßen (auch `RallySync` seit ADR 0014), ist also ein eigenes Thema für den Relay
insgesamt — Vorschlag: `WebSocketUpgrade::max_message_size` in den vier `ws`-Handlern auf eine
Größe nahe der fachlichen Deckel setzen. **Nicht** in diese Spec einbauen, sondern als eigenes
Ticket führen.

**In E3 mitzuerledigen (Befunde aus Code- und Security-Review von E1, 19.08.2026):**

- ~~**Die Deckel brauchen Aufrufer.**~~ **Erledigt in E3:** `match_events_valid` **und**
  `MAX_SHEET_LEN` werden in `forward_match_event_sync` (Relay) geprüft, der Store prüft beide
  zusätzlich selbst — der LAN-Weg läuft an keinem Relay vorbei. Tests je Weg:
  `zettel_ereignisse_werden_verbatim_durchgereicht_und_gedeckelt`,
  `nur_der_halter_darf_zettel_ereignisse_schreiben` (Relay) und die Deckel-Tests in `sheet.rs`.
  Ursprünglicher Befund: `match_events_valid` (Zahl der Ereignisse) und
  `MAX_SHEET_LEN` haben nach E1 **außerhalb der Tests keine Aufrufstelle** — in E1 korrekt, weil
  jeder Frame verworfen wird, ab E3 aber eine Lücke. Der Ingest in `server.rs`,
  `relay_client.rs` und `relay/src/main.rs` muss beide **tatsächlich aufrufen**; ein Deckel neben
  dem ungeschützten Pfad ist keiner. Ein Test je Weg hält es fest.
- **`seq` und `ts_ms` sind nur halb gedeckelt.** `seq` ist auf `>= 0` geprüft, `ts_ms` gar nicht.
  Bewusst **keine erfundene Obergrenze im Wire-Typ**: Ein zu enger Deckel würde ein legitimes
  spätes Ereignis mitten im Turnier verwerfen, und beide Werte sind reine Anzeige-Größen ohne
  Arithmetik. Stattdessen: In E5 zeigt die Protokollzeile einen unplausiblen Zeitstempel als
  „—" statt als Unsinn — mit Test.
- `MAX_SHEETS_PER_DOC` wird in **E5** an der Leseroute durchgesetzt, **bevor** je Kennung
  gearbeitet wird (AK „41 IDs werden abgewiesen").
- **Gemessen in E2:** `MAX_SHEET_LEN` greift im Normalbetrieb nie — `MAX_EVENTS_PER_MATCH`
  bindet zuerst. 64 Ereignisse wiegen mit realistischen Zahlenwerten 16.193 Bytes gegen einen
  Deckel von 16.384; erst absurde `seq`/`ts_ms` treiben sie auf 17.345. Der Größen-Deckel ist
  damit **der Riegel gegen absurde Zahlenwerte**, nicht gegen viele Ereignisse. Wer ihn prüft,
  muss volle Zahlenwerte einsetzen, sonst prüft der Test einen Pfad, den er nie betritt.

## Version und Auslieferung

Bump gegen den **dann aktuellen** main-Stand, gemeinsam in `src-tauri/Cargo.toml`,
`src-tauri/tauri.conf.json` und
`package.json`, erst im letzten Etappen-PR. (Die Spec schrieb ursprünglich 0.9.239 → 0.9.240;
main stand beim Umsetzungsstart schon auf **0.9.243** und zieht während der acht Etappen weiter —
die Zahl wird deshalb erst in E8 festgelegt.) **Reihenfolge zwingend Relay vor App:** die neuen
Tablet-Frames sind eine Wire-Erweiterung, ein alter Relay verwirft sie still. Also E3 mergen,
Relay deployen, dann den Tag setzen.

## Risiken & Rollback

| Risiko | Wirkung im laufenden Turnier | Gegenmaßnahme |
|---|---|---|
| Ereignis blockiert das Zählen | Spielbetrieb steht | Ereignis-Code ruft nie in die Zähl-Logik (Ausnahme: bestehender `addPointOnSide` der roten Karte); Erfassen ist `push` + `send` ohne `await`; Persistenz best-effort; am Host derselbe `select!` und dieselben Deckel wie bei `Rally` |
| Modal verdeckt „Weiterspielen" | Pause lässt sich nicht beenden | z-Ordnung unter dem Pausen-Overlay, Selbstschluss bei Pausenende, immer sichtbarer Abbrechen; im manuellen Test von E6 zu bestätigen |
| Sanktionsdaten im falschen Kanal | Datenschutzverstoß | drei Wächter-Tests plus eigene Route und eigener Typ; ein Leck wäre ein Testfehler, keine Laufzeitüberraschung |
| HTML aus BTP-Fremdeingaben | Injection im WebView | zwingendes Escaping, eigener Test in E5, `security-reviewer` |
| Deckel erreicht | Ereignis fehlt lautlos | verwerfen statt wachsen, wie beim Punktverlauf |

**Rollback ist vollständig additiv:** ein zurückgerollter Host verwirft die neuen Frames still,
die Routen liefern 404, die Knöpfe verschwinden. `zettel/<slug>.json` bleibt gefahrlos liegen,
`punktverlauf/<slug>.json` ist unberührt. Keine Config-Änderung, keine Migration → Downgrade
jederzeit möglich, auch mitten im Turnier.

## Reviews

`code-reviewer` nach **jeder** Etappe. **`security-reviewer` bei E1, E3, E5 und E8** — E5 ist der
eigentliche Grund: ein neuer authentifizierter Endpunkt, der Sanktionsdaten ausliefert;
**erstmals HTML-Erzeugung aus BTP-Fremdeingaben**, die im Desktop-WebView in einem
`iframe srcdoc` landet; und der Stapel-Deckel als DoS-Riegel. E8, weil die Doku-Änderung eine
ausgesprochene Datenschutz-Zusage streicht.

## Doku-Pflicht im selben Commit

**Neue CLAUDE.md-Zeile** für dieses Feature **und** die heute fehlende Zeile für den Schiri-Modus
(`src-tauri/assets/tablet.html` `STATE.umpireMode`/`openCardModal`/`applyCard` → `docs/umpire-mode.md`).

Neu: `docs/schiedsrichterzettel.md` (Bedienung). Zu ändern: **`docs/umpire-mode.md`** — der Satz
„Bewusst nicht gebaut: Spielzettel-Export" wird gestrichen **und** die Zusage „Karten werden nur
lokal protokolliert (kein Versand an Server/badhub)" korrigiert: Karten reisen künftig zum Host,
weiterhin aber **nie** zu badhub und nie in den Liveticker. Dazu `docs/punktverlauf.md` (zwei
Projektionen), `docs/tablet.md`, `docs/cloud-relay.md`, `docs/turnierleitung-web.md`,
`docs/multi-hall.md`, `docs/schiedsrichter-management.md`, `docs/roadmap.md`,
`docs/changelog.md`, `docs/adr/README.md`.

## Offene Punkte / Annahmen

- **Annahme:** Ein Turnier erzeugt je Match deutlich weniger als 64 Ereignisse (realistisch ≈ 20:
  fünf `serve_start`, einige Karten, zwei bis vier Verletzungseinträge, Rücknahmen). Zeigt der
  Feldtest mehr, wird der Deckel einmalig angehoben und die Änderung hier vermerkt.
- **Annahme:** Der Druck über WebView2 und Android-Chrome liefert ein brauchbares Seitenbild.
  Das ist der erste Prüfpunkt in E7; scheitert es, ist ADR 0039 neu zu bewerten.
- **Bewusst offen gelassen:** Spiele ohne Tablet bekommen keinen Zettel. Es gibt keine
  Datenbasis, und ein halb ausgefüllter Bogen wäre irreführender als keiner.
