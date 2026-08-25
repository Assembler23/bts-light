# Kombi-Ausrichtung je Monitor — Spezifikation

> Status: **umgesetzt 2026-08-25** in v0.9.270 (via /idee: Brief → Grill → How-To → Review).
> Quelle: Idee vom 25.08.2026. Betroffene Crates: `src-tauri/` und `src/` — `relay/` und
> `relay-proto/` bleiben unberührt.
> ADR: [0049 — Kombi-Ausrichtung: eigene Geräte-Datei statt Target-Feld](adr/0049-kombi-ausrichtung-eigene-geraete-datei.md).

## Kontext / Problem

Die Kombi-Anzeige zeigt bis zu drei Felder auf einem TV. Ihre Ausrichtung ist heute ein
**globaler** Schalter (`CourtMonitorConfig.combo_vertical`, seit v0.9.97): entweder stehen die
Felder auf **allen** Kombi-Anzeigen übereinander oder auf allen nebeneinander.

Das passt nicht zur Wirklichkeit einer Halle. Ein TV über dem Mittelgang zwischen zwei Feldern
will die Felder nebeneinander (Hochformat je Feld); ein TV an der Stirnseite über drei Feldern
will sie übereinander. Wer beides aufstellt, muss sich heute für eine Variante entscheiden — der
zweite TV steht dann falsch herum.

Die Feld-Auswahl je Gerät gibt es bereits (`MonitorTarget::CourtCombo { court_ids }`, bedient über
den Dialog „Kombi-Anzeige — Felder wählen"). Die Ausrichtung ist der letzte global gebliebene Teil
dieser Zuweisung.

Der Turnierleiter hat den Schmerz: Er stellt die Geräte auf und kann die Anzeige nicht an die
Montage anpassen.

## Zielbild & Erfolgskriterien

Der Turnierleiter weist einem TV zwei oder drei Felder zu und wählt **im selben Dialog** die
Ausrichtung. Der TV übernimmt sie, ohne dass jemand den Raspberry Pi anfassen muss und ohne dass
die anderen Kombi-TVs sich mitverändern.

**Erfolgskriterium:** Ein gekoppelter Kombi-TV zeigt die neue Ausrichtung **spätestens 2 s** nach
dem Übernehmen im Dialog — ohne Eingriff am Pi und **ohne sichtbaren Seitenaufbau** (der Satzstand
bleibt durchgehend lesbar, auch mitten im Spiel).

**Ohne Erklärung bedienbar:** Zwei beschriftete Radio-Knöpfe an der Stelle, an der die Felder
ohnehin gewählt werden. Der interne Begriff „vertikal" (der verwirrenderweise *nebeneinander*
bedeutet) erscheint in der Oberfläche nicht.

## Nicht-Ziele

- **Kein Cloud-Betrieb.** Die Kombi-Anzeige läuft ausschließlich über den Turnier-PC — der Relay
  leitet Kombi-Ziele bewusst nicht um. Daran ändert dieses Feature nichts, und die Ausrichtung
  reist auch nicht über die Wire-Ebene mit.
- **Keine neue Anordnung** jenseits der zwei bestehenden (kein Raster, kein 2+1-Mischbild).
- **Keine Änderung am Aussehen der Bänder** — Farben, Schriftgrößen, Satz-Sieger-Block und
  Pausen-Countdown bleiben, wie sie sind.
- **Keine automatische Erkennung** des Bildschirmformats.
- **Kein Sammel-Schalter** „für alle Kombi-Anzeigen übernehmen" (der Vorschlagswert deckt den
  Alltag ab; Fernwirkung auf nicht bearbeitete Geräte wäre überraschend).

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - `src-tauri/src/tablet/monitor.rs` — neuer Geräte-Store `ComboDirStore` samt Migration
  - `src-tauri/src/tablet/server.rs` — `/combo/state` liefert die Ausrichtung; das Anhängen von
    `&dir=v` an die Redirect-URL entfällt
  - `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs` — Schreib-/Lese-Command, Migrations-Aufruf
  - `src-tauri/assets/combo.html` — Klassen-Umschaltung im laufenden Poll; `rotate`-Fix
  - `src/io/comboAnsicht.mjs` + `scripts/test-combo-ansicht.mjs` — neu (Muster `courtPatch.mjs`)
  - `src/pages/CourtMonitorPanel.tsx` — Dialog um zwei Radio-Knöpfe erweitert
  - `src/pages/SetupWizard.tsx` — globaler Schalter entfällt
  - **`relay/` und `relay-proto/` unberührt** — kein Wire-Change, kein Relay-Deploy
- **Architekturregeln:** **R1** erfüllt — die Ausrichtung geht über Tauri-Commands
  (`assign_monitor` erweitert, `monitor_combo_dirs` neu), kein Netzwerkzugriff aus React. **R3**
  betrifft nur den LAN-Pfad; der Cloud-Pfad kennt keine Kombi-Ziele und bleibt unangetastet.
  **R2**, **R4**, **R5** und **R6** sind nicht berührt: keine Court→Match-Zuordnung, keine
  Ergebnisverarbeitung, keine Namespace- oder Tablet-Semantik.
- **Konfiguration & Abwärtskompatibilität:**
  - Neue Datei `monitor-combo-dir.json` im `app_config_dir`, neben `monitor-assignments-v3.json`
    und `monitor-halls.json`. Format: `{ "devices": { "<device_id>": true }, "last": true }`.
    Fehlende oder kaputte Felder fallen auf leer bzw. `false` zurück.
  - `CourtMonitorConfig.combo_vertical` bleibt **eine Version lang** lesbar im Struct (nicht mehr
    bedienbar, nicht mehr geschrieben) — es ist die Quelle der einmaligen Migration. Entfernung
    erst in einer späteren Version.
  - **Migration:** Existiert `monitor-combo-dir.json` noch nicht, wird sie beim Start angelegt und
    mit dem alten globalen Wert für alle Geräte mit Kombi-Zuweisung gefüllt. Die **Existenz der
    Datei ist der Merker**; die Migration kann sich nicht wiederholen. Für bestehende
    Installationen ändert sich am Bild nichts.
  - Die Serde-Form von `MonitorTarget` bleibt **unverändert** — bestehende Zuweisungsdateien
    werden nicht umgeschrieben.
  - Tauri-`identifier` und Updater-Pfad bleiben unangetastet.
- **Datenschutz:** nicht berührt. Der einzige neue gespeicherte Wert ist ein `bool` je Geräte-ID;
  keine Personendaten.
- **Abhängigkeiten:** keine neue Cargo- oder npm-Dependency. Kein badhub-Endpunkt, kein BTP, kein
  nginx. Bestehende Kiosk-Images und die Adresse `http://bts-light.local:8088/monitor` bleiben
  unverändert gültig.

## Akzeptanzkriterien

- [ ] **AK1** Zwei Kombi-TVs mit unterschiedlicher Ausrichtung laufen gleichzeitig; das Umstellen
      des einen lässt den anderen unverändert.
- [ ] **AK2** Das Umstellen wirkt binnen 2 s, ohne dass die Seite neu lädt — der Satzstand bleibt
      durchgehend sichtbar.
- [ ] **AK3** Drei Felder mit „nebeneinander" ergeben drei Spalten ohne Überlauf.
- [ ] **AK4** Wird ein Gerät auf ein Einzelfeld und später wieder auf Kombi gestellt, ist die
      zuvor gewählte Ausrichtung wieder da.
- [ ] **AK5** Ist das Gerät beim Umstellen offline, zeigt es nach dem Wiederverbinden die neue
      Ausrichtung.
- [ ] **AK6** Eine Kiosk-URL mit `?rotate=90` löst keine wiederholte Navigation aus.
- [ ] **AK7** Eine Kiosk-URL ohne `?device=`, aber mit `?dir=v`, startet nebeneinander und bleibt
      stabil.
- [ ] **AK8** Eine alte `config.json` mit `combo_vertical: true` führt dazu, dass nach dem Update
      alle vorhandenen Kombi-Geräte auf „nebeneinander" stehen; ein zweiter Start ändert daran
      nichts.
- [ ] **AK9** Eine Konfiguration **ohne** `combo_vertical` (Neuinstallation) ergibt den Standard
      „übereinander", ohne Fehlermeldung.
- [ ] **AK10** Nach einem Downgrade auf die Vorversion bleiben die Zuweisungen erhalten; die TVs
      folgen wieder dem globalen Schalter.
- [ ] **AK11** Ein neu angelegtes Kombi-Gerät wird mit der zuletzt gewählten Ausrichtung
      vorbelegt.
- [ ] **AK12** Der globale Schalter ist im Setup-Assistenten nicht mehr sichtbar, und das
      Speichern der Einstellungen löscht `combo_vertical` **nicht** aus der `config.json`.

## Tests

**Rust-Unit-Tests** (`src-tauri/src/tablet/monitor.rs`, TDD — Test zuerst):

- `read_combo_dirs_missing_file_is_empty_then_roundtrips` und `…_corrupt_file_is_default` —
  Serde-Roundtrip und Rückfall bei kaputter Datei.
- `unbekanntes_geraet_ist_uebereinander` (AK9), `ausrichtung_fuer_ohne_geraet_ist_none` (AK7).
- `apply_combo_dir_none_laesst_den_wert_stehen` (AK4) und `…_some_setzt_auch_zuletzt` (AK11).
- `apply_combo_dir_beruehrt_nur_das_genannte_geraet` (AK1).
- `eine_ausrichtungs_aenderung_aendert_den_dateiinhalt` — Wächter gegen einen Zwischenstand, der
  auf Zeitstempel und Länge schlüsselt.
- `migration_uebernimmt_den_globalen_wert_auf_alle_kombi_geraete` (AK8),
  `migration_laeuft_nur_einmal_und_ueberschreibt_nichts` (AK8),
  `migration_legt_die_datei_auch_ohne_kombi_geraet_an`,
  `migration_ruehrt_nicht_kombi_geraete_nicht_an`.
- `config.rs::court_monitor_ohne_combo_vertical_ist_false` (AK9).
- **Unverändert grün bleiben müssen:** `monitor_target_serde_format_is_kind_tagged` (Wächter für
  AK10 — die Zuweisungsdatei behält ihre Form) und `monitor_target_combo_redirect_path`.

**JS-Test** (`scripts/test-combo-ansicht.mjs`, Muster `test-court-patch.mjs`, in die CI
aufgenommen): `urlPasst()` ignoriert `device`, `rotate` und `dir` (AK6), erkennt aber ein **echt**
anderes Ziel weiterhin als Abweichung — sonst wäre der Fix ein Umzuweisungs-Blocker.
`ausrichtungVertikal()` lässt den URL-Startwert gelten, solange der Server nichts sagt (AK7), und
gibt dem Server-Wert danach den Vorrang (AK2).

**Manueller Turnier-Testfall:** AK3 (Lesbarkeit dreier Spalten aus der Halle heraus), AK5 (Kabel
ziehen, umstellen, stecken) und AK10 (Downgrade mit dem alten Installer) sind nicht
automatisierbar. Dazu ein Durchlauf mit zwei gleichzeitig laufenden Kombi-TVs unterschiedlicher
Ausrichtung (AK1, AK2).

`cargo test` grün und `npm run build` fehlerfrei sind Voraussetzung für den Commit.

## Risiken & Rollback

- **Downgrade auf die Vorversion** (bewusst akzeptiert): Die ältere Version kennt
  `monitor-combo-dir.json` nicht und folgt wieder dem globalen `combo_vertical`. Alle Kombi-TVs
  stehen dann einheitlich. Die **Zuweisungen bleiben erhalten**, weil die Serde-Form von
  `MonitorTarget` unverändert ist — genau deshalb wurde eine eigene Datei gewählt und keine neue
  Target-Variante (siehe ADR 0049).
- **Im laufenden Turnier:** Das Feature berührt weder Ergebnisse noch die Feldvergabe. Der
  schlimmste Fehlerfall ist ein falsch stehender TV, den man im Dialog sofort zurückstellt.
  Da die Umschaltung ohne Seitenaufbau läuft, unterbricht selbst ein Fehlgriff die Anzeige nicht.
- **Beim Update:** Der Server hängt `&dir=v` nicht mehr an die Redirect-URL. Damit die
  betroffenen TVs deswegen nicht einmal komplett neu laden (schwarzer Bildschirm mitten im Spiel),
  ignoriert der URL-Vergleich in `combo.html` künftig auch `dir`. Das ist Teil der Umsetzung, kein
  Nachtrag.
- **Mitgefixter Altbestand:** Der `rotate`-Fehler in `combo.html` besteht schon heute und trifft
  hochkant montierte Kombi-TVs (Dauer-Navigation im Sekundentakt). Er wird in diesem Zug behoben;
  das Risiko liegt darin, den URL-Vergleich zu weit zu öffnen — deshalb der Regressionstest gegen
  ein echt anderes Ziel.

## Offene Fragen / Annahmen

Aus dem Grill sind keine Fragen offen geblieben; alle sieben Blocker sind entschieden.
Verbleibende Annahmen:

- **Dokumentierte Grenze, keine Lücke:** Eine **hand-getippte** `/combo`-URL **ohne** `?device=`
  folgt Änderungen nicht — für sie gilt weiter nur der Startwert `?dir=v`. Solche Geräte sind dem
  Host nicht bekannt. Das wird in `docs/court-monitor.md` festgehalten.
- Die Ausrichtung ist eine Eigenschaft des **Geräts**, nicht des Turniers; ein Turnierwechsel
  setzt sie nicht zurück.
- „Nebeneinander" mit drei Feldern ist erlaubt. Ob es aus der letzten Reihe lesbar ist, beurteilt
  der Turnierleiter in seiner Halle — die App verbietet es nicht.

## Betroffene Doku-Dateien

| Datei | Was |
|---|---|
| `docs/court-monitor.md` | Bullet „Kombi-Anzeige: Felder nebeneinander" ersatzlos aus der Setup-Liste entfernen; Abschnitt „Kombi-Anzeige (`combo.html`)" um die Bedienung je Gerät erweitern (Beschriftungen, Vorschlagswert, Speicherort, Übernahme ohne Seitenaufbau, `?dir=v` nur noch Startwert, die Grenze ohne `?device=`, Migration und Downgrade-Rückfall) |
| `docs/adr/0049-kombi-ausrichtung-eigene-geraete-datei.md` | neu |
| `docs/adr/README.md` | Zeile für 0049 (die Tabelle endet bei 0043, während die Dateien bis 0048 laufen — die fehlenden Zeilen im selben Zug nachtragen) |
| `docs/changelog.md` | Abschnitt für die neue Version, inklusive des mitgefixten Flackerns bei `?rotate=90` |
| `CLAUDE.md` | Zeile „Court-Monitor" um `assets/combo.html` (fehlt bislang), `src/io/comboAnsicht.mjs`, `monitor_combo_dirs` und `MONITOR_COMBO_DIR_FILE` ergänzen; Doku-Spalte um diese Spec und ADR 0049 |
| `docs/roadmap.md` | Verweis auf diese Spec |

**Nicht betroffen (geprüft):** `docs/cloud-relay.md` (kein Wire-Change), `docs/multi-hall.md`,
`docs/pi-setup.md`, `docs/pi-master-image.md`, `docs/tablet.md`.

## Umsetzungs-Hinweise

Die Schritte und alle Fallstricke, die man kennen muss, stehen hier — die Spec ist ohne weitere
Unterlagen arbeitsfähig. (Der Arbeitsstand der /idee-Phasen liegt zusätzlich unter
`docs/features/_intake/kombi-ausrichtung-je-monitor/`, ist aber gitignoriert und enthält nur
Herleitung, keine zusätzlichen Vorgaben.)

Kern in zwölf Schritten:

1. **S1** Store `ComboDirStore` in `monitor.rs` nach dem Muster `MONITOR_HALLS_FILE`, über das
   vorhandene `write_atomic`.
2. **S2** Migration als **reine** Funktion (ohne `AppHandle`), damit sie testbar bleibt.
3. **S3** Migration in `lib.rs .setup()` verdrahten, synchron nach `init_logging`.
4. **S4** `/combo/state` liefert `vertical` — Zwischenstand nach dem Muster `ad_style()`, mit dem
   **Dateiinhalt** als Schlüssel.
5. **S5** Das Anhängen von `&dir=v` in `server.rs` ersatzlos streichen.
6. **S6** `src/io/comboAnsicht.mjs` + Test + CI-Schritt — läuft vor der Asset-Änderung rot.
7. **S7** `combo.html`: Inline-Kopie mit Verweis-Kommentar, `urlPasst` statt `currentUrlMatches`,
   Klassen-Umschaltung in `render()` **vor** Schleife und Leer-Rückfall.
8. **S8** `assign_monitor` um `combo_vertical: Option<bool>` erweitern (`None` = *unverändert*);
   Schreibreihenfolge **erst Store, dann Zuweisung**; `forget_monitor_device` räumt mit.
9. **S9** Lese-Command `monitor_combo_dirs`.
10. **S10** Dialog in `CourtMonitorPanel.tsx` um zwei Radio-Knöpfe erweitern.
11. **S11** Globalen Schalter aus `SetupWizard.tsx` entfernen — **der Spread
    `...initialConfig.court_monitor` bleibt stehen**, er hält die Migrationsquelle am Leben.
12. **S12** Doku, ADR, Version.

**Fallstricke, die man nur einmal übersieht:**

- Der Zwischenstand darf **nicht** auf `(mtime, len)` schlüsseln, sondern auf den Dateiinhalt:
  Windows-Zeitstempel rücken nur im ~15,6-ms-Takt vor, und zwei Geräte gegenläufig umzustellen
  (A `true→false`, B `false→true`) ist längenerhaltend — die alte Ausrichtung bliebe unbegrenzt
  stehen. Ein Test im Repo belegt das bereits für einen Nachbar-Store.
- `renderBand` entscheidet die DOM-Form beim **Bauen** (vertikal `.vnames`/`.vsetrow`, horizontal
  `.row`/`.row__sets`); eine Klassen-Umschaltung allein rührt bestehende Bänder nicht an. Der
  Umschalter gehört deshalb in `render()` vor die Schleife **und** vor den Leer-Rückfall, damit
  auch der Leerlauf-Bildschirm folgt. Sonst ist nichts neu zu rechnen: Die CSS-Variablen für
  Schrift- und Satzgrößen liest nur der horizontale Zweig, vertikal stehen feste Werte, Flaggen
  entstehen je Render neu, und die `rotate-*`-Klasse am `body` bleibt unberührt.
- `dir` gehört **niemals** in die Redirect-URL. Es erzwänge einen Seitenaufbau, und der
  URL-Vergleich ist positionsabhängig — hinter `device` stehend löste es eine Endlosschleife aus.
- **`vertical` nur senden, wenn das Gerät bekannt ist** (AK7): Ohne `?device=` erfährt der Host von
  einem TV nichts. `/combo/state` lässt das Feld dann weg (`null`), und `combo.html` reagiert nur
  auf einen echten Boolean. Ein pauschales `false` kippte jede hand-gebaute Kiosk-URL mit `?dir=v`
  auf „übereinander".
- Ein Gerät **ohne** Store-Eintrag fällt im Dialog auf „übereinander" zurück, nicht auf
  „unbestimmt" — die Geräteliste vereint LAN- und Relay-Geräte, der neue Lese-Command ist aber
  host-lokal.
- Die Inline-Kopie in `combo.html` muss für sich syntaktisch gültig sein (kein `import`/`export`):
  `scripts/check-asset-syntax.mjs` importiert jeden Inline-`<script>`-Block als Modul.
- Die Ausrichtung gehört **nicht** in den `/health`-Umschlag. Täte man es doch, müsste sie zwingend
  in dessen ETag einfließen — sonst bekäme die Seite nach dem Umschalten so lange „nichts Neues"
  gemeldet, bis sich etwas anderes ändert. `/combo/state` hat dieses Problem nicht: Es antwortet
  `no-store` und geht nicht durch den Übersichts-Cache.

**Reviews:** `code-reviewer` nach der Umsetzung (Pflicht). `security-reviewer` ist **nicht** nötig
— kein neuer Auth-Pfad, kein Datei-Upload, keine Verarbeitung fremder URLs; der einzige neue
Eingabewert ist ein `bool` aus dem Desktop-UI.

**Version** gemeinsam in `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` und `package.json`
bumpen — die konkrete Nummer erst beim Merge festlegen, da parallele PRs sonst kollidieren.
