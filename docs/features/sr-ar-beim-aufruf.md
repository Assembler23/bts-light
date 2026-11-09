# Schiedsrichter und Aufschlagrichter schon beim Aufruf zuteilen — Spezifikation

> Status: **abgestimmt 2026-11-09** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Idee der Turnierleitung vom 09.11.2026. Betroffene Crates: src-tauri, src, relay-proto, relay (nur Deploy + Text-Deckel).
> ADR: [0080 — Feste Rotations-Zuteilung beim Aufruf mit Herkunfts-Merker](../adr/0080-rotation-teilt-beim-aufruf-zu.md) (löst [0072](../adr/0072-sr-vorab-zuteilung-vorlaeufig.md) ab).
> Voraussetzungen: #457 (Wartelisten-Dialog) gemergt; Spec [`sr-rotationsmodus`](sr-rotationsmodus.md) (#460, ADR 0079) PR 1+2 gemergt.

## Kontext / Problem

Schiedsrichter (SR) und Aufschlagrichter (AR) erfahren heute erst beim Feldaufruf, dass und mit
wem sie ein Spiel betreuen: Die Rotation (`rotate_court`, ADR 0021/0022) teilt erst zu, wenn das
Spiel aufs Feld kommt. Zeit, sich vorher zu treffen (Absprache, Zettel holen, gemeinsam zum
Feld), gibt es nicht.

Ein Teil existiert: Mit eingeschaltetem **Meetingpoint** bekommt eine gerufene Partie seit
ADR 0072 einen **vorläufigen** SR (`meetingpoint::vorab_sr_verteilen`). Er hat vier Grenzen:
nur mit Meetingpoint, nur SR (kein AR), umverteilbar bis zum Feld, und er steht nur auf der
Meetingpoint-Seite — nicht in der Ansage, nicht im TL-Web, nicht im Vorbereitungs-Panel.

## Zielbild & Erfolgskriterien

- Ruft die TL ein Spiel in Vorbereitung (Turnier-PC, TL-Web oder Aufruf-Automatik), stehen SR
  und AR im selben Moment fest, **auch ohne Meetingpoint**, sobald SR-Verwaltung und die
  jeweilige Rotation (`rotation_sr`/`rotation_ar`) an sind.
- Die Aufruf-Ansage nennt sie („… Schiedsrichter X, Aufschlagrichter Y"), nach den bestehenden
  Besetzungs-Schaltern der Ansage (ADR 0040).
- TL-Web-Warteliste, Vorbereitungs-Panel am Turnier-PC und Meetingpoint-Seite zeigen SR und AR an
  der gerufenen Partie.
- Wer so zugeteilt ist, bleibt es — die Rotation verteilt nicht um; nur TL oder BTP ändern.
- Fehlen Leute, wird mit Lücke gerufen; wer frei wird, rückt still nach.
- Kein Erklärbedarf: keine neue Einstellung; das Verhalten folgt den vorhandenen Rotations-Schaltern.

## Nicht-Ziele

- **Keine Ansage beim Nachrücken.** Die Regel „eine nachträgliche Zuweisung allein löst keine
  Ansage aus" (Spec `schiedsrichter-management`) bleibt.
- Kein Vorabzettel/Autodruck beim Aufruf.
- Kein AR-Check-in am Meetingpoint; „spielbereit" (strikter Modus, `require_umpire`) hängt
  weiter nur am SR.
- Kein hallengebundener SR-Pool (ein Pool über alle Hallen, wie heute).
- Keine Erkennung „Official spielt selbst" → eigener Roadmap-Punkt.
- Keine Obergrenze „nur die ersten N Gerufenen".
- Kein eigener Schalter „Rotation schon beim Aufruf". Es gilt immer, wenn die Rotation an ist;
  der Rückweg führt über `rotation_sr`/`rotation_ar`.
- Kein Schutz für das Restfenster: Ein Hand-Aufruf zwischen zwei Sync-Läufen kann einem gerade
  in BTP belegten, noch nicht rotierten Feld die Person vorwegnehmen. Das Feld bekommt dann die
  nächste freie Person oder bleibt ohne. Das wird hingenommen.
- Aufrufe, die an einem Slave entstehen, erreichen den Master heute nicht; das bleibt so
  (bestehende Grenze, Zuteilung nur am Master).
- Keine Änderung, **wie** gewählt wird (Modus, Rolle, Vorrolle) — das regelt
  [`sr-rotationsmodus`](sr-rotationsmodus.md) (ADR 0079). Diese Spec regelt **wann** und
  **für welches Spiel**.

## Fachliche Regeln

### R-A Zuteilung beim Aufruf
1. Auslöser ist der **Zustand** „aktiver Aufruf" (`MeetingpointStore::active_entries`), kein
   Ereignis. Jeder Sync-Lauf besetzt offene Dienste gerufener Partien; zusätzlich läuft die
   Besetzung **synchron im Aufruf** (Hand-Aufruf und Aufruf-Automatik), bevor der Ansage-Auftrag
   abgelegt wird. Nach einem Sync-Neustart (Speichern der Einstellungen) ist nichts verloren.
2. Besetzt wird je Rolle nur, wenn die Rotation der Rolle an ist **und** mindestens ein Feld der
   Aufruf-Halle (`location_id`) den Dienst will (`court_switches`). Ohne Halle, im
   Ein-Hallen-Turnier oder wenn kein Feld der Halle bekannt ist, zählen alle Felder. Gesperrte
   Felder zählen mit.
3. Ein Dienst ist offen, wenn weder BTP noch Hand ihn besetzt haben und er nicht „ohne" ist
   (`Some(0)`). BTP- und Hand-Einteilungen werden nie ersetzt.
4. Gewählt wird über die eine Wahl-Funktion der Rotation (`waehle_besetzung`, ADR 0079) mit der
   Belegung aus ADR 0078 (im Dienst, reserviert, pausiert) plus allem, was im selben Lauf schon
   vergeben wurde.
5. Nur am Master (`!slave_mode`). Eine **Vergabe-Sperre** verhindert, dass ein Hand-Aufruf und
   ein gleichzeitig laufender Sync-Lauf dieselbe Person zweimal vergeben.

### R-B Bindung und Herkunft
1. Die Zuteilung ist eine echte Einteilung in `OfficialsStore` (Sicht „Hand vor BTP",
   `fuer_schreiben`) und reserviert nach ADR 0078.
2. Ein **Herkunfts-Merker** in `officials-state.json` hält fest, welche Einteilungen die Rotation
   beim Aufruf vergeben hat. Er überlebt `confirm` und Neustart. Er gilt je Rolle nur, solange die
   Einteilung noch genau diese Person zeigt; eine Hand- oder BTP-Änderung löscht ihn.
3. Gerufene Partien reservieren ihre Leute **unabhängig vom Tag** (`planned_time`); die Tagesgrenze
   aus ADR 0078 gilt nur für nicht gerufene wartende Spiele.

### R-C Nachrücken und Vorrang
1. Wird jemand frei, gehen zuerst **leere Felder** (Feld-Rotation in `track_officials`), danach
   gerufene Partien in der **Vergabe-Reihenfolge der Gerufenen** (ADR 0077, inkl. Umsortieren),
   nicht nach Aufrufzeit.
2. Nachrücken ist still: nur Anzeige, keine Ansage.

### R-D Freigabe
1. Endet der Aufruf ohne Feld (Rücknahme, Spiel beendet, Spiel aus BTP verschwunden),
   werden **nur** Rotations-Einteilungen (mit Merker) frei. Hand- und BTP-Einteilungen bleiben.
   Ein **geparkter** Aufruf (Spiel steht auf dem Feld) gibt nicht frei.
2. **„Vom Feld genommen"** gibt nicht frei: Der Aufruf lebt automatisch wieder auf und behält
   seine Besetzung.
3. Kommt das Spiel auf ein Feld, dessen Schalter einen Dienst nicht will, wird die
   Rotations-Einteilung dieser Rolle gelöst — vor jedem Feld-Write (Automatik, TL-Web, Desktop)
   und zustandsbasiert in `track_officials` (Feldvergabe in BTP, Neustart).
4. Die Freigabe hinterlässt **kein** bleibendes `Some(0)`: Steht die Person nur lokal, wird der
   Eintrag entfernt; steht sie schon in BTP, wird 0 geschrieben und das `Some(0)` nach der
   Bestätigung wieder geräumt. Danach kann die Feld-Rotation den Dienst normal besetzen.
5. Rücknahme gibt sofort frei (nicht erst im nächsten Takt).
6. Wird `rotation_sr`/`rotation_ar` mitten im Turnier abgeschaltet, bleiben bestehende
   Rotations-Einteilungen stehen (samt Merker und Freigabe-Regel); es kommt nur nichts Neues hinzu.
   Wird die SR-Verwaltung ganz abgeschaltet, räumt `clear_assignments` wie heute alles, auch die
   Merker.

### R-E „Ohne Schiedsrichter" bleibt
Ein „ohne" (`Some(0)`) an einem wartenden Spiel bleibt stehen, auch wenn BTP ebenfalls niemanden
führt. Bisher fiel es im nächsten Lauf weg (`pruefe_dienst`, ADR 0078) und eine zustandsbasierte
Besetzung hätte die Lücke sofort wieder gefüllt. Ändert danach jemand in BTP auf eine Person,
gewinnt BTP.

### R-F BTP-Write
1. Die Zuteilung geht **sofort** nach BTP, wie jede Hand-Einteilung an einem wartenden Spiel
   (#456, unverändert). Es gibt **keine Weiche**. **Annahme** (vom Nutzer gesetzt, 09.11.2026):
   BTP übernimmt `Official1ID/2ID` an einem Spiel ohne Feld, ohne die Feldzuordnung anzufassen.
2. Die Annahme wird **vor dem Release von PR 2** mit dem Messwerkzeug
   `src-tauri/tests/btp_officials_ohne_feld_probe.rs` geprüft (`#[ignore]`, Muster
   `btp_officials_probe.rs`). Gemessen werden: Write mit `CourtID 0` und ohne CourtID-Element,
   Feldzuweisung unmittelbar danach, Löschen mit 0. Das Ergebnis kommt in `docs/btp_protocol.md`.
   Ist es negativ, wird nicht released. Die Spec bekommt dann eine Weiche „kein Officials-Write an
   Spielen ohne Feld" (Konstante in `officials_entries`, gilt für alle solchen Writes).
3. **Wartende** Zeilen in TL-Web und Desktop zeigen die Sicht „Hand vor BTP" (`fuer_schreiben`).
   Damit ist eine Umteilung sofort sichtbar, auch bevor BTP sie spiegelt. Laufende Felder bleiben
   bei `effective`.

### R-G Ansage
1. Der Erst-Aufruf (`AnnounceJobKind::MeetingpointCall`) trägt die Namen von SR und AR **fertig vom
   Master** (`sr`, `ar`, optional). Das Abspielgerät schlägt nichts nach (ein Slave hat keine
   SR-Daten).
2. Satzbau: nach „Bitte in Halle …", vor dem Meetingpoint-Zusatz; gefiltert über die
   Besetzungs-Schalter der Ansage (ADR 0040, je Gerät). Fehlt eine Rolle, entfällt ihr Teil.
3. Nachrufe und Nachrücken sagen keine Besetzung an.
4. Ältere Abspielgeräte ignorieren die neuen Felder und sagen ohne Besetzung an.
5. Die Feldansage nennt die Besetzung wie heute noch einmal (Bestätigung am Feld).

### R-H Ablösung des vorläufigen Meetingpoint-SR (ADR 0072)
1. Beim ersten Lauf werden vorhandene `vorab_sr` aktiver Aufrufe aus `meetingpoint.json` einmalig
   als Rotations-Einteilung mit Merker übernommen — wenn SR-Rotation an, Rolle offen und die Person
   bekannt, nicht pausiert und nicht anderswo belegt ist. Sonst verfällt der Eintrag. Danach wird
   `vorab_sr` geleert und nicht mehr geschrieben (bleibt lesbar).
2. `vorab_sr_verteilen`, `uebernimm_vorab_sr`, `set_vorab_sr`, `alle_vorab_sr` und die interne
   Unterscheidung `SrLage.vorab` entfallen. SR-Check-in und `require_umpire` laufen auf der
   wirksamen Einteilung. Ein SR-Check-in verfällt, wenn der SR der Partie wechselt.

### R-I Anzeige
1. **TL-Web:** die Marke „SR: … · AR: …" an Wartelisten-Zeilen (#449/#456) zeigt die Zuteilung;
   kein neuer `tl.html`-Code, kein Flag.
2. **Vorbereitungs-Panel (Desktop):** `PreparationCandidate` bekommt `sr`/`ar`; Marke an der
   gerufenen Partie.
3. **Meetingpoint-Seite:** `MpPartie` bekommt `ar` (nur Anzeige); SR wie heute.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - `src-tauri/src/tablet/officials.rs` — Herkunfts-Merker, Freigabe, „ohne" dauerhaft,
    Vergabe-Sperre, `Belegung` mit Gerufenen.
  - `src-tauri/src/tablet/meetingpoint.rs` — `gerufene_besetzen` (ersetzt `vorab_sr_verteilen`),
    `hand_aufruf` (eintragen → besetzen → Ansage), Migration, `view` mit AR, Check-in-Aufräumen.
  - `src-tauri/src/sync.rs` — `run_once`-Reihenfolge, `track_officials` (Lösen am Feld,
    Übernahme-Block raus), `auto_assign` (Feld-Write-Helfer).
  - `src-tauri/src/tablet/state.rs` — `MeetingpointCall{sr, ar}`, `officials_for_court_write`.
  - `src-tauri/src/commands.rs`, `src-tauri/src/tablet/tl.rs` — Rücknahme sofort, Feld-Writes über
    den Helfer, `PreparationCandidate.sr/ar`, Sicht „Hand vor BTP" an wartenden Zeilen.
  - `relay-proto` — `MpPartie.ar`. `relay/` — Text-Deckel auf `ar`, Deploy.
  - `src/` — `types.ts`, `io/announceJobs.ts`, `io/announcer.ts`, neues `io/prepBesetzung.mjs`,
    `pages/PreparationPanel.tsx`, `io/meetingpoint.mjs` + Inline-Kopie in
    `src-tauri/assets/meetingpoint.html`.
- **Architekturregeln:**
  - R1: Desktop nur über Tauri-Commands (`preparation_candidates`).
  - R2: BTP bleibt Wahrheit — BTP-Einteilungen werden nie ersetzt, BTP-Änderungen gewinnen,
    keine Court→Match-Zuordnung wird erfunden. Dass der Officials-Write an Spielen ohne Feld die
    Feldzuweisung nicht antastet, ist eine Annahme und wird vor dem Release gemessen (R-F).
  - R3: LAN und Cloud teilen den Kern; Ansage-Aufträge reisen nur über LAN/lokal (Cloud nicht
    betroffen); Meetingpoint-Seite in beiden Wegen (Relay-Deploy).
  - R4/R5/R6: unberührt.
- **Konfiguration & Abwärtskompatibilität:** kein neues Config-Feld. Neue Datei-Felder
  (`officials-state.json` Herkunfts-Merker, `MeetingpointCall.sr/ar`, `MpPartie.ar`) sind
  `serde(default)` + `skip_serializing_if`. `meetingpoint.json.vorab_sr` bleibt lesbar.
- **Datenschutz:** nur SR/AR-Namen (Anzeigename), die schon heute in TL-Zustand, Ansage und
  Meetingpoint-Seite stehen. Kein Verein, keine Sperrliste, kein Geburtsjahr. Der
  Datenschutz-Wächter für `MpPartie` wird auf `ar` erweitert.
- **Abhängigkeiten:** BTP-Messung (R-F); keine neue Cargo-/npm-Dependency; Relay-Deploy beim
  Merge von PR 4.

## Akzeptanzkriterien

**Zuteilung**
- [ ] AK1: SR-Verwaltung an, `rotation_sr` und `rotation_ar` an, Meetingpoint **aus**: Ein Aufruf
  in Vorbereitung teilt im selben Aufruf SR und AR zu (Turnier-PC, TL-Web, Aufruf-Automatik).
- [ ] AK2: `rotation_ar` aus → nur SR wird zugeteilt; beide aus → keine Zuteilung.
- [ ] AK3: Kein Feld der Aufruf-Halle will AR → kein AR; ohne Halle zählt irgendein Feld.
- [ ] AK4: Steht in BTP oder von Hand schon jemand, wird er nicht ersetzt; „ohne" wird nicht
  aufgefüllt.
- [ ] AK5: Zugeteilte Personen werden von der Feld-Rotation und anderen Aufrufen nicht eingesetzt
  (reserviert), auch wenn das gerufene Spiel für einen späteren Tag geplant ist.
- [ ] AK6: Am Slave teilt ein lokaler Aufruf nichts zu.
- [ ] AK7: Ein Hand-Aufruf parallel zu einem Sync-Lauf vergibt keine Person doppelt.
- [ ] AK8: Nach einem Sync-Neustart sind Zuteilungen, Merker und offene Lücken unverändert; der
  nächste Lauf füllt Lücken.

**Nachrücken**
- [ ] AK9: Sind beim Aufruf nicht genug Personen frei, wird mit Lücke gerufen; wird jemand frei und
  kein Feld ist leer, bekommt ihn die erste gerufene Partie mit Lücke in Vergabe-Reihenfolge
  (ADR 0077), auch nach Umsortieren.
- [ ] AK10: Ist gleichzeitig ein Feld ohne SR belegt, bekommt das Feld die frei gewordene Person.
- [ ] AK11: Nachrücken erzeugt keinen Ansage-Auftrag.

**Freigabe**
- [ ] AK12: Rücknahme oder Parken gibt Rotations-Einteilungen sofort frei; Hand- und
  BTP-Einteilungen bleiben.
- [ ] AK13: „Vom Feld genommen" behält die Besetzung.
- [ ] AK14: Kommt das Spiel auf ein Feld ohne AR-Dienst, wird der Rotations-AR gelöst; ein Hand-AR
  bleibt.
- [ ] AK15: Nach einer Freigabe bleibt kein `Some(0)`; die Feld-Rotation kann den Dienst später
  besetzen. Gilt auch, wenn die Person schon in BTP stand oder ein Write unterwegs war.
- [ ] AK16: Eine Hand- oder BTP-Änderung an einer Rotations-Einteilung löscht deren Merker; eine
  spätere Rücknahme lässt die neue Person stehen.
- [ ] AK17: `rotation_sr` mitten im Turnier aus → bestehende Rotations-Einteilungen bleiben; neue
  Aufrufe bekommen keinen SR.

**„Ohne" und BTP**
- [ ] AK18: „Ohne Schiedsrichter" an einem wartenden Spiel bleibt über beliebig viele Läufe stehen,
  wenn BTP niemanden führt; trägt BTP danach eine Person ein, gewinnt sie.
- [ ] AK19: Die Zuteilung beim Aufruf steht ohne Feld in BTP (`Official1ID/2ID`), und die
  Feldzuordnung bleibt unverändert. Wird am Testturnier mit dem Messwerkzeug geprüft, vor dem
  Release.
- [ ] AK20: Umteilen gegen einen BTP-Vorabwert zeigt in TL-Web und Desktop an der wartenden Zeile
  sofort die neue Person.

**Ansage**
- [ ] AK21: Der Erst-Aufruf-Auftrag trägt `sr`/`ar` (fertige Namen); die Ansage nennt sie nach den
  Besetzungs-Schaltern; fehlt eine Rolle, entfällt ihr Teil; DE und EN.
- [ ] AK22: Ein Auftrag der Aufruf-Automatik trägt die Besetzung desselben Laufs.
- [ ] AK23: Nachrufe nennen keine Besetzung; ein alter Auftrag ohne `sr`/`ar` bleibt lesbar.

**Ablösung + Anzeige**
- [ ] AK24: Ein vorhandener `vorab_sr` wird beim ersten Lauf feste Rotations-Einteilung mit Merker
  und danach geleert; ist die Person belegt oder pausiert, verfällt er.
- [ ] AK25: Ein SR-Check-in am Meetingpoint verfällt, wenn der SR der Partie wechselt;
  `require_umpire` prüft die wirksame Einteilung.
- [ ] AK26: Vorbereitungs-Panel und Meetingpoint-Seite zeigen SR und AR der gerufenen Partie; die
  TL-Web-Warteliste zeigt sie über die bestehende Marke.

## Tests

TDD je PR, `cargo test` grün, `cargo clippy --workspace --all-targets -- -D warnings`, `npm run build`.

- `officials.rs`: `ohne_bleibt_stehen_solange_btp_niemanden_fuehrt`,
  `ohne_weicht_einer_aenderung_in_btp`, `rotation_beim_aufruf_merkt_ihre_herkunft`,
  `herkunft_ueberlebt_confirm_und_neustart`, `hand_einteilung_loescht_die_herkunft`,
  `btp_umteilung_loescht_die_herkunft`, `freigabe_nur_lokal_entfernt_ohne_some0`,
  `freigabe_mit_btp_wert_schreibt_null_und_raeumt_danach`,
  `freigabe_nach_spaet_gelandetem_write_raeumt_nach`,
  `nach_freigabe_besetzt_die_feldrotation_wieder`, `alte_datei_ohne_herkunft_bleibt_lesbar`,
  `abschalten_der_verwaltung_raeumt_auch_die_herkunft`,
  `gerufenes_spiel_spaeteren_tages_reserviert`.
- `meetingpoint.rs`: `aufruf_teilt_sr_und_ar_zu_auch_ohne_meetingpoint`,
  `nur_sr_wenn_ar_rotation_aus`, `halle_ohne_feld_mit_ar_teilt_keinen_ar_zu`,
  `ohne_halle_zaehlt_irgendein_feld`, `engpass_ruft_mit_luecke_und_rueckt_nach`,
  `nachruecken_in_vergabe_reihenfolge_nicht_aufrufzeit`,
  `nachruecken_nach_umsortieren_der_gerufenen`,
  `ohne_sr_am_gerufenen_spiel_wird_nicht_aufgefuellt`,
  `btp_oder_hand_einteilung_wird_nicht_ersetzt`, `ruecknahme_gibt_nur_rotation_frei_hand_bleibt`,
  `vom_feld_genommen_behaelt_die_besetzung`, `aufruf_endet_ohne_feld_paarung_weg_gibt_frei`,
  `migration_vorab_sr_wird_feste_zuteilung_und_geleert`, `migration_verwirft_belegten_vorab_sr`,
  `hand_aufruf_besetzt_vor_dem_ansage_auftrag`, `hand_aufruf_am_slave_besetzt_nicht`,
  `sr_checkin_verfaellt_bei_sr_wechsel`, `view_traegt_ar_namen`,
  `require_umpire_auf_fester_einteilung`.
- `sync.rs`: `auto_aufruf_hat_sr_ar_vor_dem_ansage_auftrag_im_selben_lauf`,
  `leeres_feld_hat_vorrang_vor_gerufener`, `zustandsbasiert_nach_sync_neustart`,
  `auto_assign_loest_rotation_an_feld_ohne_ar`,
  `track_officials_loest_rotation_an_feld_ohne_dienst_nach_btp_vergabe`,
  `officials_entries_schreibt_nicht_an_spiel_ohne_feld`,
  `officials_entries_schreibt_mit_feldzuweisung`, `hand_einteilung_warteliste_bleibt_lokal_bis_feld`,
  `rotation_abgeschaltet_laesst_bestehende_stehen`, `vergabe_sperre_verhindert_doppelvergabe`,
  `nachruecken_erzeugt_keinen_ansage_auftrag`.
- `state.rs`: `meetingpoint_call_mit_namen_roundtrip`, `alter_auftrag_ohne_namen_lesbar`.
- `tl.rs`: `retract_preparation_gibt_sofort_frei`,
  `feld_write_aus_tl_loest_rotation_an_feld_ohne_dienst`,
  `wartende_zeile_zeigt_hand_vor_btp_solange_weiche_unterdrueckt`.
- `relay-proto`: `mp_partie_ar_roundtrip_und_weggelassen_wenn_leer`; Datenschutz-Wächter für
  `MpPartie` um `ar`.
- JS: neu `scripts/test-vorbereitung-besetzung.mjs` (DE/EN, nur SR, leer, Nachruf ohne Besetzung;
  in `ci.yml`), `scripts/test-meetingpoint.mjs` (`ar`), `scripts/test-inline-kopien.mjs` grün.
- Optional: Messwerkzeug `tests/btp_officials_ohne_feld_probe.rs` (`#[ignore]`) für R-F.3.
- Manuell (Feldtest): zwei Hallen, Aufruf-Automatik, ein SR knapp; Ansage am LAN-Slave hören;
  Meetingpoint-Seite über Cloud.

## Risiken & Rollback

- **Verhalten ändert sich mit dem Update:** Die Rotation zieht früher, gerufene Partien binden
  SR/AR. Spiele ohne Aufruf finden am Feld seltener jemanden; die Auto-Vergabe wartet eher
  (ADR 0078). Update am Turnieranfang einspielen; die TL löst Einzelfälle von Hand.
- **Annahme BTP-Write ohne Feld:** Ist sie falsch, droht die CourtID-0-Klasse (#450), und das
  Feature vervielfacht diese Writes. Gegenmittel: Messung vor dem Release, sonst Weiche
  nachrüsten (R-F).
- **Doppelvergabe** bei vergessener Vergabe-Sperre → Test + gezielter Review.
- **Rollback auf eine ältere App:** ignoriert und verwirft den Herkunfts-Merker beim nächsten
  Schreiben (sichere Richtung: nach erneutem Update gelten diese Einteilungen als Hand und werden
  bei Rücknahme nicht frei); lässt „ohne" wieder los; schreibt lokale Einteilungen wartender Spiele
  wieder mit `CourtID 0`; liest `meetingpoint.json` ohne `vorab_sr` problemlos.
- **Wire:** Ein alter Relay verwirft `ar` (nur Anzeige fehlt); alte Slaves sagen ohne Besetzung an.

## Offene Fragen / Annahmen

- **Annahme** (Nutzer, 09.11.2026): BTP nimmt `Official1ID/2ID` an einem Spiel ohne Feld an,
  ohne die Feldzuweisung anzufassen. Sie wird vor dem Release von PR 2 gemessen (R-F).
- Annahme: Die Reihenfolge gegenüber `sr-rotationsmodus` bleibt wie abgestimmt (dessen PR 1+2
  zuerst, dann diese Spec, dann dessen PR 3–5). Kommt es anders, wählt `gerufene_besetzen`
  übergangsweise über `rotate_court`/`next_free`.

## Betroffene Doku-Dateien

- `docs/schiedsrichter-management.md` (Handbuch: Zuteilung beim Aufruf, Nachrücken, Freigabe,
  „ohne" bleibt — mit `pruef:`-Ankern)
- `docs/meetingpoint.md` (Abschnitt „Schiedsrichter schon beim Aufruf" neu, AR-Anzeige)
- `docs/preparation.md` (Aufruf besetzt, Marke im Panel, Ansage)
- `docs/announcements.md` (Besetzung im Aufruf, ältere Abspielgeräte)
- `docs/turnierleitung-web.md` (Marke an gerufenen Zeilen, Sicht Hand vor BTP)
- `docs/oberflaechen.md` (Vorbereitungs-Panel)
- `docs/cloud-relay.md` (`MpPartie.ar`, Deploy-Reihenfolge)
- `docs/btp_protocol.md` (Messung Officials an Spiel ohne Feld)
- `docs/multi-hall.md` (ein Pool, Halle-prüfen-Regel)
- `docs/features/tl-warteliste-schiedsrichter.md` (Nachtrag: Sicht an wartenden Zeilen)
- `docs/roadmap.md`, `docs/changelog.md` je Version
- CLAUDE.md-Doku-Tabelle: Zeile „Schiedsrichter in der Warteliste" um diese Spec ergänzen

## Umsetzungs-Hinweise

Voraussetzungen: #457 gemergt; `sr-rotationsmodus` PR 1+2 gemergt. Versionsnummer erst beim Merge
festlegen (parallele PRs). Jeder Code-PR: Version gemeinsam in `src-tauri/Cargo.toml`,
`src-tauri/tauri.conf.json`, `package.json`; `docs/changelog.md`; `code-reviewer`.

1. **PR 0 — docs(spec):** diese Spec, ADR 0080 (proposed → accepted mit Freigabe), Roadmap.
   ADR 0072 wird mit PR 2 auf „superseded".
2. **PR 1 — fix(schiedsrichter): „ohne" bleibt + Messwerkzeug.**
   - `pruefe_dienst` (officials.rs ~298) hält `wert==0 && btp==0` mit `ausgang=Some(0)`.
   - Messwerkzeug `tests/btp_officials_ohne_feld_probe.rs`; die Messung muss vor dem Release von PR 2 laufen.
   - Sicht „Hand vor BTP" an wartenden Zeilen (tl.rs
   `court_officials`-Aufruf der Queue, commands.rs Desktop-Sicht). Test
   `eine_von_btp_bestaetigte_zuweisung_wird_lokal_losgelassen` (officials.rs ~1582) anpassen.
3. **PR 2 — feat(schiedsrichter): Rotation teilt beim Aufruf zu.** Herkunfts-Merker
   (`vorab_rotation`, Freigabe-Zustand je Rolle), `gerufene_besetzen` in `run_once` statt
   `vorab_sr_verteilen` (sync.rs ~2572, nach `track_officials` ⇒ Feld-Vorrang), Automatik:
   eintragen → besetzen → Aufträge; `hand_aufruf` (meetingpoint.rs ~1032) ebenso; `retract`
   sofort; Halle-Regel (`court_infos[].location_id` + `court_switches`);
   `officials_for_court_write` vor allen Feld-Writes (sync.rs ~2359, tl.rs ~1865, commands.rs
   ~3578); Lösen in `track_officials`; Vergabe-Sperre; `Belegung` mit Gerufenen ohne Tagesgrenze;
   Migration `nimm_vorab_sr`; Vorab-Wege entfernen (sync.rs ~1806–1831, ~2339–2348);
   `bereinige_checkins` prüft SR-ID.
4. **PR 3 — feat(ansage): Aufruf nennt SR/AR + Marke im Vorbereitungs-Panel.**
   `MeetingpointCall{sr, ar}` (serde default), `types.ts`, `announceJobs.ts` (Kommentar „keine
   Namen" anpassen), `announcer.ts` Segmente + SSML über `io/prepBesetzung.mjs`,
   `PreparationCandidate.sr/ar`, `PreparationPanel.tsx`, JS-Test + CI.
5. **PR 4 — feat(meetingpoint): Seite zeigt SR und AR.** `relay-proto` `MpPartie.ar`, `view`,
   Relay-Text-Deckel, `meetingpoint.mjs` + Inline-Kopie. Relay deployt beim Merge vor dem App-Tag.
   **security-reviewer** (offene Seite ohne Anmeldung, ADR 0074).

PR 3 und PR 4 sind nach PR 2 unabhängig voneinander.
