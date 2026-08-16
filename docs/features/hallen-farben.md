# Hallen-Farben — Spezifikation

> Status: **abgestimmt 2026-08-16** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Idee (Chat 2026-08-16). Betroffene Crates: src-tauri · relay · relay-proto · src.
> ADR: [0031](../adr/0031-hallen-farben-eigener-config-store.md) ·
> [0032](../adr/0032-hallen-farben-deterministische-auto-palette.md) ·
> [0033](../adr/0033-hallen-farben-hex-auf-dem-draht.md).

## Kontext / Problem

Bei Mehr-Hallen-Turnieren wird die Halle heute überall nur als Text genannt
(Kürzel an Wartelisten-Zeilen, Gruppen-Überschriften, „Halle 2 · 6" auf
Monitoren). Helfer und Turnierleitung müssen jedes Mal lesen und zuordnen —
gerade bei ähnlichen Hallennamen ist das langsamer und fehleranfälliger als
ein Farbcode. Spieler am badhub-Aushang haben dasselbe Problem in der
Gegenrichtung: „In welche Halle muss ich?"

## Zielbild & Erfolgskriterien

Jede Halle eines Mehr-Hallen-Turniers trägt eine Farbe, die **überall
identisch** neben der Hallen-Nennung erscheint — als kleine Farbmarke
(Streifen/Punkt), nie als Ersatz für Kürzel oder Name.

- **Auto-Palette:** bts-light vergibt ohne Zutun ~10 kuratierte, auf hellem
  und dunklem Grund lesbare Töne, deterministisch über die alphabetisch
  sortierte Hallenliste (ADR 0032).
- **Übersteuerung:** Die Turnierleitung wählt je Halle per Klick einen
  anderen Palettenton auf der Felderübersicht; „Automatisch" setzt zurück.
- **Erfolgskriterium (Feldtest):** Bei ≥ 2 Hallen benennt ein Helfer ohne
  Einweisung die Ziel-Halle eines Wartelisten-Eintrags anhand der Farbe
  korrekt; es gibt keine „Farben stimmen nicht überein"-Rückfragen
  zwischen Desktop, TL-Web, Monitoren und Aushang.

## Nicht-Ziele

- **Kein** Farbbezug aus BTP oder badhub — Quelle der Wahrheit ist allein
  die Client-Config (`hall_colors`).
- **Keine** Farben bei Ein-Hallen-Turnieren (Feature strukturell
  unsichtbar, Gate `is_multi_hall()`).
- **Kein** freier Color-Picker und keine neue Dependency — nur die feste
  Palette.
- **Nicht** in dieser Etappe: Sprachansagen, Punktverlauf-Graph,
  Schirizettel-PDF, QR-/Geräte-Listen im SetupWizard.
- **Tablet-Spielzettel: entfällt.** Grill-Entscheid „nur wenn gratis" —
  die Farbe reist auf keinem bestehenden Weg zum Tablet-Kopf (LAN-Template,
  Relay-Render und `MatchAssigned` wären drei neue Verdrahtungen); die
  Halle steht dort ohnehin lesbar im `COURT_LABEL`.
- **Kein** badhub-Repo-Code in dieser Spec — nur das Push-Feld auf
  bts-light-Seite; die badhub-Anzeige ist ein Folge-PR im badhub-Repo.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - `src-tauri/src/config.rs`: `HallColorConfig { hall, color }`,
    `AppConfig.hall_colors` (`#[serde(default)]`), `upsert_hall_color`
    (Trim + case-insensitiver Ersatz + Validierung „Ton ∈ Palette"),
    `remove_hall_color` — Muster `hall_layouts` (ADR 0031).
  - `src-tauri/src/hall_colors.rs` (neu): Palette `HALL_PALETTE`
    (10 Hex-Töne, Rot/Grün/Violett der Zustandsfarben ausgespart),
    Resolver `effective_hall_colors` (ADR 0032), `paint`-Helfer für
    `CourtOverview`.
  - `src-tauri/src/commands.rs`: Commands `set_hall_color`,
    `remove_hall_color`, `hall_colors_view` (Palette + effektive Farbe +
    Override-Flag je Halle); `keep_host_managed_fields` konserviert
    `hall_colors`; `apply_imported_identity` behält den Stand bei leerem
    Bundle.
  - `src-tauri/src/tablet/state.rs`: `CourtOverview.hall_color`.
  - `src-tauri/src/tablet/tl.rs`: `TlHall.color` (nur multi_hall),
    `TlFinished.hall` (neu, aus court_id → Location; leer bei
    Papier-Ergebnis), Allowlist-Pflege.
  - `src-tauri/assets/tl.html`: Farbmarke an Hallenwähler,
    Hallen-Gruppen-Überschriften, `hall-kuerzel`-Chip (inkl. „·A") —
    Marke **neben** dem Chip, die Chip-Optik und die
    Abzeichen-Dringlichkeitsskala bleiben unangetastet —, Beendet-Zeilen,
    Hallenwähler im ⋮-Menü.
  - `src/pages/FieldOverviewPage.tsx`: Paletten-Swatches im bestehenden
    Hallen-Editor-Popover; Marke an Gruppenköpfen.
    `src/pages/PreparationPanel.tsx`: Marke neben Hallen-Kürzel.
    `src/api.ts`: Wrapper.
  - `relay-proto/src/lib.rs`: optionale Felder `CourtBrief.hall_color`,
    `PreparedMatch.hall_color`, `MonitorState.hall_color`
    (`#[serde(default, skip_serializing_if)]`, Hex-String, ADR 0033).
  - `src-tauri/src/tablet/relay_client.rs` (Befüllung), `server.rs`/
    `monitor.rs` (LAN-Routen), `relay/src/main.rs` (Cloud-Routen aus dem
    CourtBrief-Cache), `assets/{overview,preparation,monitor}.html`
    (Kopfleisten-Marke + `row__hall`).
  - `src-tauri/src/badhub/payload.rs`: `TsetCourt.hall_color` +
    `TsetMatch.hall_color` (nur `upcoming_matches`) für display=monitor
    und display=next.
  - `slave_bridge.rs`: keine Pflicht-Änderung (Feld reist per
    Serde-Default durch).
- **Architekturregeln:** R1 — React spricht nur die neuen Tauri-Commands;
  R2 — Farben sind reine Anzeige, keine Court→Match-Wahrheit, BTP bleibt
  unberührt; R3 — LAN-Routen und Cloud-Relay tragen dieselben optionalen
  Felder, Relay-Deploy vor App-Release; R5/R6 unberührt.
- **Konfiguration & Abwärtskompatibilität:** Neues Feld `hall_colors` mit
  Serde-Default (alte Configs laden, Auto-Palette greift). Downgrade:
  ältere App ignoriert das Feld; beim nächsten Speichern gehen nur
  Farb-Overrides verloren, keine Betriebsdaten. `identifier` und
  Updater-Pfad unangetastet. Farb-Overrides hängen am getrimmten,
  case-insensitiv verglichenen Hallennamen und überleben den
  Turnierwechsel (gleiche Halle = gleiche Farbe, gewollt).
- **Datenschutz:** Farben tragen keinen Personenbezug. Neue
  `TlState`-Felder (`TlHall.color`, `TlFinished.hall`) werden bewusst in
  die Allowlist von `every_published_field_is_deliberately_allowed`
  aufgenommen; der Wächter
  `the_state_never_carries_personal_data_beyond_its_purpose` bleibt
  unverändert grün.
- **Abhängigkeiten:** Keine neue Cargo-/npm-Dependency. badhub-Seite:
  alte Aushang-Seiten ignorieren die unbekannten Felder — **bts-light
  released zuerst**, der badhub-PR (Anzeige der Farbe in display=next und
  display=monitor) folgt mit Kollegen-Deploy.

## Akzeptanzkriterien

- [ ] Bei einem Turnier mit ≥ 2 Hallen zeigen Felderübersicht,
  Vorbereitungs-Panel, TL-Web (Hallenwähler, Feld-Gruppen, Wartelisten-
  und Beendet-Zeilen inkl. „·A"), overview/preparation/monitor-Seiten
  (LAN **und** Cloud) sowie der badhub-`tset` je Halle **dieselbe** Farbe.
- [ ] Ohne jede Pflege bekommt jede Halle automatisch einen Ton der
  Palette; die Zuordnung ist nach App-/BTP-Neustart und Snapshot-Neuladen
  identisch (alphabetische Vergabe, ADR 0032).
- [ ] Klick auf einen Palettenton im Hallen-Editor der Felderübersicht
  übersteuert die Farbe dieser Halle sofort in allen Ansichten;
  „Automatisch" entfernt den Override. Beides überlebt App-Neustart.
- [ ] `upsert_hall_color` lehnt Werte außerhalb der Palette ab
  (Fehlerfall mit deutscher Meldung); Hallennamen werden getrimmt und
  case-insensitiv abgeglichen („Halle 1 " ersetzt „halle 1").
- [ ] Ein-Hallen-Turnier: nirgendwo eine Farbmarke, `tset` trägt keine
  `hall_color`-Felder.
- [ ] Farbe ist nie einziger Informationsträger: Kürzel/Name stehen an
  jeder Stelle weiterhin als Text (Farbfehlsichtigkeit).
- [ ] Versions-Mix degradiert farblos statt kaputt: alter Host + neuer
  Relay, neuer Host + alter Relay, alte Monitor-/badhub-Seite — alle
  Ansichten funktionieren ohne Farbe weiter (`#[serde(default)]`).
- [ ] SetupWizard-Speichern löscht keine Farb-Overrides
  (`keep_host_managed_fields`); ein Identitäts-Import mit leerem Bundle
  ebenfalls nicht.
- [ ] Die Beendet-Zeilen in TL-Web tragen Hallen-Kürzel + Marke; bei
  Papier-Ergebnissen ohne Feld bleibt die Zeile ohne Halle und ohne Marke.
- [ ] Zustandsfarben und Abzeichen-Skala sind unverändert: kein
  Palettenton liegt im Rot-/Grün-/Violett-Bereich der Feldzustände, die
  Marke ist ein eigenes Element neben Chip/Streifen.

## Tests

TDD je Etappe (Auswahl, vollständige Liste im How-To):

- Config: `hall_colors_survive_a_config_roundtrip_and_default_empty`,
  `upsert_hall_color_trims_and_replaces_case_insensitive`,
  `upsert_hall_color_rejects_a_tone_outside_the_palette`,
  `the_wizard_cannot_wipe_the_hall_colors`,
  `apply_imported_identity_keeps_hall_colors_when_bundle_has_none`.
- Resolver: `auto_palette_is_assigned_alphabetically_regardless_of_snapshot_order`,
  `effective_colors_prefer_the_persisted_override`,
  `single_hall_tournament_gets_no_colors`,
  `palette_avoids_state_color_hues`.
- TL-Web: `the_state_carries_hall_colors_only_for_multi_hall`,
  `tl_hall_color_defaults_to_none_for_old_hosts`,
  `finished_rows_carry_their_hall_name` + Allowlist-Pflege.
- Wire: `court_brief_hall_color_roundtrips_and_defaults_none`,
  `prepared_match_hall_color_defaults_none`,
  `monitor_state_hall_color_defaults_none`,
  `cloud_overview_health_carries_hall_colors`,
  `cloud_preparation_state_carries_the_call_hall_color`,
  `cloud_monitor_state_inherits_hall_color_from_the_court_list`.
- badhub: `tset_courts_carry_their_hall_color_in_multi_hall`,
  `tset_upcoming_matches_carry_the_hall_color_of_their_call`,
  `tset_omits_hall_color_for_single_hall_tournaments`.
- `cargo test --workspace` grün, `cargo fmt --check` sauber,
  `npm run build` fehlerfrei. Manueller Testfall: 2-Hallen-Turnier gegen
  laufendes BTP — Farben in allen Ansichten identisch, Override + Reset.

## Risiken & Rollback

- Reine Anzeige: Schlimmster Fehlerfall ist eine fehlende/falsche Marke,
  nie ein falsches Spiel oder Feld (R2 unberührt).
- Degradation immer „farblos", nie „informationslos" — Kürzel bleibt.
- Rollback: Override per „Automatisch" je Halle; App-Downgrade lädt die
  Config weiter (Serde ignoriert `hall_colors`).
- Farbdopplung per Override ist sichtbar und bewusst erlaubt (ADR 0032).

## Offene Fragen / Annahmen

- Annahme: ~10 Palettentöne reichen — mehr als 10 Hallen bekommen Töne
  doppelt (`i % 10`); real sind 2–4 Hallen.
- Annahme: badhub-Parser ignoriert unbekannte `tset`-Felder (bisheriges
  Verhalten bei Schema-Erweiterungen, z. B. `preparation_call_ts`).

## Betroffene Doku-Dateien

`docs/multi-hall.md` (Konzept + Bedienung Felderübersicht),
`docs/turnierleitung-web.md`, `docs/features/turnierleitung-web.md`
(Verweis), `docs/court-monitor.md`, `docs/cloud-relay.md` (Wire-Felder),
`docs/preparation.md` + `docs/spieler-check-in.md` (Push-Schema),
`docs/changelog.md`, CLAUDE.md-Tabelle (neue Zeile „Hallen-Farben").

## Umsetzungs-Hinweise

Sechs Etappen (Details in `_intake/hallen-farben/3-how-to.md`; code-reviewer
nach jeder Etappe, kein neuer User-Input/Auth → kein security-reviewer
nötig):

1. **Config-Kern + Resolver** (config.rs, hall_colors.rs,
   keep_host_managed_fields, apply_imported_identity) — rein Rust,
   verhaltensneutral.
2. **Desktop-App** (Commands, `CourtOverview.hall_color` + `paint`,
   Swatch-Picker in FieldOverviewPage, PreparationPanel-Marke).
3. **TL-Web** (`TlHall.color`, `TlFinished.hall`, tl.html-Marken).
4. **Cloud-Weg** (relay-proto-Felder, Host-Befüllung, Relay-Ausgabe,
   Monitor-Seiten) — **Relay-Deploy vor App-Release** (läuft automatisch
   beim main-Merge).
5. **badhub-Push** (payload.rs-Felder; badhub-Repo-PR separat danach).
6. **Release:** Version-Bump gemeinsam auf **0.9.210**
   (`src-tauri/Cargo.toml` + `src-tauri/tauri.conf.json` +
   `package.json`); Reihenfolge Merge → Relay-Auto-Deploy → Tag →
   badhub-Kollegen-Deploy.
