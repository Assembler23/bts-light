# Seiten und Mannschaften am Zähl-Tablet korrigieren — Spezifikation

- **Status:** freigegeben (30.10.2026)
- **Herkunft:** /idee (Brief → Grill → How-To), Intake unter `docs/features/_intake/tablet-seiten-korrigieren/` (gitignoriert)
- **Entscheidung:** [ADR 0065](../adr/0065-mannschaften-berichtigt-kipp-ereignis.md), erweitert [ADR 0038](../adr/0038-ereignisse-append-only.md)

## Kontext / Problem

Am Zähl-Tablet (`src-tauri/assets/tablet.html`) wird zu Spielbeginn festgelegt, welche
Mannschaft links steht. Danach tauscht das Tablet die Seiten nur **automatisch**: nach
jedem Satz und bei 11 im Entscheidungssatz (`swapSides()`, `midGameSwitchDone`).

In der Halle gehen zwei Dinge schief, die der Zähler heute nicht beheben kann, sobald
der erste Punkt gefallen ist (vorher nur über „zurück" bei 0:0 → komplette Neuaufstellung):

1. **Tablet und Feld laufen auseinander.** Die Spieler haben die Hälften gewechselt, das
   Tablet nicht, oder das Tablet hat getauscht und die Spieler sind stehen geblieben.
   Der Zähler tippt dann für die falsche Mannschaft.
2. **Die Namen stehen falsch.** Bei der Aufstellung wurde die falsche Mannschaft als
   „links" gewählt. Der Zähler hat richtig auf die Seite getippt, aber die Punkte
   werden der falschen Mannschaft gutgeschrieben, an Turnierleitung, BTP und badhub.

## Zielbild & Erfolgskriterien

Der Zähler behebt beide Fehler selbst, mit wenigen Tipps, ohne PIN und ohne das Spiel
neu aufzusetzen. Nach einer Korrektur stimmen Anzeige, gemeldeter Stand, Monitore,
Zähltafel, Punktverlauf, Schiedsrichterzettel und badhub-Verlauf. Jede Korrektur ist
mit „zurück" rücknehmbar.

## Begriffe

| Begriff | Bedeutung |
|---|---|
| **Seiten tauschen** (Eingriff 1) | Mannschaften, Punkte, Satzstände und Aufschlag wandern **gemeinsam** auf die andere Seite. Der gemeldete Stand A:B bleibt gleich. |
| **Mannschaften tauschen** (Eingriff 2, intern „A/B richtigstellen") | Nur die Namen wechseln die Seite, die Punkte bleiben auf ihrer Seite. Der gemeldete Stand kippt von A:B auf B:A, auch in allen fertigen Sätzen. |

Die endgültigen Beschriftungen legt die Umsetzung fest; sie werden im Handbuch mit
`pruef`-Anker verankert.

## Nicht-Ziele

- Keine Korrektur aus TL-Web oder Desktop-App.
- Keine Änderung an der Seitenwahl vor Spielbeginn.
- Kein Tausch der Aufschlagfelder innerhalb einer Doppel-Mannschaft als eigener Eingriff
  (bei Eingriff 2 im Doppel wird die Aufstellung aber neu abgefragt, siehe unten).
- Keine Korrektur nach dem Abschicken des Ergebnisses (dann Turnierleitung).
- Keine Änderung im badhub-Repo (geprüft: der Liveticker übernimmt A:B → B:A ohne
  Monotonie-Prüfung, `tverlauf` ersetzt den Verlauf).
- Keine Rekonstruktion der Doppel-Spielerposition in **früheren** Sätzen.

## Betroffene Komponenten / Architekturregeln / Daten

- **Tablet** `assets/tablet.html`: Kopfzeile, Ergebnis-Overlay, neues Modal, Zustand,
  Undo/Reopen, Setup-Abfrage im Doppel, Senden.
- **Reine Logik** neu `src/io/seitenKorrektur.mjs` + `scripts/test-seiten-korrektur.mjs`
  + Inline-Kopie in `tablet.html` (Muster `matchEvents.mjs`); `swapSides()` nutzt sie.
- **Wire** `relay-proto`: `EventKind::TeamsCorrected` (`teams_corrected`),
  `MatchBrief.teamsCorrection: bool` (`#[serde(default)]`).
- **Relay** `relay/`: nur neu bauen (parst `EventKind` typisiert).
- **Host**: Deutungsfunktion `in_endkoordinaten` (neues `tablet/berichtigung.rs`),
  angewandt in `tablet/scoresheet.rs` und `badhub/verlauf.rs`; MatchBrief setzt
  `teams_correction: true`.
- **R2** bleibt: Die Korrektur ändert nur, wie das Tablet Seiten den Mannschaften
  zuordnet. **R5** bleibt: `process_result` unverändert, das abgeschickte Ergebnis wird
  wie bisher geprüft.
- **Datenschutz:** keine neuen personenbezogenen Daten.

## Lösung

### Bedienung

- Neuer Knopf **⇄** in der Kopfzeile der Zählansicht, sichtbar wenn die Aufstellung steht.
- In der **Ergebnis-Ansicht** (sie verdeckt die Kopfzeile) ein eigener Knopf mit derselben
  Funktion.
- Der Knopf öffnet einen Dialog mit beiden Eingriffen. Nach der Wahl folgt eine
  **Rückfrage mit Vorschau**: wer danach links/rechts (bzw. vorne/hinten, `seiteWort()`)
  steht, welcher Stand gemeldet wird und, bei Eingriff 2, wer danach vorn liegt bzw.
  gewonnen hat.
- Keine PIN.

### Wann erlaubt

| Lage | Eingriff 1 | Eingriff 2 |
|---|---|---|
| Spiel läuft (`setup === 'playing'`), auch in Pause/Satzpause | ja | ja |
| Ergebnis-Ansicht, Ergebnis noch nicht abgeschickt | ja | ja |
| `pendingResult`, `submittedOk`, `finalized` | nein | nein |
| Tablet gesperrt/passiv (`locked`, `occupied`) | nein | nein |
| Aufstellung läuft (choose-side/-server/-receiver) | nein | nein |
| Turnier-PC kann Berichtigung nicht (`match.teamsCorrection !== true`) | ja | **nicht angeboten** |
| Ereignis-Deckel reicht nicht (siehe unten) | ja | nein, Hinweis |

### Eingriff 1 — Seiten tauschen

- Zustand wie `swapSides()`: `teamOnSide`, `positions`, `currentSet`, `setsCompleted`,
  `servingSide` gespiegelt.
- `midGameSwitchDone` bleibt unberührt: Wer im Entscheidungssatz vor 11 korrigiert,
  bekommt bei 11 trotzdem den regulären Seitenwechsel. Die Rückfrage weist darauf hin.
- Punktverlauf, Ereignisse und gemeldeter Stand ändern sich nicht (sie stehen in A/B).
  Gesendet wird nur der Stand (`persistState`); Monitore und Zähltafel folgen `teamOnSide`.

### Eingriff 2 — Mannschaften tauschen

- `teamOnSide` a↔b. `rallyLog`: `startA`↔`startB`, `A`↔`B` in `points`.
  `serveSetupTeam` und lokale `cards.team` kippen mit. Karten und Strafpunkte bleiben
  auf ihrer Seite (Annahme: der Schiedsrichter hat auf die Person auf der Seite gezeigt).
- `retiredWinner` (Aufgabe/kampflos) wird zurückgesetzt, der Sieger wird neu gewählt.
- **Einzel:** `positions` erhält die Spieler-ID der jetzt dort stehenden Mannschaft.
- **Doppel:** Die Aufschlagfelder sind nach dem Tausch unbekannt. Das Tablet fragt
  sofort ab, wer gerade aufschlägt und wer annimmt (wie beim Satzbeginn, eigener Text
  „Mannschaften getauscht — wer schlägt gerade auf?"); der Stand bleibt. Läuft gerade
  eine Pause, kommt die Abfrage beim Weiterspielen (`endBreak`). Steht in der Satzpause
  die Satzbeginn-Abfrage schon an, gibt es keine zusätzliche Abfrage. Abbrechen der
  Abfrage = Korrektur zurücknehmen. In der Ergebnis-Ansicht keine Abfrage.
- Ereignis `teams_corrected` am Anker `korrekturAnker()`: im laufenden Spiel wie
  `eventAnker()`, im beendeten Spiel `{set: Zahl der Sätze, afterN: Ballwechsel des
  letzten Satzes}` (nie Satz n+1, sonst verwirft `is_valid` bei best of 5).
- Senden wie `undo()`: `commitScoreView` + `sendScoreUpdate` (kippt aus `teamOnSide`),
  `sendRallySync` (der Host prüft rally-Frames lückenlos und würde sonst verwerfen),
  Ereignis + `sendMatchEventSync`. Schiri-Ansage neutral (`lastEvent = null`).
- **Deckel:** Vor dem Eingriff prüfen, ob `ME_MAX_EVENTS` (64) noch Platz für Korrektur,
  ggf. neue Aufschlagfolge und deren Rücknahmen hat (Einzel ≥ 2, Doppel ≥ 4). Sonst wird
  der Eingriff verweigert mit „Korrektur nicht mehr möglich — bitte der Turnierleitung
  melden." Nie ohne Ereignis ausführen.

### Undo

- Jeder Eingriff ist ein **eigener `history`-Schritt** mit Marker
  `korrektur: {art, eventId, folgeIds, serveSetupTeamVorher}` am Snapshot.
- „zurück" nimmt zuerst den Eingriff zurück (Knopftext „Korrektur zurücknehmen"), danach
  wieder Punkte. Das Zurücknehmen eines älteren Punkts kann eine Korrektur nicht still
  aufheben, weil spätere Snapshots den korrigierten Stand tragen.
- Rücknahme: Snapshot wiederherstellen, `setup = 'playing'`, `serveSetupTeam` zurück,
  `teams_corrected` und die `serve_start` der Doppel-Abfrage (`folgeIds`) per Rücknahme-
  Ereignis (ADR 0038) zurücknehmen, senden wie beim Eingriff.
- In der Ergebnis-Ansicht nimmt „Match wieder öffnen" zuerst die Korrektur zurück (Knopf
  „Korrektur zurücknehmen").
- Undo ist auch während der Doppel-Abfrage nach Eingriff 2 möglich.

### Deutung „Mannschaften berichtigt" (Host)

Eine Funktion `in_endkoordinaten(&[MatchEvent]) -> Vec<MatchEvent>`, genutzt von Zettel
und badhub-Verlauf. Das Tablet deutet nicht; den Punktverlauf kippt es selbst.

- Ordnung `(set, afterN, seq, id)`.
- **Kipp-Punkte:** jedes `teams_corrected` und jede **erste** Rücknahme eines
  `teams_corrected`, jeweils an ihrer eigenen Position.
- Ein Ereignis wird gekippt (`team`, `receiver_team` 0↔1, `score_a`↔`score_b`), wenn
  nach ihm eine **ungerade** Zahl von Kipp-Punkten liegt. Spielerindizes bleiben.
- Folgen: wirkt über alle Sätze; zwei Korrekturen heben sich auf; eine Karte zwischen
  Korrektur und deren Rücknahme landet richtig.
- Zettel: setzt die Aufschlagrechnung an jedem `serve_start` mit `after_n > 0` neu auf
  (der letzte an derselben Stelle gilt, wie badhub `Verlauf.php`). Protokollzeile
  „Mannschaften berichtigt" (zurückgenommen: durchgestrichen).
- badhub-Verlauf: `aufschlaege()` sendet vorgedeutete `serve_start`; badhub nimmt nur
  `serve_start` an.

### Kompatibilität und Deploy

- `EventKind` hat bewusst keinen Catch-All: Eine **alte App** verwirft jeden Sync mit
  `teams_corrected` bis Spielende. Deshalb meldet der Turnier-PC über
  `MatchBrief.teamsCorrection`, ob er die Berichtigung kennt; nur dann bietet das Tablet
  Eingriff 2 an. Eingriff 1 braucht keinen Schalter.
- Reihenfolge ergibt sich von selbst: main-Merge deployt Relay + Cloud-Tablet-Seite, der
  App-Tag folgt. LAN: Seite und Host aus derselben Binärdatei.

## Akzeptanzkriterien

1. Im laufenden Einzel bei 5:3 (A links) → „Seiten tauschen" → A steht rechts mit 5, B links
   mit 3; gemeldet A 5 : B 3; Aufschlag bleibt beim selben Spieler; Monitor/Zähltafel zeigen A rechts.
2. Dasselbe nach Satz 1 (21:15 für A): die Satzhistorie wandert mit, gemeldet weiter 21:15 für A.
3. Entscheidungssatz, Eingriff 1 bei 7:4 → bei 11 tauscht das Tablet trotzdem automatisch.
4. Einzel bei 5:3 (A links) → „Mannschaften tauschen" → B steht links mit 5; gemeldet A 3 : B 5;
   Punktverlauf beim Host und auf badhub in B-Koordinaten; Aufschläger-Anzeige nennt den Spieler aus B.
5. Wie 4, aber nach Satz 1 21:15 → gemeldeter Satz 15:21; der Matchsieger in der Rückfrage stimmt mit dem danach gemeldeten überein.
6. Doppel bei 5:3 → „Mannschaften tauschen" → Abfrage Aufschläger/Annehmer erscheint, Stand bleibt 5:3;
   nach Auswahl stimmen die Aufschlagfelder bei ungeradem Stand; Zettel ab Ballwechsel 9 nach neuer Aufstellung.
7. „zurück" direkt nach einem Eingriff stellt den Zustand vor dem Eingriff her (Stand, Namen, gemeldeter
   Stand, Verlauf); ein zweites „zurück" nimmt den letzten Punkt zurück.
8. Eingriff 2, dann zwei Punkte, dann zweimal „zurück" → Korrektur bleibt bestehen.
9. Eingriff 2 zweimal hintereinander → Zustand und Zettel wie ohne Eingriff (Protokoll zeigt beide Zeilen).
10. Gelbe Karte nach Eingriff 2, dann Eingriff 2 zurückgenommen → Zettel zeigt die Karte bei der Mannschaft auf ihrer Seite.
11. Ergebnis-Ansicht (best of 3, 2:0 für A), nicht abgeschickt → Knopf sichtbar; Eingriff 2 → Sieger B, Ereignis mit Satz 2 (nicht 3) vom Host angenommen.
12. Nach dem Abschicken (`pendingResult`/`submittedOk`), bei `finalized`, am gesperrten/passiven Tablet und während der Aufstellung ist der Knopf nicht verfügbar.
13. Aufgabe gewählt, dann Eingriff 2 → Siegerauswahl ist zurückgesetzt.
14. Turnier-PC ohne `teamsCorrection` → Dialog bietet nur „Seiten tauschen".
15. Deckel: Bei 63 Ereignissen wird Eingriff 2 mit Hinweis verweigert, Zustand unverändert.
16. Anordnung übereinander: Dialog und Vorschau sagen vorne/hinten.
17. Reload/Tablet-Übernahme nach Eingriff: Zustand und Undo-Marker bleiben erhalten.
18. Pause läuft, Doppel, Eingriff 2 → Abfrage erscheint beim Weiterspielen.

## Grenzen

- Doppel: Welcher Spieler in **früheren** Sätzen bzw. vor der Abfrage im gekippten Team
  aufschlug, ist unbekannt; der Zettel kann dort innerhalb einer Mannschaft vertauschen.
- Nach einer Tablet-Übernahme gehen beim Host durch Eingriff 2 die Ballwechsel-Details
  früherer Sätze verloren (AK-12-Verschmelzung passt nicht mehr); Endstände bleiben.
- Eine ältere Tablet-Seite liest einen Korrektur-Snapshot als normalen Punkt-Schritt.
- In einer Pause zeigt die Feldgrafik im Doppel bis zur Abfrage ggf. falsche Positionen.

## Tests

- **Rust:** relay-proto Serde-Roundtrip `teams_corrected`, `is_valid`, MatchBrief ohne
  Feld → false; Relay leitet Sync mit neuer Art weiter; `in_endkoordinaten` (über Sätze,
  Parität, Karte zwischen K und Rücknahme, doppelte Rücknahme, Rücknahme eines Nicht-K,
  seq-Ordnung, score-Tausch); `scoresheet` (Einzel, Doppel mit neuem `serve_start`
  bei afterN 7, Protokollzeile, best of 5 beendet); `verlauf` (Deutung, Hash ändert sich).
- **JS:** `scripts/test-seiten-korrektur.mjs` (beide Eingriffe Involution, Stand-Kippen,
  rallyLog, Einzel-ID-Mapping, Doppel → Abfrage, Freigabe-Matrix, `korrekturAnker`,
  Deckel-Prüfung, Vorschau-Texte); `scripts/test-serving.mjs` erweitern; CI-Schritt.
- **Am Gerät:** beide Anordnungen, Einzel/Doppel, Pause, Satzpause, Ergebnis-Ansicht,
  Reload/Übernahme, LAN und Cloud.

## Doku

`docs/tablet.md` (Abschnitt „Seiten und Mannschaften korrigieren"), `docs/tablet-bedienen.md`
(mit `pruef`-Ankern für die Beschriftungen), `docs/umpire-mode.md`,
`docs/schiedsrichterzettel.md`, `docs/punktverlauf.md`, `docs/cloud-relay.md`, Specs
`schiedsrichterzettel-druck.md` + `punktverlauf-graph.md`, ADR 0038 Vermerk, CLAUDE.md-
Doku-Tabelle, `docs/roadmap.md`, `docs/changelog.md`. Version gemeinsam anheben, Nummer
beim Merge festlegen.
