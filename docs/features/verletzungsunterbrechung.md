# Verletzungsunterbrechung: Uhr am Tablet und Eintrag im Spielbericht — Spezifikation

> Status: **abgestimmt 2026-11-14** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Idee vom 14.11.2026. Betroffene Crates: src-tauri · relay-proto · relay (nur Neubau) · src.
> ADR: [0095 — Behandlung als Ereignispaar aus der Pause](../adr/0095-behandlung-als-ereignispaar.md)
> (ergänzt ADR 0038 und 0043).

## Kontext / Problem

Wird ein Spiel wegen einer Verletzung unterbrochen, drückt der Schiedsrichter bzw. der Zähler am
Tablet **✚**. Das startet heute eine Behandlungspause und meldet die Verletzung an die
Turnierleitung. Drei Dinge fehlen:

1. **Keine Uhr am Tablet.** Das Pausen-Overlay zeigt nur ein großes „✚"
   (`renderBreak`, `assets/tablet.html`). Wie lange die Behandlung schon dauert, sieht am Feld niemand.
2. **Der Spielbericht erfährt nichts.** Der Schiedsrichterzettel kennt die Ereignisse
   „Behandlung Beginn/Ende" (`EventKind::InjuryStart/InjuryEnd`), sie entstehen aber nur durch
   einen Handeintrag im Ereignismenü des Schiri-Modus. Wer nur ✚ und „Weiterspielen" drückt,
   hinterlässt keine Spur auf dem Zettel.
3. **Keine Dauer auf dem Zettel.** Selbst ein Handeintrag liefert nur zwei Uhrzeiten.

Dazu kommen Fallen, die die Prüfung gefunden hat: Ereignisse ohne Person werden im Anhang bei
„A"/„A1" gedruckt (`blatt.rs`), ein Undo nimmt eine Behandlung am selben Ballwechsel mit zurück,
Ereignisse tragen Tablet-Zeit, die Pausenuhr Server-Zeit, und in einer laufenden Satz- oder
11er-Pause ist ✚ unerreichbar, weil das Overlay die Kopfzeile verdeckt.

## Zielbild & Erfolgskriterien

- Nach ✚ zählt am Tablet eine große Uhr `m:ss` hoch. Nach einer **Richtzeit** (turnierweit,
  Standard 5 min) wird sie rot und zeigt „Richtzeit überschritten"; sie zählt weiter. Die Pause
  endet nur mit „Weiterspielen" oder „Spiel abbrechen".
- Der Zettel enthält jede Behandlung **automatisch**, in jedem Modus (mit und ohne Schiri-Modus):
  zwei Protokollzeilen „Behandlung Beginn" und „Behandlung Ende (Dauer m:ss)" sowie eine Marke
  **„B"** im Punkteraster am Ballwechsel vor der Unterbrechung.
- Das TL-Web-Badge „Behandlung seit …" wird ab der Richtzeit rot.
- Erfolg im Turnier: Ein Zähler ohne Einweisung drückt nur ✚ und „Weiterspielen", und auf dem
  gedruckten Zettel stehen Beginn, Ende und Dauer der Behandlung — ohne Person.

## Nicht-Ziele

- Keine Uhr auf Court-Monitor, Kombi, Übersicht, Tafel, Stream (dort bleibt „Behandlung").
- Keine Auswahl, **wer** verletzt ist; Behandlungen haben auf dem Zettel keinen Personenbezug.
- Kein Abziehen der Behandlungszeit aus der Spieldauer-Statistik der Prognose.
- Keine Regel-Durchsetzung: Die Richtzeit ist eine Turnier-Konvention, die Pause endet nie von selbst.
- Kein Vorrang am 64er-Ereignisdeckel.
- Keine Weitergabe an badhub (Liveticker, Punktverlauf).
- Nicht behoben wird hier, dass der Zettel-Anhang nach etwa 32 Zeilen abschneidet (eigener
  Roadmap-Punkt).

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - `relay-proto`: neues Feld `MatchBrief.injury_guide_secs` (`injuryGuideSecs`, `serde(default)`).
  - `relay/`: keine Logik, nur Neubau (Feld reist strukturell im `MatchBrief`) und Test-Literal.
  - `src-tauri/src/config.rs`: `injury_guide_seconds`, Konstanten `RICHTZEIT_STANDARD_S = 300`,
    `RICHTZEIT_MIN_S = 60`, `RICHTZEIT_MAX_S = 1800`, Helfer `richtzeit_s()` (klemmt).
  - `tablet/server.rs` (`push_match`) und `tablet/relay_client.rs` (Cloud-Brief): Feld setzen.
  - `tablet/tl.rs`: `TlState.injury_guide_seconds`; `assets/tl.html`: `pauseBadge`/`tickClocks`.
  - `tablet/scoresheet.rs` + `tablet/blatt.rs`: Paarung, Dauer, Marke „B", bis zu zwei Marken je
    Zelle, Spieler-Spalte nach Positivliste.
  - `assets/tablet.html`: Uhr, ✚ in der Pause, Ereignispaar, „Aufgabe", Undo-Neuverankerung,
    Server-Zeit; neu `src/io/behandlungsUhr.mjs` (+ Inline-Kopien), Erweiterung
    `src/io/matchEvents.mjs` (+ Inline-Kopie).
  - `src/pages/SetupWizard.tsx`, `src/types.ts`: Richtzeit in Minuten.
- **Architekturregeln:** R1 unberührt (Setup über bestehendes `save_config`). R2 unberührt — kein
  BTP-Write, keine Zuordnung. **R3: LAN und Cloud** tragen die Richtzeit auf demselben Weg
  (`MatchBrief`); das Relay liefert `tablet.html`/`tl.html` selbst aus und deployt sie beim Merge,
  daher die Fähigkeitsmeldung (unten). R4 unberührt. R5 unberührt — Zettel-Ereignisse sind kein
  Ergebnis. R6 unberührt.
- **Fähigkeitsmeldung:** `injuryGuideSecs > 0` bedeutet zugleich „Host kennt die neue
  Zettel-Darstellung" (Doppelrolle wie `shuttleCounter`). Nur dann erzeugt das Tablet automatische
  Ereignisse, verankert beim Undo neu und blendet die Menü-Handeinträge aus. Ohne Fähigkeit (alter
  Turnier-PC hinter neuer Cloud-Seite, altes Relay): nur die Uhr, Richtzeit 300 s, alles andere
  wie heute.
- **Konfiguration & Abwärtskompatibilität:** neues Feld mit `serde(default)` → alte `config.json`
  liest 300 s. Fremdwerte und 0 werden auf 60–1800 s geklemmt; es gibt **kein** „aus".
  `identifier` und Updater-Pfad unberührt. Gespiegelter Tablet-Stand: `pause` trägt zusätzlich
  `richtSek` und `startId` (< 40 Byte).
- **Datenschutz:** Eine Behandlung ist eine Gesundheitsangabe, sobald sie einer Person zugeordnet
  ist. Deshalb: ✚ fragt **nicht** nach einer Person; Behandlungszeilen haben auf dem Zettel eine
  **leere** Spieler-Spalte (Positivliste `traegt_person`: nur `ServeStart`, `CardYellow`,
  `CardRed`, `CardBlack`, `Disqualified` tragen eine Person; alle anderen Arten, auch künftige,
  bleiben leer). Nichts davon geht an badhub (`badhub/verlauf.rs` exportiert nur `serve_start`,
  Test sichert das). Die Richtzeit im `TlState` ist eine Konfigurationsfrist ohne Personenbezug
  (Wächter-Test `the_state_never_carries_personal_data_beyond_its_purpose` wird ergänzt).
  CLAUDE.md erhält einen Datenschutz-Absatz.
- **Abhängigkeiten:** keine neue Cargo-/npm-Dependency, kein BTP-Bezug, kein badhub-Endpunkt.
  Relay-Deploy läuft automatisch beim Merge.

## Fachliche Regeln

### Uhr am Tablet
- Start bei ✚; Startzeit = `serverNow()` (ein Stempel, identisch mit dem Ereignis-Zeitstempel).
- Anzeige: Titel „Behandlungspause", große Uhr `m:ss` (über 59 min z. B. „75:03"), darunter
  „Richtzeit m:ss (Turnier-Vorgabe)".
- Ab Erreichen der Richtzeit: Uhr rot, Titel „Behandlungspause — Richtzeit überschritten",
  einmal `tlog('warn','injury_guide_exceeded')`.
- Die Richtzeit wird beim Beginn in `pause.richtSek` eingefroren; eine Config-Änderung wirkt ab der
  nächsten Zuweisung bzw. dem nächsten Reconnect (der Cloud-Push-Schlüssel `CourtPushKey` wird
  bewusst nicht erweitert, um kein erneutes `MatchAssigned` mitten im Spiel auszulösen).
- Nach Reload oder Stand-Übernahme rechnet die Uhr aus `pause.startedAt` weiter. Ein alter Stand
  ohne `startedAt` zeigt weiter „✚" ohne Uhr.

### ✚ in Pausen und nach Spielende
- ✚ während einer 11er- oder Satzpause **löst diese Pause ab** und startet die Behandlung. Dafür
  bekommt das Pausen-Overlay einen Knopf **„✚ Verletzung · Behandlung"** (das Overlay verdeckt die
  Kopfzeile). Die abgelöste Pause wird **ohne** `endBreak()` beendet (keine „bitte spielen"-Ansage,
  keine vorzeitige Aufschlagwahl im Doppel); die Aufschlagwahl öffnet wie gewohnt erst, wenn die
  Behandlung mit „Weiterspielen" endet.
- ✚ ist nach Spielende (`STATE.finished`) gesperrt. Das Megafon bleibt verfügbar.
- ✚ während einer laufenden Behandlung tut nichts.

### Ereignisse (nur bei Fähigkeit)
- ✚ erzeugt `injury_start`. Anker und Phase werden **vor** dem Pausenwechsel genommen (sonst wäre
  die Phase immer `break_injury`).
- Jedes Ende der Behandlung über `endBreak()` („Weiterspielen", „Spiel abbrechen") erzeugt
  `injury_end`, sofern die Pause ein `startId` trägt.
- „Spiel abbrechen" in der Behandlung erzeugt danach `retired` („Aufgabe"), Anker vor dem
  Satzabschluss genommen. Auch das Spielende über den Match-beenden-Dialog mit Aufgabe
  (`finishAs('retired')`) erzeugt `retired`. „Wieder öffnen" nimmt ein wirksames `retired` zurück.
- `abortMatchRetired` legt einen Undo-Schritt an wie `finishAs`; „Wieder öffnen" nimmt dann nur den
  Abbruch zurück, nicht den letzten Punkt (Fehlerbehebung).
- Spiel geht während der Behandlung vom Feld (`clearMatch`, `einstiegMitSaetzen`): **kein** Ende
  wird erfunden.
- Alle neu erfassten Zettel-Ereignisse (alle Arten) bekommen `tsMs = serverNow()`. Ordnung und
  Vereinigung bleiben `(set, afterN, seq, id)`.
- Ohne Fähigkeit: keine automatischen Ereignisse; die Menü-Handeinträge „Behandlung beginnt/endet"
  bleiben sichtbar. Mit Fähigkeit werden sie ausgeblendet; endgültig gelöscht werden sie in einem
  späteren Aufräum-PR.

### Undo über eine Behandlung
- `injury_start`/`injury_end` sind **Zeit-Tatsachen**: Der Undo-Schnitt nimmt sie nicht zurück.
- Liegen sie hinter dem neuen Schnitt (gleicher Satz, `afterN > Schnitt`), werden sie **neu
  verankert**: Rücknahme des Originals plus Kopie mit gleichem `tsMs`, gleicher Phase, Anker = neuer
  Schnitt. Reihenfolge Beginn vor Ende.
- Passt die Neuverankerung nicht unter den 64er-Deckel, wird nichts verschoben und nichts
  zurückgenommen (die Marke steht dann leicht versetzt, geht aber nicht verloren).
- Behandlungen aus der Satzpause (Anker `(set+1, 0)`) bleiben vom Undo im Vorsatz unberührt.

### Zettel
- **Paarung** im Renderer, über wirksame Ereignisse (ohne Rücknahmen und Zurückgenommene) in
  `(set, afterN, seq, id)`; bei gleicher Position dient `ts_ms` als Hilfsordnung nur für die
  Paarung. Ein `InjuryEnd` schließt den **jüngsten offenen** `InjuryStart`.
  - Beginn ohne Ende → Beginn-Zeile „… — Ende —, Dauer offen".
  - Ende ohne offenen Beginn → nur die Ende-Zeile, ohne Dauer.
  - Ende vor Beginn oder Dauer > 6 h → „(Dauer —)". Rechnung überlauffest.
- **Protokoll:** zwei Zeilen; Beginn mit Phasenzusatz wie heute (z. B. „(Intervall)"), Ende als
  „Behandlung Ende (Dauer 4:12)" ohne Phasenzusatz. Eine neu verankerte Behandlung erscheint nur
  **einmal** (zurückgenommenes `Injury*` mit wirksamem Zwilling gleicher Art und gleichem `ts_ms`
  wird ausgeblendet).
- **Raster:** `InjuryStart` → Marke **„B"** in der Zelle `after_n` bei Phase `play` **oder**
  `break_eleven`; sonst (Satzbeginn `after_n == 0`, Satzpause, vor/nach dem Spiel) Randmarke wie
  bisher. `InjuryEnd` markiert nichts.
- **Bis zu zwei Marken je Zelle.** Rangfolge `D > F > W > R > B`, Duplikate entfallen; eine dritte
  Marke erscheint nur im Protokoll. Marke 1 rechts oben (wie heute), Marke 2 links oben; Maße und
  Budget-Asserts bleiben unverändert.
- **Spieler-Spalte** nur bei `traegt_person`, sonst leer.
- **Altbestände:** Alte Handeinträge bekommen beim Nachdruck „B", leere Spalte und — wenn
  gepaart — eine Dauer; ein einzelner alter Beginn erscheint als „Dauer offen".

### TL-Web
- `TlState.injury_guide_seconds` (fehlt bei altem Host → 300). Das Behandlungs-Badge wird ab
  `jetzt − started_at_ms ≥ Richtzeit` rot (`badge-alarm`), Titel „Richtzeit überschritten". Ändert
  sich die Richtzeit während einer laufenden Behandlung, kann das Badge kurz vom Tablet abweichen
  (hingenommen).

### Setup
- Abschnitt „Behandlungspause" neben „Ballzähler": Zahl in **Minuten** (1–30, Standard 5),
  gespeichert in Sekunden. Erklärtext: Die Richtzeit ist eine Turnier-Vorgabe, keine Regel; die Uhr
  wird rot und zählt weiter, die Pause endet nur mit „Weiterspielen".

## Akzeptanzkriterien

Uhr und Bedienung
- [ ] Nach ✚ zeigt das Tablet „Behandlungspause" und eine hochzählende Uhr ab 0:00.
- [ ] Bei Richtzeit 5 min wird die Uhr bei 5:00 rot, der Titel lautet „Behandlungspause —
      Richtzeit überschritten", die Uhr zählt weiter; die Pause endet nicht von selbst.
- [ ] Nach Neuladen des Tablets mitten in der Behandlung zeigt die Uhr die seit ✚ verstrichene Zeit.
- [ ] Ein Tablet-Stand ohne `startedAt` zeigt „✚" ohne Uhr und erzeugt kein `injury_end`.
- [ ] In einer 11er- und in einer Satzpause zeigt das Overlay „✚ Verletzung · Behandlung"; Tippen
      beendet die Pause ohne „bitte spielen"-Ansage und startet die Behandlung.
- [ ] Im Doppel nach Satzende: ✚ in der Satzpause öffnet die Aufschlagwahl **nicht**; sie öffnet
      nach „Weiterspielen" der Behandlung.
- [ ] Nach Spielende ist ✚ gesperrt, das Megafon nicht.
- [ ] Richtzeit-Änderung im Setup wirkt ab der nächsten Zuweisung; eine laufende Behandlung behält
      ihre Richtzeit.

Ereignisse und Zettel (Host mit Fähigkeit)
- [ ] ✚ + „Weiterspielen" ohne Schiri-Modus → Zettel zeigt „Behandlung Beginn" und „Behandlung
      Ende (Dauer m:ss)" mit leerer Spieler-Spalte und „B" in der Zelle des Ballwechsels davor.
- [ ] Die Dauer auf dem Zettel stimmt mit der Tablet-Uhr beim Drücken von „Weiterspielen" überein
      (auf die Sekunde gerundet), auch nach Tablet-Wechsel mitten in der Behandlung, sofern der
      Stand übernommen wurde.
- [ ] ✚ aus der 11er-Pause → „B" in der Zelle des letzten Ballwechsels, Beginn-Zeile mit „(Intervall)".
- [ ] ✚ bei 0:0 eines Satzes oder aus der Satzpause → „B" als Randmarke, nicht im Raster.
- [ ] Zelle mit „R" und „B" zeigt beide; Zelle mit „F", „R" und „B" zeigt „F" und „R", „B" steht
      nur im Protokoll.
- [ ] Spiel während der Behandlung vom Feld → Zettel zeigt „Ende —, Dauer offen".
- [ ] „Spiel abbrechen" in der Behandlung → Zettel: Behandlung Ende (mit Dauer), danach „Aufgabe"
      ohne Spieler. „Wieder öffnen" → „Aufgabe" zurückgenommen, Spielstand unverändert (kein Punkt
      verloren).
- [ ] Spielende per Dialog „Aufgabe" → „Aufgabe" im Protokoll.
- [ ] Undo eines Punktes nach einer Behandlung → Behandlung erscheint **einmal** im Protokoll,
      „B" am Ballwechsel vor der Behandlung nach dem korrigierten Stand.
- [ ] Würde die Neuverankerung den 64er-Deckel überschreiten (z. B. 61 Ereignisse, ein
      Behandlungspaar zu verschieben = 4 neue) → keine Neuverankerung, keine Rücknahme, Behandlung bleibt.
- [ ] Im Schiri-Ereignismenü fehlen „Behandlung beginnt/endet".
- [ ] Zettel-Ereignisse aller Arten tragen Server-Zeit.

Fähigkeit und Rückfall
- [ ] Neue Tablet-Seite hinter einem Host ohne `injuryGuideSecs`: Uhr läuft mit 300 s, keine
      automatischen Ereignisse, Menü-Handeinträge sichtbar.
- [ ] Neue `tl.html` hinter altem Host: Badge rot ab 300 s.
- [ ] Alte `config.json` ohne Feld → Richtzeit 300 s; Werte 0, 10, 99999 → geklemmt auf 60 bzw. 1800.

Datenschutz
- [ ] Keine Behandlungs-, Aufgabe-, Unterbrechungs-, Überstimmungs- oder OSR-Zeile trägt eine
      Person, auch nicht bei alten Handeinträgen.
- [ ] Kein Behandlungsereignis erscheint im badhub-Verlauf.

## Tests

Rust (`cargo test` grün, `clippy --workspace --all-targets -D warnings`):
- relay-proto: `match_brief_ohne_richtzeit_liest_null`, `match_brief_richtzeit_roundtrip`.
- config: `richtzeit_standard_ist_300`, `alte_config_ohne_richtzeit_liest_standard`,
  `richtzeit_wird_auf_60_bis_1800_geklemmt`.
- server: `match_brief_traegt_richtzeit_aus_config`, `match_brief_ohne_aufrufer_meldet_keine_faehigkeit`.
- relay_client: `cloud_brief_traegt_richtzeit`, `richtzeit_aendert_den_push_schluessel_nicht`.
- tl: `tl_state_traegt_richtzeit`, Wächter-Liste ergänzt.
- scoresheet: `marker_folgen_der_dbv_konvention` angepasst (InjuryStart → „B"),
  `behandlung_beginn_markiert_seine_zelle_mit_b`, `behandlung_aus_dem_intervall_markiert_die_zelle`,
  `behandlung_ende_markiert_nichts`, `behandlung_bei_satzbeginn_steht_am_rand`,
  `behandlung_aus_der_satzpause_steht_am_rand`, `zelle_traegt_bis_zu_zwei_marken`,
  `dritte_marke_faellt_nach_rangfolge_aus_dem_raster`, `paarung_beginn_ende_liefert_dauer`,
  `beginn_ohne_ende_ist_dauer_offen`, `ende_ohne_beginn_ist_nur_eine_zeile`,
  `ende_schliesst_den_juengsten_offenen_beginn`, `zurueckgenommenes_wird_vor_der_paarung_gefiltert`,
  `verschobene_behandlung_erscheint_einmal_im_protokoll`, `ende_vor_beginn_ist_unplausibel`,
  `riesige_zeitstempel_ueberlaufen_nicht`, `ereignisse_ohne_person_lassen_die_spieler_spalte_leer`
  (`match` ohne Wildcard über alle `EventKind`), `alte_handeintraege_werden_gepaart`.
- blatt: `zwei_marken_ueberdecken_den_zellwert_nicht`, `laengste_art_zeile_passt_in_die_spalte`,
  `behandlung_ohne_person_leere_spieler_spalte`.
- badhub/verlauf: bestehende „kein injury"-Tests um `InjuryEnd`, `Retired` und `pause` mit
  `richtSek`/`startId` erweitert.

JS (node-Skripte, CI):
- `scripts/test-match-events.mjs`: Undo-Schnitt lässt Zeit-Tatsachen stehen; `undoNeuverankern`
  (Rücknahme + Kopie, gleicher `tsMs`/Phase, Beginn vor Ende, Deckel-Rückfall, andere Sätze und
  `afterN ≤ Schnitt` unberührt, mehrere Undo-Schritte).
- `scripts/test-behandlungs-uhr.mjs` (neu, in `ci.yml`): `m:ss` für 0/59/60/4499 s, Schwelle genau
  bei der Richtzeit, `null` ohne `startedAt`, `richtzeitAusBrief` für fehlt/0/negativ/NaN/10⁹/String,
  `hostKenntBehandlung`.
- `scripts/test-inline-kopien.mjs`: neue Drift-Prüfung der `me*`-Kopie und der
  `behandlungsUhr`-Kopien in `tablet.html` und `tl.html`.
- `scripts/test-stand-groesse.mjs`, `scripts/test-handbuch-fakten.mjs` bleiben grün.
- `npm run build` fehlerfrei.

Manuell (LAN und Cloud): ✚ im Spiel bis zur Richtzeit; ✚ in 11er- und Satzpause (Doppel);
Reload in der Behandlung; Tablet-Wechsel in der Behandlung; Abbruch + Wieder öffnen; Undo über die
Behandlung; ✚ nach Spielende; Zettel drucken; neue Seite gegen alten Host.

## Risiken & Rollback

| Risiko | Gegenmittel |
|---|---|
| Merge von PR 2–4 deployt die Tablet-/TL-Seite sofort in die Cloud, Tablets laden neu | Nicht während eines Cloud-Turniers mergen; Uhr nach Reload aus `startedAt` |
| Alter Host druckt automatische Behandlung bei „A1" | Fähigkeitsmeldung; PR 1 steht in einem Release vor PR 2 |
| 64er-Deckel durch Neuverankerung | Vorabprüfung, Rückfall „nicht verschieben" |
| ✚ in der Satzpause öffnet die Aufschlagwahl zu früh | eigene Ablöse-Funktion statt `endBreak()` |
| Versatz der Server-Uhr springt nach der ersten Synchronisierung | Plausibilitätsregel „Dauer —"; Ordnung hängt nicht an `tsMs` |
| Tablet-Wechsel mit altem lokalem Stand desselben Spiels verliert die laufende Behandlung | hingenommen, Zettel zeigt „Dauer offen"; in `docs/tablet.md` dokumentiert |

Rollback: Revert auf `main` (das Relay deployt die alte Seite automatisch), LAN-Hosts per Update.
Ereignisse sind append-only, nichts wird migriert. Das Wire-Feld ist `serde(default)` und in beide
Richtungen kompatibel. Wer „rot" im Turnier nicht will, stellt die Richtzeit auf 30 min.

## Offene Fragen / Annahmen

- Annahme: BWF/DBV kennen keine feste Behandlungsdauer; die Richtzeit ist ausdrücklich eine
  Turnier-Konvention, deshalb „Richtzeit überschritten" statt „überzogen".
- Annahme: Behandlungen sind selten genug, dass der 64er-Deckel ohne Vorrang reicht.
- Folgepunkte außerhalb dieser Spec (Roadmap): Zettel-Anhang schneidet nach ~32 Zeilen ab;
  Aufräum-PR löscht die Menü-Handeinträge, sobald kein Host ohne Fähigkeit mehr im Feld ist.

## Betroffene Doku-Dateien

- `docs/schiedsrichterzettel.md` (Marke „B", zwei Marken, Paarung, Dauer, leere Spieler-Spalte,
  Undo, Aufgabe) · `docs/features/schiedsrichterzettel-druck.md` (Verweis)
- `docs/tablet.md` (Uhr, ✚ in der Pause, Reload, Tablet-Wechsel → „Dauer offen", Richtzeit ab
  nächster Zuweisung, Server-Zeit der Ereignisse)
- `docs/tablet-bedienen.md` (Handbuch: „ohne Countdown" fällt weg, „Richtzeit überschritten",
  Knopf „✚ Verletzung · Behandlung", Aufgabe aus der Behandlung) — mit `pruef`-Ankern
- `docs/umpire-mode.md` (Menüeinträge entfallen)
- `docs/einstellungen.md` (Richtzeit, Standard 5 min, `pruef`-Anker auf `RICHTZEIT_STANDARD_S`)
  · `docs/erste-schritte.md`, falls der Setup-Abschnitt dort aufgezählt ist
- `docs/turnierleitung-web.md` (Badge rot ab Richtzeit) · `docs/features/turnierleitung-web.md`
- `docs/cloud-relay.md` (`MatchBrief.injuryGuideSecs`, Fähigkeitsmeldung)
- `docs/spielzeiten-prognose.md` (Behandlungszeit bleibt in der Spieldauer — Nicht-Ziel)
- `CLAUDE.md`: neue Tabellenzeile „Verletzungsunterbrechung" + Datenschutz-Absatz
- `docs/roadmap.md`, `docs/changelog.md` je Release, ADR 0095 + Ergänzungsvermerk in ADR 0043

## Umsetzungs-Hinweise

Vier PRs, Version jeweils **erst beim Merge** festlegen und gemeinsam in `src-tauri/Cargo.toml`,
`src-tauri/tauri.conf.json`, `package.json` bumpen. `code-reviewer` nach jedem PR.

1. **PR 1 – Zettel-Renderer** (`scoresheet.rs`, `blatt.rs`, ADR 0095, Vermerk in ADR 0043, Doku
   Zettel). Wirkt sofort auch auf alte Handeinträge, kein Relay-Deploy. `security-reviewer`
   (leicht): untrusted `ts_ms` aus Tablet/Relay. **Muss in einem Release vor PR 2 stehen.**
2. **PR 2 – Richtzeit** (relay-proto, relay-Test, config, server, relay_client, tl.rs, tl.html,
   SetupWizard, types; Doku Einstellungen/TL-Web/Cloud). Relay-Deploy automatisch beim Merge.
   Wächter-Test ergänzen.
3. **PR 3 – Undo-Neuverankerung + Server-Zeit** (`matchEvents.mjs` + Test, `tablet.html`, neuer
   Drift-Test; Doku Zettel/Tablet).
4. **PR 4 – Uhr, ✚-Paar, ✚ in der Pause, Aufgabe, Reopen-Fix** (`behandlungsUhr.mjs` + Test +
   CI, `tablet.html`, Drift; CLAUDE.md-Zeile + Datenschutz; Handbuch). Hängt an PR 2 und PR 3.
5. *(später)* Aufräumen: Menü-Handeinträge löschen.

Wichtige Fallen aus der Planung:
- `injury_start` muss Anker und Phase **vor** dem Pausenwechsel nehmen.
- `retired` beim Abbruch: Anker **vor** `setsCompleted.push` nehmen.
- ✚ in der Pause nicht über `endBreak()` beenden (Ansage + Aufschlagwahl).
- `CourtPushKey` nicht um die Richtzeit erweitern.
- Das Relay bettet `tablet.html`/`tl.html` ein → jede Seitenänderung ist beim Merge live in der Cloud.

Details mit Datei:Zeile-Belegen standen im How-To (gitignoriert); die Stellen sind in den Abschnitten
oben benannt (`startInjury`, `startBreak`, `endBreak`, `tickBreak`, `renderBreak`,
`abortMatchRetired`, `finishAs`, `reopen`, `undo`, `erfasseEreignis`, `eventPhase`, `canSignal`,
`marker_fuer`, `Zelle.marker`, Anhang in `blatt.rs`, `pauseBadge`/`tickClocks`).
