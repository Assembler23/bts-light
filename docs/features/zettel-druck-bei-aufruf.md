# Zettel-Druck schon beim Aufruf in die Vorbereitung — Spezifikation

> Status: **abgestimmt 2026-11-10** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Idee der Turnierleitung vom 10.11.2026. Betroffene Crates: src-tauri, src.
> ADR: [0082 — Zettel-Druck beim Aufruf](../adr/0082-zettel-druck-beim-aufruf.md).
> Baut auf: [schiedsrichterzettel-autodruck](schiedsrichterzettel-autodruck.md) (ADR 0042),
> [sr-ar-beim-aufruf](sr-ar-beim-aufruf.md) (ADR 0080).

## Kontext / Problem

Der Zettel-Autodruck druckt heute erst, wenn ein Spiel auf ein Feld kommt und einen
Schiedsrichter (SR) hat (`sync.rs` `beanspruche_druckauftraege`). Seit ADR 0080 bekommt ein
Spiel seinen SR aber schon beim **Aufruf in die Vorbereitung**. Die Turnierleitung möchte den
Zettel zu diesem Zeitpunkt in der Hand haben, damit SR und Spieler mit dem Blatt zum Feld gehen
und nicht erst nach der Feldvergabe jemand zum Drucker laufen muss.

Die Bedingung „gedruckt wird nur mit zugewiesenem SR" bleibt. Kommt der SR später (in der
Vorbereitung oder erst am Feld), wird dann gedruckt.

Die bisherige Autodruck-Spec nennt „Kein Autodruck beim Aufruf" ausdrücklich als Nicht-Ziel; diese
Spec hebt das auf (ADR 0082).

## Zielbild & Erfolgskriterien

- Im Setup gibt es unter dem Haken **„Zettel automatisch drucken"** die Auswahl **„Drucken:"**
  mit **„bei der Feldvergabe"** (Voreinstellung, Verhalten wie heute) und **„schon beim Aufruf in
  die Vorbereitung"**.
- Im Aufruf-Modus liegt der Zettel spätestens einen Sync-Takt nach dem Aufruf im Drucker, sofern
  das Spiel einen SR hat.
- Auf dem Zettel steht derselbe SR, den TL-Warteliste und Meetingpoint für dieses Spiel zeigen.
- Ändert sich der SR vor der Feldvergabe, kommt ein neuer, richtiger Zettel; wird der SR entfernt,
  bekommt die Turnierleitung eine Warnung.
- Bestehende Installationen merken nach dem Update nichts (Voreinstellung „bei der Feldvergabe").
- Ein Turnierleiter kann den Modus mitten im Turnier umschalten, ohne dass ein Schwall Zettel
  aus dem Drucker kommt.

## Nicht-Ziele

- **Kein Druck ohne SR** — in keinem Modus.
- **Keine einstellbare Wartezeit**; die 20-s-Stabilitätsfrist ist fest.
- **Kein Drucker je Halle** — weiterhin ein Drucker am Master für alle Hallen.
- **Kein Auslöser** durch: gelbe Markierung/Aufrufmarke in BTP, Hallen-Vorverteilung, Wunschfeld,
  Aufruf am Slave-PC.
- **Kein Nachdruck im Modus „bei der Feldvergabe"** — dort gilt weiterhin „höchstens ein Zettel".
- **Kein Nachdruck, sobald das Spiel auf dem Feld steht** (auch nicht, wenn es vom Feld
  zurückgenommen wird).
- **Kein Stapeldruck** der ganzen Warteliste; kein Druck am Slave.
- Keine Änderung an Relay, relay-proto oder badhub.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - `src-tauri/src/config.rs` — neues Enum `PrintTrigger` (`feldvergabe` | `aufruf`),
    Feld `PrintConfig.trigger`.
  - `src-tauri/src/tablet/print_log.rs` — Druck-Gedächtnis erweitert (gedruckter SR je Spiel,
    Zustand, Modus, 20-s-Kandidat im RAM).
  - `src-tauri/src/sync.rs` — `beanspruche_druckauftraege` entscheidet nach Modus und Zeit;
    `autodruck` nennt Feld **oder** Spiel-Nr.
  - `src-tauri/src/tablet/scoresheet.rs` `dokumente` — Vorabzettel wartender Spiele mit
    Hand-vor-BTP-Sicht, Aufruf-Halle, Druckdatum (gilt auch für den Hand-Vorabzettel).
  - `src/types.ts`, `src/pages/SetupWizard.tsx` — Auswahl „Drucken:".
- **Architekturregeln:**
  - R1: kein neuer Command, die Einstellung reist über `save_config`.
  - R2: keine Feldzuordnung wird erfunden. Für **wartende** Spiele gilt die SR-Sicht „Hand vor
    BTP" (wie Warteliste und Meetingpoint, ADR 0078/0080); laufende Spiele bleiben bei BTP
    (`effective`). Festgehalten in ADR 0082.
  - R3/R4: Aufrufe aus Desktop und TL-Web (LAN und Cloud) landen im Aufruf-Speicher des
    Turnier-PCs; nur dort wird gedruckt (Slave-Rücksprung in `run_once` + `slave_mode`-Riegel).
  - R5, R6: nicht berührt.
- **Konfiguration & Abwärtskompatibilität:** `PrintConfig.trigger` mit `#[serde(default)]` und
  `#[serde(other)]` auf `Feldvergabe` — alte Configs und unbekannte Werte ergeben
  „bei der Feldvergabe". Rollback auf eine ältere Version: Das Feld fällt beim Speichern weg,
  der Modus ist still wieder „bei der Feldvergabe" (akzeptiert).
  `gedruckt.json` wird **abwärtskompatibel** erweitert (siehe Umsetzungs-Hinweise); eine ältere
  Version liest die Liste gedruckter Spiele weiter und druckt nichts doppelt.
  `identifier` und Updater-Pfad bleiben unangetastet.
- **Datenschutz:** `gedruckt.json` enthält künftig neben Match-IDs die **Official-ID** des
  gedruckten SR je Spiel (lokal auf dem Turnier-PC, wie `officials-state.json`; turniergebunden,
  verfällt mit dem Turnier). Kein Name, kein Geburtsjahr. Der Modul-Kommentar in
  `print_log.rs` („ausschließlich Match-IDs") wird angepasst. Der Zettel selbst zeigt nichts
  Neues.
- **Abhängigkeiten:** keine neue Dependency. Keine Abhängigkeit von der offenen BTP-Messung
  `btp_officials_ohne_feld_probe`: Der Zettel eines wartenden Spiels liest die lokale
  Hand-Sicht, nicht BTP.

## Verhalten

### Begriffe

- **gerufen** = das Spiel hat einen **aktiven** Aufruf im Aufruf-Speicher des Turnier-PCs
  (`preparation_calls()` / `active_calls`). Quellen: Desktop-Vorbereitung, TL-Web
  „In Vorbereitung rufen" (LAN und Cloud), Meetingpoint-Automatik.
- **vollständig** = beide Seiten der Paarung stehen fest.
- **SR wartend** (`srW`) = SR in der Sicht „Hand vor BTP" (`wartende_official_ids`).
- **SR am Feld** (`srF`) = SR in der Sicht `effective` (`court_official_ids`).
- **SR geschrieben** (`srH`) = lokaler Schreibwert (`fuer_schreiben`).
- **Frist** = 20 Sekunden.

### Modus „bei der Feldvergabe" (Voreinstellung)

Unverändert: Druck, sobald das Spiel auf einem Feld steht und einen SR hat; höchstens ein Blatt
je Spiel, kein Nachdruck bei Feld- oder SR-Wechsel.

### Modus „schon beim Aufruf in die Vorbereitung"

Gilt nur, wenn „Zettel automatisch drucken" an ist, der PC Master ist und „Mit Schiedsrichtern
spielen" an ist. Ist der SR-Betrieb aus, passiert nichts — auch keine Warnungen.

Zustände je Spiel:

| Zustand | Spiel wartet (Scheduled) | Spiel auf dem Feld (OnCourt, Feld bekannt) |
|---|---|---|
| **neu** (kein Vermerk) | gerufen, vollständig und `srW > 0` → Zettel **ohne Feld** mit `srW` → *vorab(srW)*. Sonst nichts (der SR darf später kommen). | `srF > 0` → Zettel **mit Feld** → *fertig*. (Rückfall: Spiel ohne Aufruf aufs Feld oder Aufruf ohne SR.) |
| **vorab(X)** | `srW = X` → nichts. `srW = 0` → einmal Warnung „Zettel für Spiel N ungültig – Schiedsrichter entfernt" → *ungültig(X)*. `srW = Y ≠ X` → wenn Y seit der Frist unverändert gilt und das Spiel gerufen und vollständig ist: Nachdruck ohne Feld mit Y → *vorab(Y)*. | Erst entscheiden, wenn `srF = srH` (BTP hat bestätigt). Dann: `srF = X` → *fertig*, kein Druck. `srF = Y ≠ 0` → **sofort** Nachdruck mit Feld → *fertig*. `srF = 0` → Warnung „ungültig" → *fertig*. |
| **ungültig(X)** | `srW = X` → zurück zu *vorab(X)* ohne Druck. `srW = Y > 0` → nach der Frist und gerufen: Nachdruck → *vorab(Y)*. `srW = 0` → nichts. | wie *vorab(X)*, aber bei `srF = 0` keine zweite Warnung. |
| **fertig** | nichts (auch nach Rücknahme vom Feld) | nichts |

Dazu:

- Wechselt der SR in der Frist erneut (A→B→C), beginnt die Frist für C neu. Wechselt er zurück
  auf den gedruckten SR, entfällt der Nachdruck.
- Die Frist überlebt das Speichern der Einstellungen (Sync-Neustart), nicht aber einen
  App-Neustart (dann beginnt sie neu).
- Ein Aufruf wird zurückgenommen: Eine Rotations-Zuteilung wird dabei freigegeben → `srW = 0` →
  Warnung „ungültig". Wird das Spiel später mit anderem SR erneut gerufen, druckt es nach der
  Frist neu. Eine Hand- oder BTP-Zuteilung bleibt bei der Rücknahme stehen → keine Warnung.

### Umschalten und Einschalten

Wird der Modus „beim Aufruf" **wirksam** — durch Umschalten von „bei der Feldvergabe" oder durch
Einschalten des Hakens bei schon gewähltem Aufruf-Modus — gelten alle Spiele, die in diesem
Moment gerufen und vollständig sind **und einen SR haben**, als erledigt: Sie bekommen keinen
Zettel, weder jetzt noch am Feld. Gerufene Spiele **ohne** SR drucken normal, sobald ein SR kommt.

Erkannt wird das zustandsbasiert (zuletzt wirksamer Modus steht in `gedruckt.json`), nicht beim
Speichern — so greift es auch nach einem Neustart oder Turnierwechsel.

Zurück auf „bei der Feldvergabe": Vorab gedruckte Spiele gelten als gedruckt und bekommen am
Feld kein zweites Blatt.

### Zettel ohne Feld

Gilt für jeden Vorabzettel eines **wartenden** Spiels (Autodruck und Hand-Vorabzettel aus
Vorbereitung bzw. TL-Web ⋮):

- **SR/AR**: Sicht „Hand vor BTP" (`wartende_officials`); ein ausdrücklich entfernter SR (`Some(0)`)
  bleibt leer.
- **Feld**: leer.
- **Halle**: Halle des Spiels aus BTP, sonst die Halle des Aufrufs.
- **Datum**: Druckdatum, solange das Spiel keinen Zeitstempel hat.
- Druckauftrag, Log und Druckwarnung nennen **„Spiel-Nr. N"** statt „Feld".

Laufende und beendete Spiele bleiben unverändert (SR aus `effective`, Datum aus dem Spiel).

## Akzeptanzkriterien

Einstellung
- [ ] Neue Installation und Update einer alten Config: „Drucken:" steht auf „bei der Feldvergabe".
- [ ] Ein unbekannter Wert in `print.trigger` wird als „bei der Feldvergabe" gelesen.
- [ ] Ist „Mit Schiedsrichtern spielen" aus oder „Zettel automatisch drucken" aus, ist die
      Auswahl „Drucken:" ausgegraut.
- [ ] Im Modus „bei der Feldvergabe" verhalten sich alle bestehenden Autodruck-Tests unverändert.

Aufruf-Modus — Druck
- [ ] Ein gerufenes, vollständiges Spiel mit SR wird ohne Feld gedruckt, mit dem SR aus der
      Hand-vor-BTP-Sicht.
- [ ] Ein Hand-Aufruf (Desktop, TL-Web) druckt spätestens im nächsten Sync-Takt; ein
      Automatik-Aufruf im selben Takt.
- [ ] Ein Spiel ohne aktiven Aufruf (auch: BTP-Gelb, Vorverteilung, Wunschfeld) druckt nicht vorab.
- [ ] Ein gerufenes Spiel mit offener Paarung druckt nicht.
- [ ] Ein gerufenes Spiel ohne SR druckt, sobald es in der Vorbereitung einen SR bekommt.
- [ ] Ein gerufenes Spiel, das ohne SR aufs Feld kommt, druckt am Feld, sobald es dort einen SR hat.
- [ ] Ein Spiel ohne Aufruf, das direkt aufs Feld kommt, druckt wie im Feldvergabe-Modus.

Aufruf-Modus — Nachdruck / Warnung
- [ ] SR-Wechsel X→Y in der Vorbereitung: Nachdruck erst, wenn Y 20 s stabil ist.
- [ ] X→Y→X innerhalb der Frist: kein Nachdruck.
- [ ] X→Y→Z: die Frist beginnt bei Z neu, es wird nur Z gedruckt.
- [ ] Bei der Feldvergabe mit abweichendem, von BTP bestätigtem SR: sofort Nachdruck mit Feld.
- [ ] Bei der Feldvergabe mit dem gedruckten SR: kein Druck.
- [ ] Solange `srF ≠ srH`: weder Druck noch Warnung.
- [ ] SR entfernt (wartend oder am Feld ohne SR): kein Druck, genau eine Warnung je Entfernen
      mit Spiel-Nr.
- [ ] Rücknahme eines Aufrufs mit Rotations-SR: Warnung; erneuter Aufruf mit anderem SR:
      Nachdruck nach der Frist.
- [ ] Rücknahme mit Hand-SR: keine Warnung, kein Druck.
- [ ] Steht das Spiel auf dem Feld (oder wurde es von dort zurückgenommen), kommt kein weiteres Blatt.

Robustheit
- [ ] Umschalten auf „beim Aufruf" mit gerufenen Spielen mit SR: kein Druck, auch nicht am Feld.
- [ ] Einschalten des Hakens im Aufruf-Modus: wie Umschalten.
- [ ] Umschalten mit gerufenem Spiel ohne SR: es druckt, sobald ein SR kommt.
- [ ] Zurückschalten auf „bei der Feldvergabe": vorab gedruckte Spiele drucken am Feld nicht.
- [ ] App-Neustart im Aufruf-Modus: kein Druckstoß, Zustände bleiben.
- [ ] Speichern der Einstellungen (Sync-Neustart) verliert eine laufende Frist nicht.
- [ ] Turnierwechsel verwirft alle Vermerke.
- [ ] Ein Slave druckt nie.
- [ ] Eine `gedruckt.json` der Vorgängerversion wird gelesen; ihre Spiele gelten als erledigt.
- [ ] Eine neue `gedruckt.json` ist für die Vorgängerversion lesbar (keine Doppeldrucke nach Rollback).
- [ ] Entsteht kein Blatt, wird der vorige Zustand wiederhergestellt.

Blatt
- [ ] Vorabzettel eines wartenden Spiels (Auto und Hand) zeigt Hand-vor-BTP-SR, Feld leer,
      Halle (BTP, sonst Aufruf), Druckdatum.
- [ ] Zettel eines laufenden Spiels bleibt unverändert.
- [ ] Druckauftrag und Warnung nennen „Spiel-Nr. N", wenn kein Feld feststeht.

## Tests

Rust-Unit-Tests (TDD, `cargo test` grün; Clippy mit `-D warnings`):

- `scoresheet.rs`: `vorabzettel_wartend_zeigt_handzuteilung_vor_btp`,
  `vorabzettel_laufend_bleibt_bei_btp_sicht`,
  `vorabzettel_wartend_nimmt_aufruf_halle_wenn_btp_keine_kennt`,
  `vorabzettel_wartend_traegt_druckdatum`, `normalzettel_datum_bleibt_spieldatum`,
  `vorabzettel_sr_explizit_entfernt_bleibt_leer`.
- `config.rs`: `print_trigger_fehlt_ergibt_feldvergabe`, `print_trigger_unbekannt_ergibt_feldvergabe`,
  `print_trigger_roundtrip_snake_case`.
- `print_log.rs` (Datei): `alte_datei_ohne_zettel_ist_lesbar`,
  `neue_datei_ist_fuer_alte_struktur_lesbar`, `printed_ohne_zettel_gilt_als_abgeschlossen`,
  `zettel_und_modus_ueberleben_neustart`, `turnierwechsel_verwirft_zettel_und_kandidaten`.
- `print_log.rs` (API): `beanspruche_vorab_nur_einmal`, `nachdruck_nur_wenn_alter_sr_noch_stimmt`,
  `zuruecksetzen_stellt_vorstand_wieder_her`, `kandidat_startet_neu_bei_anderem_sr`,
  `kandidat_verschwindet_bei_rueckkehr_zum_gedruckten`, `modus_angleichen_merkt_einmal_vor`.
- `sync.rs` (Entscheidung, mit `now_ms`): `aufruf_modus_druckt_gerufenes_spiel_ohne_feld`,
  `aufruf_modus_ohne_aufruf_kein_vorabdruck`, `aufruf_modus_offene_paarung_druckt_nicht`,
  `aufruf_modus_sr_kommt_spaeter_druckt_dann`,
  `aufruf_modus_ohne_aufruf_direkt_aufs_feld_druckt_mit_feld`,
  `aufruf_modus_aufruf_ohne_sr_druckt_am_feld`, `sr_wechsel_druckt_erst_nach_20_s_stabil`,
  `sr_wechsel_zurueck_druckt_nichts`, `sr_wechsel_hin_und_her_startet_frist_neu`,
  `sr_entfernt_warnt_einmal_und_druckt_nicht`, `ruecknahme_gibt_rotations_sr_frei_und_warnt`,
  `ruecknahme_und_neuer_aufruf_mit_anderem_sr_druckt_nach_frist`,
  `hand_sr_bleibt_nach_ruecknahme_keine_warnung`,
  `feldvergabe_mit_abweichendem_sr_druckt_sofort_mit_feld`,
  `feldvergabe_mit_gedrucktem_sr_druckt_nichts`, `feldvergabe_wartet_bis_btp_bestaetigt`,
  `feld_ohne_sr_dienst_warnt_ungueltig`, `auf_dem_feld_kein_weiterer_zettel`,
  `vom_feld_zurueck_kein_weiterer_zettel`, `umschalten_auf_aufruf_merkt_gerufene_vor_kein_stoss`,
  `umschalten_merkt_gerufene_ohne_sr_nicht_vor`, `vorgemerkte_drucken_auch_am_feld_nicht`,
  `umschalten_zurueck_auf_feldvergabe_druckt_vorab_gedruckte_nicht_am_feld`,
  `einschalten_im_aufruf_modus_merkt_vor`, `sr_betrieb_aus_keine_warnungen`,
  `aufruf_modus_slave_druckt_nie`, `neustart_im_aufruf_modus_druckt_nicht_nach`,
  `sync_neustart_verliert_frist_nicht`, `hand_aufruf_druckt_im_naechsten_takt`,
  `auto_aufruf_druckt_im_selben_takt`.
- `sync.rs` (`autodruck`): `druckwarnung_nennt_spielnummer_ohne_feld` (Formatfunktion).
- Frontend: `npm run build`; `node scripts/test-handbuch-fakten.mjs` für die neuen Anker.

Manueller Turnier-Testfall (Windows-PC mit Drucker): Aufruf mit Rotations-SR → Blatt ohne Feld;
TL teilt um → nach 20 s zweites Blatt; Feld ohne SR-Dienst → Warnung; Umschalten mitten im
Lauf → kein Schwall.

## Risiken & Rollback

- **Ein Warnungs-Slot**: Mehrere „ungültig"-Meldungen kurz nacheinander überschreiben sich; das
  Log behält alle. Gegenmittel: Spiel-Nr. im Text, im Handbuch beschrieben.
- **Mehr Papier**: Jede SR-Umteilung in der Vorbereitung (länger als 20 s stabil) erzeugt ein
  Blatt. Vor Ort lässt sich ohne Schwall auf „bei der Feldvergabe" zurückschalten.
- **BTP bestätigt den Feld-SR nie** (`srF ≠ srH` dauerhaft): kein Nachdruck, keine Warnung —
  bewusst still (lieber kein Blatt als ein falsches); einmal je Spiel ein Log-Hinweis.
- **Vorgemerkte Spiele** (beim Umschalten) bekommen nie ein automatisches Blatt; der Hand-
  Vorabzettel bleibt möglich. Im Handbuch.
- **Druckdatum am Vortag** bei Hand-Vorabzetteln: vertretbar, es ist dann ein Leerzettel.
- **Rollback** auf eine ältere Version: Config fällt auf „bei der Feldvergabe", `gedruckt.json`
  bleibt lesbar, vorab gedruckte Spiele drucken am Feld nicht erneut.

## Offene Fragen / Annahmen

- Annahme: Bei Rücknahme eines Aufrufs ohne neuen Aufruf gilt dieselbe Warnregel wie bei
  „SR entfernt" (im Grill festgelegt, ohne gesonderte Rückfrage).
- Keine offenen Fragen.

## Betroffene Doku-Dateien

- `docs/einstellungen.md` (Handbuch) — Abschnitt Schiedsrichterzettel: neuer Haken-Text,
  Zeile „Drucken:", „genau einmal" einschränken; Anker
  `<!-- pruef: "Zettel automatisch drucken" in src/pages/SetupWizard.tsx -->`,
  `<!-- pruef: "schon beim Aufruf in die Vorbereitung" in src/pages/SetupWizard.tsx -->`,
  `<!-- pruef: "bei der Feldvergabe" in src/pages/SetupWizard.tsx -->` und ein Anker auf den
  Standardwert in `src-tauri/src/config.rs`.
- `docs/schiedsrichterzettel.md` (Handbuch) — Druckzeitpunkt, Nachdruck, Warnung, Blatt ohne Feld,
  Umschalten.
- `docs/features/schiedsrichterzettel-autodruck.md` — Nicht-Ziele „Kein Autodruck beim Aufruf"
  und „Kein zweiter Zettel" als durch diese Spec/ADR 0082 eingeschränkt markieren.
- `docs/preparation.md`, `docs/meetingpoint.md`, `docs/multi-hall.md` (ein Drucker, Halle auf dem
  Blatt), `docs/schiedsrichter-management.md`.
- `CLAUDE.md` — Tabellenzeile „Zettel-Autodruck" um `PrintConfig.trigger`, `zettel`/`modus` in
  `gedruckt.json`, ADR 0082 ergänzen.
- `docs/roadmap.md`, `docs/changelog.md`.

## Umsetzungs-Hinweise

**Druck-Gedächtnis (Weg A, ADR 0082):** `PrintLogFile` bleibt abwärtskompatibel:

```text
tournament: String
printed:    HashSet<i64>                    // alle gedruckten/vorgemerkten Spiele (wie heute)
zettel:     HashMap<i64, ZettelStand>       // nur Aufruf-Modus; skip_serializing_if leer
modus:      Option<PrintTrigger>            // zuletzt wirksamer Modus; für das Vormerken
ZettelStand { sr: i64, feld: bool, ungueltig: bool }
RAM:        kandidat: HashMap<i64, (sr, seit_ms)>
```

Regel: ein Spiel in `printed` ohne `zettel`-Eintrag ist abgeschlossen; `zettel` ⊆ `printed`.
Alle Prüf-und-Setz-Operationen unter einem Schloss (Muster `merken`): `beanspruche_vorab`,
`beanspruche_nachdruck` (Vergleich und Setzen), `zuruecksetzen`, `stand`, `markiere_feld`,
`markiere_ungueltig`, `kandidat_pflegen`, `modus_angleichen`. Turnierwechsel verwirft auch die
Kandidaten. Der Store lebt im `TabletState` → übersteht Sync-Neustarts.

**Vormerken** zustandsbasiert im ersten Autodruck-Lauf, in dem der wirksame Modus `Aufruf` ist
und `modus != Some(Aufruf)` — also nach `set_tournament` und `binde_meetingpoint`, nicht in
`save_config`. Wirksamer Modus = `None`, solange der Haken aus ist.

**Reihenfolge in `run_once`:** `autodruck` läuft vor `apply_preparation_calls`; ein frisch aufs
Feld gekommenes Spiel kann dort noch als gerufen gelten → Entscheidung zuerst nach `m.status`.

**Auftrag:** `Druckauftrag { match_id, bezug: Feld(String) | Spiel(i64) }` für Titel, Log,
Warnung.

**Schritte (zwei PRs):**

1. PR 1 — Vorabzettel wartender Spiele (`scoresheet.rs` `dokumente`): Hand-vor-BTP-SR, Halle mit
   Rückfall auf den Aufruf, Druckdatum. Wirkt sofort auf den Hand-Vorabzettel. Doku
   `schiedsrichterzettel.md`, `preparation.md`, ADR 0082 Punkt 1, Changelog, Versionsbump.
2. PR 2 — `PrintTrigger` + Config-Tests → `print_log` Datei + API → Entscheidung in `sync.rs`
   (bestehende Tests um `now` ergänzen, bleiben grün) → `autodruck`-Bezug → `types.ts` +
   `SetupWizard` (Auswahl, Ausgrauen, Hinweis „Nachdruck, wenn sich der Schiedsrichter vor der
   Feldvergabe ändert") → restliche Doku, Versionsbump.

**Reviews:** `code-reviewer` je PR (Pflicht). `security-reviewer` nicht nötig (kein neuer
Fremd-Input; Enum durch serde begrenzt; Dateipfad unverändert).

**Version:** `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `package.json` je PR gemeinsam
bumpen; Nummer erst beim Merge festlegen.
