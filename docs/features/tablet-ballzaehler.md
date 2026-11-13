# Ballzähler am Tablet — Spezifikation

> Status: **Entwurf, Review-Checkliste bestanden 13.11.2026 — Freigabe offen**
> (via /idee: Brief → Grill → How-To → Review).
> Quelle: Idee vom 13.11.2026. Betroffene Crates: src-tauri / relay / relay-proto / src.
> ADR: [0093](../adr/0093-ballzahl-reist-mit-dem-ergebnis.md).

## Kontext / Problem

Die Turnierleitung rechnet den Federballverbrauch je Spiel ab. BTP führt dafür
am Spiel das Feld `Shuttles`. Heute zählt der Schiedsrichter auf Papier oder im
Kopf, und jemand tippt die Zahl später von Hand in BTP — oder sie fehlt.

bts-light liest und schreibt `Shuttles` bisher nicht. Das Original-BTS schreibt
es; in bts-light war der Punkt „bewusst zurückgestellt"
([btp-write-vergleich-letilo.md](../btp-write-vergleich-letilo.md), Punkt 5).

## Zielbild & Erfolgskriterien

- Am Zähl-Tablet werden die Bälle eines Spiels mit einem Tipp hochgezählt.
- Die Zahl kommt **zusammen mit dem Ergebnis** in BTP an, ohne dass jemand sie
  abtippt.
- Die Turnierleitung sieht die Zahl im TL-Web und kann sie beim Eintragen oder
  Korrigieren eines Ergebnisses selbst angeben.
- Erfolg am Turnier: Nach einem Spiel mit gezählten Bällen steht in BTP dieselbe
  Zahl wie am Tablet; bei ausgeschaltetem Schalter ändert sich für niemanden
  etwas.

## Nicht-Ziele

- Kein laufendes Schreiben nach BTP je Ball; BTP erfährt die Zahl erst mit dem
  Ergebnis.
- Kein eigener Schreibvorgang nur für die Ballzahl an einem gewerteten Spiel.
- Keine Anzeige auf Monitoren, Zähltafel, Übersicht, Stream, Meetingpoint,
  Schiedsrichterzettel oder im badhub-Liveticker.
- Keine Ballzahl bei kampflosen Spielen, Walkover-Kaskade und Disqualifikation.
- Liga-Spiele (`PlayerMatch`) sind nicht abgedeckt.
- Keine Auswertung (Summen je Turnier/Tag) in bts-light — das macht BTP.

## Verhalten

### Schalter

- Neuer Schalter im Setup: **„Bälle am Tablet zählen"**, Standard **aus**.
- Der Schalter erreicht das Tablet über `MatchBrief.shuttleCounter`. Das Feld
  ist zugleich die Fähigkeitsmeldung: Ein Host, der die Zahl nicht schreiben
  kann, sendet es nicht, und das Tablet zeigt dann keinen Zähler.
- Eine Änderung des Schalters wirkt am Tablet mit der nächsten Spielzuweisung
  bzw. dem nächsten `match_assigned`.
- Bei ausgeschaltetem Schalter ignoriert der Host eine mitgeschickte Ballzahl.

### Zählen am Tablet

- Der Zähler steht während des Spiels sichtbar in der Zählansicht: Ballsymbol,
  Zahl, **+** und **−**. Start bei **0**; jeder Ball wird getippt, auch der
  erste.
- Bereich 0–99. **−** bei 0 und **+** bei 99 tun nichts.
- Jeder am Tablet darf zählen (Schiedsrichter wie Zähltafelbediener), ohne PIN.
- Der Zähler ist **unabhängig vom Punkt-Undo**: „Punkt zurück" ändert die
  Ballzahl nicht; ein falscher Tipp wird mit **−** korrigiert.
- Die Zahl überlebt Neuladen, Satzpause, Seiten-/Mannschafts-Korrektur und den
  Wechsel auf ein anderes Tablet (sie reist im gespiegelten Stand mit).
- Übernimmt ein Tablet einen fremden Stand, gilt dessen Ballzahl. Die Ballzahl
  ist **kein** Kriterium dafür, welcher Stand „weiter" ist.
- Ein Ersatz-Tablet ohne übernommenen Stand beginnt bei 0. Die Zahl ist dann zu
  klein; die Turnierleitung kann sie beim Eintragen oder Korrigieren nachziehen.
- Mit dem Übermitteln des Ergebnisses ist die Zahl **eingefroren**: **+**/**−**
  sind gesperrt, bis das Tablet das Ergebnis wieder öffnet. Ein gepuffertes
  oder geparktes Ergebnis trägt die Zahl vom Zeitpunkt des Übermittelns.
- Bei Aufgabe reist die bis dahin gezählte Zahl mit.

### Übertragung nach BTP

- Die Ballzahl ist ein optionales Feld des Tablet-Ergebnisses
  (`ResultBody.shuttles`). `process_result` prüft sie (R5):
  - fehlt → keine Ballzahl;
  - 0–99 → übernommen;
  - größer als 99 → **Zahl verworfen, Ergebnis trotzdem angenommen**
    (Log-Zeile ohne Spielernamen).
- Die Zahl geht im **selben** `SENDUPDATE` wie das Ergebnis als
  `Match.Shuttles` nach BTP. Ist keine Zahl da, hängt das Verhalten am
  Messergebnis (siehe „Zwei Zweige").
- Die Nachschub-Queue speichert die Zahl mit dem Ergebnis und sendet sie beim
  Nachschub unverändert.
- Wird ein Tablet-Ergebnis als „überholt" abgewiesen oder im Nachschub
  verworfen, weil BTP schon ein Ergebnis hat, **verfällt** die Tablet-Zahl.
- Sendet das Tablet dasselbe Ergebnis erneut (Idempotenz), wird nicht noch
  einmal geschrieben — auch nicht, wenn sich nur die Ballzahl unterscheidet.
  Wegen des Einfrierens kann das im Normalbetrieb nicht vorkommen.

### Turnierleitung

- **Laufendes Spiel:** Der Host liest die Zahl aus dem gespiegelten
  Tablet-Stand und zeigt sie im TL-Web an der Feld-Kachel. Diese Anzeige ist
  „bestmöglich" — sie fehlt, solange kein Stand gespiegelt wurde (altes Relay,
  Tablet offline). Verbindlich ist erst der Wert im Ergebnis.
- **Beendetes Spiel:** bts-light liest `Match.Shuttles` aus BTP und zeigt die
  Zahl in der Beendet-Liste.
- Bei 0 oder fehlendem Wert zeigt das TL-Web **nichts**.
- Die Anzeige erscheint nur bei eingeschaltetem Schalter.
- **Ergebnis eintragen** (TL-Web und Desktop-App): Bei eingeschaltetem Schalter
  hat der Dialog ein Feld „Bälle" (0–99, darf leer bleiben). Wird das Spiel am
  Tablet gezählt, ist es mit der gespiegelten Tablet-Zahl vorbelegt.
- **Ergebnis korrigieren** (TL-Web, vorhandene Korrektur mit `overwrite`): Das
  Feld ist mit dem BTP-Wert vorbelegt und änderbar. Es gelten alle bestehenden
  Sperren der Korrektur (KO-Spiel mit Folgespiel). Eine Korrektur, die nur die
  Ballzahl ändert, ist erlaubt und läuft als vollständiger Ergebnis-Rewrite.
- Ein leeres Feld heißt „keine Angabe", nicht 0.
- Der Schalter „Bälle am Tablet zählen" steuert auch das Feld in den Dialogen.

### Zwei Zweige — die Messung entscheidet

Ob BTP `Shuttles` behält, wenn ein späterer Schreibvorgang am selben Spiel das
Feld weglässt, ist nicht gemessen. Bei `CourtID`, `Official1ID`/`Official2ID`
und `Highlight` liest BTP „fehlt" als 0; `Sets`/`Winner` bleiben dagegen
erhalten. Der erste Umsetzungsschritt ist deshalb ein Messwerkzeug.

- **Zweig A — „fehlt = bleibt":** `Shuttles` steht nur im Ergebnis-Write und
  nur, wenn eine Zahl vorliegt. Alle anderen Schreibvorgänge bleiben
  unverändert.
- **Zweig B — „fehlt = 0":** Jeder Match-Write von bts-light
  (`update_request`, `court_assign_request`, `highlight_request`, Planung in
  `eigene_writes`) trägt `Shuttles` mit dem zuletzt aus BTP gelesenen Wert mit —
  **auch bei ausgeschaltetem Schalter**, denn sonst löscht bts-light schon
  heute eine in BTP von Hand eingetragene Zahl. Ein Ergebnis-Write ohne eigene
  Zahl schreibt den gelesenen BTP-Wert zurück.

Nimmt BTP `Shuttles` im Ergebnis-Write gar nicht an, ist das Feature in dieser
Form nicht baubar; dann endet die Umsetzung nach PR 0 mit einem dokumentierten
Befund.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - `relay-proto`: `ResultBody.shuttles`, `RelayFrame::Result`,
    `MatchBrief.shuttle_counter`, `TlAction::EnterResult.shuttles`,
    Streichliste in `schlanker_anzeige_stand`.
  - `relay/`: Durchleitung im `Result`-Frame (wird Feld für Feld gebaut),
    Deckel auf den Wert.
  - `src-tauri`: `btp/model.rs` (`BtpMatch.shuttles`), `btp/proto.rs`
    (`MatchUpdate.shuttles`), `tablet/server.rs` (`process_result`,
    `match_brief`, TL-Ergebnispfad), `tablet/state.rs`
    (`PersistedMatchUpdate`, Lesen aus dem Stand), `tablet/tl.rs` (`TlCourt`,
    `TlFinished`, `execute_result_action`), `commands.rs` (`enter_result`),
    `config.rs`.
  - `assets/tablet.html`, `assets/tl.html`, `src/api.ts`, Ergebnis-Dialog der
    Desktop-App, `SetupWizard.tsx`.
- **Architekturregeln:**
  - R1: Desktop-Dialog nur über den Command `enter_result`.
  - R2: Verbindlich ist der Wert in BTP; die TL-Anzeige beendeter Spiele kommt
    aus `SENDTOURNAMENTINFO`, geschrieben wird per `SENDUPDATE`.
  - R3: LAN und Cloud nutzen dieselben Wege (Ergebnis-Body, Zustands-Spiegel).
  - R5: Validierung in `process_result`, gemeinsam für beide Pfade.
- **Konfiguration & Abwärtskompatibilität:** ein neues Bool-Feld mit
  `serde(default)` = aus; alte Configs bleiben lesbar, eine ältere App-Version
  ignoriert das Feld. `identifier` und Updater-Pfad unangetastet.
- **Mischversionen:**
  - Altes Tablet + neuer Host: kein Zähler, Ergebnis ohne Zahl.
  - Neues Tablet + alter Host: `shuttleCounter` fehlt → kein Zähler.
  - Altes Relay: würde `shuttles` still wegwerfen → **Relay-Deploy vor dem
    Client** (läuft automatisch beim Merge nach `main`).
  - Die Ballzahl wird **nicht** als `EventKind` modelliert (kein Catch-All).
- **Datenschutz:** kein Personenbezug. Die Zahl steht nur hinter dem
  Gerätezugang im TL-Zustand und nie in öffentlichen Anzeige-Zuständen.
- **Abhängigkeiten:** BTP-Verhalten von `Shuttles` (Messung). Keine neue
  Cargo-/npm-Dependency.

## Akzeptanzkriterien

Schalter
- [ ] Standard ist aus; eine Config ohne das Feld lädt mit „aus".
- [ ] Bei „aus" zeigt das Tablet keinen Zähler, das TL-Web keine Zahl, die
      Dialoge kein Feld.
- [ ] Bei „aus" verwirft der Host ein mitgeschicktes `shuttles` und schreibt
      das Ergebnis wie bisher.
- [ ] Ein `MatchBrief` ohne `shuttleCounter` liest sich als „aus".

Tablet
- [ ] Der Zähler beginnt bei 0; **+** erhöht um 1, **−** senkt um 1, nie unter
      0 und nie über 99.
- [ ] „Punkt zurück" ändert die Ballzahl nicht.
- [ ] Nach Neuladen, Seiten-Korrektur und Tablet-Wechsel (Stand übernommen)
      steht dieselbe Zahl.
- [ ] Nach dem Übermitteln sind **+**/**−** gesperrt; nach dem Wiedereröffnen
      wieder frei.
- [ ] Der Ergebnis-Body trägt `shuttles` genau dann, wenn der Zähler sichtbar
      war.
- [ ] Der gespiegelte Stand bleibt unter `MAX_STATE_LEN`
      (`scripts/test-stand-groesse.mjs`).

Host / BTP
- [ ] Ergebnis mit `shuttles: 7` → der `SENDUPDATE`-Match-Knoten enthält
      `Shuttles = 7`.
- [ ] Ergebnis mit `shuttles: 100` → Ergebnis angenommen, kein
      `Shuttles`-Knoten aus dem Tablet-Wert.
- [ ] Ergebnis ohne `shuttles` → Zweig A: kein `Shuttles`-Knoten; Zweig B:
      `Shuttles` = zuletzt gelesener BTP-Wert.
- [ ] Kampflos, Walkover-Kaskade und Disqualifikation tragen nie einen
      Tablet-Wert.
- [ ] Die Nachschub-Queue übersteht einen App-Neustart mit der Zahl; ein
      Queue-Eintrag aus einer älteren Version (ohne Feld) lädt.
- [ ] Ein „überholt" abgewiesenes Ergebnis schreibt keine Ballzahl.
- [ ] Der Cloud-Pfad liefert dieselbe Zahl beim Host ab wie der LAN-Pfad.
- [ ] `BtpMatch.shuttles` liest einen vorhandenen Wert und verkraftet ein
      fehlendes Feld.
- [ ] Nur Zweig B: Feldvergabe, Feldfreigabe, Officials- und Highlight-Write
      tragen `Shuttles` mit dem gelesenen Wert.

Turnierleitung
- [ ] Laufendes Spiel mit gespiegelter Zahl 3 → `TlCourt.shuttles = 3`; ohne
      gespiegelten Stand fehlt das Feld.
- [ ] Beendetes Spiel mit BTP-Wert 5 → `TlFinished.shuttles = 5`; bei 0 oder
      fehlend fehlt das Feld.
- [ ] `EnterResult` mit `shuttles` schreibt die Zahl; ohne das Feld (älterer
      Browser) verhält sich die Aktion wie bisher.
- [ ] Der Eintragen-Dialog ist mit der Tablet-Zahl vorbelegt, der
      Korrigieren-Dialog mit dem BTP-Wert.
- [ ] Eine Korrektur, die an `correction_blocker` scheitert, schreibt auch
      keine Ballzahl.
- [ ] Monitor-, Overview- und Tafel-Zustand enthalten nie ein `shuttles`-Feld.

## Tests

- **relay-proto:** Serde-Roundtrips für `ResultBody.shuttles`,
  `MatchBrief.shuttleCounter` (fehlend → `false`),
  `TlAction::EnterResult.shuttles` (fehlend → `None`);
  `schlanker_anzeige_stand` streicht `shuttles`.
- **relay:** `Result`-Frame reicht `shuttles` durch; Wert über dem Deckel wird
  entfernt, das Ergebnis geht weiter.
- **btp/proto.rs:** `update_request` mit/ohne `Shuttles`; Zweig B:
  `court_assign_request`/`highlight_request` reasserten.
- **btp/model.rs:** Parser liest `Shuttles`, fehlendes Feld → `None`.
- **tablet/server.rs:** `process_result` — gültig, zu groß, Schalter aus,
  überholt; manueller TL-Pfad.
- **tablet/state.rs:** Persistenz-Roundtrip der Queue inkl. Altformat; Lesen
  der Zahl aus dem gespiegelten Stand.
- **tablet/tl.rs:** `TlCourt`/`TlFinished`-Felder; Datenschutz-Wächter bleibt
  grün; Anzeige-Zustände ohne `shuttles`.
- **Tablet-JS:** reine Zähl-Logik als `src/io/ballzaehler.mjs` mit
  `scripts/test-ballzaehler.mjs`, Inline-Kopie in `tablet.html`, Drift in
  `scripts/test-inline-kopien.mjs`.
- **Messwerkzeug:** `tests/btp_shuttles_probe.rs` (`#[ignore]`, nur am
  Windows-PC mit BTP).
- `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --check`, `npm run build`.
- **Manuell am Turnier-PC:** Spiel zählen → Ergebnis senden → Zahl in BTP;
  danach Feld neu vergeben und Zahl in BTP erneut prüfen; Korrektur im TL-Web.

## Risiken & Rollback

- **BTP löscht die Zahl bei Folge-Writes** (Zweig B nicht erkannt) → die Messung
  in PR 0 steht vor allem anderen; die manuelle Prüfung „Feld neu vergeben"
  wiederholt sie am Ende.
- **Ergebnis-Write wird durch das neue Feld abgelehnt** → Messpunkt 2; im
  Betrieb schützt der Schalter (Standard aus).
- **Fehlbedienung** (versehentlicher Tipp) → **−**-Knopf; die Turnierleitung
  kann korrigieren.
- **Zu kleine Zahl nach Tablet-Tausch ohne Stand** → bewusst akzeptiert.
- **Rollback:** Schalter aus stellt das alte Verhalten her (in Zweig B bleibt
  nur das Reassert, das den BTP-Wert erhält). Eine ältere App-Version liest
  Config und Queue weiter.

## Offene Fragen / Annahmen

- **Offen:** Ergebnis der BTP-Messung (Zweig A oder B).
- **Offen, nur für Zweig B:** ob bts-light nach Spielende noch Officials-Writes
  auf beendete Spiele sendet; wird in PR 1 geprüft und abgedeckt.
- **Annahmen** (Grill-Empfehlungen, vom Nutzer nicht einzeln bestätigt):
  - Zähler unabhängig vom Undo; eingefroren ab dem Übermitteln.
  - Überholtes Tablet-Ergebnis → Zahl verfällt.
  - Anzeige am laufenden Spiel über den Zustands-Spiegel, „bestmöglich".
  - TL-Web zeigt bei 0 nichts.
  - Obergrenze 99; zu großer Wert kippt nicht das Ergebnis.
  - Zählen ohne Schiri-PIN.

## Betroffene Doku-Dateien

- Neu: `docs/ballzaehler.md` (Bedienung) und eine Zeile in der Doku-Tabelle der
  `CLAUDE.md`.
- `docs/btp_protocol.md` (Messung `Shuttles`, Schreibregel),
  `docs/btp-write-vergleich-letilo.md` (Punkt 5).
- `docs/tablet.md`, `docs/turnierleitung-web.md`, `docs/cloud-relay.md`
  (`shuttles`, `shuttleCounter`, Deploy-Reihenfolge).
- Handbuch mit `pruef`-Ankern: `docs/tablet-bedienen.md` (Knöpfe),
  `docs/einstellungen.md` (Schalter, Standard aus), `docs/oberflaechen.md`
  (Dialogfeld).
- `docs/roadmap.md`, `docs/changelog.md`.

## Umsetzungs-Hinweise

Erst nach Freigabe. Version je PR erst beim Merge festlegen (drei Dateien
gemeinsam). `code-reviewer` nach jedem PR; `security-reviewer` für PR 1 (neuer
Eingabewert aus dem Netz).

1. **PR 0 — Messwerkzeug.** `tests/btp_shuttles_probe.rs` nach dem Muster von
   `tests/btp_highlight_probe.rs`. Misst: Lesen von `Shuttles`; Ergebnis-Write
   mit Zahl; Ergebnis-Write ohne Zahl an Spiel mit Handwert; je ein Write ohne
   `Shuttles` für Feldvergabe, Freigabe, Officials, Highlight (laufend und
   beendet); Übernahme-Latenz. Lauf am Windows-PC, Befund nach
   `docs/btp_protocol.md`, Zweig hier eintragen.
2. **PR 1 — Kern + Wire (Relay-Deploy).** relay-proto-Felder, Relay-Durchleitung
   und Deckel, `BtpMatch.shuttles`, `MatchUpdate.shuttles`, `process_result`,
   Queue-Persistenz, Config-Schalter, `MatchBrief.shuttleCounter`. Zweig B:
   Reassert in allen Writern.
3. **PR 2 — Tablet.** Zähler-UI, Feld im Stand (außerhalb `snapshot`), Spiegel,
   Einfrieren, Body; Streichliste für Anzeige-Zustände.
4. **PR 3 — Turnierleitung.** `TlCourt`/`TlFinished`, Dialoge im TL-Web und in
   der Desktop-App inkl. Korrektur, SetupWizard, Handbuch.
