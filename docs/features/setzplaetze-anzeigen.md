# Setzplätze anzeigen — Spezifikation

> Status: **Freigegeben 12.10.2026** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Nutzerfrage vom 12.10.2026 („Bekommen wir die Setzplätze von BTP?“).
> Betroffene Crates: `src-tauri` (Kern + Assets), `relay-proto`, `relay`,
> Frontend (`CheckinPanel`). Folge-PRs: badhub-Payload (bts-light) und
> badhub-Anzeige (Repo badhub).
> ADR: [0059](../adr/0059-setzplatz-als-text-aus-dem-parser.md) ·
> Messung: [btp_protocol.md](../btp_protocol.md) (`StageEntries`, PR #380)

## Kontext / Problem

BTP führt die Setzliste eines Turniers. bts-light liest sie bisher nicht. Wer
eine Paarung einordnen will, muss deshalb in BTP oder auf dem Auslosungsbogen
nachsehen:

- Zuschauer wollen wissen, wer favorisiert ist.
- Die Turnierleitung entscheidet danach, welches Spiel aufs Hauptfeld gehört.
- Beim Check-In muss sie bei Absagen erkennen, ob ein Gesetzter fehlt, denn
  dann muss sie neu setzen.

Die Messung vom 12.10.2026 (`src-tauri/tests/btp_seed_probe.rs`, lokales
Test-BTP, 13 Konkurrenzen) ergab:

- **Ort:** `SENDTOURNAMENTINFO` liefert die Setzung an
  `StageEntries/StageEntry` in den Feldern `Seed1` und optional `Seed2`.
- **Bedeutung:** `Seed1` ist der Platz bzw. die Untergrenze, `Seed2` die
  Obergrenze eines geteilten Platzes, beide als `Int`.
- **Ungesetzt:** Es gibt kein Seed-Feld.
- **Je Stage:** Die Setzung hängt an der Stage. Eine Meldung, die ins Playoff
  kommt, trägt sie an Hauptfeld **und** Playoff.
- **Ausreißer:** Ein Eintrag hatte nur `Seed2`.

## Zielbild & Erfolgskriterien

In einem Turnier mit Setzliste steht hinter jeder gesetzten Seite einer
Paarung bzw. jeder gesetzten Meldung ihr Platz in eckigen Klammern: `[1]`,
`[3/4]`, `[5/8]`. Das gilt für:

- Court-Monitor, Feld-Übersicht, Kombi-Ansicht, Vorbereitung, Sieger
  (LAN und Cloud, Sieger nur LAN)
- TL-Web: Warteliste und Spiele auf dem Feld
- Desktop-Check-In (`CheckinPanel`)
- später badhub-Liveticker und badhub-Check-In (PR 2 und 3)

Ungesetzte Seiten tragen keine Marke. Ändert die Turnierleitung eine Setzung
in BTP, gilt:

- **TL-Web und Cloud-Monitore:** aktuell nach dem nächsten Sync-Takt bzw.
  dem erneuten Push.
- **LAN-Monitore:** aktuell mit dem nächsten Abruf (Spielstand-Nudge oder
  Fallback-Poll).
- **Liveticker:** aktuell mit dem nächsten Heartbeat.

**Erfolgskriterium:** Beim ersten Turnier mit Setzliste stimmen alle Marken
mit dem BTP-Auslosungsbogen überein, und kein Monitor zeigt einen
abgeschnittenen oder fehlenden Klammerwert.

## Nicht-Ziele

- **Nicht angezeigt** auf diesen Oberflächen:
  - Zähl-Tablet und Zähltafel (`tablet.html`, `tafel.html`, `anzeige.html`)
  - Schiedsrichterzettel
  - Sprachansage
  - Aushang
  - TL-Panel „Anfangszeiten“
  - TL-Liste der beendeten Spiele (`TlFinished`)
- **Kandidaten offener Paarungen** (`TlOpenMatch`, ADR 0052) bekommen keine
  Setzung.
- **Kein Monitor-Schalter.** Die Monitore zeigen die Setzung immer, wenn eine
  vorhanden ist.
- **Kein Schreibweg:** bts-light setzt oder ändert in BTP keine Setzung (R2).
- **`Event.SeparateSeeding`** wird nicht ausgewertet.
- **Liga-Modus** (`PlayerMatches`) ist nicht betroffen. Der Parser deckt ihn
  ohnehin nicht ab.
- **Sortieren oder Filtern** nach Setzung ist kein Ziel.

## Betroffene Komponenten / Architekturregeln / Daten

- **R2:** BTP ist die Wahrheit. Die Setzung wird nur gelesen. Die Auflösung
  findet einmal im Parser statt (ADR 0059).
- **R1:** Die Desktop-Check-In-Seite bekommt die Setzung über den
  bestehenden Command `checkin_state`, nicht über einen eigenen Netzweg.
- **Datenschutz:** Die Setzung steht öffentlich auf dem Auslosungsbogen und
  sagt über eine Person nichts aus, was über den Namen hinausgeht. Deshalb
  ist sie standardmäßig sichtbar. Der Wächter-Test in `tablet/tl.rs` lässt sie
  an Warteliste und Feld zu und hält sie aus `open_queue` heraus.
- **Neue Felder:**

  | Typ | Feld(er) |
  |---|---|
  | `BtpMatch` | `team1_seed`, `team2_seed` |
  | `BtpEntry` | `seed` |
  | `CourtOverview`, `TlMatch`, `TlCourt` | `team1_seed`, `team2_seed` |
  | `MonitorMatch` | `team1Seed`, `team2Seed` |
  | `MatchBrief`, `PreparedMatch` | `seedA`, `seedB` |
  | `Placement`, `CheckinPlayer` | `seed` |
  | `TlDisplaySettings` | `hide_seeds` |
  | `TlDisplaySettingsWire` | `hideSeeds` |

  Alle Setzungsfelder haben den Typ `Option<String>`. Auf der Wire sind sie
  optional (`default` + `skip_serializing_if`). `hide_seeds` ist ein
  `bool` mit `default` = sichtbar.
- **Keine Persistenz:** Die Setzung wird bei jedem Snapshot neu abgeleitet.

## Verhalten im Detail

### Formatierung (eine Stelle: `btp/model.rs::seed_text`)

| `Seed1` | `Seed2` | Text |
|---|---|---|
| 1 | — | `1` |
| 3 | 4 | `3/4` |
| 4 | 3 | `3/4` (aufsteigend normalisiert) |
| 3 | 3 | `3` |
| — | 6 | `6` (Ausreißer: wird angezeigt) |
| — / ≤ 0 | — / ≤ 0 | keine Setzung |

Werte ≤ 0 gelten als fehlend. Der Text enthält **keine** Klammern, die setzt
jede Anzeige selbst.

### Auflösung je Spielseite

Für jede Seite eines Spiels (`entry1_id`/`entry2_id`):

1. Ist die Entry-ID `0`, ist der Platz offen und es gibt keine Setzung.
2. Ermittelt wird die Stage des Spiels über `Draw.StageID` des Draws. Trägt
   die Meldung an **dieser** Stage eine Setzung, wird sie genommen.
3. Sonst wird die Setzung der Meldung an der Stage mit `StageType 1`
   (Hauptfeld) genommen (Rückfall).
4. Sonst gibt es keine Setzung.

Die Setzung einer **Meldung** (`BtpEntry.seed`, für den Check-In) kommt nur
aus der Stage mit `StageType 1`. Quali-Teilnehmer ohne Hauptfeld-Setzung
bekommen keine Marke.

### Darstellung

- **Wo:** hinter dem letzten Spieler einer Seite, also bei Doppeln einmal je
  Paar, als eigenes Element `span.seed` mit dem Inhalt `[x]`.
- **Lange Namen:** Die Marke wird nie gekürzt (`flex:0 0 auto;
  white-space:nowrap`). Zuerst weicht der Name über seine Ellipsis.
  `.pname-text` in Kombi und Übersicht braucht dafür `min-width:0`.
- **Seiten:**
  - `monitor.html` (`renderHalf`)
  - `overview.html` und `combo.html` (`teamRow`, vertikal `vname`)
  - `preparation.html` (`renderRow`, als Text „A / B [3/4]“)
  - `winners.html`
  - `tl.html` (`pairOf`, Warteliste und Feld)
  - `CheckinPanel.tsx`

### Transport

- **LAN-Monitore:** `CourtOverview` → `/health`. Dazu
  `MonitorMatch.team1Seed/team2Seed`, die Vorbereitungs-JSON und
  `Placement.seed`.
- **Teil-Patch:** `istPatchbar` (`src/io/courtPatch.mjs` und die Inline-Kopie
  in `overview.html`) vergleicht die Setzung mit. Ändert sie sich, wird die
  Karte neu gebaut.
- **Cloud:**
  - `MatchBrief.seedA/seedB` und `PreparedMatch.seedA/seedB`.
  - Das Relay übernimmt die Felder **ausdrücklich** in `build_monitor_state`,
    `overview_health` (`team1_seed`/`team2_seed`) und `preparation_state`.
  - Die Sieger-Seite gibt es nur im LAN, dort entfällt die Setzung.
- **Neusetzung in der Cloud:** Der Push-Schlüssel von `push_court` (heute
  `(match_id, finalized, officials)`) wird zum Typ `CourtPushKey` und enthält
  beide Setzungen. Ändert sich eine Setzung, geht `MatchAssigned` erneut. Ist
  es dasselbe Spiel, setzt das Relay den Spielstand nicht zurück.
- **TL-Web:** `TlMatch` und `TlCourt` tragen die Setzung immer. Der
  Profil-Schalter wirkt im Browser.

### Schalter (nur TL-Web)

- **Profilfeld:** `hide_seeds` („Setzplätze ausblenden“) im Panel-Profil,
  invertiert angelegt wie `hide_open_matches`. Ein fehlendes Feld bedeutet
  **sichtbar**, bestehende Profile zeigen die Setzung also ohne Migration.
- **Häkchen im Profil-Editor:** heißt „Setzplätze zeigen“ und ist
  angehakt, wenn `hide_seeds` falsch ist. Es erscheint nur, wenn der Host
  das Feld kennt (`"hideSeeds" in p.display`).
- **Eingebautes Profil:** `hideSeeds:false`.

### Desktop-Check-In

`commands.rs::checkin_state` reichert die badhub-Antwort nach `fetch_state`
mit `setzungen_eintragen(view, snapshot.entries)` an. Die Verknüpfung läuft
über `CheckinPlayer.entry_id`. Bei `entry_id == 0` (altes badhub) oder einer
unbekannten Meldung gibt es keine Marke.

## Akzeptanzkriterien

**Formatierung und Auflösung**

1. `seed_text` liefert für alle Zeilen der Formatierungstabelle den
   angegebenen Text. `None` gibt es genau für „beide fehlend oder ≤ 0“.
2. Eine Meldung mit Setzung an der Stage des Spiels zeigt diese, auch wenn
   ihre Hauptfeld-Setzung abweicht.
3. Eine Meldung in einem Playoff-Spiel **ohne** Playoff-Setzung, aber mit
   Hauptfeld-Setzung zeigt die Hauptfeld-Setzung.
4. Ein offener Platz (`entry_id == 0`) zeigt keine Marke.
5. Eine Meldung ohne jede Setzung zeigt keine Marke, und das Feld fehlt
   auf der Wire.
6. `BtpEntry.seed` übernimmt nur die Setzung aus `StageType 1`. Eine
   Quali-Meldung ohne Hauptfeld-Setzung bleibt ohne.

**Monitore**

7. Die LAN-Antwort von `/health` trägt `team1_seed`/`team2_seed` an
   belegten Feldern mit gesetzten Seiten. Übersicht, Kombi (waagerecht und
   senkrecht), Court-Monitor, Vorbereitung und Sieger zeigen `[x]` hinter
   der Seite.
8. Dasselbe gilt über die Cloud (Relay-`overview_health`,
   `build_monitor_state`, `preparation_state`). Ausgenommen ist die
   Sieger-Seite, die es dort nicht gibt.
9. Ändert sich nur die Setzung eines belegten Feldes, baut die Übersicht die
   Karte neu (`istPatchbar` → false, JS-Test).
10. Eine Neusetzung an einem belegten Feld löst im Cloud-Pfad ein erneutes
    `MatchAssigned` aus. Der Spielstand am Relay bleibt dabei erhalten.

**TL-Web und Check-In**

11. Das TL-Web zeigt die Marke in der Warteliste und an den Feldern. Im
    Profil mit „Setzplätze zeigen“ = aus zeigt es keine.
12. Ein gespeichertes Profil ohne `hide_seeds` bzw. `hideSeeds` zeigt die
    Setzung. Das Feld übersteht den Roundtrip Host ↔ Wire ↔ Host.
13. `open_queue` trägt nie ein `*_seed`-Feld (Wächter-Test).
14. Ein Budget-Test mit einem voll gesetzten Turnier hält
    `MAX_TL_STATE_LEN` ein.
15. Die Desktop-Check-In-Seite zeigt `[x]` an gesetzten Meldungen. Bei
    `entry_id == 0` oder einer unbekannten Meldung zeigt sie keine Marke und
    keinen Fehler.

**Darstellung und Verträglichkeit**

16. Bei einem Doppel mit zwei langen Namen bleibt die Marke vollständig
    sichtbar, der Name wird mit „…“ gekürzt (Sichtprüfung `monitor.html`
    und Kombi, beide Ausrichtungen).
17. Verträglichkeit beim Mischbetrieb:
    - Ein altes Relay oder eine alte App ignoriert die neuen Felder ohne
      Fehler (Serde-Test: fehlendes Feld → `None`).
    - Ein neues Relay mit alter App zeigt keine Marke und kein Häkchen.

## Tests

**Rust**

- Formatierung: `seed_text_*` (nur lo, nur hi, gleich, verkehrt, Spanne,
  None, 0, negativ).
- Auflösung (Fixture-Muster `turnier_mit_stages`):
  - Setzung je Stage
  - Rückfall auf das Hauptfeld
  - offener Platz
  - Quali-Meldung ohne Hauptfeld-Setzung
  - `BtpEntry.seed` nur aus `StageType 1`
- Monitore: `overview_traegt_die_setzung` und
  `build_monitor_state_traegt_die_setzung`.
- `relay-proto`: Serde-Roundtrips für `MatchBrief`, `PreparedMatch`,
  `MonitorMatch` und `TlDisplaySettingsWire` jeweils mit fehlendem Feld,
  `None` (nicht serialisiert) und „3/4“.
- Relay: Schlüssel in `overview_health` und `preparation_state`, Übernahme in
  `build_monitor_state`.
- `push_court_sendet_bei_neusetzung_erneut`.
- LAN-Vorbereitung und `winners::placement` mit Setzung.
- TL:
  - Setzung in Queue und Courts
  - nicht in `open_queue`
  - Positivliste `ERLAUBT`
  - Wächter-Test
  - Budget
  - `hide_seeds`-Roundtrip, Profil ohne Feld = sichtbar
- Check-In: `setzungen_eintragen_*` (Doppel, `entry_id` 0, unbekannte
  Meldung).

**JS:** `scripts/test-court-patch.mjs`, Fall „nur die Setzung geändert →
nicht patchbar“.

**Manuell:**

- Turnier mit Setzliste (`btp_seed_probe.rs` zur Kontrolle) in LAN und Cloud.
- Langer Doppelname, Kombi waagerecht und senkrecht.
- Neusetzung in BTP, während das Spiel auf dem Feld liegt.

## Risiken & Rollback

- **Deploy-Reihenfolge:**
  - Der Merge von PR 1 deployt das Relay automatisch. Ein neues Relay mit
    alter App zeigt keine Marke.
  - Die App kommt mit dem Release-Tag (Admin).
  - PR 2 (Payload) kann jederzeit danach folgen, badhub ignoriert unbekannte
    Felder.
  - PR 3 kommt mit dem badhub-Deploy (Kollege).
- **LAN-Monitor bei Neusetzung mitten im Spiel:** Ohne Belegungswechsel gibt
  es keinen Nudge. Die Setzung kommt mit dem nächsten Spielstand-Nudge oder
  Fallback-Poll. Akzeptiert.
- **Cloud:** Das erneute `MatchAssigned` erreicht auch das Zähl-Tablet. Das
  ist harmlos, wie beim Wechsel der Schiedsrichter-Besetzung.
- **Liveticker (PR 2):** Eine Neusetzung kommt erst mit dem Heartbeat an
  (≤ 60 s). Die Meldeliste wird nach dem Update einmal neu gesendet.
  Beides ist akzeptiert.
- **TL-Zustand:** ca. 16 Byte je gesetzter Seite. Das Budget wird per Test
  abgesichert.
- **Rollback:** Alle Felder sind optional. Ein Revert der App genügt, das
  Relay muss nicht zurück.

## Offene Fragen / Annahmen

- **Spielertausch:** Bleibt die `EntryID` einer Meldung gleich? Davon wird
  ausgegangen, die Setzung bleibt dann an der Meldung. Feldtest.
- **`SeparateSeeding`:** Die Annahme ist, dass der Schalter die Bedeutung von
  `Seed1`/`Seed2` nicht ändert. Feldtest bzw. Probe an einem Turnier mit
  `true`.
- **Mehrere Stages:** Die Annahme ist, dass bei Gruppe → Playoff beide
  Stage-Einträge dieselbe Setzung tragen. So war es im Messturnier. Weichen
  sie ab, gilt die Setzung der Stage des Spiels.

## Betroffene Doku-Dateien

- **`docs/btp_protocol.md`:** Auflösungsregel, Rückfall, Randfälle.
  „bts-light liest noch nicht“ entfällt.
- **`docs/court-monitor.md`:** Marke, `pruef`-Anker auf `seed` in
  `monitor.html`.
- **`docs/turnierleitung-web.md`:** Häkchen „Setzplätze zeigen“ mit
  `pruef`-Anker in `tl.html`.
- **`docs/spieler-check-in.md`:** Desktop-Marke, in PR 2 das Payload-Schema.
- **`docs/oberflaechen.md`:** `CheckinPanel`.
- **`docs/cloud-relay.md`:** Wire-Felder `seedA`/`seedB`/`team1Seed`/
  `hideSeeds` und `CourtPushKey`.
- **CLAUDE.md:**
  - Tabellenzeile „Setzplätze“
  - Zeile „Hallen-Check-In“ ergänzen
  - Datenschutz-Absatz zur Setzung
- **`docs/changelog.md`**, **`docs/roadmap.md`**.

## Umsetzungs-Hinweise

**Etappen**

- **PR 1:** Parser, Monitore LAN und Cloud, Relay, TL-Web, Desktop-Check-In.
- **PR 2:** `badhub/payload.rs`:
  - `TsetMatch.p0_seed/p1_seed`, gesetzt in `to_tset_match`
  - `RosterEntry.seed`, gesetzt in `build_checkin_roster`
  - **kein** neuer Fingerabdruck in `diff.rs`
- **PR 3 (badhub):**
  - Liveticker zeigt `[x]` hinter dem Team
  - Check-In-Seite zeigt `centry.seed` und hebt fehlende Gesetzte hervor
  - Fehlt das Feld, gibt es keine Marke

**Reihenfolge PR 1 (TDD)**

1. `seed_text` (`btp/model.rs`, einzige Formatierungsstelle).
2. `SeedIndex` mit `by_stage`, `main` und `draw_stage`:
   - einmal in `parse_snapshot` gebaut, neben `non_main_stage_entries`
   - befüllt `parse_matches` und `entry_list`
   - 31 `BtpMatch`- und 11 `BtpEntry`-Literale bekommen `None`
3. `CourtOverview` (`overview_from`) und `MonitorMatch`
   (`monitor.rs::build_monitor_state`).
4. Wire-Typen in `relay-proto`.
5. `relay/src/main.rs`: `build_monitor_state`, `overview_health`,
   `preparation_state`.
6. `CourtPushKey` in `relay_client.rs`. Der LAN-`push_match` bleibt
   unverändert.
7. `server.rs::info_preparation_state`, `server.rs::match_brief`,
   `relay_client.rs::build_prepared_list`, `winners.rs::placement`.
8. TL:
   - `TlMatch` und `TlCourt` mit Setzung
   - `hide_seeds` mit beiden Abbildungsfunktionen
   - Positivliste `ERLAUBT` und Wächter-Test
   - Budget-Test
9. `checkin_state.rs::setzungen_eintragen` plus Aufruf in
   `commands.rs::checkin_state`, `src/types.ts`.
10. `courtPatch.mjs` plus Inline-Kopie in `overview.html`.
11. HTML/TSX mit `span.seed`:
    - Monitor-Seiten (in Kombi und Übersicht `min-width:0` an
      `.pname-text`)
    - `tl.html` (`pairOf`, `zeigeSetzplaetze`, Häkchen hinter
      `hostKenntSetzplaetze`, eingebautes Profil)
    - `CheckinPanel.tsx`
12. Doku, Handbuch-Anker, ADR-Index, Changelog. Die Version erst beim Merge
    festlegen, alle drei Versionsdateien gemeinsam.

Die Alternative, die Setzung erst in den Ansichten aufzulösen statt im
Parser, ist verworfen, siehe ADR 0059.
