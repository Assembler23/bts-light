# Feinschliff Turnierleitungssicht — Spezifikation

> Status: **abgestimmt 2026-08-18** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Anforderung der Turnierleitung vom 18.08.2026 (vier Punkte).
> Betroffene Crates: `src-tauri/`, `relay/`, `relay-proto/`, `src/`.
> ADR: [`docs/adr/0036-hallen-achse-im-messwert.md`](adr/0036-hallen-achse-im-messwert.md)
> (neu, für Punkt 1) · Nachtrag an [ADR 0007](adr/0007-zaehltafelbediener.md) (Punkt 2).

## Kontext / Problem

Vier voneinander unabhängige Reibungspunkte aus dem laufenden Turnierbetrieb,
alle an der Turnierleitungs-Oberfläche:

1. **Die Spielzeiten-Statistik beantwortet nur eine Frage.** Sie zeigt
   ausschließlich Zeilen je Klasse × Disziplin. Die Turnierleitung will aber
   auch wissen: Wie lange dauern die A-Spiele im Schnitt? Sind Doppel
   langsamer als Einzel? **Läuft eine Halle systematisch langsamer als die
   andere?** Letzteres ist die Frage, wegen der man bei einem Zwei-Hallen-
   Turnier überhaupt auf die Uhr schaut — und genau die kann heute niemand
   beantworten.
2. **Der Zähltafelbediener wird einmal gerufen und dann nie wieder.** Beim
   Feld-Aufruf sagt die Anlage „Tabletbedienung: {Name}". Kommt niemand,
   bleibt der Turnierleitung nur, die Spieler erneut zu rufen — für die
   Bedienung gibt es keinen Nachruf, obwohl es ihn für beide Spielparteien
   gibt. Der Baustein ist in
   [`zaehltafelbediener.md`](../zaehltafelbediener.md) seit ADR 0007 als
   offen geführt.
3. **„Fangt endlich an" lässt sich nicht ansagen.** Ein Feld ist besetzt,
   die Spieler stehen da, es fällt kein Punkt. Die Turnierleitung sieht das
   an der roten Aufruf-Uhr, hat aber kein Mittel, es der Halle zu sagen —
   außer selbst hinzulaufen oder einen Aufruf zu wiederholen, der die
   Aufruf-Zählung verfälscht.
4. **Spielerlinks gibt es nur in der Warteliste.** Seit 17.08.2026 verlinkt
   die Warteliste jeden Spieler auf seine öffentliche badhub-Seite. In den
   laufenden Feld-Kacheln und in der Liste der beendeten Spiele steht
   derselbe Name als toter Text — obwohl das genau die Stellen sind, an
   denen man während des Turniers nachschlägt.

## Zielbild & Erfolgskriterien

Nach der Umsetzung kann die Turnierleitung am Tablet die Spielzeiten nach
vier Achsen auswerten, Zähltafelbediener genauso nachrufen wie Spieler, ein
Feld zum Spielbeginn auffordern und jeden angezeigten Spieler direkt
nachschlagen.

**Erfolgskriterien:**

- **E-1** Am nächsten Turnier von der Turnierleitung **ohne Rückfrage**
  bedient — kein Punkt braucht eine Erklärung durch die Entwicklung.
- **E-2** Die TL-Web-Renderzeit (`tlRenderMessen`) bleibt nach allen vier
  Punkten im Rahmen des vorher gemessenen Werts. Vor PR 1 und nach dem
  letzten PR je einmal messen und den Wert im PR-Text festhalten.
- **E-3** Bei einem Zwei-Hallen-Turnier liefert die Hallen-Achse nach einem
  Turniertag eine belastbare Aussage — beide Hallen haben ≥ 3 Messwerte.

## Nicht-Ziele

- **N-1** Keine Statistik-Anzeige außerhalb TL-Web (keine Monitore, kein
  Desktop-Dashboard, kein badhub-Push).
- **N-2** Kein neuer BTP-Schreibpfad in irgendeinem der vier Punkte. BTP
  bleibt unangetastet (R2).
- **N-3** **Die Prognose-Fallback-Kette bleibt unverändert**
  (Klasse × Disziplin → Klasse → Turnier → Default). Die Hallen-Achse ist
  reine Auswertung und beeinflusst weder Wartelisten-Prognose noch
  Live-Restzeit.
- **N-4** Keine Spielerlinks auf Monitoren oder im Liveticker. Die
  Lizenznummer bleibt auf den TL-Zustand beschränkt; `CourtOverview` und
  `MatchBrief` werden **nicht** angefasst.
- **N-5** Punkt 3 erklingt **nicht** in einer per Relay angebundenen fernen
  Halle — wie alle anderen TL-Web-Ansagen auch. Der Ansage-Auftrag entsteht
  am Turnier-PC, und ein Cloud-Slave holt bewusst keine Aufträge ab.
- **N-6** Punkt 2 rührt die Zuweisungslogik aus ADR 0007 **nicht** an. Kein
  Bediener beim Vorbereitungs-Aufruf, keine Reservierung je Match, keine
  Änderung an „bevorzugt der Verlierer des Vorspiels auf diesem Feld".
- **N-7** Der Bediener-Zähler wird **nicht** ausgeliefert. Er bestimmt allein
  die Ansage-Stufe; die Knopf-Beschriftung bleibt konstant. Anders als beim
  Spieler-Aufruf steht hinter dem Bediener-Nachruf keine kampflose Wertung,
  also gibt es auch keinen „letzten Aufruf" mit Rechtsfolge, den man
  anzeigen müsste.

## Betroffene Komponenten / Architekturregeln / Daten

**Crates/Komponenten**

| Punkt | Betroffen |
|---|---|
| 1 | `tablet/match_times.rs` (`MatchTimeEntry`, `reconcile`) · `sync.rs` (`reconcile_match_times`) · `tablet/predict.rs` (`Measurement`, `TimeStats`, `StatsRow`, `time_stats`) · `tablet/tl.rs` (`TlTimeStats`, `build_state_limited`, `state_for_relay`) · `config.rs` (`TlDisplaySettings`) · `relay-proto` (`TlDisplaySettingsWire`) · `assets/tl.html` (`renderTimes`, Profil-Editor) |
| 2 | `relay-proto` (`TlAction::AnnounceScorekeeper`) · `tablet/state.rs` (`AnnounceJobKind`, `scorekeeper_call_stages`) · `tablet/tl.rs` (`apply_state_action`, `action_fingerprint`, `action_label`) · `assets/tl.html` (⋯-Menü) · `src/io/announcer.ts`, `announceCourt.ts`, neue `src/io/scorekeeperCallText.mjs` · `src/components/AnnounceJobPlayer.tsx` · `src/types.ts` |
| 3 | wie 2, mit `TlAction::AnnounceStartPlay` und `src/io/startPlayText.mjs`; zusätzlich `commands.rs` (`pending_announce_jobs`) |
| 4 | `tablet/tl.rs` (`TlCourt`, `TlFinished`, `build_state_limited`, beide Wächter-Tests) · `assets/tl.html` (`pairOf`-Aufruf, `siegerZuerst`, `finRow`) |

**Architekturregeln (CLAUDE.md R1–R6)**

- **R1** — eingehalten. Die Desktop-Seite spricht den Kern über
  Tauri-Commands, TL-Web über `TlAction`. Kein direkter Netzwerkzugriff.
- **R2** — eingehalten. Die Statistik wertet ausschließlich **eigene**
  Messwerte aus; Halle, Klasse und Disziplin kommen aus dem BTP-Snapshot und
  werden nur übernommen, nie erfunden. Kein Punkt schreibt nach BTP (N-2).
- **R3** — **beide Wege sind betroffen.** Punkt 1 und 4 ändern `tl.html`, das
  ins Relay einkompiliert ist (`include_str!`) — Cloud-Geräte sehen die
  Änderung erst nach dem Relay-Rebuild. Punkt 2 und 3 bringen je eine neue
  `TlAction`, die ein altes Relay mit **422** abweist. Daraus folgt die
  Ausroll-Regel (siehe Umsetzungs-Hinweise).
- **R4** — unberührt.
- **R5** — unberührt. Kein Punkt nimmt Ergebnisse entgegen.
- **R6** — unberührt.

**Konfiguration & Abwärtskompatibilität**

- `TlDisplaySettings` bekommt `time_stats_axis` (neues Enum, Default
  `Group` = heutige Ansicht). Bestehende Profile ohne das Feld lesen sich als
  `Group` — sichtbares Verhalten ändert sich für niemanden, der nichts
  umstellt.
- `MatchTimeEntry` bekommt `hall: String` mit `#[serde(default)]`.
  `match-times.json` eines laufenden Turniers bleibt lesbar; alte Messwerte
  tragen eine leere Halle.
- `TlCourt`/`TlFinished` und `TlTimeStats` wachsen rein additiv, alle neuen
  Felder mit `#[serde(default)]` für ältere Gegenstellen.
- `identifier` `de.badhub.btslight` und der Updater-Pfad
  `download/bts-light/` bleiben unangetastet.

**Datenschutz**

Punkt 4 **hebt eine bewusste Beschränkung auf**: Lizenznummern reisen bisher
nur in den Wartelisten-Einträgen. Der aufgeschriebene Zweck der Ausweitung
lautet: *Nachschlagen der Spielerhistorie auf der öffentlichen badhub-Seite
während des Turniers.* Die Lizenznummer ist der öffentliche URL-Schlüssel
genau dieser Seite (`/spieler/<Nr>/live`); ein weiteres personenbezogenes
Feld kommt nicht hinzu.

**Unverändert draußen bleiben:** Geburtsjahr (überall),
Check-In-Spielernamen, Sperrlisten und Vereinszugehörigkeit der
Schiedsrichter. Beide Wächter-Tests bleiben bestehen und werden angepasst,
nicht abgeschafft.

**Abhängigkeiten**

Keine neue Cargo- oder npm-Dependency. Keine BTP-Protokolländerung — die
Halle kommt aus dem bereits geparsten Snapshot
(`BtpSnapshot::court_location_name`). Kein badhub-Endpunkt betroffen. Der
Relay muss vor den Client-Releases von Punkt 2 und 3 neu deployt sein
(läuft automatisch beim `main`-Merge).

## Akzeptanzkriterien

### Punkt 1 — Spielzeiten-Statistik mehrachsig

- [ ] **A1.1** Das Panel „Spielzeiten" zeigt die Auswertung wahlweise nach
      **Klasse**, **Disziplin**, **Halle** oder **Klasse × Disziplin**.
- [ ] **A1.2** Ohne Zutun zeigt es **Klasse × Disziplin** — das heutige
      Verhalten. Ein bestehendes Profil, das nichts vom neuen Feld weiß,
      landet ebenfalls dort.
- [ ] **A1.3** Die gewählte Achse ist eine **Profil-Einstellung**: Sie
      überlebt das Neuladen der Seite und gilt für alle Geräte mit diesem
      Profil.
- [ ] **A1.4** Jede Achse zeigt **alle** ihre Zeilen ab einer Messung; die
      Panel-Vorschau zählt die Zeilen der **aktiven** Achse.
- [ ] **A1.5** Die Summe der Messwerte (`count`) ist über alle vier Achsen
      **identisch** — jede Achse zerlegt dieselbe Menge.
- [ ] **A1.6** Bei einem Ein-Hallen-Turnier ist die Hallen-Achse **nicht
      wählbar**.
- [ ] **A1.7** Die Halle wird bei der **ersten** Feldzuweisung gestempelt.
      Wechselt ein Spiel danach die Halle, bleibt es in der Zeile seiner
      ersten Halle.
- [ ] **A1.8** Messwerte ohne Halle (vor dem Update gestempelt) stehen in
      einer eigenen Zeile „ohne Halle" und verfälschen keine andere.
- [ ] **A1.9** Eine `match-times.json` ohne `hall`-Feld lässt sich
      fehlerfrei lesen.
- [ ] **A1.10** **Negativfall Prognose:** `group_duration` und `group_times`
      liefern mit und ohne Hallen-Messwerte **exakt dieselben** Werte. Die
      Wartelisten-Prognose und die Live-Restzeit ändern sich nicht (N-3).
- [ ] **A1.11** Die vier Achsen werden **einmal je Messwert-Generation**
      berechnet, nicht je Poll. Zwei aufeinanderfolgende TL-State-Bauten ohne
      neuen Messwert liefern denselben Cache-Zeiger.

### Punkt 2 — Zähltafelbediener-Nachruf

- [ ] **A2.1** Ist einem Feld ein Zähltafelbediener zugewiesen, bietet das
      ⋯-Menü der Feld-Kachel einen Nachruf an.
- [ ] **A2.2** Der Nachruf sagt: **„Feld {X}. {Namen}, bitte als
      Tabletbedienung melden."** — englisch **„Court {X}. {names}, please
      report as scoreboard operator."**
- [ ] **A2.3** Ab dem zweiten Nachruf steht das Stufenwort davor
      („Zweiter Aufruf.", dann „Dritter und letzter Aufruf."), höchstens bis
      Stufe 3.
- [ ] **A2.4** **Negativfall:** Ist **kein** Bediener zugewiesen, ist der
      Knopf **nicht sichtbar** — nicht sichtbar und deaktiviert, sondern gar
      nicht da. Das gilt für alle drei Fälle: leere Warteschlange, Feld mit
      abgeschalteter Bediener-Vergabe, global ausgeschaltete Verwaltung.
- [ ] **A2.5** **Der Nachruf lässt die Spieler-Aufrufe unberührt:** Nach
      einem Bediener-Nachruf sind `call_stages` und `prep_call_stages`
      unverändert, und die an der Kachel angezeigte Nachruf-Zahl der Spieler
      steht still.
- [ ] **A2.6** Wechselt das Spiel auf dem Feld, beginnt der Bediener-Zähler
      wieder bei Stufe 1.
- [ ] **A2.7** Der Nachruf ist **beliebig oft** auslösbar; zwei Auslösungen
      erzeugen zwei Ansagen (Doppeltipp wird entprellt, Wiederholung nicht).
- [ ] **A2.8** **Negativfall:** Hängt kein Ansage-Gerät in der Halle, meldet
      die Seite das als Warnhinweis, die Aktion gilt trotzdem als ausgeführt
      — wie bei allen anderen Ansagen.

### Punkt 3 — Ansage „Bitte mit dem Spielen beginnen"

- [ ] **A3.1** Das ⋯-Menü der Feld-Kachel bietet die Ansage an, solange das
      Feld belegt ist und **noch kein Punkt gefallen** ist.
- [ ] **A3.2** Die Ansage lautet **„Feld {X}. Bitte mit dem Spielen
      beginnen."** — englisch **„Court {X}. Please start playing."** — mit
      Gong und **ohne** Paarung.
- [ ] **A3.3** **Die Ansage ist kein Aufruf:** `call_stages` bleibt
      unverändert, an der Kachel springt kein Aufruf-Abzeichen, und die
      Anzeige „n. Aufruf fällig" ändert sich nicht.
- [ ] **A3.4** Sie ist beliebig oft auslösbar.
- [ ] **A3.5** Sie geht nur in die **Halle des Felds**.
- [ ] **A3.6** **Negativfall:** Steht auf dem Feld kein Spiel, wird die
      Aktion abgelehnt.
- [ ] **A3.7** **Negativfall Mischbetrieb:** Ein Ansage-Gerät mit **älterem**
      Stand, das eine Auftragsliste mit einer unbekannten Ansageart abholt,
      überspringt den unbekannten Auftrag und **spricht die übrigen normal**.
      Es darf weder die ganze Charge verwerfen noch einen unbekannten Auftrag
      als anderen Typ aussprechen.

### Punkt 4 — Spielerlinks

- [ ] **A4.1** In den laufenden Feld-Kacheln ist jeder Spielername mit
      Lizenznummer auf `badhub.de/spieler/<Nr>/live` verlinkt.
- [ ] **A4.2** In der Liste der beendeten Spiele ebenso.
- [ ] **A4.3** **Negativfall:** Ein Spieler ohne Lizenznummer bleibt
      unverlinkt; sein Name steht unverändert da. Gilt auch für Walkover- und
      in BTP eingetragene Papier-Ergebnisse.
- [ ] **A4.4** Namen und Nummern werden beim Einsetzen ins HTML **escapt**
      bzw. URL-kodiert.
- [ ] **A4.5** **Datenschutz-Invariante:** Der TL-Zustand trägt weiterhin
      **kein** Geburtsjahr, keine Check-In-Spielernamen und keine
      Sperrlisten- oder Vereinsdaten der Schiedsrichter.

### Übergreifend

- [ ] **A0.1** **Relay-Größengrenze:** Ein Turnier mit 26 belegten Feldern
      (Doppelpaarungen), 30 beendeten Spielen und 120 wartenden Spielen —
      alle mit Lizenznummern, alle vier Statistik-Achsen gefüllt — bleibt
      unter `MAX_TL_STATE_LEN`. Reicht es nicht, kürzt der Zustand in der
      Reihenfolge **`queue` → `checkin_times` → `time_stats` → `finished`**
      und geht **nicht** verloren.
- [ ] **A0.2** Beide Wächter-Tests laufen grün und decken die neuen Felder
      bewusst ab.

## Tests

**Punkt 1 — Rust**

`match_times.rs`: `der_erststempel_haelt_auch_die_halle_fest` ·
`ein_hallenwechsel_aendert_den_hallenstempel_nicht` ·
`nach_dem_e4_reset_stempelt_die_neue_halle` ·
`ein_alter_stand_ohne_halle_bleibt_lesbar` (JSON ohne `hall` deserialisieren
— der `#[serde(default)]`-Beweis, A1.9).

`sync.rs`: `der_e4_stempel_traegt_die_halle_des_felds` (Mehr-Hallen-Snapshot)
· `ein_ein_hallen_turnier_stempelt_eine_leere_halle`.

`predict.rs`: `die_klassen_achse_fasst_alle_disziplinen_zusammen` ·
`die_disziplin_achse_fasst_alle_klassen_zusammen` ·
`die_hallen_achse_zaehlt_je_halle` ·
`messwerte_ohne_halle_stehen_in_einer_eigenen_zeile` (A1.8) ·
`die_vier_achsen_zaehlen_dieselben_messwerte` (A1.5) ·
**`die_hallen_achse_aendert_die_prognose_kette_nicht`** (A1.10 — der
N-3-Wächter).

`tl.rs`: `das_zeiten_panel_liefert_alle_vier_achsen` ·
`ein_ein_hallen_turnier_liefert_keine_hallen_achse` (A1.6) · den bestehenden
Cache-Test um die Zusicherung erweitern, dass vier Achsen **eine** Generation
kosten (A1.11).

`relay-proto`: `ein_profil_ohne_achsen_feld_liest_sich_als_gruppe`
(Serde-Roundtrip, A1.2). `tl.rs`:
`die_achse_reist_durch_profile_to_wire_und_zurueck`.

**Punkt 2 — Rust + Node**

`relay-proto`: `announce_scorekeeper_roundtrip` + Aufnahme in den
ALL-Vektor. `state.rs`:
**`der_bediener_nachruf_zaehlt_getrennt_von_den_spieler_aufrufen`** (A2.5 —
der eigentliche Grund für die eigene Ansageart) ·
`ein_neues_spiel_setzt_den_bediener_zaehler_zurueck` (A2.6). `tl.rs`:
`ein_bediener_nachruf_ohne_zugewiesenen_bediener_wird_abgelehnt` ·
`der_bediener_nachruf_legt_einen_auftrag_in_der_halle_des_felds_ab` ·
`ein_bediener_nachruf_ohne_ansage_geraet_zaehlt_trotzdem_und_sagt_es` (A2.8).

Node: neue `src/io/scorekeeperCallText.mjs` +
`scripts/test-scorekeeper-call-text.mjs` (CI-Eintrag). Prüffälle: DE/EN
wörtlich (A2.2), Stufen 1/2/3 (A2.3), leere Namensliste → leeres Ergebnis,
Namen mit `/` verbunden wie bei den Schiedsrichtern, **kein** XML-Escaping in
der `.mjs` (das macht der SSML-Bauer).

**Punkt 3 — Rust + Node**

`state.rs`/`commands.rs`:
**`ein_unbekannter_auftragstyp_verwirft_nicht_die_ganze_charge`** (A3.7 —
läuft **vor** der Umsetzung und entscheidet zwischen den beiden Wegen, siehe
Umsetzungs-Hinweise). `relay-proto`: `announce_start_play_roundtrip` +
ALL-Vektor. `tl.rs`: **`die_spielbeginn_ansage_laesst_call_stages_unberuehrt`**
(A3.3) · `die_spielbeginn_ansage_braucht_ein_spiel_auf_dem_feld` (A3.6) ·
`die_spielbeginn_ansage_ist_beliebig_oft_ausloesbar` (A3.4) ·
`die_spielbeginn_ansage_geht_nur_in_die_halle_des_felds` (A3.5).

Node: `src/io/startPlayText.mjs` + `scripts/test-start-play-text.mjs`.
Prüffälle: DE/EN wörtlich, **keine** Paarung, **kein** Stufenwort.

**Punkt 4 — Rust + Browser-Build**

`tl.rs`: `laufende_und_beendete_spiele_tragen_die_lizenznummer_als_linkziel`
(A4.1/A4.2) · `ein_spieler_ohne_lizenznummer_bleibt_ohne_id` (A4.3) · beide
Wächter-Tests angepasst (A4.5, A0.2) · den bestehenden Größentest auf das
A0.1-Fixture heben.

**Übergreifend:** `cargo test` grün, `cargo fmt --all --check` sauber,
`cargo clippy --workspace --all-targets` ohne Warnung (die CI prüft genau
das), `npm run build` fehlerfrei, `node scripts/check-asset-syntax.mjs`.

**Manueller Turnier-Testfall je PR** — vor dem Tag durchzuspielen:

1. *Punkt 4:* Zwei Felder belegen, ein Spiel werten, auf einem Tablet je
   einen Link in Feld-Kachel und Beendet-Liste antippen.
2. *Punkt 3:* Feld belegen, ohne Punkt die Ansage auslösen, prüfen dass die
   Aufruf-Uhr und das Aufruf-Abzeichen **stehenbleiben**.
3. *Punkt 1:* Zwei Hallen, je zwei beendete Spiele, alle vier Achsen
   durchschalten und die `count`-Summen vergleichen.
4. *Punkt 2:* Bediener-Verwaltung an, Feld mit Bediener → Knopf da; Feld mit
   abgeschalteter Vergabe → Knopf weg.
5. *Punkt 3, Mischbetrieb:* **Zwei-Rechner-Aufbau** mit einem Master auf
   neuem und einem Ansage-Slave auf altem Stand — die zweite Halle muss
   normale Aufrufe weiter aussprechen.

## Risiken & Rollback

**Übergreifend.** Ein Client-Release ist ein Auto-Update ohne Rückwärtsgang;
der einzige Rollback, der die installierte Basis erreicht, ist **Revert-Commit
+ Patch-Bump + neuer Tag**. Der Relay deployt automatisch beim `main`-Merge,
dort genügt ein Revert-Merge. **Achtung:** Ein Relay-Rollback **nach** einem
Client-Release macht die neuen `TlAction`s wieder zu 422ern.

Ein Update mitten im Turnier bedeutet App-Neustart am Turnier-PC. Dabei geht
— unabhängig von diesen PRs — immer verloren: die Bediener-Warteschlange, die
Aufruf-Zähler und der Ansage-Auftrags-Ring. Erhalten bleiben: `match-times.json`,
manuelle Reihenfolge, Auto-Hallen, Schiedsrichter-Stand. Das ist der
Turnierleitung vor einem Update mitten am Tag zu sagen.

**Punkt 4.** Größtes Risiko ist die Relay-Größengrenze: Reißt sie, verwirft
der Relay das **ganze** Frame samt Vorgänger, und die Cloud-TL-Seite wird
stumm — das Hallennetz bleibt heil. Dagegen A0.1. Zweites Risiko: unescaptes
HTML in der Beendet-Zeile. Rollback folgenlos.

**Punkt 3.** Der Mischbetrieb neuer Master / alter Ansage-Slave ist das
Betriebsrisiko: Ohne die Absicherung schweigt die zweite Halle 60 Sekunden
lang **komplett**, auch für normale Aufrufe. Dagegen A3.7 plus der manuelle
Zwei-Rechner-Test. Sonst: Ein altes Relay antwortet 422, die Seite zeigt
einen Fehlerhinweis statt einer Ansage. Rollback folgenlos.

**Punkt 1.** Der Rollback ist hier **nicht verlustfrei**: Eine ältere Fassung
liest `match-times.json` weiter, ignoriert `hall` aber und schreibt die Datei
**ohne** das Feld zurück — alle bis dahin gestempelten Hallen sind dann weg.
Steht im ADR und gehört in den PR-Text. Zweitens berührt die geänderte
Store-Signatur den heißen Sync-Pfad; ein Fehler dort kostet Messwerte, nie
ein Ergebnis (der Store ist ausdrücklich „best effort"). Drittens berührt die
Statistik den 2-Sekunden-Pfad — daher E-2.

**Punkt 2.** Geringstes Risiko: Der Knopf ist nur bei zugewiesenem Bediener
sichtbar. Rollback lässt ihn verschwinden; offene Aufträge verfallen ohnehin
nach 60 Sekunden.

## Offene Fragen / Annahmen

Keine blockierenden Fragen.

**Annahmen:**

- **AN-1** Die BTP-Hallennamen kommen als kurze Kürzel („Lu", „Ky") an und
  sind damit auf einem Tablet in einer Tabellenspalte lesbar. **Beim
  Feldtest zu prüfen**; kommen lange Namen an, braucht die Spalte eine
  Kürzung.
- **AN-2** Der Hallen-Schlüssel ist der **getrimmte BTP-Name**, nicht die
  `location_id`. Bewusste Folge: Eine Umbenennung mitten im Turnier spaltet
  die Zeile in zwei. Als seltener Fall in Kauf genommen.
- **AN-3** Die Reihenfolge der Spieler in `CourtOverview.team1` und im
  BTP-Match ist identisch — beide entstehen aus derselben Quelle. Darauf
  stützt sich die Zuordnung der Lizenznummern zu den Namen in den laufenden
  Kacheln.
- **AN-4** Der Bediener steht zum Zeitpunkt des Nachruf-Knopfdrucks fest
  (das Feld ist belegt). Dass die Warteschlange selbst nur im
  Arbeitsspeicher lebt, ist damit für Punkt 2 unerheblich.

## Betroffene Doku-Dateien

| PR | Zu pflegen im selben Commit |
|---|---|
| **Punkt 4** | `docs/features/turnierleitung-web.md` (Datenschutz-Abschnitt) · `docs/turnierleitung-web.md` · `docs/cloud-relay.md` (additive Felder, Kürzungskaskade) · **`CLAUDE.md`** (der Datenschutz-Absatz nennt die Wartelisten-Beschränkung wörtlich) · `docs/regression-suite.md` · `docs/changelog.md` |
| **Punkt 3** | `docs/announcements.md` (neue Ansageart, Auftragstypen-Tabelle, die `#[serde(other)]`-Begründung) · `docs/turnierleitung-web.md` · `docs/features/turnierleitung-web.md` · `docs/cloud-relay.md` · `docs/multi-hall.md` (N-5: die ferne Cloud-Halle bleibt stumm) · `docs/regression-suite.md` · `docs/changelog.md` |
| **Punkt 1** | `docs/spielzeiten-prognose.md` · `docs/features/spielzeiten-prognose.md` · `docs/features/tl-web-panelsystem.md` (neuer Profil-Schalter) · `docs/turnierleitung-web.md` · `docs/cloud-relay.md` · `docs/multi-hall.md` · **`docs/adr/0036-hallen-achse-im-messwert.md`** (neu) · `docs/regression-suite.md` · `docs/changelog.md` |
| **Punkt 2** | `docs/zaehltafelbediener.md` (offenen Baustein streichen, Abschnitt „Nachruf") · `docs/announcements.md` · **Nachtrag an `docs/adr/0007-zaehltafelbediener.md`** · `docs/preparation.md` (festhalten, dass der Vorbereitungs-Aufruf bewusst **nicht** betroffen ist, N-6) · `docs/cloud-relay.md` · `docs/turnierleitung-web.md` · `docs/regression-suite.md` · `docs/changelog.md` |

Verweis in `docs/roadmap.md` auf diese Spec.

## Umsetzungs-Hinweise

*Erst nach Freigabe relevant. Ergebnis der How-To-Phase; die vollständige
Herleitung steht in `_intake/tl-sicht-feinschliff/3-how-to.md`.*

### Reihenfolge: 4 → 3 → 1 → 2

Vier getrennte PRs mit je eigenem Versions-Bump
(`src-tauri/Cargo.toml` + `src-tauri/tauri.conf.json` + `package.json`
gemeinsam), ausgehend von v0.9.227.

Nutzer-Vorgabe: **Punkt 4 und 3 zuerst** — die beiden, die im Turnierbetrieb
sofort etwas bringen. Punkt 4 vor Punkt 3, weil er der kleinste, rein
additive Schnitt ist und als Erster die Relay-Größengrenze beweist (A0.1);
danach legen die anderen nur noch gegen eine bereits gehaltene Grenze nach.
Punkt 1 vor Punkt 2, weil der Hallen-Stempel **erst ab Installation** Daten
sammelt — jeder Tag früher ist ein Tag mehr Statistik.

> **Folge der Umstellung gegenüber dem How-To:** Die
> Auftragslisten-Absicherung wandert in den **Punkt-3-PR**, weil der nun die
> **erste** neue Ansageart mitbringt. Sie muss zwingend mit der ersten neuen
> Ansageart landen, sonst greift sie zu spät.

### Ausroll-Regel (R3)

Für **Punkt 2 und 3**: Der Relay-Merge nach `main` (Auto-Deploy) muss
**vor** dem Client-Tag liegen, sonst weist ein altes Relay die neue
`TlAction` mit 422 ab. Für **Punkt 1 und 4** gilt dasselbe schwächer:
`tl.html` ist ins Relay einkompiliert, Cloud-Geräte sehen die Änderung erst
nach dem Rebuild.

### Punkt 1 — Kernentscheidungen

- **Alle vier Achsen reisen im Zustand mit**, die Seite zeigt die aktive. Ein
  geräteabhängiger Zustand ist **nicht möglich**: Der TL-Zustand ist ein
  Broadcast mit einer geteilten Revision und einem geräteunabhängigen
  Push-Cache (ADR 0034), während die Profil-Wahl geräteweise ist.
- **Zeilenform:** vier Listen derselben Zeilenform; die Zeile bekommt `hall`
  dazu, je Achse bleiben die nicht zutreffenden Schlüsselfelder leer. Der
  Host baut **kein** fertiges Label — die Anzeige-Hoheit bleibt bei der
  Seite.
- **Die Halle ist am E4-Stempel fertig verfügbar**
  (`BtpSnapshot::court_location_name`, gibt bei Ein-Hallen-Turnieren bewusst
  leer zurück). Einmal je Poll eine `HashMap<court_id, Hallenname>` vor der
  Schleife bauen — sonst scannt jeder Aufruf linear, und die Tupel müssten
  allokieren.
- **`rows()` klont heute** und wird bei **jedem** TL-State-Bau gerufen (alle
  ~2 s je Gerät). Mit vier Achsen wären das vier Klone — die Zugriffe daher
  auf Slices umstellen. Das ist die einzige Stelle, an der Punkt 1 E-2
  gefährden könnte.
- **Hallen-Achse ausblenden auf beiden Seiten:** Host liefert die Liste leer
  bei Ein-Hallen-Turnieren (spart Bytes, serverseitig testbar), die Seite
  prüft zusätzlich.

### Punkt 2 und 3 — gemeinsames Muster

Beide folgen der eingespielten `AnnounceOfficials`-Kette: neue
`TlAction`-Variante (mit ALL-Roundtrip-Vektor) → neuer `AnnounceJobKind` →
Arm in `apply_state_action` (Guards: Snapshot da → Spiel auf dem Feld →
punktspezifische Bedingung) → `action_fingerprint`/`action_label` → Knopf im
⋯-Menü → `types.ts` → `AnnounceJobPlayer` → `announceCourt.ts` → Gatter in
**beiden** Bauern von `announcer.ts` (Web Speech **und** Azure-SSML, Vorbild
das bestehende Schiedsrichter-Gatter).

Drei Fallstricke, alle im Code als Kommentar festzuhalten:

1. **Nicht das 20-Sekunden-`op_id`-Fenster benutzen.** Sonst schluckt die
   Wiederholungserkennung eine **bewusst** wiederholte Ansage stumm. Beide
   brauchen das Nonce-Muster des „Aufrufe unbegrenzt"-Pfads plus
   `btn.disabled` gegen den Doppeltipp (A2.7/A3.4).
2. **Punkt 3 ruft weder `due_call_stage` noch `note_court_call_at_least`.**
   Das ist der ganze Inhalt von A3.3 — der Kommentar verhindert, dass es
   jemand „konsistenzhalber" nachrüstet.
3. **`speak()` im Ansage-Abspieler auf `switch` mit `default: return;`
   umbauen** — heute fällt alles Unbekannte in den Vorbereitungs-Zweig
   **durch**. Mit zwei neuen Ansagearten wäre das ein still **falsch
   gesprochener** Aufruf, nicht nur Schweigen.

**Sichtbarkeit Punkt 2:** Das eine Flag „Bediener zugewiesen" deckt alle drei
A2.4-Fälle ab (leere Warteschlange, abgeschaltete Vergabe am Feld, global
aus) — kein zusätzlicher Guard nötig; als Kommentar festhalten.

**Aufräumen im Punkt-2-PR:** Die Hallen-Auflösung steht in `tl.rs` bereits
dreimal wörtlich; mit dem neuen Arm das vierte Mal. → in eine Funktion
ziehen.

### Punkt 3 — die Auftragslisten-Absicherung

Der Schadensfall sitzt in **einer** Zeile: dem typisierten Deserialisieren
der Auftragsliste im LAN-Slave-Zweig. Zwei Wege, **TDD entscheidet**:

- **Weg A:** `#[serde(other)]`-Auffangvariante am Ansage-Auftragstyp. Näher
  an der ursprünglichen Empfehlung und wirkt auch für künftige Abholwege —
  **muss aber erst bewiesen werden**, denn der Typ ist intern getaggt und
  wird geflattet; ob die Auffangvariante **unter `flatten`** greift, zeigt
  erst der Test.
- **Weg B:** tolerantes Parsen genau an der Schadensstelle (Liste generisch
  lesen, je Element typisieren, Fehlschläge verwerfen). Wirkt garantiert und
  hält die Ausnahme in **einer** Funktion.

Der Test `ein_unbekannter_auftragstyp_verwirft_nicht_die_ganze_charge` ist
der **erste** Schritt des PRs. Fährt Weg A grün, nimm Weg A; sonst Weg B —
dann wandert der begründete Doku-Abschnitt vom Typ zur Funktion.

**Richtigstellung zum Grill:** Der Ansage-Auftragstyp lebt in
`src-tauri/src/tablet/state.rs`, **nicht** in `relay-proto`. Die dortige
„kein Serde-Default"-Regel steht im Kommentar von `TlAction` und bleibt
unangetastet. Die Begründung gehört an den Typ bzw. in
`docs/announcements.md` — **kein ADR**.

### Punkt 4 — Kernentscheidungen

- Die IDs entstehen **in** der Aufbau-Closure des TL-Zustands, nicht in einer
  zweiten Schleife. Ein Feld, das man nachbefüllt, vergisst man.
- Für die laufenden Felder **eine `HashMap<match_id, &BtpMatch>` einmal vor
  dem Felder-Block** bauen und auch im Prognose-Block nutzen, der heute je
  belegtem Feld linear sucht. Zwei lineare Suchen je Feld alle 2 s sind
  vermeidbar — und E-2 misst genau das.
- Die Feld-Kachel ist trivial (der Paarungs-Renderer kann die Links bereits,
  inklusive URL-Kodierung und „ohne Nummer bleibt es Text"). Die
  **Beendet-Zeile** rendert heute reinen Text und wird dafür in eine
  Text- und eine HTML-Variante **gespalten** — Auflage: jeder Name einzeln
  escapt, jede Nummer URL-kodiert. Genau dort schaut der
  `security-reviewer` hin.
- **Befund zu den Wächter-Tests:** Der Allowlist-Wächter hätte diese Änderung
  **gar nicht bemerkt** — die Feldnamen stehen bereits in seiner flachen
  Liste und sind damit automatisch auch in den neuen Strukturen erlaubt. Dort
  ist nur der Kommentar zu korrigieren, der heute „nur die Warteliste" sagt.
  Die echte Arbeit steckt allein im Datenschutz-Wächter, wo die
  Lizenznummern von der Verbots- in die Positiv-Prüfung umziehen. **Beides
  gehört in den PR-Text**, damit die bewusste Änderung nachvollziehbar ist.

### Reviews

- `code-reviewer` nach **jeder** Code-Änderung (Pflicht, alle vier PRs).
- `security-reviewer` **Pflicht bei Punkt 4** (neue personenbezogene Felder
  über eine aus dem Internet erreichbare Seite) und als kurzes Gate bei
  **Punkt 3** (neue schreibende `TlAction`).
