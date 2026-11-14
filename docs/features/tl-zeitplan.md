# Zeitplan im TL-Web — Spezifikation

> Status: **abgestimmt 2026-11-14** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Idee vom 14.11.2026. Betroffene Crates: src-tauri, relay-proto, relay (nur Neubau).
> ADR: [0094](../adr/0094-zeitplan-stand-prognose-gegen-plan.md).

## Kontext / Problem

Die Turnierleitung sieht im TL-Web heute nicht, ob das Turnier im Zeitplan liegt. BTP führt je
Spiel eine geplante Zeit (`PlannedTime`); bts-light liest sie, sortiert danach und schickt sie an
den Liveticker, zeigt sie in der Turnierleitungs-Oberfläche aber nirgends. Wer wissen will, ob er
hinterherhinkt, muss in BTP nachsehen und im Kopf rechnen.

Zwei Eigenheiten bestimmen die Lösung:

- **Planzeiten sind Zeitfenster.** In BTP tragen alle Spiele eines Fensters dieselbe Zeit — eine
  ganze Runde steht auf 9:00 (`docs/btp_protocol.md`, „Alle Spiele eines Zeitfensters tragen
  dieselbe Zeit"). Ein Vergleich „Planzeit des nächsten Spiels gegen die Uhr" meldete mitten im
  Fenster Stunden Rückstand, obwohl alles nach Plan läuft.
- **Die oberste Zeile der Warteliste ist nicht das planmäßig nächste Spiel.** Gerufene und von
  Hand vorgezogene Spiele stehen oben, Spiele späterer Tage stehen in derselben Liste.

## Zielbild & Erfolgskriterien

1. **Geplante Zeiten anzeigen** (Profil-Schalter, Standard aus): In der Warteliste steht an jeder
   Zeile die Planzeit, an beendeten Spielen „geplant 10:20 · +18 min" (Planzeit und Abstand
   zum tatsächlichen Start).
2. **Panel „Zeitplan"** (standardmäßig nicht eingeblendet): zeigt groß „Im Zeitplan" oder
   „N min dahinter" / „N min davor".

Erfolg heißt: Die Turnierleitung liest den Stand ohne Erklärung ab, und die Kachel meldet an
einem normal laufenden Turnier mit Zeitfenstern keinen Rückstand, den es nicht gibt.

### Was die Kachel misst

Für das **nächste noch nicht begonnene Zeitfenster von heute**: voraussichtlicher Start
(vorhandene Prognose) minus Planzeit.

- **Zeitfenster** = alle Spiele des Turniers mit derselben Planzeit, deren Datum das heutige
  Datum des Turnier-PCs ist.
- **Begonnen** ist ein Fenster, sobald **zwei** seiner Spiele gerufen, auf dem Feld oder beendet
  sind (Walkover zählt nicht). Hat das Fenster nur ein Spiel, genügt dieses eine. Ein einzeln
  vorgezogenes Spiel verschiebt die Messung also nicht.
- **Bezugswert** = die früheste Prognose unter den wartenden, noch nicht gerufenen Spielen des
  frühesten unbegonnenen Fensters. Bewusst das Minimum, nicht das erste der Liste: Ist das erste
  Spiel blockiert, weil ein Spieler noch spielt, läuft das Fenster trotzdem pünktlich an.
- **Abweichung** `m` = `trunc((max(Prognose, jetzt) − Planzeit) / 60 000 ms)`; positiv heißt
  später als geplant. „Jetzt" ist die Uhr des Turnier-PCs (`serverNow()`), nicht die des Geräts.
- **Anzeige:** `|m| ≤ 5` → „Im Zeitplan"; `m ≥ 6` → „m min dahinter"; `m ≤ −6` → „|m| min
  davor". Die Toleranz ist fest. Vorsprung wird gemeldet, auch in einer Pause mit freien Feldern
  („50 min davor" = die nächste Runde könnte vorgezogen werden).
- **Nebenzeile** bei einem Wert: „Nächstes Zeitfenster 11:00 · voraussichtlich 11:18", mit
  „ca." vor der Uhrzeit, wenn die Prognose nur auf Startwerten beruht.

### Zustände ohne Wert

| Lage | Text |
|---|---|
| Kein Spiel im Turnier hat eine Planzeit | „Keine geplanten Zeiten in BTP" |
| Heute steht kein Spiel mehr aus (auch: nur noch Spiele späterer Tage) | „Heute keine Spiele mehr" |
| Es warten heute noch Spiele, aber jedes Fenster ist begonnen | „Letzte Runde läuft" |
| Spielzeiten-Prognose ist ausgeschaltet | „Prognose ist ausgeschaltet" |
| Im Bezugsfenster hat kein Spiel eine Prognose (z. B. nur offene Paarungen) | „Noch keine Prognose", Nebenzeile „Nächstes Zeitfenster 15:00" |
| Heute hat noch kein Spiel begonnen und es ist höchstens 5 min nach der Planzeit des Bezugsfensters | „Beginn 09:00" |

Ohne Prognose gibt es kein Ersatzmaß und keinen Sprung zum übernächsten Fenster.

### Geplante Zeit an den Zeilen

- Warteliste: an wartenden, gerufenen und offenen Paarungen, in der Meta-Zeile als
  „geplant 10:20". Spiele eines anderen Tages mit Wochentag („geplant So 09:00"). Ohne Planzeit
  steht nichts.
- Beendete Spiele: „geplant 10:20 · +18 min", „· −4 min", „· ±0 min". Die Abweichung ist
  `trunc((tatsächlicher Start − Planzeit) / 60 000)`; sie entfällt bei Walkover, ohne gemessenen
  Start, ohne Planzeit und bei mehr als 720 min Betrag — dann steht nur „geplant 10:20".
- Nie an den Feldkacheln.
- **Ein** Schalter „Geplante Zeiten anzeigen" für beide Stellen.

## Nicht-Ziele

- Planzeit an laufenden Spielen (Feldkacheln), in der Desktop-App, auf Spielfolge-TV,
  Meetingpoint, Liveticker oder in Suchtreffern.
- Einstellbare Toleranz; Wert je Halle (bewusste Vereinfachung: bei Hallen mit eigenen
  Zeitplänen kann eine Halle voraus und die andere zurück sein).
- Verlauf des Rückstands über den Tag.
- Schreiben von Planzeiten nach BTP.
- Prognose für offene Paarungen (denkbare Folge-Etappe an `predict.rs`).

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - `src-tauri/src/btp/model.rs` — `planned_time_unix_ms` (zieht aus `badhub/payload.rs` um).
  - `src-tauri/src/tablet/zeitplan.rs` (neu, rein) — `lage`, `start_abweichung_min`.
  - `src-tauri/src/tablet/tl.rs` — `TlSchedule`, `TlState.schedule`, `TlFinished.planned_time` /
    `start_delay_mins`, Nullen in `state_fingerprint`, Schalter-Übersetzung, Positivliste.
  - `src-tauri/src/config.rs` — `TlDisplaySettings.show_planned_times`.
  - `relay-proto` — `TlDisplaySettingsWire.show_planned_times` (`showPlannedTimes`).
  - `relay/` — nur Neubau und Deploy.
  - `src-tauri/assets/tl.html` — Schalter, Zeilen, Panel `zeitplan`, `PANEL_STANDARD_AUS`.
  - `src/io/zeitplan.mjs` + `scripts/test-zeitplan.mjs` + Inline-Kopie in `tl.html`.
- **Architekturregeln:** R2 — Planzeiten werden nur gelesen. R3 — der Stand entsteht am Host,
  **vor** der Cloud-Kürzung der Warteliste auf 40 Zeilen, damit LAN und Cloud dasselbe zeigen.
  R1/R4/R5/R6 unberührt.
- **Daten im TL-Zustand:**

  | Feld | Typ | Bedeutung |
  |---|---|---|
  | `today` | u32 | Datum des Turnier-PCs, YYYYMMDD (oberste Ebene, schon mit PR 1: die Seite braucht es für den Wochentag) |
  | `schedule.kind` | String | `no_plan`, `done_today`, `last_round`, `off`, `no_forecast`, `ok` |
  | `schedule.started` | bool | heute ist mindestens ein Spiel gerufen, auf dem Feld oder beendet |
  | `schedule.planned_time` | Option&lt;i64&gt; | Planzeit des Bezugsfensters, roh |
  | `schedule.planned_ms` | Option&lt;u64&gt; | dieselbe als Unix-ms |
  | `schedule.predicted_start_ms` | Option&lt;u64&gt; | früheste Prognose des Fensters |
  | `schedule.predicted_uncertain` | bool | Prognose nur aus Startwerten |
  | `finished[].planned_time` | Option&lt;i64&gt; | Planzeit, roh |
  | `finished[].start_delay_mins` | Option&lt;i64&gt; | Abweichung des tatsächlichen Starts |

  Alle Felder additiv mit `serde(default)`; `Option`-Felder mit `skip_serializing_if`.
  `schedule` ist `None` nur ohne Snapshot. Keine Match-ID im Objekt (das Minimum kann zwischen
  zwei Spielen desselben Fensters wechseln und würde die Revision unruhig machen).
- **Host-Regel für `kind`,** in dieser Reihenfolge:
  1. Kein Spiel hat eine Planzeit → `no_plan`.
  2. H = Spiele mit heutiger Planzeit. W = H mit Status „angesetzt" und ohne Aufruf.
     `started` = in H ist ein Spiel gerufen, auf dem Feld oder (ohne Walkover) beendet.
  3. W leer → `last_round`, wenn in H noch ein Spiel gerufen ist oder auf dem Feld steht; sonst
     `done_today`.
  4. T = kleinste Planzeit in W, deren Fenster nicht begonnen ist. Keines → `last_round`.
  5. `planned_time` = T, `planned_ms` aus der Umrechnung.
  6. Prognose ausgeschaltet → `off`.
  7. Kein wartendes, nicht gerufenes Spiel des Fensters hat eine Prognose, oder `planned_ms`
     fehlt (mehrdeutige Wandzeit bei der Zeitumstellung) → `no_forecast`.
  8. Sonst `ok` mit dem Minimum der Prognosen; `predicted_uncertain` vom selben Spiel, bei
     Gleichstand vom ersten in Listenreihenfolge.
- **Revision:** `predicted_start_ms` und `predicted_uncertain` in `schedule` werden im
  Fingerabdruck genullt wie die Prognose der Zeilen. Reiner Zeitablauf bewegt die Revision nicht;
  die Seite führt die Kachel selbst im Uhr-Takt nach und klemmt eine alte Prognose auf „jetzt".
- **Panelsystem:** neuer Schlüssel `zeitplan`. Neue Regel nur auf der Seite: Schlüssel in
  `PANEL_STANDARD_AUS` werden, wenn sie in einem Profil fehlen, als **unsichtbar** ergänzt (auch
  im eingebauten Standardprofil). Der Host kennt weiterhin keine Panel-Schlüssel, es gibt keine
  Migration. Das Panel ist nur wählbar, wenn der Host `schedule` liefert.
- **Konfiguration & Abwärtskompatibilität:** einziges neues Config-Feld ist der Profil-Schalter
  (fehlt = aus). Bestandsprofile bleiben unverändert lesbar. Kein Standardwert in den
  Setup-Einstellungen. `identifier` und Updater-Pfad unberührt.
- **Datenschutz:** keine neuen personenbezogenen Daten — nur Zeiten. Der Wächter
  `the_state_never_carries_personal_data_beyond_its_purpose` wird um die neuen Felder in der
  Positivliste ergänzt.
- **Abhängigkeiten:** Spielzeiten-Prognose (`config.prediction`) für die Kachel; gemessener Start
  (`match-times.json`) für die Abweichung an beendeten Spielen. **Relay vor Client:** Das Relay
  reicht Profil-Aktionen typisiert durch; ein altes Relay verliert das neue Häkchen beim
  Speichern in der Cloud. Keine neue Cargo-/npm-Dependency.

## Akzeptanzkriterien

Schalter und Zeilen (PR 1):

- [ ] Schalter aus — auch in einem Bestandsprofil ohne das Feld —: Warteliste und beendete
      Spiele sehen aus wie vor dem Update.
- [ ] Schalter an: wartende, gerufene und offene Zeilen zeigen „geplant HH:MM"; ein Spiel von
      morgen zeigt den Wochentag; ein Spiel ohne Planzeit zeigt nichts; Feldkacheln zeigen nie
      eine Planzeit.
- [ ] Beendetes, gemessenes Spiel: „geplant 10:20 · +18 min"; früher gestartet „−4 min";
      Walkover oder ungemessen nur „geplant 10:20".
- [ ] Das Häkchen erscheint nur, wenn der Host das Feld kennt; in der Cloud bleibt es nach dem
      Speichern haften (neues Relay).
- [ ] Ein TL-Gerät mit anderer Zeitzone zeigt dieselben Uhrzeiten wie der Turnier-PC.

Panel (PR 2):

- [ ] Nach dem Update zeigt kein Bestandsprofil und nicht das Standardprofil das Panel
      „Zeitplan"; im Profil-Editor lässt es sich einschalten und bleibt über ein Neuladen sichtbar.
      Alle übrigen Panels verhalten sich wie bisher.
- [ ] Fenster 11:00 unbegonnen: früheste Prognose 11:04 → „Im Zeitplan"; 11:05:59 → „Im
      Zeitplan"; 11:06:00 → „6 min dahinter"; 10:54:00 → „6 min davor"; 10:54:01 → „Im Zeitplan".
- [ ] Acht Felder, sechzehn Spiele im laufenden 10:00-Fenster: gemessen wird am nächsten
      Fenster, nicht an den wartenden Resten von 10:00.
- [ ] Ein einzelnes vorgezogenes Spiel aus dem 14:00-Fenster: gemessen wird weiter am
      14:00-Fenster; ab dem zweiten gilt es als begonnen. Ein Fenster mit nur einem Spiel gilt mit
      diesem einen als begonnen.
- [ ] Erstes Spiel des Fensters blockiert, ein anderes prognostiziert pünktlich → „Im Zeitplan".
- [ ] Pause mit freien Feldern, nächstes Fenster 14:00, jetzt 13:10, Prognose „sofort" →
      „50 min davor".
- [ ] Unveränderter Zustand, die Uhr läuft weiter: Der Rückstand wächst mit und springt nie
      zurück; die Revision des Zustands ändert sich durch Zeitablauf nicht.
- [ ] TL-Gerät mit zwei Stunden falscher Uhr zeigt denselben Wert wie ein richtig gestelltes.
- [ ] LAN und Cloud zeigen bei 60 wartenden Spielen (Bezugsfenster hinter Zeile 40) denselben Wert.
- [ ] Jeder Zustand der Tabelle „Zustände ohne Wert" erscheint mit seinem Text; vor dem ersten
      Spiel „Beginn 09:00", ab 09:06 ohne Start „6 min dahinter" (bzw. der neutrale Text, wenn
      es keine Prognose gibt).
- [ ] Spiele späterer Tage zählen nie mit; nach dem letzten Spiel des Tages steht „Heute keine
      Spiele mehr".
- [ ] Alter Host ohne `schedule`: Das Panel fehlt, auch im Editor, ohne Fehler. Alter
      Browser-Stand gegen neuen Host: unverändert.
- [ ] BTP-Verbindung weg: Die Kachel rechnet mit dem letzten Stand weiter (wie die übrige Seite).
- [ ] Der Zustand bleibt unter 64 KiB (bestehende Größentests grün).

## Tests

Rust (`cargo test`, `clippy --workspace --all-targets -D warnings`):

- `btp/model.rs`: `planned_time_unix_ms_rechnet_wandzeit` (zieht mit um); der bestehende
  `build_sched_rechnet_planned_time_in_unix_ms` bleibt grün.
- `relay-proto`: `display_settings_planned_times_roundtrip_and_default` (Profil ohne Feld → aus).
- `config.rs`: Standard- und Roundtrip-Test; `tl.rs`:
  `profil_schalter_geplante_zeiten_reist_in_beide_richtungen`.
- `zeitplan.rs::start_abweichung_min`: `abweichung_spaeter_gestartet`,
  `abweichung_frueher_schneidet_zur_null`, `abweichung_fehlt_bei_walkover`,
  `abweichung_fehlt_ohne_stempel`, `abweichung_fehlt_ueber_zwoelf_stunden`.
- `tl.rs`: `beendetes_spiel_traegt_planzeit_und_abweichung`,
  `beendetes_spiel_ohne_planzeit_traegt_nichts`; Positivliste und Datenschutz-Wächter ergänzt.
- `zeitplan.rs::lage` (mit `heute` als Parameter, unabhängig von der Zone des Testrechners):
  `keine_planzeiten`, `heute_keine_spiele_mehr`, `letzte_runde_laeuft`,
  `ein_vorgezogenes_spiel_beginnt_kein_fenster`, `zwei_spiele_beginnen_das_fenster`,
  `fenster_mit_einem_spiel_beginnt_mit_diesem`, `walkover_beginnt_kein_fenster`,
  `fruehestes_unbegonnenes_fenster_gewinnt`, `minimum_der_prognosen_nicht_das_erste_der_liste`,
  `nur_offene_paarungen_ergibt_no_forecast`, `prognose_aus`, `spiele_anderer_tage_zaehlen_nicht`,
  `vor_dem_ersten_spiel_started_false`.
- `tl.rs`: `der_zustand_traegt_den_zeitplan`, `zeitplan_ist_im_lan_und_relay_stand_gleich`,
  `der_zeitplan_bewegt_den_fingerprint_nicht`, `tageswechsel_bewegt_die_revision_einmal`.

JavaScript (`scripts/test-zeitplan.mjs`, eigener CI-Schritt nach dem Muster von
`scripts/test-zuletzt-gespielt.mjs`, inklusive Drift-Prüfung der Inline-Kopie in `tl.html`):

- `planZeitText`: heute, anderer Tag mit Wochentag, ohne `today`, ohne Planzeit.
- `startAbweichungText`: plus, minus, null, fehlend.
- `zeitplanLage`: Grenzen 5:59 / 6:00 in beide Richtungen, Klemme auf „jetzt" bei alter
  Prognose, „Beginn HH:MM" vor und nach der Toleranz, alle neutralen Zustände, fehlendes oder
  kaputtes Objekt.
- `normalizedPanels` (ausgeschnitten): Bestandsprofil ohne `zeitplan` → unsichtbar; Profil mit
  ausgeschriebenem `visible: true` bleibt sichtbar; übrige Schlüssel weiterhin sichtbar ergänzt.

`npm run build` fehlerfrei. Manuell: Testturnier mit Zeitfenstern, LAN und Cloud nebeneinander.

## Risiken & Rollback

- **Relay-Reihenfolge:** Mit altem Relay verschwindet das Häkchen in der Cloud still nach dem
  Speichern. Das Relay rollt beim Merge automatisch aus; der App-Tag folgt danach.
- **Uhr oder Zeitzone des Turnier-PCs falsch:** alle Werte verschieben sich (gilt heute schon
  für den Liveticker-Spielplan).
- **Ungepflegte oder sehr grobe Planzeiten** (ein ganzer Tag auf 9:00): Die Kachel steht lange
  auf „Letzte Runde läuft". Das ist richtig, aber wenig hilfreich; das Handbuch sagt es.
- **bts-light mitten im Turniertag eingerichtet:** Für die dann laufenden Spiele gilt der
  Moment der Einrichtung als Start; ihre Abweichung an der Beendet-Zeile ist zu groß. Nicht
  erkennbar, im Handbuch benannt.
- **Liegengebliebenes Spiel:** Wartet ein Spiel eines frühen, noch nicht begonnenen Fensters
  dauerhaft (Spieler fehlt, von der Vergabe ausgenommen), misst die Kachel weiter an diesem
  Fenster und der Rückstand wächst, bis das Spiel entschieden oder umgesetzt ist. Im Handbuch
  benannt.
- **Turnier über Mitternacht:** Spiele mit Planzeit von gestern zählen nicht mehr als „heute".
- **Endphase:** häufig „Noch keine Prognose", weil die Paarungen des nächsten Fensters offen sind.
- **Bezugsfenster hinter Platz 120 der spielbereiten Liste:** ebenfalls „Noch keine Prognose".
- **Vertrauen:** Eine große Kachel mit falschem Wert schadet der ganzen Oberfläche — deshalb
  die Fensterlogik, die Zwei-Spiele-Regel und Standard „aus".
- **Rollback:** Häkchen aus, Panel ausblenden. Alle Felder sind additiv; eine ältere Version
  liest Profile und Zustand weiter.

## Offene Fragen / Annahmen

Keine offenen Fragen. Vom Nutzer am 14.11.2026 bestätigte Annahmen: ein Schalter für beide
Stellen; Planzeit auch an gerufenen und offenen Zeilen; neutrale Zustände statt Ersatzmaß;
„davor" wird gemeldet; Berechnung am Host.

## Betroffene Doku-Dateien

- `docs/turnierleitung-web.md` (öffentliches Handbuch): Abschnitte Panels und Profile, neuer
  Abschnitt „Zeitplan", klar abgegrenzt vom Panel „Anfangszeiten". Anker `<!-- pruef: -->` für
  „Geplante Zeiten anzeigen", „Zeitplan", „Im Zeitplan", „Letzte Runde läuft", „Heute keine
  Spiele mehr", „Keine geplanten Zeiten in BTP" gegen `src-tauri/assets/tl.html`.
- `docs/features/tl-web-panelsystem.md` (Regel „standardmäßig unsichtbar").
- `docs/spielzeiten-prognose.md` und `docs/features/spielzeiten-prognose.md` (Verweis; für
  welche Spiele es eine Prognose gibt).
- `docs/cloud-relay.md` (`showPlannedTimes`, `schedule` im Zustand, Relay vor Client).
- `docs/btp_protocol.md` (geteilte Umrechnung der Planzeit).
- `docs/adr/0094-…` + `docs/adr/README.md`, `docs/roadmap.md`, `docs/changelog.md`.
- `CLAUDE.md`: neue Tabellenzeile „Zeitplan im TL-Web".

## Umsetzungs-Hinweise

Zwei PRs; die Versionsnummer wird je PR erst beim Merge festgelegt und in
`src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` und `package.json` gemeinsam gesetzt.
`code-reviewer` je PR; `security-reviewer` nicht nötig (einzige neue Eingabe ist ein Bool im
Profil).

**PR 1 — „Geplante Zeiten anzeigen"** (Relay-Deploy vor dem App-Tag)

1. `planned_time_to_unix_ms` aus `badhub/payload.rs` nach `btp/model.rs` als
   `pub(crate) fn planned_time_unix_ms` umziehen, Aufrufer und Test anpassen.
2. `relay-proto`: Wire-Feld mit Test.
3. `config.rs`: Feld mit Tests; Übersetzung in `tl.rs` in beide Richtungen.
4. `tablet/zeitplan.rs` mit `start_abweichung_min` (Tests zuerst).
5. `TlFinished` füllen (tatsächlicher Start aus `match_times`), Positivliste, Wächter,
   Größentests.
6. `src/io/zeitplan.mjs` (`planZeitText`, `startAbweichungText`), Test mit Drift-Prüfung,
   CI-Schritt.
7. `tl.html`: Häkchen im Profil-Editor (nur wenn der Host das Feld kennt), Meta-Zeile der
   Warteliste, Beendet-Zeile.

**PR 2 — „Panel Zeitplan"**

1. `zeitplan.rs::lage(spiele, heute, prognose_an)` rein, Tests zuerst.
2. Verdrahtung in `build_state_limited` nach `prognose_rechnen`, vor der Kürzung.
3. Nullen und Wiederherstellen in `state_fingerprint`.
4. `zeitplan.mjs::zeitplanLage` mit Tests und Drift-Prüfung.
5. Panelsystem: `PANEL_KEYS`, `PANEL_TITLES`, `PANEL_STANDARD_AUS` in `normalizedPanels` und
   `builtInProfile`, `panelAvailable("zeitplan")`.
6. Darstellung: großer Text mit an die Kachel gebundener Schriftgröße, Farben aus den
   vorhandenen Tönen (im Plan grün, dahinter Warnfarbe, davor und neutral unauffällig),
   Vorschau in der zugeklappten Kopfzeile, Nachführen in `tickClocks` nur bei Textänderung.
