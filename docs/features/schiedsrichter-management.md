# Schiedsrichtermanagement — Spezifikation

> Status: **freigegeben 2026-08-13** (via /idee: Brief → Grill → How-To → Review; inkl. Nachträge Sperrlisten-Pflege in TL-Web und Einsatz-Ableitung aus beendeten Spielen).
> Quelle: Idee (Nutzer-Brief 2026-08-13). Betroffene Crates: src-tauri, relay-proto, relay (Neucompile/Deploy), src.
> ADR: [0021 (Rücksync: eigenständiger Write)](../adr/0021-officials-ruecksync-eigenstaendiger-write.md) ·
> [0022 (Ablage Turnierdaten)](../adr/0022-officials-turnierdaten-eigene-datei.md).
> Schritt 1 (BTP-Messung) am 13.08.2026 **erledigt** — Ergebnisse eingearbeitet,
> Details in [btp_protocol.md](../btp_protocol.md) („Officials: Struktur & Schreibweg").

## Kontext / Problem

In BTP pflegt die Turnierleitung eine Schiedsrichterliste (`Officials`) und
kann je Spiel einen Schiedsrichter (SR) und einen Aufschlagrichter (AR)
setzen (`Match.Official1ID`/`Official2ID`). BTS Light kennt dieses Konzept
bisher gar nicht (`docs/btp-write-vergleich-letilo.md`: „bts-light hat kein
Schiedsrichter-Konzept"). Turniere mit Schiedsrichtern verwalten die
Einteilung deshalb auf Papier oder direkt in BTP — ohne Rotation, ohne
Konfliktprüfung, ohne Sichtbarkeit in TL-Web, am Tablet oder in den
Ansagen. Der Schmerz liegt bei der Turnierleitung (manuelle, fehleranfällige
Einteilung) und bei den Schiedsrichtern (unklare Reihenfolge, keine
Ansage).

Real existieren drei Betriebsformen, oft gemischt im selben Turnier:

1. SR bedient das Tablet selbst — kein Spieler als Tabletbediener nötig.
2. SR schiedst mit Papier-Zettel — zusätzlich ein Spieler als Tabletbediener.
3. Kein SR — nur ein Spieler als Tabletbediener.

## Zielbild & Erfolgskriterien

Nach Umsetzung kann eine Turnierleitung ohne technischen Hintergrund:

- global sagen, ob **mit oder ohne Schiedsrichter** gespielt wird — ohne
  Schiedsrichter erscheint nirgends ein SR/AR-Bedienelement oder eine
  SR/AR-Info (wie heute);
- die **BTP-Schiedsrichterliste** in BTS Light sehen (Client + TL-Web);
- jedem Spiel **SR und AR zuweisen** — optional, jederzeit änderbar, auch
  bei laufendem Spiel, aus Client **und** TL-Web;
- sich auf **Konflikt-Warnungen** verlassen: schiedst ein SR ein Spiel mit
  Beteiligung seines Vereins, eines zusätzlich gesperrten Vereins oder
  eines gesperrten Spielers, warnt BTS Light — es blockiert nie;
- die **automatische Rotation** nutzen (getrennt für SR und AR
  aktivierbar): freie Felder werden aus der Reihenfolge bestückt,
  Konflikt-SR werden übersprungen, die Reihenfolge ist jederzeit manuell
  veränderbar, SR können **pausiert** werden (Pause, kommt später, geht
  früher);
- je Feld sagen, ob SR-Rotation, AR-Rotation und **Tabletbediener-Vergabe**
  dort aktiv sind (damit sind alle drei Betriebsformen abbildbar);
- SR und Tabletbediener **ansagen lassen** (Feld-Ansage nennt beide; für
  nachträgliche Zuweisungen gibt es einen manuellen Ansage-Knopf);
- die **Einsätze je Official nachvollziehen**: TL-Web zeigt je Official die
  Zahl der bisherigen Einsätze; ein Overlay listet sie im Detail (Spiel,
  Rolle SR/AR, Feld, Uhrzeit).

Erfolgskriterium am nächsten Turnier mit Schiedsrichtern: Die Einteilung
läuft komplett über BTS Light (keine Papierliste), und kein SR wird von der
Auto-Rotation einem Spiel mit eigenem Vereins-/Personen-Konflikt zugeteilt.

## Nicht-Ziele

- **Keine Anzeige** von SR/AR auf Court-Monitor, Übersichts-Monitor oder
  badhub-Liveticker.
- **Kein eigenständiges Pflegen der SR-Stammliste** in BTS Light — Quelle
  ist BTP (R2). BTS Light pflegt nur Zusatzdaten (Sperrlisten, Pausen,
  Reihenfolge, ggf. Vereins-Override).
- **Keine Erkennung** „SR ist gerade selbst als Spieler aufgerufen" —
  Officials und Players sind in BTP getrennte Container ohne belegte
  Verknüpfung (bekannte Grenze, wird dokumentiert).
- Kein BTP-Auscheck von Tabletbedienern (bleibt Phase 2 des
  Zähltafelbediener-Features, ADR 0007).

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - `src-tauri/src/btp/model.rs` — `BtpOfficial` + Parser, `BtpMatch.official1_id/official2_id`.
  - `src-tauri/src/btp/proto.rs` — eigenständiger `officials_request` (Muster `highlight_request`) + `court_assign_request` um `Official1ID`/`Official2ID` erweitern.
  - `src-tauri/src/tablet/state.rs` — Official-Roster (Rotation, Pausen, Sperrlisten, Zuweisungen).
  - `src-tauri/src/sync.rs` — Master-Hook `track_officials`; Feld-Filter der Bediener-Vergabe.
  - `src-tauri/src/config.rs` — `OfficialsConfig { enabled, rotation_sr, rotation_ar }`.
  - `src-tauri/src/commands.rs`, `src/api.ts`, `src/types.ts` — `official_*`-Commands + Spiegel.
  - `relay-proto/` — `MatchBrief`-Felder, `TlState`/`TlCourt`-Felder, neue `TlAction`-Varianten.
  - `relay/` — Neucompile gegen relay-proto; **Deploy vor Client-Release**.
  - `src-tauri/src/tablet/tl.rs`, `assets/tl.html` — TL-State, Action-Arme, Bedien-UI.
  - `assets/tablet.html` — SR/AR-Anzeige am Spielzettel.
  - `src/io/announcer.ts`, `src/io/announceCourt.ts`, `src/components/MatchAnnouncer.tsx` — Ansage-Segmente.
  - `src/pages/` (SetupWizard, FieldOverviewPage bzw. eigener Abschnitt) — Schalter + Bedienung.
- **Architekturregeln:**
  - **R1**: Alle Bedienung läuft über Tauri-Commands (Client) bzw. `TlAction` (TL-Web).
  - **R2**: BTP ist die Wahrheit. SR-Liste kommt aus `SENDTOURNAMENTINFO`.
    **Konfliktregel (ADR 0021, Entscheidung 3):** Trägt der Snapshot am
    Match `Official1ID`/`Official2ID`, gilt dieser Wert. Eine **frisch
    geschriebene** Zuweisung wird jedoch bis zur Bestätigung im Snapshot
    lokal weiter angezeigt — die Messung 13.08.2026 hat gezeigt, dass BTP
    den Write **asynchron** übernimmt (≤ 1 s, sofortiges Zurücklesen zeigt
    noch den alten Stand). Ohne dieses Halten würde jede Zuweisung im
    Moment nach dem Klick auf den alten Wert zurückspringen. Lokale
    Zuweisungen sind also ein Overlay für das Bestätigungsfenster und für
    den Fehlerfall des Writes — nicht nur für Spiele, an denen BTP nichts
    stehen hat.
  - **R3**: LAN und Cloud gleichwertig — Tablet-Anzeige über `MatchBrief`
    (beide Wege), TL-Web-Aktionen über den bestehenden Action-Kanal, ferne
    Halle (Cloud-Slave) inklusive Ansage von Anfang an.
  - **R5**: unberührt — Feature nimmt keine Ergebnisse entgegen.
- **Konfiguration & Abwärtskompatibilität:**
  - Neu: `AppConfig.officials` (`enabled: false`, `rotation_sr: false`,
    `rotation_ar: false`) mit `#[serde(default)]` — ältere config.json
    bleibt lesbar, Bestandsinstallationen verhalten sich unverändert.
  - Turnier-gebundene Laufzeitdaten (feldweise Schalter, Reihenfolge,
    Pausen, Sperrlisten, Vereins-Overrides, Zuweisungen) liegen **nicht** in
    config.json, sondern in einer eigenen Datei im App-Datenverzeichnis,
    am Turnier geschlüsselt (ADR 0022; Muster `live-scores.json` /
    ADR 0015). Feldweise Bediener-Vergabe: Default **alle Felder aktiv** —
    Verhalten bestehender Installationen ändert sich nicht.
  - `identifier` und Updater-Pfad bleiben unangetastet.
- **Datenschutz:**
  - Sperrlisten (Sperr-Vereine, Sperr-Spieler) kodieren persönliche
    Beziehungen: **turnier-gebunden** (bei Turnierwechsel verworfen),
    **nicht im laufend gepollten TL-State** (der trägt an Spielen nur
    Warn-Flag + Kategorie „Verein"/„Person" — Wächter-Test), **nicht im
    Identitäts-Export** (liegen außerhalb der `AppConfig`). Die **Pflege
    ist auch aus TL-Web möglich**: Der Pflege-Dialog lädt die Sperrlisten
    eines Officials **gezielt auf Anfrage** über den authentifizierten
    TL-Kanal (Geräte-Token, TLS; Lese-Route nach dem Muster der
    Punktverlauf-Route, Schreiben über `TlAction`) — sie liegen damit nie
    im Broadcast-Zustand aller Geräte, sondern nur in der aktiven
    Pflege-Ansicht.
  - SR-Namen erscheinen in Client, TL-Web und am Tablet — zweckgebunden wie
    Spielernamen; der Wächter-Test
    `the_state_never_carries_personal_data_beyond_its_purpose` wird um
    Officials erweitert (Freigabe-Begründung im Test, wie bei
    Nation/Verein). Kein Geburtsjahr, keine Lizenznummer.
- **Abhängigkeiten:**
  - **BTP-Messung — erledigt 13.08.2026** (`btp_officials_probe.rs`,
    Test-BTP mit drei Officials): `Official{ID, Name, FirstName, Country}`,
    **kein Verein** auf dem Draht ⇒ Stammverein wird in BTS Light gepflegt.
    `Official1ID` = SR, `Official2ID` = AR (an der BTP-Maske verifiziert).
    Official-Writes werden **angenommen** — eigenständig wie eingebettet,
    Löschen per `0`, Übernahme asynchron ≤1 s (Zurücklesen mit Poll).
    Details: [btp_protocol.md](../btp_protocol.md), ADR 0021.
  - **Relay-Deploy auf badhub.de vor dem Client-Release** (neue
    `TlAction`-Varianten; Deploy macht ein Kollege).
  - Keine neue Cargo-/npm-Dependency.

## Fachliches Verhalten (verbindlich)

1. **Globaler Schalter** `officials.enabled` („mit Schiedsrichtern
   spielen"): aus ⇒ keine SR/AR-Elemente in Client, TL-Web, Tablet,
   Ansagen; ein Abschalten mitten im Turnier räumt alle Zuweisungen
   (analog `clear_scorekeeper_assignments`).
2. **Zuweisung je Spiel:** SR und AR einzeln, optional, jederzeit setz-,
   änder- und entfernbar (auch bei laufendem Spiel), aus Client und
   TL-Web. Manuelle Zuweisung mit Konflikt ⇒ Zuweisung wird ausgeführt
   **und** Warnung (Kategorie) angezeigt.
3. **Konflikt-Begriff:** Ein Official hat einen Konflikt mit einem Spiel,
   wenn (a) sein **in BTS Light gepflegter Stammverein** (BTP überträgt
   keinen Verein am Official — Messung 13.08.2026) mit
   dem Verein eines beteiligten Spielers übereinstimmt, (b) ein an ihm
   gepflegter Sperr-Verein beteiligt ist oder (c) ein an ihm gepflegter
   Sperr-Spieler beteiligt ist. Kategorien: „Verein", „Person".
   Sperr-Vereine und Sperr-Spieler werden je Official gepflegt — aus dem
   Client **und** aus TL-Web (dort lädt der Pflege-Dialog die Listen
   gezielt auf Anfrage, siehe Datenschutz).
4. **Rotation:** Getrennt aktivierbar für SR (`rotation_sr`) und AR
   (`rotation_ar`). Gemeinsamer Pool (die BTP-Officials-Liste); dieselbe
   Person hat nie gleichzeitig zwei Dienste. Wird ein Feld mit aktiver
   Rotation neu belegt und hat weder BTP noch die lokale Zuweisung einen
   SR (bzw. AR), wird der nächste nicht pausierte, dienstfreie,
   konfliktfreie Official der Reihenfolge zugewiesen; Konflikte werden
   **übersprungen** (ohne Warnung — die Warnung gilt nur manueller
   Zuweisung). Nach Spielende rückt der Official ans Ende der Reihenfolge.
   Ist niemand verfügbar, bleibt das Feld ohne SR/AR.
5. **Reihenfolge & Pausen:** Die Rotationsreihenfolge ist jederzeit manuell
   veränderbar (vorziehen/verschieben). Ein Official kann pausiert und
   wieder aktiviert werden; Pausierte werden von der Rotation ignoriert,
   behalten aber ihre Position.
6. **Feldweise Schalter** (drei, unabhängig, je CourtID): SR-Rotation,
   AR-Rotation, Tabletbediener-Vergabe. Felder ohne aktive
   Bediener-Vergabe verbrauchen keinen Eintrag aus der
   Zähltafelbediener-Warteschlange. Die drei Betriebsformen ergeben sich
   aus den Kombinationen; eine Unterdrückungslogik gibt es nicht.
7. **Anzeige:** Client-Spielübersicht und TL-Web zeigen je Feld SR/AR samt
   Warn-Flag; das Schiri-Tablet zeigt SR/AR zum laufenden Spiel (LAN und
   Cloud, auch ferne Halle).
8. **Ansagen:** Die Feld-Ansage nennt nach der Tabletbedienung den
   Schiedsrichter („Schiedsrichter: {Name}.", EN „Umpire: …") und — falls
   zugewiesen — den Aufschlagrichter („Aufschlagrichter: {Name}.", EN
   „Service judge: …"). Nachträgliche Zuweisungen lösen **keine**
   automatische Ansage aus; Client und TL-Web haben einen manuellen Knopf
   „SR/AR ansagen".
9. **Rücksync nach BTP — jede Änderung, sofort** (ADR 0021, Messung
   13.08.2026: beide Schreibformen nachweislich angenommen): Jede
   SR/AR-Zuweisungsänderung geht per eigenständigem Match-Update
   (`ID, DrawID, PlanningID, Official1ID, Official2ID`, Löschen = `0`,
   **ohne** `Status`-Feld — Check-in-Bits-Falle v0.9.103) nach BTP;
   beim Ruf aufs Feld wandern die Officials zusätzlich mit ins bestehende
   Zuweisungs-Update. Reconcile-Muster wie die Highlights (Diff, Retry,
   Stand nur bei `Ok` übernehmen). BTP übernimmt asynchron (≤1 s) — die
   Anzeige hält den geschriebenen Wert, bis der Snapshot ihn bestätigt.
10. **Persistenz:** Reihenfolge, Pausen, Sperrlisten, Overrides, feldweise
    Schalter, lokale Zuweisungen und die Einsatz-Historie überleben
    App-Neustart/Absturz; Turnierwechsel verwirft sie.
11. **Einsatz-Nachvollziehbarkeit:** Es gibt **keine eigene
    Historien-Datenhaltung** — die Einsätze eines Officials werden aus den
    **beendeten Spielen** abgeleitet: alle Spiele, an denen er beim
    Spielende als SR oder AR stand (BTP-Wert oder lokale Zuweisung), mit
    Rolle, Feld, Spiel und Endezeit (`finished_at`). Dafür gilt: Lokale
    Zuweisungen werden beim Spielende **nicht verworfen**, sondern bleiben
    dem beendeten Spiel zugeordnet (turniergebunden persistiert wie die
    übrigen Zuweisungen). TL-Web zeigt je Official den Einsatz-Zähler in
    der Roster-Liste; das Detail-Overlay lädt die Einsatzliste **gezielt
    auf Anfrage** (gleicher authentifizierter Leseweg wie die
    Sperrlisten). Der Client zeigt denselben Zähler/dieselbe Liste über
    Commands.

## Akzeptanzkriterien

Positiv:

- [ ] Bei `officials.enabled = false` (Default) sind Client, TL-Web,
  Tablet und Ansagen frei von SR/AR-Elementen; bestehende Installationen
  verhalten sich nach dem Auto-Update unverändert (Config-Roundtrip-Test).
- [ ] Die BTP-Officials-Liste erscheint nach dem Sync in Client und
  TL-Web; ein BTP-Snapshot **ohne** `Officials`-Container ergibt eine leere
  Liste, keinen Fehler.
- [ ] Eine manuelle SR-Zuweisung auf ein laufendes Spiel ist aus Client und
  TL-Web möglich und sofort auf allen Oberflächen (inkl. Tablet, LAN und
  Cloud) sichtbar.
- [ ] Trägt das BTP-Match `Official1ID`, zeigt BTS Light diesen Official an,
  auch wenn lokal ein anderer zugewiesen war (BTP gewinnt).
- [ ] Manuelle Zuweisung eines SR mit Vereins-Konflikt wird ausgeführt und
  zeigt die Warnung „Verein"; mit Sperr-Spieler-Konflikt die Warnung
  „Person". Die Sperrlisten-Inhalte selbst erscheinen in keinem
  TL-State-Frame (Wächter-Test).
- [ ] Sperrlisten sind aus Client und TL-Web pflegbar; der TL-Web-Abruf
  liefert die Listen eines Officials nur auf gezielte, per Geräte-Token
  authentifizierte Anfrage — nie im Broadcast-Zustand.
- [ ] Nach zwei beendeten Spielen eines Officials (einmal SR, einmal AR)
  zeigt die TL-Web-Roster-Liste den Zähler „2"; das Overlay listet beide
  Einsätze mit Spiel, Rolle, Feld und Endezeit — abgeleitet aus den
  beendeten Spielen, ohne eigene Historien-Datenhaltung. Eine vor
  Spielbeginn entfernte Zuweisung erscheint nicht; die Zählung stimmt
  nach App-Neustart weiterhin und ist nach Turnierwechsel leer.
- [ ] Auto-Rotation bestückt ein neu belegtes Feld mit aktivem SR-Schalter
  mit dem nächsten nicht pausierten, dienstfreien, konfliktfreien SR;
  pausierte, im Dienst befindliche und Konflikt-SR werden übersprungen;
  nach Spielende rückt der SR ans Ende.
- [ ] AR-Rotation funktioniert identisch, aus demselben Pool, und weist nie
  eine Person zu, die gerade SR oder AR auf einem anderen Feld ist.
- [ ] Ein Feld mit deaktivierter Tabletbediener-Vergabe verbraucht keinen
  Eintrag aus der Zähltafelbediener-Warteschlange; Default ist „alle Felder
  aktiv" (Bestandsverhalten).
- [ ] Die Feld-Ansage nennt zugewiesenen SR (und AR, falls gesetzt) nach der
  Tabletbedienung; der manuelle Ansage-Knopf sagt SR/AR eines Feldes an;
  eine nachträgliche Zuweisung allein löst keine Ansage aus.
- [ ] Jede SR/AR-Zuweisungsänderung in BTS Light erscheint in BTP
  (eigenständiges Match-Update, Löschen = `0`); beim Ruf aufs Feld
  enthält das Zuweisungs-Update die Official-IDs zusätzlich
  (Request-Aufbau-Tests: additiv, ohne `Status`-Feld). Ein
  fehlgeschlagener Write wird im nächsten Sync-Zyklus wiederholt
  (Reconcile-Diff-Test).
- [ ] Rotationsreihenfolge, Pausen, Sperrlisten und feldweise Schalter sind
  nach App-Neustart unverändert; nach Turnierwechsel sind sie verworfen.

Fehlerfälle:

- [ ] Verschwindet ein zugewiesener SR aus der BTP-Officials-Liste, bleibt
  die App stabil; seine Anzeige am Spiel erlischt, seine Zusatzdaten
  bleiben inert (kehrt er zurück, gelten sie wieder).
- [ ] Ein leerer/verdächtiger Snapshot (Schutz `empty_snapshot_is_suspect`)
  verwirft weder Roster-Zusatzdaten noch Zuweisungen.
- [ ] Abschalten von `officials.enabled` mitten im Turnier räumt alle
  SR/AR-Zuweisungen und blendet alle Elemente aus; kein veralteter Name
  bleibt in einer Anzeige hängen.
- [ ] Nach Tablet-Reconnect (LAN und Cloud) zeigt das Tablet den aktuellen
  SR/AR-Stand, keinen veralteten.
- [ ] Schlägt der BTP-Write beim Ruf aufs Feld fehl, verhält sich der
  bestehende Feldvergabe-Pfad wie heute (Fehler sichtbar, kein
  Inkonsistenz-Zustand im Roster).

## Tests

TDD-Pflicht; `cargo test` (Workspace, Clippy `--workspace --all-targets`)
grün, `npm run build` fehlerfrei. Rust-Unit-Tests mindestens:

- **Parser:** `official_map` (Felder, Club-Auflösung, fehlender Container ⇒
  leere Liste), `BtpMatch.official1_id/2` (`filter(>0)`-Konvention).
- **Rotation/Pausen:** alle Zweige aus Verhalten Nr. 4/5 (überspringen von
  pausiert/Dienst/Konflikt, Ende-der-Reihenfolge nach Spielende,
  Idempotenz je (Feld, Match), `retain`/`clear`-Aufräumen, Determinismus
  nach CourtID).
- **Konflikt-Erkennung:** Verein (BTP-Club + Override), Sperr-Verein,
  Sperr-Spieler; Kategorien-Zuordnung.
- **Persistenz:** Roundtrip der Turnierdatei, Turnierwechsel verwirft,
  Sperrlisten liegen außerhalb der `AppConfig` (Identitäts-Export-Test).
- **Einsatz-Ableitung:** beendete Spiele mit SR/AR (BTP-Wert und lokale
  Zuweisung) ergeben Zähler und Liste; lokale Zuweisung bleibt nach
  Spielende dem Match zugeordnet (Persistenz-Roundtrip); Entfernen vor
  Spielbeginn ⇒ kein Einsatz; Zähler konsistent zur Liste.
- **Config:** Serde-Roundtrip + „fehlt in alter Datei ⇒ Default aus".
- **Wire:** Serde-Roundtrips der neuen relay-proto-Typen;
  `every_tl_action` um alle neuen Varianten erweitert; Ablehnung
  unbekannter Aktionen bleibt.
- **TL-State:** Allowlist-Wächter (`ERLAUBT`) + Datenschutz-Wächter mit
  Officials-Fixture (nicht-leer-Assert, Verbotsliste um Sperrlisten-Marker
  ergänzt, Gegenprobe SR-Name mit Freigabe-Begründung).
- **Rücksync:** Request-Aufbau-Tests für `officials_request` und den
  erweiterten `court_assign_request` (Official-Felder vorhanden, Löschen
  = `0`, kein `Status`); Reconcile-Diff (nur Änderungen schreiben, bei
  `Err` im nächsten Zyklus erneut).
- **Ansagen:** Segment-/SSML-Bauer nennen SR/AR nur bei Zuweisung
  (Frontend-seitig über bestehende announcer-Testmuster, sonst manueller
  Abgleich Segments ↔ SSML).
- **Messwerkzeuge** (`#[ignore]`, echtes BTP): `btp_officials_probe`
  (Struktur, ClubID, Official1/2-Semantik) und Officials-Schreib-Experiment
  mit Zurücklesen.

Manueller Turnier-Testfall: Zwei-Felder-Testturnier mit drei Officials —
Rotation, Pause, manueller Konflikt, Ansage, Tablet-Anzeige, App-Neustart.

## Risiken & Rollback

- **Andere BTP-Version verhält sich beim Official-Write anders** (gemessen
  wurde eine Version an einem Turnier): Das Reconcile-Muster liest ohnehin
  über den Snapshot zurück; verwirft ein BTP die Werte doch, bleibt der
  Overlay-Betrieb vollwertig (Anzeige, Rotation, Ansagen ohne Rücksync).
  Die Probe (`btp_officials_probe.rs`) bleibt zur Gegenprüfung im Repo.
- **Vereinspflege ist Handarbeit:** BTP liefert keinen Verein am Official
  (gemessen) — die Vereins-Konflikt-Warnung (a) greift nur, wenn die
  Turnierleitung den Stammverein in BTS Light gepflegt hat.
- **Relay-Deploy-Reihenfolge:** Alte Relays lehnen unbekannte
  `TlAction`-Varianten ab — TL-Web-Officials-Bedienung im Cloud-Modus
  funktioniert erst nach dem Relay-Deploy (Kollege); LAN ist nicht
  betroffen. In Release-Notes vermerken.
- **Rollback:** Ältere App-Version bleibt installierbar; `OfficialsConfig`
  ist `#[serde(default)]`-tolerant in beide Richtungen (unbekannte Felder
  werden von älteren Versionen ignoriert). Die Turnierdatei wird von
  älteren Versionen schlicht nicht gelesen. Keine Änderung an
  `identifier`/Updater-Pfad.
- **Laufendes Turnier:** Alle neuen Pfade sind opt-in (`enabled`-Default
  aus); der einzige Berührpunkt mit Bestandsverhalten ist der Feld-Filter
  der Bediener-Vergabe — Default „alle aktiv" hält das Verhalten identisch.

## Offene Fragen / Annahmen

- **Messung erledigt (13.08.2026, Testturnier mit drei Officials):**
  (a) `Official`-Felder: `ID`, `Name`, `FirstName`, `Country` — **kein
  Verein** (auch nach Pflege in BTP kam nur `Country` hinzu) ⇒ Stammverein
  wird in BTS Light gepflegt; (b) `Official1ID` = Schiedsrichter,
  `Official2ID` = Aufschlagrichter — bestätigt; (c) Official-Writes werden
  angenommen (eigenständig und eingebettet), Übernahme asynchron ≤1 s.
- **Annahme:** Die BTP-Officials-Liste ändert sich im Turnierverlauf selten;
  ein Sync-Zyklus (Bestandsmechanik) reicht als Aktualisierungsweg.
- **Bekannte Grenze:** Ein SR, der selbst Spieler ist, wird von der
  Rotation nicht als „gerade aufgerufen" erkannt (getrennte
  BTP-Container).

## Betroffene Doku-Dateien

Im selben Commit wie der jeweilige Code:

- **Neu:** `docs/schiedsrichter-management.md` (Feature-Doku) + neue Zeile
  in der CLAUDE.md-Tabelle.
- `docs/btp_protocol.md` — Official-Felder + Messergebnisse.
- `docs/zaehltafelbediener.md` — feldweise Vergabe-Schalter.
- `docs/announcements.md` — SR/AR-Segmente + manueller Knopf.
- `docs/turnierleitung-web.md` (+ `docs/features/turnierleitung-web.md`) —
  Officials-Bedienung in TL-Web.
- `docs/cloud-relay.md` — neue Wire-Typen/Aktionen.
- `docs/multi-hall.md` — Querverweis-Liste ergänzen.
- `docs/adr/` — ADR 0021 (Rücksync-Modell), ADR 0022 (Ablage Turnierdaten); beide angelegt 13.08.2026.
- `docs/changelog.md` — je veröffentlichter Version.
- `docs/roadmap.md` — Verweis auf diese Spec.

## Umsetzungs-Hinweise

Details und Code-Anker: How-To unter
`docs/features/_intake/schiedsrichter-management/3-how-to.md`
(gitignoriert). Reihenfolge der kleinen, je für sich grünen Schritte:

1. ~~Messung am echten BTP~~ **erledigt 13.08.2026**
   (`tests/btp_officials_probe.rs` bleibt als Messwerkzeug im Repo;
   Ergebnisse in `btp_protocol.md`, ADR 0021/0022 angelegt).
2. Parser (`BtpOfficial`, `official_map` nach `player_map`-Muster,
   Match-Felder nach `location_id`-Muster).
3. `OfficialsConfig` + `types.ts` + SetupWizard-Schalter.
4. Roster-State + turniergebundene Persistenzdatei (ADR 0022). Zuweisungen
   bleiben nach Spielende dem Match zugeordnet (Basis der
   Einsatz-Ableitung — keine eigene Historien-Datenhaltung).
5. Rotation + Konflikt-Erkennung + Sync-Hook `track_officials`
   (nach `track_scorekeepers`-Muster, Master-only).
6. Feldweise Schalter inkl. Bediener-Vergabe-Filter in
   `assign_scorekeeper_for_court`.
7. Commands + Client-UI (inkl. Sperrlisten-Pflege).
8. Wire + TL-Web (`MatchBrief`, `TlState`/`TlCourt`, `TlAction`-Varianten
   `OfficialAssign`/`OfficialClear`/`OfficialPause`/`OfficialReorder`/
   `OfficialsCourtToggle`/`AnnounceOfficials`/`OfficialBlocklistSet`;
   Einsatz-Zähler je Official im TL-State; Leseroute für Sperrlisten
   **und** Einsatz-Historie nach dem Muster der `tl_timeline`-Routen in
   `server.rs` + `relay/`, Overlay in tl.html; `every_tl_action`,
   `touches_courts`, `action_fingerprint`, `action_label`, beide
   Wächter-Tests; tl.html). **Relay-Deploy vor Client-Release.**
9. Tablet-Anzeige (tablet.html, `match_brief()`), ferne Halle über
   `AnnounceCourt`/`MatchAssigned`.
10. Ansagen (Segments + SSML synchron, Aufrufer, manueller Knopf).
11. Rücksync (ADR 0021): eigenständiger `officials_request` (Muster
    `highlight_request`) + Reconcile im Sync-Loop (Muster
    `reconcile_highlights`); zusätzlich `court_assign_request` additiv
    erweitern (Verdrahtung an allen drei Einstiegen über die gemeinsame
    `MatchCourt`-Struct).
12. Doku + Version dreifach bumpen (`src-tauri/Cargo.toml`,
    `src-tauri/tauri.conf.json`, `package.json`).

Reviews: `code-reviewer` nach jeder Code-Änderung (Pflicht);
`security-reviewer` bei Schritt 7/8 (neuer User-Input über TL-Actions,
Sperrlisten = Personendaten, Cloud-Schreibpfad).
