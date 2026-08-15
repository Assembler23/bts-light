# Spielliste per Drag&Drop manuell sortierbar — Spezifikation

> Status: **abgestimmt 2026-08-14** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Idee der Turnierleitung, 14.08.2026.
> Betroffene Crates: src-tauri, relay-proto, src.
> ADR: [0023-manuelle-spielreihenfolge-praefix-je-halle.md](adr/0023-manuelle-spielreihenfolge-praefix-je-halle.md)
> (Präfix-Grundlage: atomare Züge, ein gemeinsamer Sortier-Helfer — weiter
> gültig).
> **Nachtrag 15.08.2026:** Der Präfix ist seither **hallenübergreifend**
> statt je Halle getrennt (der weiter unten beschriebene „je Halle"-Teil
> ist damit abgelöst) — siehe
> [0026-spielliste-eine-globale-reihenfolge-eine-liste.md](adr/0026-spielliste-eine-globale-reihenfolge-eine-liste.md)
> und den Nachtrag in [features/tl-web-panelsystem.md](tl-web-panelsystem.md).

## Kontext / Problem

Die Turnierleitung sieht die spielbereiten, noch nicht zugewiesenen
Spiele aktuell strikt in BTPs eigener Reihenfolge
(`PlannedTime → DisplayOrder → MatchNr → ID`). Es gibt keine Möglichkeit,
ein Spiel kurzfristig manuell nach vorn zu ziehen — etwa weil ein Feld
frei wird und ein bestimmtes Spiel jetzt bevorzugt drankommen soll, ohne
dafür extra die Feldvergabe-Ausnahme oder eine manuelle Feldzuweisung zu
nutzen. Das betrifft sowohl die Anzeige (TL-Web, Desktop, Tablet/Monitor,
Liveticker) als auch die automatische Feldvergabe selbst.

BTPs Sortierreihenfolge ist laut `docs/btp_protocol.md` „die eine
Definition, die überall gilt" und wird an **fünf** Stellen im Code
dupliziert genutzt — eine manuelle Überschreibung muss an allen fünf
konsistent ankommen, sonst zeigt jede Ansicht eine andere „nächste
Begegnung".

## Zielbild & Erfolgskriterien

Die Turnierleitung kann ein spielbereites, noch nicht gerufenes Spiel
per Drag&Drop (Maus oder Touch) nach vorn ziehen. Ab diesem Moment gilt
für dieses Spiel und alle bereits zuvor gezogenen Spiele derselben Halle
eine manuelle Reihenfolge — alle noch nicht angefassten Spiele folgen
unverändert weiter BTPs eigener Reihenfolge dahinter. Diese Reihenfolge
wirkt identisch in allen Anzeigen und bei der automatischen Feldvergabe.
Ein globaler Knopf setzt die komplette manuelle Sortierung aller Hallen
auf einmal zurück. Erfolg heißt: kein Setup-Schritt, keine Erklärung
nötig — Ziehen fühlt sich an wie bei der bereits vorhandenen
Schiedsrichter-Rotation.

## Nicht-Ziele

- Kein Rückschreiben der Reihenfolge nach BTP — durch Messung
  (14.08.2026, `btp_displayorder_probe.rs`) belegt: `Match.DisplayOrder`
  wird per `SENDUPDATE` still ignoriert (`Result=1`, Wert bleibt
  unverändert), exakt wie der bereits dokumentierte `LocationID`-Befund.
  Die manuelle Reihenfolge bleibt daher grundsätzlich rein lokal in
  bts-light.
- Kein Reset je einzelne Halle — der Reset-Knopf ist bewusst global und
  kennt in der Wire-Form keinen Hallen-Parameter.
- Kein Vorrang vor bereits „in Vorbereitung gerufenen" Spielen — die
  bestehende Aufruf-Reihenfolge bleibt unverändert an erster Stelle.
- Keine Änderung der Feldvergabe-Ausnahme-Logik — beide Features bleiben
  orthogonal (siehe „Betroffene Komponenten").

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - `src-tauri/src/tablet/queue_order.rs` (neu) — `QueueOrderStore`.
  - `src-tauri/src/tablet/assign.rs` — neue Funktionen
    `sort_key_with_manual_order`, `resolve_and_sort_key`.
  - `src-tauri/src/sync.rs` — `auto_assign` umgestellt,
    `reconcile_queue_order` (neu).
  - `src-tauri/src/tablet/state.rs`, `src-tauri/src/commands.rs` —
    Store-Anbindung, Tauri-Commands `queue_reorder`, `queue_order_reset`.
  - `src-tauri/src/tablet/tl.rs` — `TlAction::QueueReorder`/
    `QueueOrderReset`-Dispatch, `build_state` umgestellt, `TlMatch`
    bekommt `manual: bool`.
  - `src-tauri/src/tablet/server.rs` — `info_preparation_state`
    umgestellt, `hall`-Feld ergänzt.
  - `src-tauri/src/badhub/payload.rs`, `src-tauri/src/badhub/diff.rs` —
    `build_tset`/`upcoming` bekommen einen Kontext-Parameter.
  - `relay-proto/src/lib.rs` — `TlAction::QueueReorder { match_id,
    before_match_id }`, `TlAction::QueueOrderReset`.
  - `src/pages/PreparationPanel.tsx` — Hallen-Gruppierung (neu),
    `useDragReorder`-Einbindung, Reset-Knopf.
  - `src/api.ts`, `src/types.ts` — `queueReorder`, `queueOrderReset`,
    `hall`/`manual`-Felder.
  - `assets/tl.html` — `enableReorderDrag` je Hallen-Abschnitt der
    Warteliste, Reset-Knopf, Markierung manuell einsortierter Spiele.
- **Architekturregeln (CLAUDE.md R1–R6):**
  - R1: Frontend löst ausschließlich über die neuen Tauri-Commands/
    `TlAction`-Varianten aus, kein direkter Store-Zugriff.
  - R2: BTP bleibt die Wahrheit für Matches/Courts — der Präfix
    überschreibt nur die *Sortierung*, keine Court-Zuordnung, und
    erfindet keine Halle (die Halle wird serverseitig aus
    `hall_for_match` desselben Matches abgeleitet, das Frontend
    übergibt nur zwei Match-IDs relativ zueinander).
  - R3: Beide Verbindungsarten bedacht — die Wire-Form läuft über den
    bestehenden `TlAction`-Kanal, der bereits LAN und Cloud-Relay
    gleichermaßen bedient.
  - R4/R6: unberührt (kein neuer Court- oder Namespace-Bezug).
  - R5: unberührt (keine neue Ergebnis-Validierung nötig).
- **Konfiguration & Abwärtskompatibilität:** Neue Datei
  `queue-order.json` im App-Datenverzeichnis, turniergebunden (Muster
  ADR 0022). Fehlt die Datei (ältere Version, frische Installation,
  neues Turnier), startet jede Halle mit leerem Präfix — Verhalten
  identisch zu heute. Keine neuen Felder in `config.json`. `identifier`
  und Updater-Pfad bleiben unangetastet.
- **Datenschutz:** Der Präfix speichert ausschließlich Match-IDs, keine
  Spieler- oder Personendaten. Kein Geburtsjahr, keine Lizenznummer
  betroffen.
- **Abhängigkeiten:** Keine neue Cargo-/npm-Dependency. BTP-seitig
  bereits gemessen (siehe Nicht-Ziele) — kein weiterer BTP-Protokoll-
  Bedarf.

## Akzeptanzkriterien

- [ ] Ein Ziehen eines noch nicht gerufenen, spielbereiten Spiels nach
      vorn (TL-Web oder Desktop) versetzt es an die gezogene Position;
      alle zuvor bereits gezogenen Spiele derselben Halle behalten ihre
      relative Reihenfolge; alle nie gezogenen Spiele folgen dahinter
      weiter in BTPs eigener Reihenfolge.
- [ ] Dieselbe resultierende Reihenfolge erscheint identisch in: TL-Web-
      Warteliste, Desktop-Vorbereitungs-Kandidaten, Tablet/Monitor-
      Vorbereitungs-Kandidaten und Liveticker „anstehende Spiele" —
      abgesichert durch den Cross-Site-Regressionstest.
- [ ] Die automatische Feldvergabe weist ein frei werdendes Feld
      bevorzugt dem am weitesten vorn stehenden, nicht ausgenommenen
      Spiel des Präfix zu (vor allen nicht manuell einsortierten
      Spielen derselben Halle).
- [ ] Ein bereits „in Vorbereitung gerufenes" Spiel steht immer vor
      jedem Präfix-Eintrag, unabhängig von dessen Position.
- [ ] Der Präfix ist je Halle getrennt: Umsortieren in Halle A verändert
      die Reihenfolge in Halle B nicht.
- [ ] Ändert sich die abgeleitete Halle eines im Präfix stehenden
      Spiels (z. B. durch `SetHall` oder einen neuen
      Vorbereitungs-Aufruf in anderer Halle), verliert es seinen
      Präfix-Platz in der alten Halle automatisch und ordnet sich in
      der neuen Halle normal (ohne manuellen Vorrang) ein.
- [ ] Ein Spiel verlässt den Präfix automatisch, sobald es einem Feld
      zugewiesen wird (OnCourt), beendet wird oder aus dem BTP-Snapshot
      verschwindet.
- [ ] Ein manuell in den Präfix eingereihtes Spiel bleibt sichtbar
      markiert (Badge/Symbol), solange es im Präfix steht.
- [ ] Der globale Reset-Knopf (TL-Web und Desktop) verwirft die
      manuelle Reihenfolge aller Hallen auf einmal; danach folgt jede
      Anzeige wieder ausschließlich BTPs Reihenfolge. Es gibt keine
      Bedienmöglichkeit, nur eine einzelne Halle zurückzusetzen.
- [ ] Ein Zug bleibt über einen App-Neustart/Turnier-PC-Neustart hinweg
      erhalten (turniergebundene Persistenz).
- [ ] Ein Turnierwechsel verwirft den gesamten Präfix.
- [ ] Ein von der automatischen Feldvergabe ausgenommenes Spiel
      (Feldvergabe-Ausnahme) bleibt weiterhin ausgenommen, unabhängig
      von seiner Präfix-Position — es wird angezeigt, aber nie
      automatisch zugewiesen.
- [ ] Ein Zug wird als atomare Operation übertragen; zwei nahezu
      gleichzeitige Züge von TL-Web und Desktop auf verschiedene Spiele
      derselben Halle verlieren keinen der beiden.
- [ ] Ein Zug, dessen Zielspiel zwischenzeitlich nicht mehr in derselben
      Halle steht wie das gezogene Spiel, wird serverseitig verworfen
      (No-Op) statt eine falsche Zuordnung zu erzeugen.
- [ ] Mehrhallen-Turniere: Desktop zeigt die Vorbereitungs-Kandidaten in
      Hallen-Abschnitten (wie TL-Web), Drag&Drop wirkt je Abschnitt.
- [ ] Ans Ende ziehen (`before = null`) übernimmt nie mehr Spiele in den
      Präfix, als die ziehende Oberfläche zeigen konnte — TL-Web kappt je
      Halle bei `QUEUE_LIMIT_PER_HALL` (120); ein Zug darf keine Spiele
      jenseits dieser Grenze unsichtbar in die manuelle Reihenfolge ziehen
      (Code-Review-Fund 14.08.2026, `TabletState::queue_reorder`).

## Tests

TDD ist Pflicht, Reihenfolge siehe „Umsetzungs-Hinweise":

- `tablet/queue_order.rs`: Roundtrip-Tests für `QueueOrderStore`
  (Reorder-Varianten inkl. „vor sich selbst"/unbekanntes Ziel/ans Ende,
  Persistenz übersteht Neuladen, Turnierwechsel verwirft, unlesbare
  Datei bleibt unangetastet, `retain` je Halle, `reset_all`).
- `tablet/assign.rs`: `sort_key_with_manual_order` — gerufenes Match
  schlägt jeden Präfix-Eintrag; Präfix schlägt BTP-Zeitplan; Match ohne
  Präfix-Eintrag fällt dahinter, behält BTP-Reihenfolge untereinander;
  leerer Präfix ergibt identisches Ergebnis zu `sort_key`
  (Rückwärtskompatibilität).
- **Cross-Site-Regressionstest** (neu, verpflichtend): ruft alle fünf
  produktiven Sortier-Stellen mit denselben Testdaten auf und
  vergleicht die resultierende Match-ID-Reihenfolge je Halle.
- `sync.rs`: `auto_assign` bevorzugt ein manuell vorgezogenes Spiel bei
  einem frei werdenden Feld; `reconcile_queue_order` räumt bei
  Statuswechsel (OnCourt/Finished/verschwunden) und bei Hallenwechsel
  automatisch auf; Zusammenspiel mit `AutoAssignExclusionStore`
  (ausgenommenes Spiel bleibt ausgenommen, auch im Präfix).
- `relay-proto`: Serde-Roundtrip für `TlAction::QueueReorder`/
  `QueueOrderReset`.
- `tablet/tl.rs`: Präfix wirkt nur innerhalb der eigenen Halle; ein Zug
  über Hallengrenzen wird verworfen.
- `commands.rs`/`tablet/server.rs`: `hall`-Feld korrekt aufgelöst,
  Ergebnis deckungsgleich mit `tl.rs` für dieselben Testdaten.
- `badhub/payload.rs`: `upcoming()` zeigt bei aktivem Präfix dieselbe
  Reihenfolge wie die TL-Web-Warteliste.
- `cargo test` grün, `npm run build` fehlerfrei vor jedem Commit.
- Manueller Turnier-Testfall: Zwei-Hallen-Szenario, ein Spiel je Halle
  nach vorn ziehen, prüfen, dass sich beide Hallen unabhängig verhalten
  und die automatische Feldvergabe den Präfix respektiert.

## Risiken & Rollback

- **Divergenz-Risiko der fünf Sortier-Stellen** — durch den
  verpflichtenden gemeinsamen Helfer plus Cross-Site-Regressionstest
  strukturell abgesichert (ADR 0023).
- **`payload.rs`-Signaturänderung** ist der Umbau mit dem größten
  Blast-Radius (rund 15 bestehende Testaufrufe) — höheres Risiko für
  Merge-Konflikte, aber rein mechanisch, kein Verhaltensrisiko im
  laufenden Turnier.
- **Rollback:** Ältere App-Versionen ignorieren `queue-order.json`
  vollständig — ein Downgrade mitten im Turnier verliert nur die
  manuelle Sortierung selbst (Anzeige fällt auf BTPs Reihenfolge
  zurück), keine BTP-Daten sind betroffen.
- **Laufendes Turnier:** Die Änderung ist rein additiv zur bestehenden
  Sortierung (leerer Präfix = unverändertes Verhalten) — ein Turnier,
  das das Feature nie nutzt, bemerkt keinen Unterschied.

## Offene Fragen / Annahmen

- Keine offenen Blocker mehr — alle sechs Grill-Blocker und beide
  Review-Rückfragen (Desktop-Hallen-Gruppierung, visuelle Markierung)
  sind geklärt (siehe `docs/features/_intake/spielliste-manuelle-reihenfolge/2-grill.md`).
- Annahme: Die `queue_limit`-Kappung je Halle in TL-Web (nur die ersten
  N Spiele werden angezeigt) bleibt unverändert bestehen — ein sehr
  langer manueller Präfix kann dadurch normal sortierte Spiele aus der
  sichtbaren Liste drängen. Das ist beabsichtigtes Verhalten (die
  Turnierleitung hat es selbst so sortiert), keine Fehlerquelle.
- Annahme: Ein Zug über Hallengrenzen kann UI-seitig praktisch nicht
  ausgelöst werden (jede Halle ist ein eigener Drag-Container) — die
  serverseitige Prüfung ist ein Sicherheitsnetz gegen veraltete
  Zustände zwischen zwei gleichzeitig geöffneten Geräten, kein
  regulärer Bedienweg.

## Betroffene Doku-Dateien

- `docs/btp_protocol.md` — Tabelle „Diese eine Definition gilt überall"
  um `sort_key_with_manual_order` ergänzen (Zeilen 366–376); Abschnitt
  „`DisplayOrder` zurückschreiben — geht nicht" ist bereits eingetragen
  (14.08.2026).
- `docs/features/spielliste-manuelle-reihenfolge.md` — diese Spec.
- `docs/adr/0023-manuelle-spielreihenfolge-praefix-je-halle.md` — bereits
  angelegt.
- `docs/preparation.md` — Querverweis auf diese Spec ergänzen (dieselbe
  Kandidatenliste betroffen).
- `CLAUDE.md` — neue Zeile in der Doku-Pflicht-Tabelle: „Manuelle
  Spielreihenfolge" → `tablet/queue_order.rs`, `tablet/assign.rs`
  (`sort_key_with_manual_order`/`resolve_and_sort_key`), `sync.rs`
  `reconcile_queue_order`, `relay-proto` `TlAction::QueueReorder`/
  `QueueOrderReset`, die fünf Call-Site-Dateien, `assets/tl.html`
  `enableReorderDrag`, `pages/PreparationPanel.tsx` →
  `docs/features/spielliste-manuelle-reihenfolge.md`.
- `docs/roadmap.md` — Eintrag von „Spezifiziert" nach Umsetzung
  verschieben (Muster Feldvergabe-Ausnahme).
- `docs/changelog.md` — bei Release.

## Umsetzungs-Hinweise

Reihenfolge (Details siehe
`docs/features/_intake/spielliste-manuelle-reihenfolge/3-how-to.md`):

1. `QueueOrderStore` (`tablet/queue_order.rs`, neu) + Tests.
2. `TabletState`-Anbindung (Store-Feld, Wrapper-Methoden).
3. `sort_key_with_manual_order` + `resolve_and_sort_key` in `assign.rs`
   + Tests (inkl. Rückwärtskompatibilität bei leerem Präfix).
4. Cross-Site-Regressionstest (verpflichtend, vor den Call-Site-Umbauten
   anlegen, damit er jede nachfolgende Änderung sofort absichert).
5. Wire-Form: `TlAction::QueueReorder`/`QueueOrderReset` in
   `relay-proto`, Fingerprint/Label-Arme in `tl.rs`, Tauri-Commands
   `queue_reorder`/`queue_order_reset`.
6. `sync.rs::auto_assign` umstellen + `reconcile_queue_order`
   einhängen.
7. `tablet/tl.rs::build_state` umstellen, `TlMatch.manual`-Feld,
   Präfix-nur-innerhalb-der-Halle-Test.
8. `commands.rs::preparation_candidates`: `hall`-Feld ergänzen,
   Sortierung umstellen.
9. `tablet/server.rs::info_preparation_state`: analog Schritt 8.
10. `badhub/payload.rs`/`badhub/diff.rs`: Kontext-Parameter, Sortierung
    umstellen, alle bestehenden Testaufrufe anpassen.
11. TL-Web-UI (`assets/tl.html`): `enableReorderDrag` je Hallen-
    Abschnitt, Markierung manuell einsortierter Spiele, globaler
    Reset-Knopf.
12. Desktop-UI (`PreparationPanel.tsx`): Hallen-Abschnitte,
    `useDragReorder`-Einbindung, Markierung, Reset-Knopf,
    `src/api.ts`-Wrapper.
13. Reset-Knopf Ende-zu-Ende-Test (beide Oberflächen, kein Halle-
    weise-Reset möglich).
14. Zusammenspiel-Integrationstest mit Feldvergabe-Ausnahme.

Nach jedem Schritt: `code-reviewer` (Pflicht bei jeder Code-Änderung).
Version gemeinsam bumpen in `src-tauri/Cargo.toml` +
`src-tauri/tauri.conf.json` + `package.json` vor dem Release-Commit.
Kein `security-reviewer` nötig (kein neuer User-Input/Auth/Datei-URL-
Handling über die bestehenden, bereits geprüften Tauri-Command-/
`TlAction`-Muster hinaus).
