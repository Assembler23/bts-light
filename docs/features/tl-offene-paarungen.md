# Offene Paarungen in der TL-Spielliste — Spezifikation

> Status: **Entwurf 30.08.2026** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Nutzer-Idee vom 29.08.2026. Betroffene Crates: `src-tauri` (Kern +
> `assets/tl.html`), `relay-proto` (ein Feld), `src/io` (ein Modul).
> ADR: [0051](../adr/0051-offene-spiele-eigene-gedeckelte-liste.md) ·
> [0052](../adr/0052-beschriftung-offener-plaetze.md) ·
> [0053](../adr/0053-offene-spiele-in-der-manuellen-reihenfolge.md)

## Kontext / Problem

Die Turnierleitungs-Weboberfläche zeigt heute **nur Spiele mit vollständig
bekannten Teilnehmern**. Jedes KO-Spiel, dessen Vorrunde noch läuft,
verschwindet aus der Liste — obwohl BTP es auf demselben Turnier bereits führt.
Die Turnierleitung sieht in BTP ein Viertelfinale, findet es in der App nicht
und misstraut der Anzeige.

Ursache ist der Filter `!team1.is_empty() && !team2.is_empty()` in
`tl.rs::build_state_limited`. Am echten BTP-Mitschnitt
(`src-tauri/tests/fixtures/btp-tournament-2halls.bin`) gemessen: von 36
`IsMatch=true`-Zeilen tragen **22** mindestens einen offenen Platz — 20 davon
beide. Sie haben eine echte `MatchNr`, eine `PlanningID`, `From1`/`From2` und
einen `RoundName` („HF", „Finale", „3/4"), aber weder `Winner` noch `CourtID`
und werden deshalb korrekt als `Scheduled` geparst.

Den Schmerz hat die **Turnierleitung**: Sie plant den Tagesverlauf und muss
wissen, was nach der laufenden Runde kommt.

## Zielbild & Erfolgskriterien

Ein TL-Gerät zeigt standardmäßig die vollständige BTP-Spielliste einschließlich
noch offener Paarungen, an regulärer Sortierposition, klar als „noch offen"
erkennbar und — wo BTP es hergibt — mit den möglichen Teilnehmern beschriftet.
Wer die kurze Arbeitsliste bevorzugt, schaltet die offenen Spiele am eigenen
Gerät ab.

**Erfolgskriterien**

- Beim nächsten Turnier fragt niemand mehr „wo ist das Viertelfinale, in BTP
  steht es doch".
- Kein TL-Gerät sieht nach dem Update **weniger** echte Wartelisten-Spiele als
  vorher — auch nicht über die Cloud.
- Der TL-Zustand bleibt über das Relay unter `MAX_TL_STATE_LEN` (64 KiB): Die
  offene Liste wird **zuletzt aufgefüllt** und damit **zuerst geopfert** — im
  dokumentierten Worst Case (26 belegte Felder, 30 Ergebnisse) entfällt sie
  ganz, bevor irgendetwas anderes leidet.
- Die Renderzeit der Liste (`tlRenderMessen`) bleibt im bisher gemessenen
  Rahmen.
- Der Schalter erklärt sich ohne Handbuch.

## Nicht-Ziele

- **Kein Vorbereitungs-Aufruf** für offene Spiele (G2). Der Knopf ist an der
  Zeile aus. Begründung: `state.rs::apply_preparation_calls` verwirft heute
  jeden Aufruf für ein Match mit leerer Mannschaft, BTP bekäme kein Highlight,
  der Vorbereitungs-Monitor zeigte nichts und die Sprachansage hätte keine
  Namen anzusagen.
- **Keine Feldzuweisung und keine automatische Feldvergabe** für offene Spiele
  (`sync.rs::auto_assign` bleibt unverändert).
- **Keine Startzeit-Prognose** für offene Spiele — sie belegen in der
  Simulation keine Felder und verschieben damit keine echten Startzeiten.
- **Kein Vorabzettel und kein Punktverlauf** an der Zeile eines offenen Spiels.
- **Nur TL-Web** (G8). Desktop-Vorbereitungsliste, Hallen-Monitor,
  `preparation.html` und der badhub-Liveticker bleiben unverändert; die neun
  weiteren Filterstellen im Code (`sync.rs` 352/1041/1741, `commands.rs` 2348,
  `server.rs` 1987, `state.rs` 3775, `relay_client.rs` 1064, `assign.rs` 641)
  werden **nicht** angefasst.
- **Keine rekursive Auflösung** über mehr als eine Turnierbaum-Ebene.
- **Kein neuer Wire-Frame und keine neue `TlAction`** — der TL-Zustand reist
  als opakes JSON; `relay-proto` bekommt einzig das Schalter-`bool` im
  bestehenden Profil-Wire.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  `src-tauri/src/tablet/tl.rs` (`build_state_limited`, neue Liste im `TlState`,
  Kappungskaskade, Wächter-Test) · `src-tauri/src/tablet/assign.rs`
  (`ready_queue`, Zustandsmarke) · `src-tauri/src/tablet/queue_order.rs` +
  `src-tauri/src/tablet/sync.rs` (`reconcile_queue_order`) ·
  `src-tauri/src/btp/model.rs` (Feeder-Auflösung Slot→Match) ·
  `src-tauri/src/config.rs` (`TlDisplaySettings`) ·
  `src-tauri/assets/tl.html` (Rendern, Mischen, Schalter) ·
  `src/io/offeneSpiele.mjs` + `scripts/test-offene-spiele.mjs` (neu, die
  Misch-/Beschriftungslogik als testbares Modul mit Inline-Kopie) ·
  `relay-proto/src/lib.rs` (**ein** Feld: `TlDisplaySettingsWire.hide_open_matches`).
  **Nicht** betroffen: `relay/` (außer dem automatischen Re-`include_str!` von
  `tl.html`) und die React-Oberfläche unter `src/pages`/`src/components`.
- **Architekturregeln:**
  **R1** unberührt — die Änderung liegt vollständig hinter den bestehenden
  TL-Routen; das React-Frontend wird nicht angefasst.
  **R2** gewahrt: Kandidaten werden **ausschließlich** aus dem BTP-Snapshot
  abgeleitet (`from1`/`from2` → `planning_id` im selben `draw_id`), nie geraten,
  nie gespeichert, nie nach BTP zurückgeschrieben.
  **R3** beide Wege: LAN (`QUEUE_LIMIT` 120) und Cloud (Kappungsstufen
  `[40, 20, 10, 5]`, 64 KiB) tragen dieselbe Logik mit unterschiedlichen
  Deckeln.
  **R4/R5/R6** unberührt — keine Ergebnisse, keine Namespace-Fragen.
- **Konfiguration & Abwärtskompatibilität:**
  Ein neues Feld in `TlDisplaySettings`, **invertiert** benannt
  (`hide_open_matches: bool`), damit eine bestehende `config.json` ohne diesen
  Eintrag per `#[serde(default)]` auf `false` fällt — also „offene Spiele
  anzeigen" (G7). `identifier` und Updater-Pfad bleiben unangetastet.
- **Datenschutz:**
  Kandidatennamen sind Spielernamen von Personen, die dieses Spiel womöglich
  nie bestreiten. Sie stehen hinter dem Gerätezugang und dienen demselben
  Liveticker-/Turnierleitungszweck wie die übrigen Wartelisten-Namen — bewusst
  freigegeben (G6). **Keine Lizenznummern und keine Spieler-IDs** für
  Kandidaten: `team1_ids`/`team2_ids` sind ausschließlich der badhub-Link
  feststehender Teilnehmer. Kein Geburtsjahr, kein Verein an Kandidaten.
  Da der Schalter clientseitig wirkt, reisen die Labels **immer** im Zustand
  mit, auch zu Geräten mit ausgeschaltetem Schalter — akzeptiert.
- **Abhängigkeiten:** BTP-Protokoll (`Match.From1`/`From2`, `PlanningID`,
  `DrawID`). Keine neue Cargo- oder npm-Dependency.

## Verhalten im Detail

### Welche Spiele erscheinen

Jedes Match mit `status == Scheduled`, das heute allein am leeren `team1` oder
`team2` scheitert. Auch Spiele ohne `planned_time` und ohne bekannte
Vorgängerpaarung.

### Beschriftung eines offenen Platzes (Kaskade)

1. **Kandidaten** — findet sich im selben Draw ein Match mit
   `planning_id == from1` (bzw. `from2`), werden dessen Teilnehmer genannt,
   getrennt durch **„ oder "**: `Müller oder Schmidt`. Der Schrägstrich bleibt
   dem Doppelpaar vorbehalten (`Müller/Meier oder Schmidt/Klein`).
   **Genau eine Ebene** — ist das Vorspiel selbst offen, greift Stufe 2.
2. **Herkunft** — `aus Spiel 42`, wenn das Vorspiel gefunden wird, aber selbst
   noch offen ist und eine `match_num` trägt. Bewusst **neutral**: Bei
   Platzierungsspielen speist der **Verlierer** den Slot, „Sieger aus 42" wäre
   dort schlicht falsch, und BTP liefert die Seite nicht eindeutig.
3. **„noch offen"** — wenn zu `from1`/`from2` kein Match im selben Draw
   existiert (Setzplatz, Freilos, Gruppen-/Qualifikations-Speisung über
   Draw-Grenzen) oder die Spielnummer fehlt.

Der halb aufgelöste Sonderfall (`entry1_id != 0`, aber `team1` leer — die
Paarung steht, die Namen sind nicht auflösbar; `model.rs:1221`) wird wie ein
offener Platz behandelt und neutral beschriftet.

### Position und Zustandsmarke

Offene Spiele erscheinen **eingereiht** an ihrer regulären Stelle nach dem
bestehenden Sortierschlüssel. Sie tragen eine eigene Zustandsmarke „noch
offen" — nicht das leere `blocked: None`, das sie sonst optisch als
spielbereit ausweisen würde (`PlayerAvailability::blocked` liefert für ein
Match ohne Spieler `None`).

### Transport

Der Zustand trägt die offenen Spiele in einer **zweiten, schlanken und separat
gedeckelten Liste** (`open_queue`); `tl.html` mischt beide beim Rendern. Die
Mischposition kommt **vom Host**: Jeder offene Eintrag trägt `queue_index` =
„wie viele echte Wartelisten-Spiele stehen vor mir". Der Client fügt an dieser
Position ein und sortiert **nicht** selbst — `tl.html:6089` sagt ausdrücklich
zu, dass die Sortierung serverseitig verbindlich ist.

Über die Cloud wird die offene Liste **adaptiv** aufgefüllt: Die bestehende
Kappungsleiter bestimmt zuerst unverändert die Stufe der echten Warteliste,
danach wird `open_queue` so weit aufgefüllt, wie das 64-KiB-Fenster es noch
hergibt (ADR 0051). Ein fester Zahlenwert wäre falsch — im Worst-Case-Fixture
liegt der Zustand nach Opferung von `time_stats` bereits bei 55 286 B.

**Wichtig für die Reihenfolge im Client:** erst mischen, **dann** nach Halle
filtern — `queue_index` ist eine Positionsangabe, keine Match-ID.

### Schalter

Ein Anzeige-Schalter je TL-Gerät/Profil, wie die bestehenden
(`showNations`, Vereine). Standard: **an**. Intern invertiert gespeichert
(`hide_open_matches`), damit Bestandsprofile ohne Eintrag ebenfalls „an"
stehen.

### Aktionen an der Zeile

| Aktion | Offenes Spiel |
|---|---|
| Umsortieren (ziehen, „↑ nach oben") | **erlaubt** — volle Teilnahme an der globalen manuellen Reihenfolge |
| Halle setzen, Wunschfeld, Feldvergabe-Ausnahme | **erlaubt** — greifen erst, wenn die Paarung feststeht |
| Vorbereitungs-Aufruf (Megafon) | **gesperrt**, Knopf aus |
| Feld zuweisen, automatische Vergabe | **gesperrt** |
| Vorabzettel drucken, Punktverlauf | **ausgeblendet** |

## Akzeptanzkriterien

- [ ] Ein Match mit leerem `team1` **und** leerem `team2` erscheint in der
      TL-Spielliste, wenn der Schalter an ist.
- [ ] Ein Match mit genau einem leeren Team erscheint ebenfalls, und der
      besetzte Platz zeigt weiterhin die echten Namen.
- [ ] Steht im selben Draw ein Match mit `planning_id == from1`, zeigt der
      offene Platz dessen Teilnehmer, getrennt durch „ oder ".
- [ ] Ist dieses Vorspiel selbst offen und trägt eine `match_num`, steht dort
      `aus Spiel <Nr>` — **nie** „Sieger aus".
- [ ] Existiert zu `from1` kein Match im selben Draw, steht dort `noch offen`.
- [ ] Fehlt dem gefundenen Vorspiel die `match_num`, steht dort `noch offen`.
- [ ] Ein offenes Spiel trägt **keine** Lizenznummer und **keine** Spieler-ID
      in seinen Kandidatenfeldern.
- [ ] Der Wächter-Test in `tl.rs` schlägt fehl, sobald ein Kandidatenfeld eine
      Lizenznummer, ein Geburtsjahr oder einen Verein mitführt.
- [ ] Mit ausgeschaltetem Schalter zeigt die Liste exakt dieselben Zeilen wie
      vor dieser Änderung.
- [ ] Eine `config.json` **ohne** das neue Feld ergibt „Schalter an".
- [ ] Ein offenes Spiel trägt **kein** `predicted_start_ms`, und die
      prognostizierten Startzeiten der echten Spiele sind mit und ohne offene
      Spiele identisch.
- [ ] `sync.rs::auto_assign` weist einem offenen Spiel **nie** ein Feld zu.
- [ ] Der Vorbereitungs-Aufruf ist an einem offenen Spiel nicht auslösbar.
- [ ] Ein offenes Spiel lässt sich in der Liste verschieben, und die neue
      Position überlebt den nächsten Sync-Takt (`reconcile_queue_order` wirft
      es **nicht** aus dem Präfix).
- [ ] Ein Zug **vor** ein offenes Spiel wird angenommen und wirkt.
- [ ] Wird ein offenes Spiel durch ein BTP-Ergebnis zu einem vollständigen
      Spiel, behält es seine manuell gesetzte Position.
- [ ] Derselbe Zustand einmal **mit** und einmal **ohne** offene Spiele ergibt
      über die Cloud dieselbe `queue.len()` — die echte Warteliste wird durch
      offene Spiele keine Kappungsstufe kürzer.
- [ ] Reicht der Platz nicht, wird die offene Liste gekürzt oder geleert,
      während `queue`, `time_stats` und `finished` unangetastet bleiben.
- [ ] Ein großes Turnier bleibt auch mit offenen Spielen unter
      `MAX_TL_STATE_LEN`.
- [ ] Die Revision bleibt an der **vollen** Fassung hängen, auch mit offener
      Liste (Rev-Churn-Schutz).
- [ ] Ein offener Eintrag kennt über `queue_index` seinen Platz zwischen den
      echten Spielen.
- [ ] Eine neue `tl.html` an einem alten Host (ohne die neuen Felder) zeigt
      schlicht keine offenen Spiele und wirft keinen Fehler.

## Tests

**Rust-Unit-Tests** (TDD-Pflicht, `cargo test` grün):

- `offenes_spiel_steht_in_der_tl_liste` — Match ohne Teams erscheint.
- `halb_offenes_spiel_behaelt_die_bekannten_namen`
- `kandidaten_kommen_aus_dem_vorspiel_desselben_draws`
- `kandidaten_werden_mit_oder_getrennt_nicht_mit_schraegstrich`
- `offener_platz_ohne_feeder_heisst_noch_offen` — `from1` zeigt auf eine
  `PlanningID`, zu der es im Draw kein Match gibt.
- `offener_platz_faellt_auf_die_spielnummer_zurueck`
- `feeder_ohne_spielnummer_heisst_noch_offen`
- `feeder_wird_nur_eine_ebene_tief_aufgeloest`
- `halb_aufgeloester_slot_gilt_als_offen` — `entry1_id != 0`, `team1` leer.
- `offene_paarungen_tragen_kandidatennamen_aber_keine_lizenznummern`

**Zwei Wächter sind zu erweitern, nicht einer:**

- `every_published_field_is_deliberately_allowed` — die flache
  Feldnamen-Whitelist bekommt `open_queue`, `open_queue_truncated`,
  `open_slot1_label`, `open_slot2_label`, `queue_index`, jeweils mit
  Begründung. Ohne Eintrag muss der Test **rot** werden.
- `the_state_never_carries_personal_data_beyond_its_purpose` — der strukturelle
  `_ids`-Pfad-Wächter mit `assert_eq!(fundorte.len(), erlaubt.len())` muss bei
  seiner bisherigen Zahl bleiben; das ist der Beweis, dass die offene Liste
  keine `_ids`-Struktur trägt.

Beide Fixtures brauchen ein offenes Spiel **samt** `assert!(!s.open_queue.is_empty())`
— sonst prüfen die Wächter die neuen Felder nie. Dieselbe Falle ist bei
`finished`/`layouts`/`checkin_times` bereits dokumentiert.

- `die_warteliste_wird_durch_offene_spiele_keine_stufe_kuerzer` — derselbe
  Zustand mit und ohne offene Spiele, `queue.len()` identisch.
- `der_cloud_zustand_opfert_die_offenen_spiele_vor_der_warteliste`
- `ein_grosses_turnier_bleibt_auch_mit_offenen_spielen_unter_der_relay_grenze`
- `die_revision_bleibt_an_der_vollen_fassung_haengen_auch_mit_offener_liste`
- `offenes_spiel_bekommt_keine_prognose`
- `prognose_ist_mit_und_ohne_offene_spiele_identisch`
- `auto_assign_vergibt_kein_offenes_spiel`
- `ready_queue_enthaelt_offene_spiele`
- `reconcile_queue_order_behaelt_offene_spiele_im_praefix`
- `manuelle_position_ueberlebt_das_bekanntwerden_der_paarung`
- `alte_config_ohne_feld_zeigt_offene_spiele`

**Bestandstests, die unverändert grün bleiben müssen** (Regressionsbeweis):

- `queue_reorder_never_backfills_matches_beyond_what_tl_web_could_show`
  (`state.rs`) — die getrennte Zählung fällt bei einem Turnier ohne offene
  Spiele exakt auf 120 zurück. Ein pauschales `QUEUE_LIMIT + OPEN_QUEUE_LIMIT`
  täte das nicht; das ist der Grund für die aufwendigere Rechnung.
- `auto_assign_skips_match_with_unknown_opponent` (`sync.rs`) — im Kommentar
  als Wächter dieser Spec kennzeichnen.
- `check_assign` lehnt ein Spiel ohne Mannschaften bereits mit
  `MatchNotPlayable` ab (`assign.rs:641`). A3 ist **Bestand** und wird nur
  festgenagelt, nicht gebaut.

**Bestehender Test anzupassen:** `src-tauri/tests/queue_order_consistency.rs`
bekommt eine ausdrückliche Ausnahme „offene Spiele gehören nicht zum Vergleich
zwischen TL-Web, Desktop und Liveticker" — samt Kommentar, **warum** (G8).

**Frontend:** `npm run build` fehlerfrei. Für die Misch-Logik in `tl.html` ein
ausgelagertes Modul unter `src/io/` mit Node-Test unter `scripts/`, nach dem
Muster von `queueOrder`/`dragScroll` — Asset-JS ist sonst nicht testbar.

**Manueller Turnier-Testfall:** Turnier mit laufender Gruppenphase öffnen,
prüfen, dass HF/Finale mit Kandidaten erscheinen; ein Gruppenspiel entscheiden
und prüfen, dass die Kandidatenliste im Folgespiel kürzer wird und beim
Feststehen der Paarung in echte Namen umschlägt.

## Risiken & Rollback

- **Längere Liste im laufenden Turnier.** Standard „an" verlängert nach dem
  Auto-Update jede TL-Liste ungefragt. Gegenmittel: Der Schalter ist am Gerät
  in einem Griff erreichbar; die Zustandsmarke macht offene Spiele auf einen
  Blick unterscheidbar.
- **Frame-Größe über die Cloud.** Bereits heute liegt der dokumentierte Worst
  Case bei 62 467 von 65 536 B. Würde die offene Liste nicht zuletzt aufgefüllt,
  kippte der ganze Frame — das Relay verwirft ihn samt Vorgänger, und die
  Cloud-TL sähe **gar nichts** mehr. Deshalb ist die Reihenfolge
  Akzeptanzkriterium, nicht Implementierungsdetail.
- **Präfix-Semantik.** Offene Spiele in der globalen manuellen Reihenfolge
  vergrößern den Präfix, den ein einzelner Zug einfriert (ADR 0050). Bewusst
  getragen, in ADR 0053 begründet.
- **Versionsdrift.** `relay/` bindet `tl.html` per `include_str!` ein und wird
  bei jedem main-Merge deployt, der Host kommt über Release-Tags. Regel:
  **fehlendes Feld ⇒ altes Verhalten** — eine neue Seite an einem alten Host
  zeigt keine offenen Spiele und läuft normal weiter.
- **Rollback:** Ältere Version installierbar; das neue Config-Feld wird von
  alten Ständen ignoriert, die `config.json` bleibt lesbar.

## Offene Fragen / Annahmen

- **OF1 Freilose — beantwortet am 30.08.2026 (Schritt 0).** Gemessen am
  2-Hallen-Mitschnitt (Test `der_mitschnitt_sagt_wie_viele_spiele_mit_offenem_platz_btp_liefert`):
  36 Paarungen, davon **22 mit offenem Platz** — und **kein einziges** Spiel,
  das BTP mit offenem Platz bereits als entschieden oder auf dem Feld führt.
  Es entstehen also **keine Freilos-Dauerzeilen**; der zusätzliche Ausschluss
  in Schritt 3 entfällt.
  **Nebenbefund mit Folgen für die Erwartung:** Von 42 offenen Plätzen haben
  nur **8** ein auffindbares Vorspiel im selben Draw, **34** nicht (Setzplatz
  oder Speisung über Draw-Grenzen). In diesem Mini-Turnier — fünf Spieler,
  neun Auslosungen, KO aus Gruppen gespeist — wird also meist „noch offen"
  stehen und selten ein Kandidatenname. Der Anteil auflösbarer Plätze wächst
  mit der Größe der KO-Bäume; an einem echten Turnier ist er nachzumessen,
  bevor jemand aus der seltenen Kandidatenanzeige einen Fehler ableitet.
- **OF2 Deckelwert — beantwortet am 30.08.2026 (Schritt 5).** Am Host gilt
  `OPEN_QUEUE_LIMIT = 120`, dieselbe Grenze wie für die Warteliste. Über die
  Cloud entscheidet **keine feste Zahl**, sondern die Reihenfolge: Die
  bestehende Leiter schneidet zuerst unverändert die Arbeitsliste zu, danach
  füllt `offene_auffuellen` die offene Liste über `[40, 20, 10, 5]` so weit
  auf, wie das 64-KiB-Fenster hergibt — und lässt sie ganz weg, wenn nicht
  einmal fünf passen. Ein fester Wert wäre falsch: Im dokumentierten Worst
  Case liegt der Zustand schon ohne offene Spiele bei gut 55 KiB, während ein
  mittleres Turnier mühelos hundert Einträge trüge. Ein Turnier ohne offene
  Spiele kostet die neue Stufe **keine** zusätzliche Serialisierung.
- **OF3 Stiller No-Op — entschärft, nicht beseitigt.** `QueueReorder` antwortet
  heute auch dann `ok: true`, wenn `queue_order.rs:161` den Zug still verwirft.
  Nach G3 steht jedes angezeigte offene Spiel in `ready_queue`, und der
  getrennt zählende Sichtbarkeits-Deckel spannt bis dorthin — es bleibt nur der
  theoretische Fall, dass sich der Turnierstand zwischen Anzeige und Zug
  ändert. Ein ehrlicher Ablehnungspfad ist **nicht** Teil dieser Spec.
- **A1** Auflösungstiefe genau eine Ebene, keine Rekursion.
- **A2** Trennzeichen „ oder ", weil „/" das Doppelpaar trennt.
- **A9** Neue Feldnamen sind spezifisch (`open_slot1_label`), nie generisch wie
  „candidates" — der Wächter-Test arbeitet mit einer flachen Whitelist nach
  Feldnamen.

## Betroffene Doku-Dateien

- `docs/features/tl-offene-paarungen.md` (diese Spec)
- `docs/turnierleitung-web.md` — Bedienung: Schalter, Zustandsmarke,
  gesperrte Aktionen
- `docs/features/tl-web-panelsystem.md` — der neue Anzeige-Schalter
- `docs/features/spielliste-manuelle-reihenfolge.md` — offene Spiele nehmen an
  der globalen Reihenfolge teil (G3/ADR 0053)
- `docs/btp_protocol.md` — Feeder-Auflösung Slot→Match über
  `planning_id`/`draw_id`, Sieger/Verlierer-Semantik **und das Messergebnis zu
  OF1** (Freilos-Zeilen)
- `docs/features/tl-web-push.md` — Nachtrag zur Zustandsgröße: `open_queue` als
  letzter Zusatz und erster Opferkandidat im Cloud-Fenster
- `docs/cloud-relay.md` — **nur** der `hideOpenMatches`-Eintrag in der
  Profil-Wire-Tabelle; kein neuer TL-Frame
- `docs/preparation.md` — Einzeiler „offene Spiele erscheinen hier bewusst
  nicht" (verhindert die Rückfrage)
- `docs/regression-suite.md` — manuelle Prüfzeile: Schalter aus/an, Zeile
  ziehen, Aufruf-Knopf fehlt
- `CLAUDE.md` — neue Zeile in der Doku-Pflicht-Tabelle; Ergänzung im Abschnitt
  **Datenschutz** (Kandidatennamen bewusst freigegeben, **ohne**
  Lizenznummern — parallel zur Nation-/Verein-/Lizenz-Chronik)
- `docs/changelog.md` — zur veröffentlichten Version
- `docs/roadmap.md` — Verweis auf diese Spec

**Ein einziges neues `relay-proto`-Feld:** `TlDisplaySettingsWire.hide_open_matches`
(`#[serde(rename = "hideOpenMatches", default)]`). Der TL-Zustand selbst reist
als opakes JSON; `TlAction` bleibt geschlossen.

## Umsetzungs-Hinweise

Ergebnis der How-To-Phase. Vierzehn Schritte, jeder einzeln testbar und
mergefähig; TDD — Test zuerst.

| # | Schritt | Dateien |
|---|---|---|
| 0 | **Messung OF1** — liefert BTP Freilos-Zeilen mit `IsMatch=true`? **Vorbedingung**: Fällt die Antwort ungünstig aus, braucht Schritt 3 einen zusätzlichen Ausschluss. | `tests/btp_capture.rs`, ignorierte Sonde nach Muster `btp_displayorder_probe.rs` |
| 1 | `offener_platz_text(snap, m, seite)` als reine Funktion, noch von niemandem aufgerufen | `tl.rs`, neben `correction_blocker` |
| 2 | `TlOpenMatch` + `open_queue`/`open_queue_truncated` + `OPEN_QUEUE_LIMIT`, noch leer befüllt | `tl.rs` |
| 3 | `build_state_limited`: Filter auf `status == Scheduled` reduzieren, nach dem Sortierlauf in echte/offene partitionieren, `queue_index` mitzählen; der Prognoseblock iteriert **nur** über die echten | `tl.rs:3171-3404` |
| 4 | Beide Wächter mitziehen, Fixtures um ein offenes Spiel ergänzen samt `assert!` | `tl.rs:8501`, `:9256` |
| 5 | Adaptives Cloud-Budget in `state_for_relay`; `OPEN_QUEUE_LIMIT` hier messen und begründen | `tl.rs:810-925` |
| 6 | `ready_queue` nimmt offene Spiele auf; `reconcile_queue_order`-`keep` wird zu „alle `Scheduled`" | `assign.rs:188-215`, `sync.rs:1094-1103` |
| 7 | Sichtbarkeits-Deckel zählt echte und offene Einträge **getrennt** | `state.rs:1386-1439` |
| 8 | Cross-Site-Ausnahme + Test `offene_spiele_im_praefix_aendern_die_reihenfolge_der_echten_spiele_nicht` | `tests/queue_order_consistency.rs` |
| 9 | `CallPreparation` für offene Spiele **explizit** mit `TlErrorCode::NotAllowed` ablehnen (heute läuft der Aufruf durch und verschwindet still in `state.rs:3780`) | `tl.rs:287-316` |
| 10 | Geräteschalter `hide_open_matches`/`hideOpenMatches`, invertiert, `#[serde(default)]` | `config.rs:1060`, `relay-proto:1624`, `tl.rs:3660` |
| 11 | `mischeOffene(queue, openQueue)` + `offeneMarke(eintrag)` als testbares Modul mit Inline-Kopie — Muster `courtPatch.mjs`/`monitorSeq.mjs` | `src/io/offeneSpiele.mjs`, `scripts/test-offene-spiele.mjs` |
| 12 | `tl.html`: **erst mischen, dann nach Halle filtern**; offener Zweig in `queueRow` (Marke, Kandidatentext im `title`, kein Megafon/Vorabzettel/Punktverlauf, `pickable = false`, Griff und ↑ bleiben) | `assets/tl.html:1769, 4376, 6104, 6113, 7104, 7175` |
| 13 | Doku (siehe oben) + Versions-Bump, im selben Commit wie 12 | — |

**Nicht anfassen:** `queue_order.rs` (der Store kennt nur Match-IDs — nur der
Modulkommentar bekommt einen Zusatz), `commands.rs:2348`, `sync.rs:352/1041`,
`server.rs:1987`, `state.rs:3775`, `relay_client.rs:1064`, `assign.rs:641`,
`auto_assign` (G8/A3).

**Zwei Fallstricke aus der Recherche:**

- Die Draw-Bindung bei der Feeder-Suche ist **zwingend** — PlanningIDs
  kollidieren zwischen Draws; belegt durch den Bestandstest
  `slots_with_same_planning_id_in_different_draws_do_not_collide`.
- Der Schalter darf im Profil-Editor nur erscheinen, wenn
  `Array.isArray(state.open_queue)` — sonst steht an einem alten Host ein
  wirkungsloses Häkchen. Dieselbe Falle wie bei `can_lock_courts` und
  `can_set_wish_court`.

**Version:** gemeinsam auf **v0.9.273** (`src-tauri/Cargo.toml:3`,
`src-tauri/tauri.conf.json:4`, `package.json:4`), **einmalig** im letzten
Commit der Reihe — die Schritte 0–11 haben keine sichtbare Wirkung.
`scripts/check-version-tagged.mjs` prüft die Kopplung. Der Release-Tag ist
Admin-Sache (siehe `docs/release.md`).

**Reviews:** `code-reviewer` nach jeder Code-Änderung (Pflicht).
`security-reviewer` ist **nicht** zwingend — kein neuer User-Input, keine
Auth-, Datei- oder URL-Behandlung; der einzige neue Eingang ist ein `bool` im
bestehenden Profil-Wire. Der Datenschutz-Aspekt ist über die beiden
Wächter-Tests abgedeckt.

**Reihenfolge gegenüber Spur B** („Verschiebe-Modus", ADR 0050): Spur B ist
spezifiziert, aber **nicht in Arbeit** (kein Branch, `queueGap.mjs` existiert
nirgends) und fasst kein Rust an. Empfehlung: **diese Spec zuerst**, weil sie
Rust und Wire berührt; Spur B setzt danach auf der bereits gemischten Liste
auf. Kommt Spur B doch zuerst, muss `queueGap.mjs` von Anfang an ein
`offen`-Flag je Zeile kennen — sonst wird sein Datenvertrag zweimal
geschrieben.
