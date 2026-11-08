# Schiedsrichter in der TL-Web-Warteliste fest einteilen — Spezifikation

> Status: **freigegeben 2026-11-08** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Wunsch der Turnierleitung vom 08.11.2026. Betroffene Crates: src-tauri, src (Anzeige).
> relay/relay-proto: kein Code, aber Relay-Deploy (eingebettetes `tl.html`).
> ADR: [0078 — Die Besetzung wartender Spiele reserviert ihre Schiedsrichter](../adr/0078-besetzung-wartender-spiele-reserviert.md).
> Vorarbeiten (gemergt): #449 (v0.9.348, Marke „SR: … · AR: …" in der Warteliste),
> #450 (v0.9.349, kein eigenständiger Officials-Write mit `CourtID 0`, solange eine
> Feldzuweisung unterwegs ist).

## Kontext / Problem

Für Finals und Topspiele will die Turnierleitung (TL) den Schiedsrichter (SR) und
Aufschlagrichter (AR) **vorab fest** bestimmen. Heute geht das im TL-Web nur an der
Feld-Kachel, also erst, wenn das Spiel schon auf dem Feld steht. Zwei Lücken kommen dazu:

1. **Die Rotation kennt keine Vorab-Einteilung.** Wer in BTP vorab an einem wartenden
   Spiel steht, gilt für die Rotation als frei (`im_dienst` in `sync.rs::track_officials`
   zählt nur laufende Spiele). Die Person wird anderswo eingesetzt und fehlt, wenn ihr
   Spiel aufs Feld kommt.
2. **Umteilen gegen BTP wirkt nicht.** Teilt die TL einen anderen SR ein, als BTP führt,
   verwirft `OfficialsStore::confirm` die Hand-Einteilung im nächsten Lauf, weil BTP
   „irgendeinen" Wert trägt. In `docs/turnierleitung-web.md` steht das als Grenze.

## Zielbild & Erfolgskriterien

- Die TL öffnet im TL-Web am ⋮-Menü einer Wartelisten-Zeile „Schiedsrichter einteilen",
  wählt SR und/oder AR und sieht die Marke sofort an der Zeile. Das gilt auch für Spiele,
  deren Paarung noch offen ist (Finale vor dem Ende der Halbfinals).
- Wer für ein wartendes Spiel eingeteilt ist, wird von der Rotation **nirgends** sonst
  eingesetzt, bis sein Spiel beendet ist. Das gilt auch für SR, die in BTP vorab
  eingetragen sind.
- Kommt das Spiel aufs Feld, pfeift genau die eingeteilte Person. Steht sie noch auf
  einem anderen Feld, wartet die automatische Vergabe mit diesem Spiel.
- Kein Erklärbedarf: Menüpunkt, Dialog und Marken sehen aus wie an der Feld-Kachel.

## Nicht-Ziele

- Keine Einteilung in der Desktop-App. Die Desktop-App **zeigt** nur „reserviert Sp. N"
  in der SR-Liste.
- Keine klickbare Marke. Bedient wird nur über das ⋮-Menü.
- Doppel-Einteilungen werden nicht verhindert, nur markiert.
- Kein Notschalter in den Einstellungen.
- Die Prognose (`predict.rs`) berücksichtigt die Wartesperre nicht. Für betroffene
  Spiele ist sie bewusst leicht zu optimistisch.
- Die manuelle Feldvergabe (Kachel, Desktop, BTP) wird durch die Wartesperre nicht
  blockiert.

## Begriffe

- **Wartendes Spiel:** `MatchStatus::Scheduled`, also auch gerufene, offene Paarungen und
  vom Feld genommene Spiele. Ein Turniertag-Feld gibt es in BTP nicht.
- **Reservierend:** ein wartendes Spiel, dessen `planned_time` (Schlüssel `YYYYMMDDHHMM`)
  **am heutigen Tag oder davor** liegt oder fehlt. „Heute" ist das lokale Datum des
  Turnier-PCs. Spiele späterer Tage reservieren nicht.
- **Besetzung für die Reservierung** (Sicht „Hand vor BTP"): die lokale Absicht, falls
  vorhanden, sonst der BTP-Wert. `Some(0)` („ohne") heißt kein Dienst. Gezählt wird nur,
  wer in der BTP-Officials-Liste steht.
- **Anzeige** an Zeile und Kachel: weiter `effective` (bestätigter Stand, R2).

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - `src-tauri/src/tablet/officials.rs`: `fuer_schreiben`, `Belegung`, `confirm` (Umteilen),
    `FinishedMatch.walkover`, `appearances`, `RotationInput.belegt`, `pruefe_einteilung`.
  - `src-tauri/src/tablet/state.rs`: `officials_for_write` gibt an `fuer_schreiben` weiter.
  - `src-tauri/src/sync.rs`: `track_officials`, `auto_assign` (Wartesperre, auch bei der
    Wunschfeld-Reservierung), `reconcile_officials`.
  - `src-tauri/src/tablet/meetingpoint.rs`: `uebernimm_vorab_sr`, `vorab_sr_verteilen`;
    `officials_im_dienst` geht in `Belegung` auf.
  - `src-tauri/src/tablet/tl.rs`: Flag `can_assign_waiting_officials`, `TlMatch.sr_id/ar_id`,
    `TlOpenMatch.sr/ar/sr_id/ar_id/official_warn`, Erweiterung von `official_warn`,
    `TlOfficial.reserved/reserved_match_num`, Prüfung von `OfficialAssign`/`OfficialClear`,
    Protokoll-Etikett.
  - `src-tauri/assets/tl.html`: ⋮-Eintrag, Dialog im Spiel-Modus, Vorschau „nächster",
    Marke „reserviert Sp. N".
  - `src-tauri/src/commands.rs` + `src/api.ts` + `src/pages/OfficialsPanel.tsx`: Anzeige
    „reserviert Sp. N"; dieselbe Prüfung in `official_assign`.
- **R1:** Die Desktop-Anzeige läuft über das bestehende Command `officials_roster`.
- **R2:** BTP bleibt die Wahrheit. Hand-Einteilungen gehen über den bestehenden Rücksync
  nach BTP. Ändert die TL danach **in BTP**, gewinnt BTP (siehe AK 12).
- **R3:** Kein neuer Wire-Typ, `OfficialAssign`/`OfficialClear` mit `courtId: 0` gibt es
  schon. Der Relay reicht Zustand und Aktion unverändert durch. Weil er `tl.html`
  einbettet und bei jedem Merge neu deployt wird, schützt das Flag
  `can_assign_waiting_officials` die Zeit bis zum App-Tag.
- **R4–R6:** nicht berührt.
- **Konfiguration:** keine neuen Felder. `officials-state.json` bleibt im Format, neue
  Felder höchstens mit `serde(default)`.
- **Datenschutz:** nur SR/AR-Name, ID und Konflikt-**Kategorie**, wie an der Kachel.
  Sperrlisten und Stammverein bleiben draußen. An `TlOpenMatch` ist die Besetzung keine
  Kandidaten-Information und trägt keine Lizenznummer. Der Wächter-Test wird erweitert.
- **Abhängigkeiten:** BTP-Verhalten bei `Official1ID` an Spielen ohne Feld ist ungemessen
  (siehe Offene Fragen). Keine neue Dependency.

## Akzeptanzkriterien

**Einteilen (TL-Web)**
- [ ] AK 1: Mit Schiedsrichter-Betrieb **und** einem Host, der
  `can_assign_waiting_officials` meldet, trägt jede Wartelisten-Zeile im ⋮-Menü
  „Schiedsrichter einteilen", auch offene Paarungen. Ohne Flag (alter Host) oder ohne
  Schiedsrichter-Betrieb fehlt der Eintrag.
- [ ] AK 2: Der Dialog zeigt als Titel Spielnummer und Paarung, wählt die aktuelle
  Besetzung über die ID vor und kennzeichnet Optionen mit „(im Dienst)",
  „(reserviert Sp. N)" und „(Pause)". Er sendet `official_assign`/`official_clear` mit
  `courtId: 0`.
- [ ] AK 3: Nach dem Speichern zeigt die Zeile „SR: … · AR: …". Die Marke an offenen
  Paarungen erscheint genauso.
- [ ] AK 4: Der Turnier-PC lehnt mit verständlicher Meldung ab: unbekanntes Spiel,
  beendetes Spiel (Einteilen **und** Lösen), SR = AR an demselben Spiel. Es wird nichts
  gespeichert und nichts nach BTP geschrieben.
- [ ] AK 5: Das Protokoll nennt „Schiedsrichter für Spiel N (Warteliste)" statt „(Feld 0)".

**Reservierung**
- [ ] AK 6: Ist X an einem reservierenden Spiel A eingeteilt (BTP-vorab oder von Hand),
  setzt die Rotation X an keinem anderen Feld ein. Die Meetingpoint-Vorab-Verteilung
  überspringt X ebenfalls, außer an A.
- [ ] AK 7: Ist X an einem Spiel eines späteren Tages eingeteilt, bleibt X in der Rotation.
- [ ] AK 8: Die Reservierung endet, wenn A beendet ist (auch durch Walkover oder
  Disqualifikation). Wird A vom Feld genommen, ist X wieder reserviert. Eine Person, die
  nicht mehr in der BTP-Officials-Liste steht, reserviert nicht.
- [ ] AK 9: Ein ausdrückliches „ohne" (`Some(0)`) reserviert niemanden, auch nicht den
  BTP-Wert davor.
- [ ] AK 10: Pausiert die TL eine reservierte Person, bleibt die Einteilung bestehen. Die
  Zeile warnt mit „SR in Pause" bzw. „AR in Pause".
- [ ] AK 11: Ist X an A **und** an einem zweiten wartenden Spiel B eingeteilt, warnen
  beide Zeilen mit „SR doppelt" bzw. „AR doppelt". Dienst auf einem laufenden Feld ist
  keine Doppel-Einteilung. Mehrere Warnungen stehen in fester Reihenfolge
  „Verein/Person · doppelt · Pause" in `official_warn`.

**Umteilen gegen BTP**
- [ ] AK 12: Führt BTP Y an A und die TL teilt X ein, geht X nach BTP und ersetzt Y; bis
  zur Bestätigung ist X reserviert und Y frei. Gilt auch an der Feld-Kachel. Ändert danach
  jemand **in BTP** auf Z, gilt Z, und bts-light schreibt nicht dagegen.

**Feldvergabe**
- [ ] AK 13: Kommt A aufs Feld (Automatik, Kachel, Desktop oder BTP), pfeift X. Die
  Rotation ersetzt eine feststehende Besetzung nicht.
- [ ] AK 14: Die automatische Vergabe legt A nicht aufs Feld, solange X oder der AR von A
  auf einem anderen Feld Dienst tun. Sie vergibt in einem Lauf nie zwei Felder mit
  derselben Person. Eine Wunschfeld-Reservierung hält das Feld für A nicht frei, solange
  dessen SR im Dienst ist. Pause blockiert nicht.
- [ ] AK 15: Die manuelle Feldvergabe bleibt in diesem Fall möglich. Die Kachel zeigt die
  Besetzung wie bisher.

**Anzeige der SR-Liste**
- [ ] AK 16: In TL-Web und Desktop trägt eine reservierte Person die Marke
  „reserviert Sp. N" (ohne Spielnummer „reserviert"). Die Vorschau „nächster: …"
  überspringt reservierte Personen.

**Statistik**
- [ ] AK 17: Ein beendetes Spiel mit Ergebnis Walkover zählt in `appearances` (TL-Web,
  Desktop, Detail-Listen) **nicht** als Einsatz. Aufgabe und Disqualifikation zählen
  weiter. `move_to_end` bleibt unverändert.

**Nebenwirkungen**
- [ ] AK 18: Der Vorabzettel (`Modus::Vorab`) druckt die Hand-Besetzung eines wartenden
  Spiels.
- [ ] AK 19: Ohne Schiedsrichter-Betrieb ändert sich nichts: keine Marken, kein
  Menüpunkt, keine Wartesperre.

## Tests

TDD, `cargo test --workspace` grün, `clippy -D warnings`, `npm run build`.

- **officials.rs:** Umteilen über einen BTP-Wert bleibt bis zum Write (`confirm`), und
  danach gewinnt BTP; `fuer_schreiben` (Hand vor BTP, `Some(0)`); `Belegung`:
  BTP-vorab und Hand reservieren; spätere Tage nicht; früheres Datum und fehlende Zeit
  schon; Person nicht in BTP zählt nicht; Ende bei `Finished` inkl. Walkover; nach
  „vom Feld" wieder reserviert; `belegt_ausser` nimmt das eigene Spiel aus; „doppelt"
  nur bei zwei wartenden; offene Paarung reserviert; `pruefe_einteilung` (unbekannt,
  beendet, SR = AR); `appearances` ohne Walkover, mit Aufgabe und Disqualifikation.
- **sync.rs:** `track_officials` überspringt Reservierte; `auto_assign` wartet bei SR auf
  anderem Feld; keine zwei Felder mit derselben Person in einem Lauf; Wunschfeld hält
  nicht frei; pausierter Reservierter blockiert nicht; `officials_entries` schreibt die
  Umteilung X statt Y.
- **meetingpoint.rs:** Vorab-Verteilung überspringt Reservierte; Vorab-SR X an der
  gerufenen Partie A, TL reserviert X für B → A kommt aufs Feld ohne X und bekommt einen
  anderen; Übernahme greift nicht bei einer Reservierung für ein anderes Spiel.
- **tl.rs:** IDs an `TlMatch` (leer weggelassen); Besetzung an `TlOpenMatch`;
  Reihenfolge in `official_warn`; `OfficialAssign`/`OfficialClear`-Ablehnungen;
  Wartespiel mit `courtId 0` ok; Etikett „(Warteliste)"; `TlOfficial.reserved`;
  Flag gemeldet; Serde-Roundtrip eines alten Zustands ohne die neuen Felder;
  Datenschutz-Wächter mit SR an einer offenen Paarung (Name und Kategorie positiv,
  Sperrlisten und Verein nirgends).
- **Manuell (PR 4b):** LAN-Seite und Cloud-Seite; alter Host ohne Flag zeigt keinen
  Menüpunkt; offene Paarung; Vorabzettel.

## Risiken & Rollback

- **Kleinerer Pool:** Viele BTP-Vorabeinträge für heute nehmen Personen sofort nach dem
  Update aus der Rotation. Die Rotation findet dann vielleicht niemanden mehr, und die
  Automatik wartet häufiger. Gegenmittel: Update am Turnieranfang einspielen; die TL löst
  einzelne Einteilungen per Hand.
- **Leeres Feld durch die Wartesperre:** gewollt, damit kein Spiel ohne anwesenden SR
  beginnt. Die TL kann den SR lösen, das Spiel ausnehmen oder es von Hand vergeben.
- **Eigenständiger Officials-Write mit `CourtID 0`** an wartenden Spielen wird häufiger.
  #450 schützt nur Vergaben aus bts-light. Vergibt jemand gleichzeitig **in BTP** ein Feld,
  bleibt ein Restrisiko, das die BTP-Messung klärt.
- **Rollback:** Eine ältere Version lässt sich installieren. Neue Felder sind
  `serde(default)`, `officials-state.json` bleibt lesbar. Ein neueres Cloud-`tl.html` an
  einem alten Host zeigt dank Flag keinen Menüpunkt.

## Offene Fragen / Annahmen

- **BTP-Messung vor PR 4b** (Ergebnis in `docs/btp_protocol.md`): Nimmt BTP `Official1ID`
  per `SENDUPDATE` an einem Spiel **ohne Feld** an, (i) mit `CourtID 0`, (ii) ganz ohne
  CourtID-Element, ohne die Feldzuordnung anzufassen? Fällt (ii) positiv aus, lässt der
  eigenständige Write bei Spielen ohne Feld das CourtID-Element weg (kleine `proto.rs`-
  Änderung samt Test). Speichert BTP gar nicht, bleibt die Hand-Einteilung lokal, wirkt
  trotzdem als Reservierung und reist beim Feldaufruf im Zuweisungs-Write mit. Das
  gehört dann ins Handbuch.
- **Annahme „heute oder davor":** Ein liegengebliebenes Spiel vom Vortag reserviert. Es
  ist Arbeit des laufenden Tages.
- **Annahme:** Eine Person, die nicht mehr in der BTP-Officials-Liste steht, reserviert
  nicht. Ihre ID wird aber weiter zurückgeschrieben, wie heute.

## Betroffene Doku-Dateien

- `docs/schiedsrichter-management.md`: Reservierung, Umteilen, Walkover-Statistik; im
  Handbuch mit Prüf-Ankern für „Schiedsrichter einteilen" und „reserviert".
- `docs/turnierleitung-web.md`: Menüpunkt, Dialog, Marken; den Satz „Steht in BTP schon
  ein Schiedsrichter … wirkt die Auswahl hier nicht" ersetzen.
- `docs/features/schiedsrichter-management.md`: Verweis auf diese Spec.
- `docs/meetingpoint.md`: Vorrang und Reservierung bei der Vorab-Verteilung.
- `docs/cloud-relay.md`: neue Zustandsfelder, Flag, Deploy-Reihenfolge.
- `docs/btp_protocol.md`: Ergebnis der Messung.
- `docs/roadmap.md`, `docs/changelog.md`, ADR 0078.

## Umsetzungs-Hinweise

Plan: `docs/features/_intake/tl-warteliste-schiedsrichter/3-how-to.md` (gitignoriert).
Reihenfolge, je PR mit Versionssprung in `src-tauri/Cargo.toml`,
`src-tauri/tauri.conf.json` und `package.json`, `code-reviewer` immer:

1. **fix: Umteilen gegen BTP** (AK 12). `confirm` bestätigt eine Rolle nur, wenn BTP gleich
   lokal ist **oder** die Einteilung schon geschrieben wurde (`officials_written`); dann
   heißt Abweichung „in BTP umgeteilt". Alternative bei Bedarf: BTP-Ausgangswert an der
   Zuweisung merken (`serde default`).
2. **feat: Rotation reserviert die Besetzung wartender Spiele** (AK 6–10, 13–14; ADR 0078).
   Eine Quelle `Belegung::aus(store, snap, heute)` in `officials.rs`; Verbraucher:
   `track_officials`, `auto_assign` mit eigener Wartesperre (nicht über
   `PlayerAvailability`, die Hand-Vergabe bliebe sonst hängen), `uebernimm_vorab_sr`,
   `vorab_sr_verteilen`. `officials_for_write` gibt an `fuer_schreiben` weiter.
3. **feat: Walkover zählt nicht als Einsatz** (AK 17). `FinishedMatch::aus_btp` an allen
   drei Bau-Stellen.
4. a) **feat(tl-web): Turnier-PC-Seite** (AK 4, 5, 11, 16, Felder, Flag, Desktop-Anzeige).
   b) **feat(tl-web): Menüpunkt + Dialog** (AK 1–3, 18), nur `tl.html`. Darf vor dem
   App-Tag mergen, weil das Flag schützt. Für 4a/4b zusätzlich `security-reviewer`
   (neuer Eingabeweg über die Cloud auf `OfficialAssign`, Datenschutz `TlOpenMatch`).
