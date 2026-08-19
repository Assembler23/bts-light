# Spiele von der automatischen Feldvergabe ausnehmen — Spezifikation

> Status: **umgesetzt 2026-08-14** (via /idee: Brief → Grill → How-To → Review). Feldtest an einem
> laufenden Turnier steht noch aus.
> Quelle: Idee im Gespräch mit der Turnierleitung. Betroffene Crates: src-tauri, relay-proto, src.
> ADR: keiner nötig — Persistenz folgt 1:1 dem in [ADR 0022](adr/0022-officials-turnierdaten-eigene-datei.md) etablierten Muster.

## Kontext / Problem

Die automatische Feldvergabe (`sync.rs::auto_assign`) belegt freie, lange
genug freie, nicht gesperrte Felder automatisch mit dem nächsten
spielbereiten Match. Bisher lässt sich das nur **global** pausieren
(`auto_assign_paused`, `TlAction::SetAutoAssign`) — nicht pro Spiel. Wenn
ein Spieler kurzfristig nicht greifbar ist oder die Turnierleitung ein
bestimmtes Spiel bewusst zurückhalten will (z. B. wegen einer noch
ungeklärten Ansetzungsfrage), gibt es dafür aktuell keine gezielte
Möglichkeit — nur das Abschalten der gesamten Automatik, was alle anderen
Felder mit betrifft, oder eine manuelle Dauerbeobachtung durch die
Turnierleitung.

## Zielbild & Erfolgskriterien

Die Turnierleitung kann ein einzelnes Spiel per Knopfdruck — in TL-Web
**und** am Turnier-PC — als „von der Auto-Vergabe ausgenommen" markieren.
Ein ausgenommenes Spiel wird von `auto_assign` komplett übersprungen, bis
es wieder aktiviert wird; alle anderen Felder und Spiele laufen unbeeinflusst
automatisch weiter. Manuelles Zuweisen bleibt für ein ausgenommenes Spiel
jederzeit möglich.

**Erfolgskriterium:** Beim nächsten Turnier wird kein als ausgenommen
markiertes Spiel von der Automatik zugewiesen (prüfbar per Log-Review:
kein `Auto-Feldvergabe`-Eintrag für eine ausgenommene `match_id`, solange
die Ausnahme aktiv ist).

## Nicht-Ziele

- Kein Einfluss auf manuelles Zuweisen (`AssignCourt`/`MoveMatch`) — ein
  ausgenommenes Spiel lässt sich weiterhin von Hand aufs Feld legen.
- Keine Mehrfachauswahl/Sammel-Aktion (Einzel-Toggle pro Spiel genügt
  fürs Erste, YAGNI).
- Kein automatisches Zeit-Timeout für die Ausnahme — sie bleibt aktiv, bis
  sie manuell zurückgenommen wird.
- Keine Änderung an der globalen Auto-Vergabe-Pause (`SetAutoAssign`) —
  bleibt als separater, weiterhin nur laufzeit-lokaler Schalter bestehen.
- Kein zusätzlicher globaler `AppConfig`-Schalter (wie `officials.enabled`)
  — das Feature ist immer verfügbar, es wirkt ohnehin nur, solange die
  Auto-Vergabe selbst aktiv ist.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - `src-tauri/src/tablet/exclusion.rs` (neu) — Store, Muster ADR 0022.
  - `src-tauri/src/tablet/state.rs` — Wiring in `TabletState`.
  - `src-tauri/src/sync.rs` — Filter in `auto_assign`, Aufräumen bei
    Spielende.
  - `src-tauri/src/tablet/tl.rs` — TL-Web-Action-Handler, `TlMatch`-Feld.
  - `src-tauri/src/commands.rs` — Desktop-Command, `PreparationCandidate`-
    Feld, Init-Pfad.
  - `src-tauri/src/lib.rs` — Handler-Registrierung.
  - `relay-proto/src/lib.rs` — neue `TlAction`-Variante.
  - `assets/tl.html` — Badge + Toggle in der Warteliste.
  - `src/pages/FieldOverviewPage.tsx`, `src/types.ts`, `src/api.ts` —
    Desktop-Anzeige + Command-Aufruf.
- **Architekturregeln (CLAUDE.md R1–R6):**
  - **R1** eingehalten: Desktop-UI ruft ausschließlich den neuen Tauri-
    Command `auto_assign_exclude` (kein direkter Zustandszugriff aus React).
  - **R2** eingehalten: Der Ausnahme-Zustand ist reiner bts-light-lokaler
    Bedienzustand (wie die Schiedsrichter-Reihenfolge), keine
    BTP-Rückschreibung — BTP bleibt für Match/Court-Zuordnungen die
    Wahrheit, die Ausnahme wirkt nur als zusätzlicher, lokal geprüfter
    Filter vor der automatischen Vergabe.
  - **R3** eingehalten: Der Store lebt in `TabletState`, dort wo LAN-Server,
    Relay-Client und Tauri-Commands ohnehin denselben Stand sehen — kein
    Unterschied zwischen LAN- und Cloud-Pfad.
  - **R4/R5/R6** nicht berührt (kein Court/Namespace-Bezug, keine
    Ergebnisvalidierung, keine `install_id`-Nutzung).
- **Konfiguration & Abwärtskompatibilität:** Kein neues Feld in `config.rs`
  (bewusst, siehe Nicht-Ziele) — die neue `TlAction`-Variante ist additiv
  und ohne `#[serde(default)]` (Modul-Konvention: keine alte Gegenstelle zu
  schonen). `identifier` und Updater-Pfad unangetastet.
- **Datenschutz:** Der Store enthält ausschließlich `match_id`-Werte — kein
  Personendatum, keine Geburtsjahre, keine Spielernamen.
- **Abhängigkeiten:** Keine neue Cargo-/npm-Dependency. Keine BTP-Protokoll-
  Abhängigkeit (rein lokaler Zustand).

## Persistenz (Muster ADR 0022)

Eigene, turniergebundene Datei `excluded-matches.json` im
App-Datenverzeichnis (`tablet_exclusions_path`, analog
`tablet_officials_path`), **außerhalb** `config.json`:

```json
{ "tournament": "<Turniername>", "excluded": [1216, 1420] }
```

- Turnier-Kopf mit Verwerfungsregel wie ADR 0022: Beim Laden wird der Stand
  nur übernommen, wenn `tournament` mit dem aktuellen Turnier
  übereinstimmt — bei Abweichung „lieber verwerfen als falsch zuordnen".
- Unlesbare Datei (IO-Fehler) bleibt beim Laden unangetastet, kein
  Überschreiben — Retry beim nächsten Snapshot (`Ladung::Unlesbar`,
  identisches Verhalten zu `officials.rs`).
- Kaputter/nicht parsbarer Inhalt gilt als `Leer` und wird beim nächsten
  Schreiben überschrieben (nicht zu retten).
- Atomares Schreiben über `.json.tmp` + `rename`.
- **Aufräumen:** Ein Ausnahme-Eintrag wird automatisch entfernt, sobald das
  zugehörige Match `MatchStatus::Finished` erreicht (deckt auch
  Walkover/Retired ab, die in BTP ebenfalls als `Finished` + `Winner`
  geführt werden) oder nicht mehr im Snapshot vorkommt. Setzt eine spätere
  Ergebniskorrektur (ADR 0013) das Match wieder auf offen, ist die
  Ausnahme weg — das Match landet dann ohne Ausnahme in der normalen
  Auto-Vergabe-Kandidatenliste (gewolltes Verhalten, kein Datenverlust-Bug).

## Akzeptanzkriterien

- [ ] Ein Spiel, das als ausgenommen markiert ist, wird von
      `auto_assign` bei der Kandidatenauswahl übersprungen, solange die
      Ausnahme aktiv ist — auch wenn ein Feld frei wird.
- [ ] Dasselbe Spiel lässt sich weiterhin manuell (TL-Web „Auf Feld
      legen", Desktop-Zuweisung) auf ein freies Feld legen.
- [ ] Die Markierung ist sowohl über TL-Web (`TlAction::
      ExcludeFromAutoAssign`) als auch über den Desktop-Command
      `auto_assign_exclude` setz- und rücknehmbar; beide Wege mutieren
      denselben Store — eine über TL-Web gesetzte Ausnahme ist sofort auch
      im Desktop-Badge sichtbar und umgekehrt.
- [ ] Die Warteliste in TL-Web (`assets/tl.html`) zeigt ein Badge/Symbol an
      einem ausgenommenen Spiel.
- [ ] Die Tabelle „Nicht zugewiesene Spiele" in
      `FieldOverviewPage.tsx` zeigt dasselbe Badge/Symbol.
- [ ] Ein App-Neustart mitten im Turnier erhält aktive Ausnahmen (Datei-
      Roundtrip, gleiches Turnier).
- [ ] Ein neues/anderes Turnier (abweichender `tournament`-Kopf) startet
      mit leerer Ausnahme-Liste — der alte Stand wird nicht übernommen,
      auch nicht auf der Platte überschrieben, solange kein neuer Eintrag
      geschrieben wird.
- [ ] Eine unlesbare (aber vorhandene) Ausnahme-Datei führt beim Start
      nicht zum Absturz und nicht zum Überschreiben — der nächste Snapshot
      versucht das Laden erneut.
- [ ] Sobald ein ausgenommenes Spiel den Status `Finished` erreicht (auch
      Walkover/Retired), wird die Ausnahme automatisch entfernt.
- [ ] Zwei parallele Zuweisungs-Versuche (Doppel-Klick/zwei Geräte) auf
      dieselbe Ausnahme-Aktion sind idempotent (Fingerprint-Dedupe, wie bei
      bestehenden `TlAction`-Varianten).
- [ ] Ein unbekanntes `match_id` (nicht im aktuellen Snapshot) wird beim
      Setzen der Ausnahme abgelehnt, nicht stillschweigend gespeichert.

## Tests

TDD-Pflicht, `cargo test` grün, `npm run build` fehlerfrei:

- `tablet/exclusion.rs`: Roundtrip über Neustart, Turnierwechsel verwirft
  (auch auf Platte), fremder Dateistand beim Start verworfen, unlesbare
  Datei bleibt unangetastet (Testnamen-Vorbild: die entsprechenden
  `officials.rs`-Tests).
- `state.rs`: `snapshot_bindet_das_exclusion_roster_ans_turnier`.
- `sync.rs`: `auto_assign_skips_excluded_match`,
  `exclusion_wird_bei_spielende_automatisch_entfernt`.
- `relay-proto`: Serde-Roundtrip für `TlAction::ExcludeFromAutoAssign`.
- `tablet/tl.rs`: Dispatch-Test für den neuen `apply_state_action`-Arm
  (Erfolg + Ablehnung bei unbekanntem Match), Fingerprint-Idempotenz-Test.
- Manueller Turnier-Testfall: Spiel in TL-Web ausnehmen → Feld wird frei →
  Auto-Vergabe überspringt es sichtbar im Log → Spiel manuell zuweisbar →
  reaktivieren → nächste Feld-Freigabe berücksichtigt es wieder.

## Risiken & Rollback

- **Laufendes Turnier:** Die Änderung ist rein additiv (neuer Filter, neue
  Datei, neue Wire-Variante) — bestehende Auto-Vergabe-Logik für
  nicht-ausgenommene Spiele ändert sich nicht. Ein Rollback auf eine
  ältere Version lässt die neue Datei einfach ungenutzt liegen (keine
  `config.json`-Änderung, daher keine Kompatibilitätsfrage beim
  Downgrade).
- **Risiko „vergessene Ausnahme":** Ein ausgenommenes Spiel könnte
  unbemerkt dauerhaft von der Automatik ausgeschlossen bleiben, wenn
  niemand daran denkt, es zu reaktivieren. Mitigation: sichtbares Badge in
  beiden Oberflächen (Akzeptanzkriterium oben) plus automatisches
  Aufräumen bei Spielende — die Ausnahme kann also nicht über das
  Turnierende hinaus „vergessen" liegen bleiben.
- **Race zwischen TL-Web- und Desktop-Schreibweg:** Beide mutieren
  denselben Store direkt (kein BTP-Umweg, kein Async-Settle-Fenster wie bei
  den Officials-Writes) — „letzter Schreibvorgang gewinnt" ist hier
  unproblematisch, da es sich um einen einfachen Boolean-Zustand ohne
  Seiteneffekte auf BTP handelt.

## Offene Fragen / Annahmen

Keine — alle im Grill identifizierten Blocker sind geklärt (siehe
`docs/features/_intake/feldvergabe-ausnahme/2-grill.md`). Explizite
Annahme: Der Dateiname `excluded-matches.json` ist ein Vorschlag aus der
How-To-Phase, kann bei der Umsetzung nach denselben Kriterien wie
`officials-state.json` benannt werden, sofern das Muster (eigene Datei,
Turnier-Kopf, Verwerfungsregel) erhalten bleibt.

## Betroffene Doku-Dateien

- **Diese Datei** (`docs/features/feldvergabe-ausnahme.md`) — primäre
  Doku-Heimat, da kein Tabelleneintrag in CLAUDE.md `sync.rs::auto_assign`
  oder einen neuen eigenständigen Store direkt abdeckt.
- `docs/turnierleitung-web.md` (Bedienung) und
  `docs/features/turnierleitung-web.md` (Spec-Referenz) — Tabellenzeile
  trifft auf die berührten `tablet/tl.rs`/`assets/tl.html`/`relay-proto`
  `Tl*`-Typen zu.
- `docs/changelog.md` bei Release (Pflicht für jede veröffentlichte
  Version).

## Umsetzungs-Hinweise

Vollständiger Schritt-für-Schritt-Plan in
`docs/features/_intake/feldvergabe-ausnahme/3-how-to.md` (13 Schritte).
Kurzfassung:

1. Neuer Store `tablet/exclusion.rs` (Muster `officials.rs`/ADR 0022).
2. Wiring in `TabletState` (`state.rs`) + Turnierbindung im Sync-Zyklus.
3. Pfad + Init in `commands.rs` (App-Start).
4. Aufräumen bei Spielende als neue Sync-Reconcile-Methode (`sync.rs`,
   analog `reconcile_officials`).
5. Filter in `auto_assign`s Kandidaten-Closure (`sync.rs`).
6. Neue `TlAction`-Variante (`relay-proto`).
7. Neuer Arm in `apply_state_action` (`tablet/tl.rs`) inkl. Fingerprint/
   Label.
8. `TlMatch`-Feld + Aufbau (`tl.rs`).
9. TL-Web-Badge/Toggle (`assets/tl.html`).
10. Desktop-Command `auto_assign_exclude` (`commands.rs`, Muster
    `official_pause`) + Handler-Registrierung (`lib.rs`).
11. `PreparationCandidate`-Feld (`commands.rs` + `src/types.ts`).
12. Desktop-Badge/Toggle (`FieldOverviewPage.tsx`, `src/api.ts`).
13. `npm run build`, `cargo test`, `cargo clippy --workspace --all-targets`.

**Reviews:** `code-reviewer` nach der Umsetzung (Pflicht). Kein
`security-reviewer` nötig — kein neuer externer User-Input/Auth/Datei-URL-
Pfad über das etablierte, bereits abgesicherte TL-Web-Actions- und
Tauri-Command-Muster hinaus.

**Version:** Bei Release gemeinsam bumpen in `src-tauri/Cargo.toml`,
`src-tauri/tauri.conf.json`, `package.json` (Projektregel).
