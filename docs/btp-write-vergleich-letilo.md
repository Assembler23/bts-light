# BTP-Rückschreibung: Vergleich Original-BTS (letilo) vs. bts-light

Stand 19.07.2026, nach Sonderversion 0.9.147. Vollständige Inventur beider
Codebasen (letilo-bts: `btp_proto.js`, `btp_conn.js`, `match_utils.js`;
bts-light: `btp/proto.rs`, `tablet/server.rs`, `commands.rs`, `sync.rs`).
Anlass: Tilos System ist länger im Einsatz — wo schreibt es mehr nach BTP
zurück, und was davon lohnt die Übernahme?

## Feld-für-Feld-Vergleich

### Im Kern gleichwertig (seit v0.9.147)

| Feld/Block beim Ergebnis | Original-BTS | bts-light |
|---|---|---|
| ID / DrawID / PlanningID, Sets, Winner, Duration (Minuten), Status | ✓ | ✓ |
| CourtID bleibt am beendeten Match (Feld sichtbar) | ✓ | ✓ seit 0.9.147 |
| Courts-Block: MatchID setzen/entfernen = Feld belegt/frei | ✓ | ✓ |
| Players-Block am Spielende: ID, LastTimeOnCourt (lokal), CheckedIn:false | ✓ | ✓ seit 0.9.147 |

### Was das Original-BTS zusätzlich schreibt

1. **`Highlight` im Match-Knoten** (btp_proto.js:91; Trigger u. a.
   match_utils.js:732/921/2158): Tilos **Vorbereitungs-Aufruf wird nach
   BTP gemeldet** und beim Ruf aufs Feld wieder auf 0 gesetzt. Unsere
   Aufrufe sind rein bts-light-intern — BTP kennt den Zustand über dieses
   Feld aber sehr wohl. **Wichtigster Fund.**
2. **Match-Update beim Ruf aufs Feld** (call_match, match_utils.js:193):
   zusätzlich Official1ID/Official2ID (Schiedsrichter), Highlight=0,
   optional Check-in-Bits. Wir schreiben nur
   ID/CourtID/DrawID/PlanningID + Courts-Block.
3. **`ScoreStatus = 3` (Disqualifikation)** — Tilo kennt 0/2/3, wir 0/1/2.
4. **Officials `Official1ID`/`Official2ID`** — bts-light hat kein
   Schiedsrichter-Konzept.
5. **`Shuttles`** (Federballverbrauch je Match).
6. **`MatchOrder`** (immer mitgespiegelt; geringer Wert).
7. **Eigenständige Spieler-Updates** (`update_players_request`): Check-in
   auch außerhalb des Spielendes (Pausenende → wieder einchecken,
   Tabletoperator-Verwaltung).
8. **Check-in-Bits im `Status`-Feld** — nur bei BTP-Einstellung
   `check_in_per_match`; 4 Bits (je Spieler) werden ins Status-Feld
   ge-OR-t.
9. **Robustheit: `btp_needsync` + `pushall()`** (btp_conn.js:232-252) —
   nicht zugestellte Ergebnisse werden nach BTP-Reconnect automatisch
   nachgeschoben. bts-light sendet one-shot.
10. **5-Minuten-Fenster** für den Players-Checkout (btp_proto.js:166) —
    verhindert, dass ein spätes Replay Spieler erneut auscheckt.

### Was nur bts-light schreibt

- **`ScoreStatus = 1` (Walkover)** inkl. Turnierleitungs-Walkover-Flow —
  das Original setzt nie einen Walkover-Code.
- Beide senden **kein** TimeStart/TimeEnd/DetailedResult (existieren im
  Protokoll, nutzt auch das Original nicht).

## Übernahme-Empfehlungen (Roadmap, priorisiert)

- **P1 — `Highlight` für Vorbereitungs-Aufrufe (S/M, hoher Nutzen):**
  Beim Aufruf `Match{ID, DrawID, PlanningID, Highlight:1}` schreiben, bei
  Rücknahme/Ruf aufs Feld `Highlight:0`. **Kein `Status` in diesem
  Request** (Check-in-Bits-Falle, vgl. Regression v0.9.103 in
  [btp_protocol.md](btp_protocol.md)). Vorher am echten BTP prüfen, wie
  das Highlight dort dargestellt wird. Betroffene Stellen:
  `btp/proto.rs` (neuer schlanker Request), `commands.rs`
  (`call_matches`/`remove_preparation_call`), Doku `preparation.md`.
- ~~**P2 — Retry-Queue für Ergebnis-Writes (M)**~~ → **umgesetzt
  (Cluster A5):** Nachschub-Queue im Sync-Loop (periodisch alle 30 s,
  robuster als Tilos nur-beim-Reconnect-`pushall`), 5-Minuten-Guard für
  den Players-Checkout, Nie-Überschreiben-Regel und bedingte
  Feld-Freigabe — Details in
  [btp_protocol.md](btp_protocol.md) → „Nachschub-Queue".
- **P3 — Disqualifikation als `ScoreStatus 3` (S):** dritte Option im
  Turnierleitungs-Dialog; kein Tablet-UI nötig.
- **Bewusst zurückgestellt:** Check-in-Bits (nur mit
  `check_in_per_match`-Turnieren relevant; riskant — aber offener
  Prüfpunkt: löscht unser hartes `Status=0` im Ergebnis dort Bits?),
  Officials/Shuttles/MatchOrder und eigenständige Spieler-Updates (setzen
  Schiedsrichter-/Shuttle-Verwaltung in bts-light voraus — eigenes
  Feature).

## Verifikation bei Umsetzung

Je Übernahme: Rust-Tests für den Request-Aufbau; am echten BTP: Aufruf →
Highlight sichtbar, Ruf aufs Feld → Highlight weg, Ergebnis bei
getrenntem BTP kommt nach Reconnect an; Check-in-Verhalten bei
Feldzuweisung unverändert (Regressionsschutz v0.9.103).
