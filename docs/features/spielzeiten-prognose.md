# Spielzeiten-Protokollierung & Startzeit-Prognose — Spezifikation

> Status: **abgestimmt 2026-08-16** (via /idee: Brief → Grill → How-To → Review;
> Umsetzung vom Nutzer am 2026-08-16 pauschal freigegeben).
> Quelle: Idee (Chat vom 2026-08-16). Betroffene Crates: src-tauri, src (SetupWizard),
> Assets `tl.html`/`tablet.html`; relay-proto/relay unverändert (TlState reist als opakes JSON).
> ADR: [0027](../adr/0027-spielzeit-stempel-hostseitig.md) (Stempel-Quelle),
> [0028](../adr/0028-pause-haelt-bis-weiterspielen.md) (Pausen-Semantik).

## Kontext / Problem

Turnierleitungen wissen heute weder, wie lange Spiele wirklich dauern, noch
wann ein wartendes Spiel voraussichtlich dran ist. BTP bekommt zwar eine
`Duration` zurückgemeldet, aber mehrere Pfade senden 0 (manuelles Ergebnis,
TL-Web-Ergebnis, App-Neustart mitten im Spiel), und es gibt keinerlei
Unterscheidung zwischen Anlaufzeit (Feldzuweisung → erster Punkt) und
tatsächlicher Spielzeit. Die geplanten BTP-Zeiten taugen nicht als Prognose
(alle Spiele eines Zeitfensters tragen dieselbe Uhrzeit). Zusätzlich sind
laufende Satzpausen für die Turnierleitung unsichtbar — insbesondere, ob
eine Pause überzogen wird.

## Begriffe

- **Bruttozeit** = erste Feldzuweisung durch BTP → Beendigung des Spiels.
- **Nettozeit** = erster gespielter Punkt → Beendigung; durchgehende Uhr,
  Satz-/Zwischenpausen zählen mit (nicht pausenbereinigt).
- **Differenz** = Brutto − Netto = Anlaufzeit (Weg zum Feld, Einspielen,
  Anschreiben) — eine Feld-Logistik-Metrik, keine Spieler-Bewertung.
- **Gruppe** = Klasse (`class_label`) × Disziplin; Statistik-Fallback-Kette
  Gruppe → Klasse → Turnier → konfigurierter Default.

## Zielbild & Erfolgskriterien

Die Turnierleitung sieht in TL-Web: wie lange Spiele je Gruppe wirklich
dauern (Median Brutto/Netto/Differenz), wann jedes wartende Spiel
voraussichtlich aufgerufen wird, und welche Felder gerade in Pause sind —
inklusive Überziehung. BTP bekommt die Bruttozeit auch auf den heutigen
0-Pfaden zurückgemeldet.

**Erfolgskriterien (E12):** Beim nächsten Testturnier liegt die Prognose für
≥ 70 % der Spiele innerhalb ± 10 Minuten der echten Startzeit (auswertbar
über das Diagnose-Log: beim Bruttostart-Stempel wird die zuletzt publizierte
Prognose des Matches geloggt), und jede Satzpause samt Überziehung ist in
der TL-Sicht ohne Nachfrage am Feld erkennbar.

## Nicht-Ziele

- Keine Anzeige der Prognose auf Hallen-Monitoren oder im badhub-Ticker.
- Kein turnierübergreifendes Lernen der Spieldauern.
- Keine Spieler-Ruhezeiten-Anzeige (nur Satz-/Behandlungspausen am Feld).
- Keine Pausen-Bereinigung der Nettozeit.
- Kein neues BTP-Wire-Feld (nur die bestehende `Duration` wird befüllt);
  Walkover sendet weiterhin `Duration: 0`.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:** `src-tauri/src/tablet/match_times.rs` (neu, Store),
  `tablet/predict.rs` (neu, Statistik+Simulation), Hooks in `sync.rs`,
  `tablet/server.rs`, `tablet/state.rs`, `tablet/tl.rs`, `commands.rs`;
  `config.rs` (`PredictionConfig`); `assets/tl.html`, `assets/tablet.html`
  (Etappe C); `src/` SetupWizard-Abschnitt. `relay/`/`relay-proto/`
  unverändert (TlState = opakes JSON in `HostFrame::TlState`); Cloud braucht
  trotzdem einen **Relay-Deploy vor dem Client-Release**, weil `tl.html`/
  `tablet.html` im Relay einkompiliert sind.
- **Architekturregeln:** R1 gewahrt (UI nur über Tauri-Commands bzw.
  TL-Web-Frames). R2: die Prognose erfindet keine Court→Match-Zuordnung —
  sie simuliert die vorhandene Vergabelogik (gleiche Sortierung wie
  `ready_queue`) rein zur Anzeige. R3: Berechnung host-seitig in
  `build_state_limited`, damit LAN und Cloud identisch anzeigen. R5: alle
  Stempel liegen hinter den bestehenden Gates (`handle_score`-Filter,
  `process_result`-Validierung); es gibt keine neuen eingehenden Pflichtdaten
  (nur das opake `startedAt` der Pause, typisiert geparst).
- **Konfiguration & Abwärtskompatibilität:** neu `AppConfig.prediction:
  PredictionConfig { enabled: bool = true, default_duration_mins: f64 = 25.0 }`
  mit `#[serde(default)]` — alte `config.json` lädt unverändert.
  Übergangspuffer fest im Code (2 min). Identifier/Updater-Pfad unangetastet.
- **Datenschutz:** `match-times.json` enthält nur Match-IDs, Zeitstempel,
  Klassen-Label, Disziplin — keine Personendaten. Der Wächter-Test
  `the_state_never_carries_personal_data_beyond_its_purpose` wird auf die
  neuen TlState-Felder ausgedehnt.
- **Abhängigkeiten:** keine neuen Cargo-/npm-Dependencies; kein
  badhub-Endpunkt; BTP-Protokoll unverändert (nur Quellwert der `Duration`).

## Fachliche Entscheidungen (aus dem Grill, E1–E12)

1. **E1** `Duration`-Semantik bleibt; die 0-Pfade (manuelles Ergebnis,
   TL-Web-Ergebnis, App-Neustart) werden aus dem Store gefüllt; Walkover
   bleibt explizit 0.
2. **E2** Der Host stempelt den ersten Punkt beim ersten eingehenden
   Punktestand > 0 (eine Uhr, gilt für Tablet und Zähltafel). Undo auf 0:0
   löscht den Stempel nicht.
3. **E3** Beendigung = Host-Eingang des Ergebnisses; eine Ergebniskorrektur
   überschreibt den Ende-Stempel nicht (idempotent, „nur wenn leer").
4. **E4** Bruttostart = **erste** Feldzuweisung; immun gegen Feldwechsel und
   App-Neustart. Reset nur, wenn das Match in **3 aufeinanderfolgenden
   Snapshots** `Scheduled` ohne `court_id` und nicht `Finished` ist
   (`DEASSIGN_CONFIRM_POLLS = 3`; filtert Sync-Flackern).
5. **E5/E6** Median je Gruppe (Klasse×Disziplin); eigene Werte ab 3
   Messungen; Fallback Klasse → Turnier → Default. Leeres `class_label`
   springt direkt auf die Turnierstufe.
6. **E7** Default-Bruttodauer konfigurierbar im Setup (Default 25 min).
7. **E8** Vollmodell-Simulation: Restzeit laufender Spiele =
   max(0, Gruppenwert − verstrichene Bruttozeit); Spieler-Mindestpause
   (`rest_minutes`/`blocked.until_ms`) und Feldvergabe-Ausnahmen fließen ein
   (ausgenommene Spiele belegen kein Feld und erhalten keine Prognose);
   Übergangspuffer = max(2 min, `auto_assign.wait_minutes` falls Automatik
   aktiv). Prognosen werden **minutengerundet** (Rev-Churn-Wächter).
8. **E9** Das Tablet hält die Pause, bis aktiv weitergespielt wird (kein
   Auto-Ende bei Countdown 0); das Overlay wechselt auf „überzogen"
   (rot, hochzählend). Bewusste Verhaltensänderung am SR-Gerät.
9. **E10** Behandlungspausen (injury) erscheinen in der TL-Sicht als
   „Behandlung seit …" (Dauer, ohne Countdown/Überziehung).
10. **E11** Nur regulär beendete Spiele (Tablet-Pfad, `score_status == 0`,
    kein WO/Aufgabe/DQ) mit vorhandenem Erster-Punkt-Stempel liefern
    Messwerte; manuelle/tablet-lose Ergebnisse setzen zwar `finished_ms`
    (für die BTP-Duration), zählen aber nicht in die Statistik.

## Akzeptanzkriterien

**Messung/Store**
- [ ] Ein Match, das per BTP einem Feld zugewiesen wird, erhält genau einmal
  `first_assigned_ms`; Feldwechsel und App-Neustart ändern den Wert nicht
  (persistiert in `match-times.json`, ADR-0022-Muster, Turnier-gebunden).
- [ ] Erscheint ein zugewiesenes, nicht beendetes Match in 3
  aufeinanderfolgenden Snapshots ohne Feld, werden `first_assigned_ms` und
  `first_point_ms` verworfen; 1–2 Snapshots (Flackern) ändern nichts.
- [ ] Der erste beim Host eingehende Punktestand > 0 setzt `first_point_ms`;
  spätere Stände, 0:0-Stände sowie verworfene Scores (Stale-Filter,
  Finalisiert-Gate) verändern ihn nicht.
- [ ] Das erste beim Host eingehende Ergebnis setzt `finished_ms`; eine
  Ergebniskorrektur oder ein wiederholter POST verändert ihn nicht.
- [ ] Turnierwechsel verwirft den Store-Inhalt; eine unlesbare Datei lässt
  den Bestand unangetastet (Ladung-Muster).

**BTP**
- [ ] Tablet-Ergebnis nach App-Neustart mitten im Spiel sendet
  `Duration > 0` (aus dem Store) statt 0.
- [ ] Manuelles TL-Ergebnis und TL-Web-Ergebnis senden `Duration` aus
  Store bzw. `on_court_since`-Fallback.
- [ ] Walkover sendet weiterhin `Duration: 0`.

**Prognose/Anzeige (nur TL-Web)**
- [ ] Wartende Spiele zeigen „dran ca. hh:mm"; beruht der Wert nur auf dem
  konfigurierten Default, wird er als unsicher gekennzeichnet („~hh:mm").
- [ ] Ein durch Spieler-Mindestpause blockiertes Spiel bekommt als Prognose
  max(simuliertes Feld frei + Puffer, Pause-Ende).
- [ ] Das Spielzeiten-Panel zeigt je Gruppe Median Brutto/Netto/Differenz
  und Anzahl der Messungen; beendete Spiele zeigen ihre Ist-Zeiten.
- [ ] Zwei TL-State-Bauten im Abstand < 60 s ergeben denselben Fingerprint
  (keine Revision-Inflation durch die Prognose).
- [ ] `prediction.enabled = false` blendet Prognose und Panel aus; eine
  alte `config.json` ohne `prediction`-Block lädt fehlerfrei mit Defaults.

**Pausen (Etappe C)**
- [ ] Läuft eine 60-s-/120-s-Pause, zeigt TL-Web je Feld einen Countdown;
  nach Ablauf wechselt die Anzeige auf rotes „überzogen +m:ss", bis am
  Tablet weitergespielt wird.
- [ ] Das Tablet beendet die Pause nicht mehr automatisch; ein Reload des
  Tablets während einer (auch überzogener) Pause behält die Pause bei.
- [ ] Eine Behandlungspause erscheint in TL-Web als „Behandlung seit …";
  ein altes Tablet (ohne `startedAt`) zeigt „Behandlung" ohne Dauer.
- [ ] Altes Tablet + neuer Host: Pause endet wie bisher automatisch bei 0 —
  die TL-Anzeige zeigt nie fälschlich „überzogen".

## Tests

Rust-Unit-Tests (TDD, `cargo test` grün, `npm run build` fehlerfrei):
Store-Roundtrip/Turnierwechsel/Unlesbar; E4-Stempel-Matrix (Erststempel,
Feldwechsel-, Neustart-Immunität, Flacker, Reset nach 3 Polls, Finished nie);
E2-Matrix (erster Stand > 0, Folgestände, 0:0, verworfene Scores);
E3-Idempotenz + `regular`-Matrix; Duration-Regression der drei Pfade +
Walkover-0; Median/Fallback-Kette; `predict_starts`-Szenarien (Grundfall,
Restzeit, Pause-Blocker, Ausnahme, Hallenregel, Puffer, gesperrte Felder);
Serde-Default-Roundtrip `PredictionConfig`; Fingerprint-Stabilität;
Datenschutz-Wächter; `TlPause`-Parse (injury ohne endsAt, optionales
`started_at_ms`, Fremdfeld-Filter). Manuelle Prüfliste: LAN + Cloud
(Prognose-Anzeige, Pausen-Countdown, Tablet-Reload in überzogener Pause).

## Risiken & Rollback

- Rollback trivial: alte Versionen ignorieren `match-times.json`; die
  Duration-Pfade fallen aufs heutige Verhalten zurück; `config.json` bleibt
  ohne Migration lesbar.
- Cloud-Rollout: Relay-Deploy (einkompilierte Seiten) **vor** dem
  Client-Release; alte TL-Seiten ignorieren die neuen JSON-Felder.
- E9-Risiko „SR vergisst Weiterspielen": Pause klemmt sichtbar (Overlay rot,
  TL sieht es feldgenau) — gewollter Effekt; Nettozeit unberührt.
- Verspätet zugestellte Ergebnisse (ADR 0018) machen die Messung minimal zu
  lang — per E3 bewusst toleriert.

## Offene Fragen / Annahmen

- A1–A4 aus dem Brief bestätigt (erster Punkt = erster Host-Eingang > 0;
  Ende = Ergebnis-Eingang; Prognose als Uhrzeit in der TL-Spielliste;
  Walkover/Aufgabe/DQ ohne Messwert).
- Angenommen: 3 Polls als Deassign-Bestätigung genügen auch bei langsamen
  Poll-Intervallen (bewusst poll- statt zeitbasiert; am Testturnier prüfen).
- Angenommen: Minutengranularität der Prognose reicht fachlich (TL denkt in
  Minuten).

## Betroffene Doku-Dateien

- Diese Spec; Bedien-Doku **`docs/spielzeiten-prognose.md`** (neu, eigenes
  großes Feature); `docs/btp_protocol.md` (Duration-Quelle);
  `docs/turnierleitung-web.md` (Prognose, Spielzeiten-Panel, Pausenanzeige);
  `docs/tablet.md` (E9-Verhaltensänderung); `docs/cloud-relay.md`
  (TlState-JSON gewachsen, Deploy-Reihenfolge); `docs/changelog.md` je
  Version; CLAUDE.md-Tabelle (neue Zeile); ADR 0027 + 0028.

## Umsetzungs-Hinweise

Drei Etappen (= drei PRs), Details in
`docs/features/_intake/spielzeiten-prognose/3-how-to.md` (gitignoriert):

- **Etappe A — Zeiten-Store + BTP-0-Pfade:** `tablet/match_times.rs`
  (ADR-0022-Muster, Datei `match-times.json`), Sync-Hook nach
  `reconcile_on_court` (E4 inkl. Reset), E2-Hook in `handle_score`,
  E3/E11-Hooks in allen Ergebnis-Pfaden, Duration-Quellen umstellen.
  ADR 0027 hier schreiben. Kein security-reviewer nötig.
- **Etappe B — Statistik + Prognose + TL-Anzeige:** `tablet/predict.rs`
  (Median/Fallback, `predict_starts`-Simulation als reine Funktion),
  `PredictionConfig` + SetupWizard, TlState-Erweiterung (`TlMatch.
  predicted_start_ms`/`predicted_uncertain`, `TlFinished.brutto_mins`/
  `netto_mins`, `TlState.time_stats`) in `build_state_limited`, `tl.html`
  (Prognose, Panel „Spielzeiten", Ist-Zeiten).
- **Etappe C — Pausen:** `tablet.html` (Pause hält bis Weiterspielen,
  „überzogen"-Zustand, `startedAt`, Reload-Regel), `TlPause`-Erweiterung
  (+ injury-Parse-Fix), `tl.html`-Countdown. ADR 0028 hier schreiben;
  kurzes security-reviewer-Gate (neues opakes Tablet-Feld `startedAt`,
  typisierter Parse als Mitigation).

Je Etappe: Version gemeinsam bumpen (Cargo.toml + tauri.conf.json +
package.json), code-reviewer (Pflicht), Doku im selben Commit.
