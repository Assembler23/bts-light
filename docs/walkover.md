# Kampflose Wertung nach Aufgabe (Walkover)

Gibt eine Mannschaft mitten im Spiel auf, betrifft das oft mehr als die
eine Begegnung: In einer Gruppe (Round Robin) hat die aufgebende
Mannschaft meist noch weitere Spiele, die sie nicht mehr antreten kann.
bts-light schlägt der Turnierleitung vor, diese Spiele kampflos
(Walkover) für den jeweiligen Gegner zu werten.

Eingeführt in v0.5.0.

## Ablauf

1. **Aufgabe** — am Tablet beendet „Spiel abbrechen" das Match per
   Aufgabe (`ScoreStatus = 2`, siehe [tablet.md](tablet.md)). Das Ergebnis
   geht über `process_result` nach BTP.
2. **Vorschlag** — nach dem erfolgreichen BTP-Schreiben prüft
   `register_walkover_proposal` (`tablet/server.rs`), ob die aufgebende
   Mannschaft in derselben Disziplin noch ungespielte Spiele hat. Wenn ja,
   wird ein `WalkoverProposal` im geteilten `TabletState` hinterlegt.
3. **Anzeige** — das Frontend (`WalkoverPanel.tsx`) pollt alle 4 s die
   offenen Vorschläge und blendet ein Modal „Aufgabe – kampflose Wertung"
   ein: die Liste der betroffenen Spiele, standardmäßig alle ausgewählt.
4. **Bestätigung** — die Turnierleitung wählt die Spiele und bestätigt.
   Erst dann schreibt `confirm_walkover` für jedes Spiel einen kampflosen
   Sieg nach BTP.

## Scope: nur die Disziplin der Aufgabe

Maßgeblich ist die BTP-**`EntryID`**. Ein Entry ist eine Mannschaft
*innerhalb einer Auslosung* (Einzel-Spieler:in oder Doppelpaarung). Damit
ist der Scope automatisch korrekt:

- Gibt im **Doppel** ein:e Spieler:in auf, kann die Doppelpaarung nicht
  weiter antreten — alle restlichen Spiele *dieses* Entrys werden
  vorgeschlagen.
- Spielt der gesunde Partner zusätzlich Einzel oder Mixed mit einem
  **anderen** Partner, ist das ein **anderer Entry** mit eigener `EntryID`
  und bleibt vollständig unberührt.

## Datenmodell

`BtpMatch` (`btp/model.rs`) trägt seit v0.5.0 `entry1_id` / `entry2_id`
(0 = Platz noch offen). Der Parser löst sie über Slot → Entry auf.

`tablet/state.rs`:

- `WalkoverCandidate` — ein kampflos wertbares Spiel: `match_id`,
  `draw_id`, `planning_id`, `round_name`, `opponent`, `retired_is_team1`.
- `WalkoverProposal` — `id` (= `EntryID` als String), `entry_id`,
  `retired_team`, `draw_name`, `created_at_ms`.

`TabletState` hält die offenen Vorschläge in `walkovers`. Die
Kandidaten-Spiele werden **nicht** gespeichert, sondern bei jeder Abfrage
frisch aus dem Snapshot ermittelt (`walkover_candidates`) — so fallen
bereits gewertete Spiele von selbst heraus.

`walkover_candidates(entry_id)` liefert alle Matches mit Status
`Scheduled`, an denen der Entry beteiligt ist **und** der Gegner schon
feststeht (offene KO-Plätze werden übersprungen).

## Tauri-Commands (`commands.rs`)

| Command | Zweck |
|---|---|
| `walkover_proposals` | Offene Vorschläge + live aufgelöste Kandidaten. Vorschläge ohne verbleibende Kandidaten werden aufgeräumt. |
| `confirm_walkover(proposal_id, match_ids)` | Schreibt für die ausgewählten Spiele einen Walkover nach BTP. |
| `dismiss_walkover(proposal_id)` | Verwirft einen Vorschlag ohne Wertung. |

`confirm_walkover` baut je Spiel ein `proto::MatchUpdate` mit
`score_status = 1` (Walkover), leerer Satzliste und `team1_won` = die
jeweils **nicht** aufgebende Seite, und schreibt es per `SENDUPDATE`
(`write_result_to_btp`).

### Ergebnis aus der Turnierleitung eintragen (`enter_result`)

Verwandte TL-Wertung (Plan 12, Backend-Finalisierung — Tilo 20.07.:
„ein Spiel aus dem Backend beenden, wenn das Finalisieren vergessen wurde
oder durch einen Verbindungsabbruch nicht klappte"):

| Command | Zweck |
|---|---|
| `enter_result(match_id, sets)` | Trägt für ein Spiel ein **reguläres** Satz-Ergebnis nach BTP ein (Sieger aus der Satzmehrheit). |
| `disqualify_match(match_id, loser_team, sets)` | **Disqualifikation** (P3, `ScoreStatus = 3`): `loser_team` (1/2) wird disqualifiziert, der Gegner gewinnt; ein Zwischenstand bleibt erhalten (keine Vollständigkeitsprüfung). |

Erreichbar in der **Spielübersicht** über den Knopf „Ergebnis" auf einem
belegten Feld; der Dialog ist mit dem aktuellen Live-Satzstand vorbelegt
(häufiger Fall: nur bestätigen). Die **Satz-/Sieger-Validierung teilt
sich `enter_result` mit dem Tablet-Weg** über `server::derive_result`
(eine Quelle der Wahrheit, R5). Weil die manuelle Eingabe — anders als
das Tablet — die Satzregeln nicht clientseitig erzwingt, prüft
`server::set_is_complete` zusätzlich, dass **jeder Satz regulär zu Ende
gespielt** ist (gegen das Zählformat des Matches: Ziel + 2 Punkte bzw.
Deckel) — so wird ein noch laufender Satz aus der Vorbelegung nicht als
gewonnener gewertet. Die gesamte Kernlogik (Guards, Validierung,
`MatchUpdate`-Bau) liegt rein & getestet in
`server::build_manual_result_update`. Steht das Spiel noch auf einem Feld, wird
es im selben `SENDUPDATE` freigegeben und die Spieler ausgecheckt; sonst
geht nur das Ergebnis raus. Schutz: ein in BTP bereits gewertetes Spiel
wird nie überschrieben, Kampflos/Aufgabe laufen weiter über den
Walkover-/Aufgabe-Flow. Fehlgeschlagene Writes landen in der
Nachschub-Queue (siehe [btp_protocol.md](btp_protocol.md)).

**Disqualifikation (P3, v0.9.159):** Derselbe Ergebnis-Dialog hat einen
Abschnitt „Disqualifikation" mit je einem Knopf pro Team. `disqualify_match`
baut den `MatchUpdate` über `server::build_manual_dq_update`: der Gegner des
disqualifizierten Teams gewinnt, `ScoreStatus = 3`, und ein bereits
eingetippter Zwischenstand bleibt erhalten — eine Disqualifikation kann
mitten im Spiel fallen, daher **keine** Satz-Vollständigkeitsprüfung — der
eingetippte Zwischenstand wird (außer dem 0..=99-Bereich + Satzanzahl) **nicht
auf Scoring-Plausibilität geprüft**; die Verantwortung dafür liegt bewusst bei
der Turnierleitung (jede Regel-Prüfung würde den „mitten im Spiel"-Zweck
verhindern). In der UI ist die DQ zweistufig bestätigt. Sieger-/
Status-Ableitung teilt sich `disqualify_match` über den erweiterten
`server::derive_result` (`disqualified`-Zweig) mit den anderen Wegen; Feld-
Freigabe, Auscheck-Block und Nachschub-Queue sind identisch zu `enter_result`.
`ScoreStatus = 3` sollte einmalig am echten BTP gegengeprüft werden (BTP-
Anzeige des DQ-Status).

## Sicherheit & Robustheit

- **Schreib-Grenze:** `confirm_walkover` löst die Kandidaten erneut live
  aus dem Snapshot auf und schreibt nur Spiele, die **sowohl** in der
  Anfrage **als auch** in der aktuellen Kandidatenliste stehen. Eine
  beliebige Match-ID lässt sich darüber nicht werten.
- **Leere Auswahl** entfernt den Vorschlag nicht (Schutz gegen
  versehentliches Verwerfen).
- **Teilfehler:** Schlägt das Schreiben einzelner Spiele fehl (z. B. BTP
  kurz nicht erreichbar), bleibt der Vorschlag stehen; beim nächsten
  Versuch fallen die bereits gewerteten Spiele automatisch heraus.

## Beteiligte Dateien

- `src-tauri/src/btp/model.rs` — `entry1_id`/`entry2_id` auf `BtpMatch`.
- `src-tauri/src/tablet/state.rs` — `WalkoverProposal`/`WalkoverCandidate`,
  Speicherung, `walkover_candidates`.
- `src-tauri/src/tablet/server.rs` — `register_walkover_proposal` in
  `process_result`.
- `src-tauri/src/commands.rs` — die drei Walkover-Commands.
- `src/components/WalkoverPanel.tsx` — das Bestätigungs-Modal.
