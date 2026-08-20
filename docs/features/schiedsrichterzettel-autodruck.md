# Schiedsrichterzettel vorab und automatisch drucken — Spezifikation

> Status: **abgestimmt 2026-08-20** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Idee vom 20.08.2026, Vorlage `Schiedsrichterzettel.pdf` (DBV-Blankobogen, vermessen).
> Betroffene Crates: `src-tauri`, `relay`, `src/`.
> ADR: [0042](../adr/0042-stiller-druck-ueber-elementliste.md) ·
> [0043](../adr/0043-zettelblatt-nach-dbv-vorbild.md)
> Baut auf und ändert: [schiedsrichterzettel-druck.md](schiedsrichterzettel-druck.md).

## Kontext / Problem

Der Schiedsrichterzettel existiert seit v0.9.243/246 — aber erst **nach** dem Spiel, als
Archivausdruck für tablet-gezählte Partien. Am Feld braucht der Schiedsrichter das Blatt jedoch
**vorher**: Er trägt Punktverlauf, Aufschlagfolge und Vorkommnisse während des Spiels von Hand
ein. Heute muss die Turnierleitung dafür Blankobögen des Verbands vorhalten und Spielnummer,
Namen, Feld und Besetzung von Hand daraufschreiben — bei jeder Ansetzung neu.

Drei Lücken stehen dem im Weg:

1. **Kein Zettel ohne Aufzeichnung.** `scoresheet::dokumente` wirft ein Match ohne Punktverlauf
   und ohne Ereignisse weg. Das war eine bewusste Festlegung („ein halb ausgefüllter Bogen wäre
   irreführender als keiner") — für ein Blatt, das erst noch von Hand gefüllt wird, ist sie falsch.
2. **Kein stiller Druck.** Gedruckt wird über `iframe srcdoc` + `contentWindow.print()`
   (ADR 0039) — immer mit Systemdialog, immer von Hand, ohne Wahl des Zieldruckers. Bei
   automatischer Feldvergabe, die im Minutentakt Felder belegt, ist das unbrauchbar.
3. **Falsches Seitenbild.** Das heutige Raster (60 Spalten je Satz plus Fortsetzungsgruppe) ist
   eine Eigenkonstruktion. Schiedsrichter kennen den Bogen des Deutschen Badminton Verbands:
   sechs Blöcke à 33 Spalten, vier Zeilen je Block, A/R-Spalte, Satzergebniskasten im Kopf.

## Zielbild & Erfolgskriterien

**Ein Blatt, zwei Wege aufs Papier.**

- **Leerzettel auf Zuruf:** Zu jedem Spiel der Warteliste lässt sich ein Zettel drucken, dessen
  Kopf bereits alles trägt, was bekannt ist (Turnier, Disziplin, Runde, Spielnummer, Namen,
  Verein; Feld, Halle, SR und AR, sofern schon zugeordnet). Das Raster bleibt leer und wird von
  Hand geführt. Erreichbar in **TL-Web** und in der **Desktop-Warteliste**.
- **Autodruck bei Feldvergabe:** Ist der Autodruck eingeschaltet, geht das Blatt selbsttätig an
  einen fest eingestellten Drucker, sobald ein Spiel **auf einem Feld steht und ein
  Schiedsrichter zugeordnet ist** — gleich ob das Feld von Hand, über TL-Web oder durch die
  automatische Feldvergabe belegt wurde.

Erfolgskriterien:

- Ein Turnier läuft eine Runde lang ohne einen einzigen handbeschrifteten Blankobogen.
- Der Zettel liegt am Feld, bevor die Spieler es betreten — ohne dass jemand einen Druckdialog
  bestätigt.
- Ein Schiedsrichter erkennt das Blatt ohne Erklärung als „seinen" Bogen wieder.
- Keine Papierverschwendung: pro Spiel höchstens ein Blatt, auch über App-Neustarts hinweg.
- Der Turnierleiter richtet den Drucker in einem Auswahlfeld ein, ohne Netzwerk- oder
  Windows-Kenntnisse.

## Nicht-Ziele

- **Kein Autodruck beim Aufruf „in Vorbereitung"** — allein die Feldvergabe löst aus.
- **Kein Stapeldruck der ganzen Warteliste** auf einen Schlag.
- **Kein Druck auf einem Slave-PC.** Nur der Master (der BTP-Verbindung und Feldvergabe hält)
  druckt; eine zweite Halle hängt bei Bedarf einen Netzwerkdrucker in die Einstellung.
- **Kein zweiter Zettel** zu einem Spiel, auch nicht nach Feld- oder Schiedsrichterwechsel.
- Keine Änderung an Ereigniserfassung, Punktverlauf, `process_result` oder am BTP-Schreibpfad.
- Kein Druck aus TL-Web heraus auf den Host-Drucker — der TL-Web-Knopf druckt über das Gerät,
  auf dem TL-Web läuft.
- Keine Nutzung von Logo oder Schriftzug des Deutschen Badminton Verbands.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  **neu** `src-tauri/src/tablet/blatt.rs` (Blattlayout als Elementliste) ·
  **neu** `src-tauri/src/print/mod.rs` + `src-tauri/src/print/windows.rs` (stiller Druck) ·
  **neu** `src-tauri/src/tablet/print_log.rs` (Druck-Gedächtnis) ·
  `src-tauri/src/tablet/scoresheet.rs` (Blockbegriff, `dokumente`-Modus, `render_html`) ·
  `src-tauri/src/sync.rs` (Auslöser) · `src-tauri/src/config.rs` (`PrintConfig`) ·
  `src-tauri/src/commands.rs` (`printer_list`, `print_scoresheet`, Modus-Parameter) ·
  `src-tauri/src/tablet/server.rs` + `relay/src/main.rs` (Modus-Parameter an `tl_scoresheet`) ·
  `src-tauri/assets/tl.html` · `src/pages/PreparationPanel.tsx` · `src/pages/SetupWizard.tsx`.
- **Architekturregeln (CLAUDE.md R1–R6):**
  **R1** gewahrt — der Desktop druckt über Tauri-Commands, kein `fetch` auf `127.0.0.1:8088`.
  **R2** gewahrt — ein Druck liest nur; er erzeugt **niemals** eine Court→Match-Zuordnung, ein
  Ergebnis oder einen BTP-Write. Der Autodruck reagiert auf die Zuweisung, die BTP zurückmeldet,
  und läuft der Vergabe nie voraus.
  **R3** — die Leseroute bleibt unter identischem Pfad in LAN und Cloud; der neue Parameter wird
  an **beiden** Enden gleich behandelt. Der stille Druck ist **lokal am Host** und damit von der
  Verbindungsart unabhängig.
  **R4/R5/R6** unberührt.
  Die feature-lokalen Regeln **S-R1 bis S-R6** aus
  [schiedsrichterzettel-druck.md](schiedsrichterzettel-druck.md) gelten unverändert weiter;
  **S-R2** („der Zettel ist reine Druckform") wird durch den Autodruck ausdrücklich bekräftigt.
- **Konfiguration & Abwärtskompatibilität:** neu `PrintConfig { auto_enabled: bool,
  printer_name: String, }` mit `#[serde(default)]` — Vorgabe **aus**, leerer Druckername =
  Windows-Standarddrucker. Eine alte `config.json` bleibt lesbar; eine zurückgerollte Version
  ignoriert das Feld. `identifier` und Updater-Pfad unangetastet. Das Druck-Gedächtnis liegt
  turniergebunden neben `match-times.json`.
- **Datenschutz:** Der Zettel trägt Spielernamen und **neu auch immer den Verein**, sofern BTP
  ihn kennt — bewusste Ausnahme vom Schalter `show_club_names`, weil der Verein ohnehin auf
  Aushang und Meldeliste steht und das Vorbild eine Vereinszeile hat. **Kein Geburtsjahr**,
  **keine Lizenznummer** — beide werden auch beim Leerzettel nicht gelesen. Karten bleiben
  Zettel-only (Wächter-Tests aus der Vorgänger-Spec bleiben gültig). Der Druckername in der
  Config ist keine personenbezogene Angabe.
- **Abhängigkeiten:** **eine neue Cargo-Abhängigkeit** — `windows` (Features
  `Win32_Graphics_Gdi`, `Win32_Graphics_Printing`), MIT/Apache-2.0, von Microsoft gepflegt,
  transitiv ohnehin im Baum (Tauri auf Windows). Keine npm-Abhängigkeit, keine neue
  Tauri-Permission, kein badhub- oder Relay-seitiger Dienst.

## Das Blatt (Layout nach DBV-Vorbild)

Maße aus der vermessenen Vorlage (A4 quer, `/Rotate 90`), als **Konstanten** in `blatt.rs`, nicht
als CSS-Text:

| Maß | Wert |
|---|---|
| Rasterbreite gesamt | 275 mm |
| Namensspalte | 40 mm · A/R-Spalte 9,5 mm |
| Zellen je Block | **33** à **6,41 mm** = 211,5 mm |
| Endspalte rechts | 14 mm |
| Zeilenhöhe | 5,27 mm · 4 Zeilen je Block = 21,1 mm |
| Blockraster | 23,2 mm (Block + Zwischenraum) · **6 Blöcke** |
| Kopf | ≤ 52 mm · Fuß ≤ 8 mm · `@page`-Rand 5 mm |

**Breitenbudget:** 40 + 9,5 + 211,5 + 14 = 275 mm ≤ 287 mm (297 − 2 × 5).
**Höhenbudget:** 52 + 6 × 23,2 + 8 = 199,2 mm ≤ 200 mm (210 − 2 × 5).
Beide Budgets sind **Wächter-Tests**, keine Zusagen — das ist die Lehre aus v0.9.246, wo das
Raster 16 mm über den Blattrand hinauslief.

Aufbau:

- **Kopf links:** Spiel-Nr., Turnier, Feld-Nr., Datum. **Kopf Mitte:** Kasten Team A (zwei
  Namenszeilen + Vereinszeile, Marke „L"), Satzergebniskasten (drei Zeilen `x : y`), Kasten
  Team B (Marke „R"). **Kopf rechts:** Schiedsrichter, Aufschlagrichter, Beginn, Ende, Dauer
  (Min.). Links oben statt der Verbandsmarke das **Turnierlogo** (dasselbe Bild wie im
  Court-Monitor) und der Turniername. **Das Logo ist optional:** Ist keines hinterlegt oder kann
  der Druckweg es nicht laden, steht dort nur der Turniername — das Blatt bleibt vollständig
  gültig. Die Kopfhöhe ist in beiden Fällen dieselbe.
- **Raster:** sechs Blöcke à vier Zeilen; je Zeile Spielername, A/R-Spalte, 33 Zellen,
  Endspalte. Die beiden Zeilen von Team B sind grau hinterlegt.
- **Ein Satz beginnt immer in einem neuen Block** und läuft, wenn er mehr als 33 Ballwechsel
  hat, im nächsten Block weiter. Reichen sechs Blöcke nicht, folgt eine zweite Seite mit
  verkürztem Kopf.
- **Marker in der Zelle:** `W` Warnung (gelbe Karte) · `F` Fault (rote Karte) · `R` Referee
  gerufen · `D` Disqualifikation.
- **Fuß:** Unterschriftszeilen „Schiedsrichter …" und „Referee …", Erzeugungszeitpunkt und
  Version. **Der Vermerk „Internes Turnier-Archiv — kein amtlicher Beleg" entfällt** — bewusste
  Rücknahme gegenüber [schiedsrichterzettel-druck.md](schiedsrichterzettel-druck.md) und
  [umpire-mode.md](../umpire-mode.md); beide Dokumente sind mitzuziehen.
- **Leerzettel** ist dasselbe Blatt: leeres Raster, Kopffelder ohne Wert bleiben als
  beschreibbare Linie stehen.

## Auslöse-Regel des Autodrucks

Geprüft wird ein **Zustand**, kein Ereignis — je Sync-Lauf, nach `track_officials`:

```
für jedes Match mit Status OnCourt und gesetztem Feld:
    schon im Druck-Gedächtnis?      → nichts tun
    kein SR aus court_officials?    → nichts tun (nächster Lauf prüft erneut)
    sonst: Match vermerken, Blatt in die Druckwarteschlange geben
```

- Der SR darf **nach** der Feldvergabe dazukommen (Rotation oder Handzuteilung) — der nächste
  Lauf löst dann aus.
- Der Vermerk steht **vor** dem Druckversuch: ein fehlgeschlagener Druck wiederholt sich nicht
  endlos, ein Feld- oder SR-Wechsel erzeugt kein zweites Blatt.
- Das Gedächtnis ist **persistent und turniergebunden**: Ein App-Neustart mitten im Turnier
  druckt nichts nach, ein Turnierwechsel beginnt eine neue Datei.
- Ein **AR allein genügt nicht.** Ist der Schiedsrichter-Bereich abgeschaltet
  (`officials.enabled() == false`), kennt der Host nie einen SR und der Autodruck bleibt
  bauartbedingt stumm — der Einstellungstext sagt das.
- Der Aufruf steht **hinter** der `slave_mode`-Rückkehr in `run_once`: ein Ansage-Slave druckt nie.
- Der Sync-Lauf wartet **nie** auf den Drucker; gedruckt wird in einer eigenen Aufgabe, seriell.

## Akzeptanzkriterien

**Blatt und Layout**
- [ ] Das Breitenbudget geht auf: Namensspalte + A/R + 33 Zellen + Endspalte ≤ nutzbare Breite.
- [ ] Das **Höhenbudget** geht auf: Kopf + sechs Blockraster + Fuß ≤ nutzbare Höhe.
- [ ] Ein Satz mit 40 Ballwechseln belegt zwei Blöcke; der zweite beginnt mit Ballwechsel 34.
- [ ] Ein Satz beginnt nie mitten in einem Block, den ein anderer Satz angefangen hat.
- [ ] Ein Spiel mit drei langen Sätzen passt auf eine Seite; ein siebter Block erzwingt eine
      zweite Seite mit verkürztem Kopf.
- [ ] Ein langer Doppelname wird gekürzt, statt das Raster zu verschieben — in **beiden**
      Ausgaben (HTML und Druck).
- [ ] Marker erscheinen als `W`, `F`, `R`, `D`; eine rote Karte belegt genau eine Zelle.
- [ ] Der Verein wird gedruckt, wenn BTP ihn kennt — unabhängig von `show_club_names`.
- [ ] Kein Geburtsjahr und keine Lizenznummer erscheinen auf dem Blatt.
- [ ] Ein Spielername mit `<script>` wird escaped ausgegeben (HTML-Weg).
- [ ] Weder Wort noch Bildmarke des Verbands stehen im Dokument; im Kopf steht das Turnierlogo.
- [ ] Ohne hinterlegtes Turnierlogo entsteht dasselbe Blatt mit gleicher Kopfhöhe, nur ohne Bild.
- [ ] Der Vermerk „kein amtlicher Beleg" ist im Dokument nicht mehr enthalten.
- [ ] Die Elementliste ist **nach Seiten gegliedert**; ein Spiel mit sieben Blöcken ergibt zwei
      Seiten, und beide Treiber (HTML-Seitenumbruch, GDI `StartPage`/`EndPage`) geben dieselbe
      Seitenzahl aus.

**Leerzettel**
- [ ] Ein Match ohne jede Aufzeichnung liefert im **Vorab-Modus** ein vollständiges Blatt mit
      leerem Raster.
- [ ] Derselbe Abruf im **Normalmodus** liefert weiterhin **404** — das Akzeptanzkriterium der
      Vorgänger-Spec bleibt gültig.
- [ ] Der Leerzettel trägt Feld, Halle, SR und AR **nur**, wenn sie bekannt sind; sonst bleibt
      die Zeile leer.
- [ ] Ein Match außerhalb des aktuellen Turnier-Snapshots liefert auch im Vorab-Modus 404.
- [ ] Der Deckel `MAX_SHEETS_PER_DOC` greift im Vorab-Modus unverändert vor der Arbeit.
- [ ] LAN und Relay liefern für denselben Zustand und denselben Modus dasselbe Dokument.

**Autodruck**
- [ ] Ein Match, das auf ein Feld kommt **und** einen SR hat, erzeugt genau einen Druckauftrag.
- [ ] Wird der SR **nach** der Feldvergabe zugeordnet, druckt der nächste Sync-Lauf.
- [ ] Ein Match ohne SR erzeugt keinen Druckauftrag, auch nicht nach beliebig vielen Läufen.
- [ ] Ein Match mit nur einem AR erzeugt keinen Druckauftrag.
- [ ] Ein Feldwechsel oder SR-Tausch nach dem Druck erzeugt **kein** zweites Blatt.
- [ ] Nach einem App-Neustart mit 20 belegten Feldern entsteht **kein** Druckauftrag.
- [ ] Ein Turnierwechsel setzt das Gedächtnis zurück, die alte Datei bleibt erhalten.
- [ ] Bei `slave_mode` entsteht nie ein Druckauftrag.
- [ ] Bei ausgeschaltetem `auto_enabled` entsteht nie ein Druckauftrag.
- [ ] Ein fehlgeschlagener Druck wird **nicht** wiederholt und hinterlässt eine sichtbare Warnung
      in der Desktop-App sowie einen Log-Eintrag mit Feld und Druckername.
- [ ] Der Sync-Lauf bleibt in seiner üblichen Dauer, auch wenn der Drucker nicht antwortet.

**Einstellungen**
- [ ] Die Druckerliste zeigt die im System eingerichteten Drucker; ein leerer Eintrag bedeutet
      „Windows-Standarddrucker".
- [ ] Ein Druckername, der nicht mehr existiert, führt zu einer Warnung — nicht zum Absturz und
      nicht zu einem stillen Fehlschlag.
- [ ] Eine `config.json` ohne `print`-Abschnitt wird gelesen; der Autodruck ist dann aus.
- [ ] Eine ältere App-Version liest eine Config mit `print`-Abschnitt unverändert.

**Bedienung**
- [ ] In TL-Web erscheint der Zettel-Eintrag im ⋮-Menü jeder Zeile der Warteliste.
- [ ] In der Desktop-Warteliste erscheint ein Zettel-Knopf je Zeile.
- [ ] Beide Wege öffnen das bekannte Druckbild; der TL-Web-Weg druckt über das Gerät, auf dem
      TL-Web läuft.

## Tests

**Rust-Unit-Tests** (Namen wie in den Kriterien):
`breitenbudget_geht_auf` · **`blatt_passt_in_die_hoehe`** · `satz_ueber_33_laeuft_im_naechsten_block_weiter` ·
`sechs_bloecke_je_seite` · `lange_namen_werden_gekuerzt` · `marker_folgen_der_dbv_konvention` ·
`kein_verbandslogo_im_dokument` · `vorabzettel_ohne_aufzeichnung_liefert_ein_blatt` ·
`normalabruf_ohne_aufzeichnung_bleibt_404` · `vorabzettel_traegt_nur_bekannte_kopfangaben` ·
`weder_geburtsjahr_noch_lizenznummer_auf_dem_blatt` · `neustart_druckt_nicht_nach` ·
`sr_nach_der_vergabe_loest_den_druck_aus` · `feldwechsel_druckt_kein_zweites_blatt` ·
`ohne_sr_wird_nicht_gedruckt` · `ar_allein_druckt_nicht` · `slave_druckt_nie` ·
`druckfehler_wiederholt_nicht_endlos` · `alte_config_ohne_print_abschnitt_bleibt_lesbar`.

Die bestehenden Wächter-Tests der Vorgänger-Spec
(`sanktionsdaten_erreichen_den_anzeige_zustand_nie`,
`punktverlauf_datei_bleibt_frei_von_sanktionsdaten`, Escaping) müssen **grün bleiben**; der
Golden-String-Test `graph_dto_bleibt_byte_gleich` ebenfalls.

**Prüfbar ohne Drucker:** Das Blattlayout ist eine reine Funktion (Elementliste in Millimetern);
der GDI-Treiber fährt sie nur ab. Getestet wird die Liste, nicht das Papier.

**Pflichtläufe:** `cargo test`, `cargo clippy --workspace --all-targets -D warnings`,
`cargo fmt --check`, `npm run build`, die bestehenden `node`-Tests.

**Manuell:** Druck auf einem echten Windows-Drucker (Laser, A4 quer), Vergleich mit dem
Blankobogen; Autodruck an einem Testturnier mit automatischer Feldvergabe; Probe mit
ausgeschaltetem und ausgestecktem Drucker; TL-Web-Druck von einem Tablet.

## Risiken & Rollback

| Risiko | Wirkung im laufenden Turnier | Gegenmaßnahme |
|---|---|---|
| Druck blockiert den Sync-Lauf | Liveticker und Feldvergabe stehen | Druck in eigener Aufgabe, Sync wartet nie; Warteschlange seriell |
| Massendruck nach Neustart | 20 Blatt Papier, Verwirrung am Feld | persistentes, turniergebundenes Druck-Gedächtnis; eigener Test |
| Zweites Blatt nach Feldwechsel | zwei Zettel zu einem Spiel im Umlauf | Vermerk vor dem Druck, höchstens ein Blatt je Spiel |
| Blatt läuft über den Rand | unbrauchbarer Ausdruck, Papier weg | Breiten- **und** Höhenbudget als Wächter-Test; Maße sind Konstanten |
| Drucker antwortet nicht | TL wartet auf Zettel, die nie kommen | sichtbare Warnung in der App + Log; kein stiller Fehlschlag |
| Neue Windows-Abhängigkeit | Baufehler, Abstürze im Kern | dünner Treiber hinter einer reinen Funktion, `security-reviewer` in E4; Rückfallpfad WebView2 in ADR 0042 benannt |
| Layout-Umbau bricht den Archivzettel | alte Zettel sehen anders aus | ein Layout für beide Zettelarten, bestehende Tests bleiben grün; alte Ausdrucke sind Papier und unberührt |

**Rollback:** additiv. Eine zurückgerollte Version ignoriert `print` in der Config, druckt nicht
automatisch, kennt den Vorab-Modus nicht (Route antwortet wie früher) und rendert das alte
Layout. `gedruckt.json` bleibt gefahrlos liegen. Keine Migration, kein Datenverlust — Downgrade
jederzeit möglich, auch mitten im Turnier.

## Offene Fragen / Annahmen

- **Annahme:** Sechs Blöcke à 33 Spalten decken praktisch jedes Spiel ab; die zweite Seite ist
  der seltene Ausnahmefall. Zeigt der Feldtest etwas anderes, wird das im Blatt vermerkt.
- **Annahme:** Der Drucker steht am Master-PC oder ist von dort als Netzwerkdrucker erreichbar.
- **Annahme:** GDI trägt den Textsatz des Blatts (Umlaute, Kürzung, Graustufen) auf gängigen
  Windows-Druckern. Erster Prüfpunkt in E4; scheitert es, greift der in ADR 0042 benannte
  Rückfallpfad.
- **Annahme:** Das Turnierlogo lässt sich im stillen Druck darstellen (GDI+ `GdipLoadImage…`).
  Trägt das nicht, bleibt das Bild dem HTML-Weg vorbehalten und der stille Druck zeigt nur den
  Turniernamen — ohne neue Bild-Abhängigkeit und ohne das Blatt zu verändern.
- **Bewusst offen:** Kein Stapeldruck der Warteliste und kein Druck vom Slave-PC. Beides ist
  nachrüstbar, ohne diese Spec zu brechen.

## Betroffene Doku-Dateien

`docs/schiedsrichterzettel.md` (Bedienung: Leerzettel, Autodruck, Druckerwahl) ·
`docs/features/schiedsrichterzettel-druck.md` (Rücknahme zweier Festlegungen: „kein Zettel ohne
Tablet" und der Archiv-Vermerk) · `docs/umpire-mode.md` (Archiv-Vermerk) ·
`docs/turnierleitung-web.md` (Knopf in der Warteliste) · `docs/preparation.md` (Knopf im
Desktop-Panel) · `docs/schiedsrichter-management.md` (SR-Bedingung des Autodrucks) ·
`docs/multi-hall.md` (nur der Master druckt) · `docs/cloud-relay.md` (Modus-Parameter der Route) ·
`docs/adr/README.md` · `docs/changelog.md` · `docs/roadmap.md` ·
**neue CLAUDE.md-Zeile** für `blatt.rs`, `print/`, `print_log.rs` und `PrintConfig`.

## Umsetzungs-Hinweise

Sechs Etappen, jede einzeln grün und rückbaubar:

| E | Inhalt |
|---|---|
| **E1** | `blatt.rs`: Elementliste in Millimetern, DBV-Maße als Konstanten, 33er-Blockfolge; `scoresheet::render_html` malt nur noch die Liste; Marker W/F/R/D; Vermerk raus; Verein immer. Beide Budget-Tests. |
| **E2** | Vorab-Modus in `dokumente`, Parameter an `match_scoresheet_html` und `tl_scoresheet` (LAN **und** Relay), Normalverhalten unverändert. |
| **E3** | Knöpfe: ⋮-Menü der TL-Web-Warteliste (Muster: Beendet-Liste, inklusive der `content-visibility`-Falle) und `PreparationPanel`. |
| **E4** | `print/windows.rs` (GDI-Treiber), `printer_list`, `PrintConfig`, SetupWizard-Abschnitt mit Druckerauswahl und Hinweis auf den SR-Bereich. |
| **E5** | Autodruck: `print_log.rs`, Hook in `run_once` **nach** `track_officials`, Warnung im Dashboard. |
| **E6** | Doku, ADRs, Version, Changelog. |

**Reviews:** `code-reviewer` nach jeder Etappe. `security-reviewer` bei **E2** (neuer Parameter
an einer authentifizierten Route, die nun auch für Spiele ohne Aufzeichnung Namen ausgibt) und
**E4** (neue Cargo-Abhängigkeit, Windows-API, Druckername aus der Config in eine Win32-Funktion).

**Version** in E6 gemeinsam in `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` und
`package.json` gegen den dann aktuellen main-Stand. **Relay vor App:** E2 erweitert die Route,
ein alter Relay kennt den Parameter nicht — also E2 mergen, Relay-Deploy abwarten, dann taggen.
