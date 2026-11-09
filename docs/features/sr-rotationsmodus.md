# Rotationsmodus für Schiedsrichter und Aufschlagrichter — Spezifikation

> Status: **Entwurf** (via /idee: Brief → Grill → How-To → Review, 09.11.2026).
> Quelle: Idee der Turnierleitung (09.11.2026). Betroffene Crates: src-tauri, src, später relay-proto/relay (PR 5).
> ADR: [0079](../adr/0079-rotationsmodi-schiedsrichter.md). Basis: v0.9.355 (#457, Wartelisten-Einteilung).

## Kontext / Problem

Die automatische Officials-Rotation (Spec `schiedsrichter-management`, ADR 0021/0022/0078)
kennt **eine** Reihenfolge `order`. Wird ein Feld frei, bekommt der erste freie Official
den SR-Dienst, der nächste den AR-Dienst. Nach Spielende rücken beide als `[SR, AR]` ans
Ende (`sync.rs` `track_officials` → `move_to_end`). Dadurch bleiben Paare in der Liste
zusammen und bekommen beim nächsten Mal **dieselben Dienste wieder**. Ein Rollenwechsel
passiert nur zufällig, wenn Pause, Konflikt oder Reservierung das Paar trennen.

Turniere teilen aber unterschiedlich ein:
- getrennte Pools (wer nur AR machen darf oder soll, steht nur in der AR-Liste),
- eine Liste, abwechselnd SR/AR,
- eine Liste, bei der jeder abwechselnd SR und AR macht.

## Zielbild & Erfolgskriterien

Im Setup wählt die Turnierleitung eines von drei Einteilungsprinzipien. Die Automatik,
die Vorab-SR des Meetingpoints und die Vorschau „als Nächstes" folgen dem gewählten
Prinzip übereinstimmend.

| Modus | Name im Setup | Regel |
|---|---|---|
| **A** `getrennt` | Getrennte Listen | Eigene SR-Reihenfolge und eigene AR-Reihenfolge. |
| **B** `abwechselnd` | Abwechselnd (Standard) | Eine Liste. Der erste Freie wird SR, der nächste AR. Was jemand vorher gemacht hat, zählt nicht. |
| **C** `rollenwechsel` | Rollenwechsel | Eine Liste. Jeder bekommt den Dienst, den er im zuletzt beendeten Spiel **nicht** hatte. |

Für alle Modi gilt eine **Rolle je Person**: *beides* (Standard), *nur SR* oder *nur AR*.
Die Automatik setzt niemanden in einen Dienst, den seine Rolle nicht erlaubt.

Erfolg zeigt sich so:
- In C wechseln SR und AR je Person verlässlich ab.
- In A wird eine „nur AR"-Person nie automatisch SR.
- Die Marke „als Nächstes SR/AR" im Panel stimmt mit der tatsächlichen nächsten Besetzung überein.

## Begriffe

| Begriff | Bedeutung | Code |
|---|---|---|
| Dienst | SR oder AR an einem Spiel | bestehendes `OfficialRole { Sr, Ar }` |
| Rolle | was eine Person übernehmen darf | neu `Einsatzrolle { Beides, NurSr, NurAr }`, JSON `"beides"`/`"sr"`/`"ar"` |
| Vorrolle | Dienst im zuletzt beendeten Spiel (ohne Walkover) | abgeleitet, `HashMap<i64, OfficialRole>` |
| Modus | A/B/C | neu `RotationMode { Getrennt, Abwechselnd, Rollenwechsel }`, Default `Abwechselnd` |
| frei | bekannt in BTP, nicht pausiert, nicht belegt/reserviert (`Belegung`, ADR 0078), kein Konflikt (`official_conflict`) | wie heute |
| kann(p, d) | p ist frei **und** seine Rolle erlaubt Dienst d | neu |

## Regeln

### Wahl je Feld

Eine reine Funktion `waehle_besetzung` besetzt die **offenen** Dienste eines Felds
gemeinsam. Offen heißt wie heute: Die Rotation ist für diesen Dienst global (`rotation_sr/ar`)
und am Feld eingeschaltet, und der Dienst ist weder durch BTP noch von Hand besetzt.
`Some(0)` (ausdrücklich keiner) zählt nicht als offen.

**A – Getrennt**
- SR: die erste Person der SR-Liste (`order`) mit kann(p, SR).
- AR: die erste Person der AR-Liste (`order_ar`) mit kann(p, AR), die nicht schon SR ist.
- SR wird zuerst gewählt.

**B – Abwechselnd:** wie A, aber beide Dienste kommen aus `order`.

**C – Rollenwechsel (personenweise):**
1. *Durchgang 1:* `order` wird von oben gelesen, solange noch ein Dienst offen ist. Jede freie
   Person bekommt einen offenen, von ihrer Rolle erlaubten Dienst nach dieser Regel:
   - feste Rolle (nur SR / nur AR): ihr Dienst, falls offen; die Vorrolle spielt keine Rolle;
   - Rolle beides, ohne Vorrolle: SR, falls offen, sonst AR;
   - Rolle beides, mit Vorrolle: das Gegenteil der Vorrolle, falls offen. Ist es schon
     vergeben oder nicht offen, wird die Person **übersprungen**.
2. *Durchgang 2 (Rückfall):* Für jeden noch offenen Dienst (SR vor AR) nimmt die Rotation die
   erste Person aus `order` mit kann(p, d), die in diesem Feld noch keinen Dienst hat.

Ein Dienst bleibt nur leer, wenn niemand mit passender Rolle frei ist. Wie heute wird er dann
später **nicht** automatisch nachgefüllt.

### Vorrolle (Modus C)

- Wird aus den beendeten Spielen abgeleitet und nicht gespeichert. Dieselbe Quelle wie
  `appearances`: wirksame Besetzung `effective`, Hand, BTP und Automatik gleichwertig.
- Walkover zählt nicht.
- Maßgeblich ist je Person das Spiel mit dem größten Schlüssel `(finished_at oder 0, match_id)`.
  Bei gleicher Endezeit gilt die höhere `match_id` als später. Ein Spiel ohne Endezeit gilt
  als älter als jedes gestempelte.
- Die Vorrolle wird einmal je Sync-Takt aus dem Snapshot gerechnet. Besetzungen im selben Takt
  ändern sie nicht, sie wirken nur über die Belegung.

### Nach Spielende

- **B und C:** In `order` rücken erst der AR, dann der SR ans Ende (`[AR, SR]`). Ersetzt das
  heutige `[SR, AR]`.
- **A:** Der SR rückt ans Ende von `order`, der AR ans Ende von `order_ar`. Die jeweils andere
  Liste bleibt unberührt.
- Auch nach einem Walkover rücken die Officials weiter ans Ende. Nur die Vorrolle ignoriert ihn.

### Zwei Listen (Modus A)

- `order` bleibt in B/C die gemeinsame Liste und ist in A die SR-Liste. Neu kommt
  `order_ar: Option<Vec<i64>>` hinzu.
- **Lesen:** ohne `order_ar` gilt eine Kopie von `order`. Dabei wird nichts gespeichert.
  IDs, die in `order` stehen, aber in `order_ar` fehlen, werden beim Lesen hinten angehängt.
- **Anlegen:** `order_ar` wird nur beim ersten Schreibbedarf als Kopie von `order` angelegt
  und gespeichert. Schreibbedarf entsteht durch AR-Umsortieren oder durch „nach Spielende"
  bzw. eine AR-Besetzung in A.
- **Neue BTP-Officials:** kommen an `order` und, falls vorhanden, an `order_ar`, jeweils ohne Doppel.
- **B/C:** `order_ar` bleibt unverändert stehen.
- **Turnierwechsel:** verwirft `order_ar` wie den übrigen Stand (ADR 0022).
- **Rolle beides:** Die Person steht in beiden Listen auf getrennten Plätzen.

### Meetingpoint (Vorab-SR, ADR 0072)

- `vorab_sr_verteilen` wählt über dieselbe Funktion `waehle_besetzung` (nur SR offen, Modus,
  Vorrolle, Rolle). In A kommt der Vorab-SR aus `order`.
- Ein Vorab-SR verfällt zusätzlich, wenn seine Rolle SR nicht mehr erlaubt. `uebernimm_vorab_sr`
  übernimmt ihn dann nicht, und die Rotation besetzt den SR neu.
- `next_free` wird auf die Wahl-Funktion umgebogen oder entfernt. Ziel: Es gibt keinen zweiten
  Wahlweg mehr.

### Handzuweisung gegen die Rolle

- Erlaubt, mit **Warnung**: „SR: nur AR" bzw. „AR: nur SR".
- Keine Ablehnung in `pruefe_einteilung` und kein neuer `ConflictKind`. Der SR⇄AR-Tausch im
  TL-Dialog muss weiter funktionieren, und BTP-Werte gewinnen ohnehin (R2).
- Die Warnung gilt auch für BTP-Besetzungen (Sicht `effective`). Sie erscheint an:
  - Feldkachel, TL-Feld und Übersicht (`court_officials`),
  - der TL-Warteliste (`wartelisten_warnung`),
  - als Rückmeldung von `official_assign`.
- Im TL-Einteilungsdialog steht „(nur AR)" bzw. „(nur SR)" schon vor der Auswahl.

### Vorschau „als Nächstes"

- Der **Host** rechnet die Vorschau, ohne etwas zu verändern. Er simuliert Runde für Runde mit
  derselben Wahl-Funktion (Kandidaten: frei ohne Konfliktprüfung, nicht Vorab-SR; offene Dienste
  nach den globalen Schaltern) und vergibt je Person `(Dienst, Runde)`.
- Sind beide Rotationen aus, gibt es keine Vorschau.
- **Desktop-Panel:** Jede freie Person, die die Automatik einsetzen würde, trägt die Marke SR oder
  AR. Runde 0 ist als „als Nächstes SR/AR" hervorgehoben.
- **TL-Web:** Der Kopf zeigt „nächster SR: X · AR: Y". Fehlen die Felder (alter Host, Rotation
  aus), bleibt die bisherige Anzeige.
- **Modus A:** Das Desktop-Panel zeigt zwei Listen, jede einzeln ziehbar. Die SR-Liste enthält
  alle außer „nur AR", die AR-Liste alle außer „nur SR". Das TL-Web beschriftet seine Liste bis
  PR 5 als „SR-Reihenfolge".

### Moduswechsel

- Nur im Setup, geräteweit (wie `rotation_sr/ar`), als Laufzeit-Spiegel im Store. Ein Neustart der
  Übertragung ist nicht nötig.
- Gilt ab der nächsten Besetzung. Laufende und eingeteilte Besetzungen bleiben unverändert.
- Es werden keine Daten umgebaut.
- A und C sind auch wählbar, wenn nur eine Rotation an ist. Ein Hinweistext im Setup erklärt die
  Wirkung, etwa: in A mit abgeschalteter AR-Rotation werden „nur AR"-Personen nie automatisch
  eingesetzt.

## Nicht-Ziele

- Keine Moduswahl im TL-Web oder während des Turniers außerhalb des Setups.
- Kein Modus je Feld oder je Halle.
- Keine gespeicherte Einsatzhistorie (die Vorrolle wird abgeleitet).
- Kein automatisches Nachfüllen leerer Dienste.
- Kein Schutz gegen Datenverlust beim Downgrade (wird nur dokumentiert).
- Zweite Liste und Rollenpflege im TL-Web erst in PR 5.

## Betroffene Komponenten / Architekturregeln / Daten

- **Komponenten:**
  - `tablet/officials.rs`: Wahl-Kern, `vorrollen`, `prognose`, `nach_spielende`, `rollen_warnung`,
    `Einsatzrolle`, `order_ar`, `reorder(…, liste)`, Store-Spiegel `rotation_mode`
  - `sync.rs`: `track_officials`
  - `tablet/meetingpoint.rs`: `vorab_sr_verteilen`, `uebernimm_vorab_sr`
  - `tablet/state.rs`: `court_officials`
  - `tablet/tl.rs`: `TlOfficial`, `officials_view`, `wartelisten_warnung`,
    `TlState.officials_rotation_mode`, Wächter-Test
  - `commands.rs`: Spiegel beim Start und Speichern, `OfficialView`, `officials_roster`,
    `official_reorder(list)`, neu `official_set_role`, `official_assign`
  - `config.rs`: `OfficialsConfig.rotation_mode`
  - Frontend: `SetupWizard.tsx`, `OfficialsPanel.tsx`, `api.ts`, `types.ts`, `assets/tl.html`
  - PR 5: `relay-proto` `TlAction::OfficialReorder.list`, `TlAction::OfficialSetRole`
- **R2:** BTP-Besetzung gewinnt. Die Rotation füllt nur offene Dienste und schreibt wie heute
  über den Rücksync.
- **R1:** neue Bedienung nur über Tauri-Commands.
- **R3:** LAN und Cloud teilen den TL-Zustand. Neue `TlOfficial`-Felder reisen opak durch den
  Relay, ein Relay-Deploy ist erst für PR 5 nötig.
- **Konfiguration:** `rotation_mode` mit `serde(default)` → `abwechselnd`. Ein unbekannter Wert
  darf das Laden nicht brechen (`#[serde(other)]` auf `Abwechselnd`).
- **`officials-state.json`:**
  - neue Felder optional: `role` wird bei beides weggelassen, `order_ar` fehlt, solange es nicht
    angelegt ist;
  - `OfficialExtra::is_empty` berücksichtigt `role`;
  - eine v0.9.355-Datei lädt unverändert.
- **Datenschutz:** Die Rolle ist eine Einsatzplanung, keine Beziehungs-Angabe wie die
  Sperrlisten. Sie darf in den TL-Zustand hinter dem Gerätezugang, aber **nicht** in
  Monitor-/Overview-States, Tablet-`MatchBrief` oder badhub-Payloads. Der Wächter
  `the_state_never_carries_personal_data_beyond_its_purpose` bekommt eine Gegenprobe: Die Rolle
  steht drin, Sperrlisten und Stammverein weiterhin nicht.
- **Abhängigkeiten:** keine neuen Pakete.

## Akzeptanzkriterien

**Konfiguration und Daten**
- [ ] AK1: Ohne `rotation_mode` in config.json gilt „Abwechselnd". Ein unbekannter Wert lädt ebenfalls als „Abwechselnd". Die Auswahl erscheint nur bei eingeschaltetem Schiedsrichter-Betrieb.
- [ ] AK2: Ein Moduswechsel wirkt ab der nächsten Feldbelegung, ohne die Übertragung neu zu starten. Laufende und eingeteilte Besetzungen bleiben unverändert.
- [ ] AK3: Eine Rolle „nur SR"/„nur AR" überlebt einen App-Neustart. „Beides" ohne weitere Zusatzdaten erzeugt keinen Dateieintrag.
- [ ] AK4: Eine `officials-state.json` von v0.9.355 lädt mit Reihenfolge, Pausen und Sperrlisten.
- [ ] AK5: Beim ersten Wechsel nach A entspricht die AR-Liste der bisherigen Reihenfolge. A → B → A lässt beide Listen unverändert.
- [ ] AK6: Ein neuer BTP-Official erscheint am Ende beider Listen, ohne Doppel.

**Modus A**
- [ ] AK7: SR ist die erste freie SR-fähige Person der SR-Liste. AR ist die erste freie AR-fähige Person der AR-Liste, die nicht SR ist. Eine „beides"-Person oben in beiden Listen wird SR, der AR kommt vom Nächsten der AR-Liste.
- [ ] AK8: Nach Spielende rückt der SR nur in der SR-Liste, der AR nur in der AR-Liste ans Ende.
- [ ] AK9: Umsortieren in einer Liste lässt die andere unverändert.

**Modus B**
- [ ] AK10: Nach Spielende stehen AR und SR in der Folge AR, SR am Listenende. Kommt das Paar zusammen wieder hoch, wird der bisherige AR SR.
- [ ] AK11: Eine Person „nur AR" wird nie automatisch SR und behält ihren Platz in der Liste.

**Modus C**
- [ ] AK12: Liste [P1 zuletzt SR, P2 zuletzt AR], beide Dienste offen → P1 = AR, P2 = SR.
- [ ] AK13: Liste [P1 zuletzt SR, P2 zuletzt SR, P3 zuletzt AR] → P1 = AR, P3 = SR, P2 bleibt frei.
- [ ] AK14: Eine Person ohne Einsatz wird SR, wenn SR offen ist, sonst AR.
- [ ] AK15: Nur SR offen, alle Freien zuletzt SR → SR bekommt die erste freie SR-fähige Person (Rückfall).
- [ ] AK16: Eine Person „nur SR", die zuletzt SR war, wird wieder SR.
- [ ] AK17: Ein Walkover ändert die Vorrolle nicht, die Officials rücken trotzdem ans Ende.
- [ ] AK18: Zwei Spiele mit gleicher Endezeit → die höhere Match-ID bestimmt die Vorrolle. Ein Spiel ohne Endezeit verliert gegen jedes gestempelte.
- [ ] AK19: Zwei Felder im selben Takt: Die Vorrolle stammt aus den beendeten Spielen. Feld 1 beeinflusst Feld 2 nur über die Belegung.

**Alle Modi**
- [ ] AK20: Ist niemand mit passender Rolle frei, bleibt der Dienst leer und wird später nicht nachgefüllt.
- [ ] AK21: Feldschalter und `rotation_sr/ar` gelten unverändert. Ein von Hand gelöster Dienst (`Some(0)`) wird nicht besetzt.
- [ ] AK22: Eine BTP-Besetzung gewinnt. Die Rotation füllt nur offene Dienste. Reservierte Personen (ADR 0078) werden übersprungen.

**Meetingpoint**
- [ ] AK23: Der Vorab-SR folgt Modus und Rolle: in A aus der SR-Liste, in C wird eine Person bevorzugt, deren Vorrolle nicht SR ist.
- [ ] AK24: Wird eine Person mit Vorab-SR auf „nur AR" gestellt, wird sie nicht übernommen, und die Rotation besetzt den SR neu.

**Warnung**
- [ ] AK25: Eine Handzuweisung einer Person „nur AR" als SR wird ausgeführt. Feldkachel, TL-Feld, Warteliste und die Desktop-Rückmeldung zeigen „SR: nur AR".
- [ ] AK26: Der SR⇄AR-Tausch im TL-Dialog funktioniert unabhängig von Rollen.
- [ ] AK27: Eine BTP-Besetzung gegen die Rolle erzeugt dieselbe Warnung.

**Anzeige**
- [ ] AK28: Im Desktop-Panel trägt jede freie Person, die die Automatik einsetzen würde, die Marke SR oder AR. Die nächste Runde ist hervorgehoben. In A gibt es zwei einzeln ziehbare Listen.
- [ ] AK29: Pausierte, im Dienst befindliche oder reservierte Personen und Vorab-SR tragen keine Marke. Eine Person „nur AR" bei abgeschalteter AR-Rotation trägt ebenfalls keine.
- [ ] AK30: Das Abrufen der Liste ändert keine Reihenfolge und legt keine AR-Liste an.
- [ ] AK31: Der TL-Web-Kopf zeigt „nächster SR: X · AR: Y" passend zum Desktop. Ein alter Host ohne Vorschau-Felder ergibt die bisherige Anzeige.
- [ ] AK32: Sperrlisten und Stammverein bleiben aus dem TL-Zustand draußen (Wächter grün).

**Fehlerfälle**
- [ ] AK33: Rolle setzen oder AR-Liste umsortieren ohne Schiedsrichter-Betrieb → Fehlermeldung, nichts gespeichert.
- [ ] AK34: Umsortieren mit unbekannter ID oder unbekanntem Ziel → keine Änderung.

## Tests

Pflicht ist TDD: Rust-Unit-Tests zuerst. `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo fmt --check` und `npm run build` müssen grün sein.

- **`officials.rs`, reine Funktion `waehle_besetzung` je Modus:**
  - AK7, AK11–AK16, AK20
  - nur ein Dienst offen
  - feste Rolle ignoriert die Vorrolle
- **`vorrollen`:** AK17, AK18
- **`nach_spielende`:** AK8, AK10 (je Modus)
- **Store:**
  - serde-Kompatibilität alter Dateien (AK4)
  - `is_empty` mit Rolle (AK3)
  - `order_ar` lesen ohne Anlegen (AK30)
  - `reorder(Ar)` (AK9)
  - `sync_roster` beide Listen (AK6)
  - Turnierwechsel verwirft `order_ar`
- **`config.rs`:** fehlender oder unbekannter `rotation_mode` → Abwechselnd (AK1)
- **`sync.rs`:**
  - zwei Felder in einem Takt in C (AK19)
  - A rückt je Liste (AK8)
  - Walkover (AK17)
  - Spiegel ohne Sync-Neustart (AK2)
  - Anpassung der bestehenden Tests zur Ende-Folge (`track_officials_rueckt_nach_spielende_ans_ende…`, `…nach_courtid_sortiert`)
- **`meetingpoint.rs`:** AK23, AK24
- **Warnung:**
  - `rollen_warnung` je Kombination
  - `court_officials`/`wartelisten_warnung` verbinden Konflikt und Rolle mit „ · "
  - `official_assign` führt trotz Rolle aus
  - `pruefe_einteilung` lehnt nicht ab (AK25–AK27)
- **Vorschau:**
  - ohne Seiteneffekte
  - B alternierend, C nach Vorrolle, A je Liste
  - keine Marke für pausierte, belegte, reservierte Personen und Vorab-SR
  - leer bei abgeschalteter Rotation
  - Desktop und TL liefern dieselbe Vorschau (AK28–AK30)
- **`tl.rs`:** Wächter-Gegenprobe (AK32); `TlOfficial` ohne neue Felder deserialisierbar
- **Manuell:** Testturnier mit vier bis sechs Officials je Modus, Vorschau gegen tatsächliche Besetzung vergleichen.

## Risiken & Rollback

- **B ändert sich für bestehende Turniere** (`[AR, SR]`): gewollt, steht im Changelog und im Handbuch.
- **Meetingpoint und Rotation laufen auseinander**, wenn ein zweiter Wahlweg übrig bleibt. Gegenmittel: `next_free` entfernen bzw. umbiegen; im Review prüfen.
- **Unzuverlässige Endezeiten** nach Erststart oder Turnierwechsel: Die Vorrolle stützt sich dann auf die Match-ID. Hingenommen.
- **`tl.html` geht beim main-Merge über den Relay live**, bevor die Hosts aktualisiert sind. Der Rückfall auf die alte Vorschau ist Pflicht.
- **PR 5 ohne vorherigen Relay-Deploy:** Der Zug landete in der SR-Liste. Deshalb Relay zuerst.
- **Downgrade:** Eine ältere Version verliert Rolle und AR-Liste beim nächsten Speichern, ohne Rückmeldung. Wird dokumentiert.
- **Rollback:** Modus auf „Abwechselnd" stellen. Das entspricht dem alten Verhalten bis auf `[AR, SR]`; der Rollenfilter wirkt bei „beides" für alle nicht. Eine ältere App liest die Datei weiter.

## Offene Fragen / Annahmen

- **Annahme:** Die Rolle ist kein schutzbedürftiges Personendatum im Sinne der Sperrlisten (Datenschutz-Abschnitt). Bei Freigabe der Spec wird das in CLAUDE.md „Datenschutz" vermerkt.
- **Annahme:** Weniger als 100 Officials je Turnier, deshalb ist die Runden-Simulation mit O(n²) unkritisch.

## Betroffene Doku-Dateien

- `docs/schiedsrichter-management.md`: Regeln je Modus, Datenmodell, B-Änderung, Downgrade-Hinweis.
- `docs/meetingpoint.md`: Vorab-SR beachtet Modus und Rolle.
- `docs/einstellungen.md` (Handbuch, mit `pruef`-Ankern für die Modusnamen und den Standard „Abwechselnd").
- `docs/erste-schritte.md`, falls Schiedsrichter dort erwähnt sind.
- `docs/oberflaechen.md` (Panel: Rolle, Marken, zwei Listen) und `docs/turnierleitung-web.md` (Vorschau, Marke, Modus-A-Beschriftung).
- `docs/cloud-relay.md` (nur PR 5), `docs/changelog.md` je Version, `docs/roadmap.md`.
- CLAUDE.md-Tabelle: Zeilen „Schiedsrichtermanagement" und „Schiedsrichter in der Warteliste" um die neuen Symbole ergänzen.

## Umsetzungs-Hinweise

Erst nach Freigabe. Grundlage ist der Merge von #457. Jeder PR bumpt die Version gemeinsam in
`src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` und `package.json` (Nummer erst beim Merge
festlegen). Nach jedem PR läuft der `code-reviewer`. Ein `security-reviewer` ist nicht nötig, weil es
keinen neuen externen Input gibt: `official_set_role` nimmt nur Enum-Strings vom Turnier-PC; in PR 5
dann einmal für den neuen `TlAction`.

1. **PR 1 – Datenmodell + Konfiguration, ohne Verhaltensänderung:**
   - `RotationMode`, `OfficialsConfig.rotation_mode`, Store-Spiegel (Start + `save_config`)
   - `Einsatzrolle` + `OfficialExtra.role` + `set_role`
   - `order_ar` mit Lesen/Anlegen/Selbstheilung, `sync_roster`, `reorder(…, liste)`
   - ADR 0079 auf accepted
2. **PR 2 – Wahl-Kern:**
   - `waehle_besetzung` (rein, auf geklonten Listen/Extras), `vorrollen`
   - `RotationInput.modus/vorrolle`, `rotate_court` umstellen, `nach_spielende`
   - Verdrahtung in `track_officials`
   - Meetingpoint (`vorab_sr_verteilen`, `uebernimm_vorab_sr`), `next_free` abbauen
3. **PR 3 – Setup + Rollenpflege Desktop + Warnung:**
   - Radio „Einteilungsprinzip" mit je einem Satz Erklärung und dem Schalter-Hinweis
   - `official_set_role`, Rollenauswahl im OfficialsPanel
   - `rollen_warnung` in `court_officials`, `wartelisten_warnung`, `official_assign`
4. **PR 4 – Vorschau:**
   - `prognose`, `OfficialView`/`TlOfficial` (`role`, `position_ar`, `next_role`, `next_rank`), `TlState.officials_rotation_mode`
   - OfficialsPanel mit Marken und zwei Listen in A
   - `tl.html`: Vorschau mit Rückfall, Zeilen-Marke, „(nur AR)" im Dialog, „SR-Reihenfolge" in A
5. **PR 5 – TL-Web zwei Listen + Rollenpflege:**
   - `relay-proto` `OfficialReorder.list`, `OfficialSetRole`, `TlState.officials_ar_order`
   - **Relay-Deploy vor dem Client-Release**
