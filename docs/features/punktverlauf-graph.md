# Punktverlauf-Graph pro Satz — Spezifikation

> Status: **abgestimmt 2026-08-11** (via /idee: Brief → Grill → How-To → Review;
> Umsetzungsfreigabe durch den Nutzer am 2026-08-11 im Chat).
> Quelle: Idee vom 2026-08-11. Betroffene Crates: src-tauri · relay · relay-proto · src.
> ADR: [0014 (Datenerfassung)](../adr/0014-punktverlauf-expliziter-rally-frame.md) ·
> [0015 (Speicherformat)](../adr/0015-punktverlauf-datei-je-turnier.md).

## Kontext / Problem

Die Tablet-Spielzettel dokumentieren jeden Ballwechsel — aber nur flüchtig
und lokal. Turnierleitung und Schiedsrichter sehen nirgends, **wie** ein
Satz verlaufen ist: Läufe, Führungswechsel, Aufholjagden. Genau diese
Information ist beim Coaching, bei Streitfällen („wir lagen doch vorn")
und für die spätere Anzeige auf badhub.de wertvoll. Heute erreicht die
Ballwechsel-Folge den Turnier-PC nicht einmal — Tablets senden nur
Stand-Schnappschüsse, nach Offline-Phasen sogar nur den Endstand.

## Zielbild & Erfolgskriterien

Zu jedem **tablet-gezählten** Spiel (laufend oder beendet) öffnet ein
Fingertipp/Klick ein Overlay mit **einem Liniendiagramm je Satz**:
x-Achse = gespielte Ballwechsel, y-Achse = erreichte Punkte, **beide
Parteien als zwei Linien im selben Diagramm** (Führungswechsel ablesbar).

Erfolgskriterien:

- Funktioniert ohne Erklärung: Klick aufs Spiel → Graph; Spiele ohne
  Verlauf bieten den Klick gar nicht erst an.
- Bei laufenden Spielen wächst die Kurve im bestehenden Poll-Takt mit.
- Verläufe überleben Host-Neustart und Turniertage (dauerhaft auf Platte)
  und sind badhub-tauglich geschnitten (Folge-Feature „badhub-Push"
  braucht keinen Umbau).
- Das Zählen am Tablet wird durch nichts davon gestört (best-effort).

## Nicht-Ziele

- **Kein badhub-Push und keine Anzeige auf badhub.de** — eigenes
  Folge-Feature mit eigener Spec; hier wird nur das Format vorbereitet.
- **Kein Schiedsrichterzettel-Druck (PDF)** — vom Nutzer am 2026-08-11
  als weiteres Folge-Feature angekündigt: ausgefüllte Zettel aus dem
  Verlauf drucken. Das Verlaufsformat ist die Datenbasis dafür und per
  `serde(default)` additiv erweiterbar (z. B. um den ersten Aufschläger
  je Satz); mehr wird hier bewusst nicht vorgebaut.
- **Kein Verlauf für Papier-/Dialog-Ergebnisse** — es gibt dort keine
  Ballwechsel-Daten; rückwirkend (vor Einführung) existiert nichts.
- **Keine Statistik-Auswertung** (längster Lauf, Punkte-Serien o. ä.) —
  nur der rohe Verlauf als Diagramm.
- **Kein Opt-in/Konfigurationsschalter** — das Feature ist additiv und
  läuft immer mit.
- **Keine Slave-Persistenz** — der Verlauf entsteht zentral beim Master
  (Fern-Hallen-Tablets hängen ohnehin am Master-Relay).

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - `relay-proto`: neue Wire-Typen `TabletMsg::Rally`/`RallySync`,
    `RelayFrame::Rally`/`RallySync`/`TimelineRequest`,
    `HostFrame::TimelineData`, DTO `MatchTimeline`, Größen-Caps.
  - `src-tauri/src/tablet/timeline.rs` (**neu**): `TimelineStore`
    (RAM-Map + Persistenz je Turnier, Ingest, Finalisierung).
  - `src-tauri/src/tablet/{state,server,relay_client,tl}.rs`: Anbindung,
    WS-Ingest (LAN + Cloud), LAN-Route, `has_timeline`-Flags.
  - `relay/`: Durchleitung der Tablet-Frames, On-Demand-Route
    `GET /{ns}/tl/api/timeline/{match_id}` nach `tl_forward`-Muster.
  - `src-tauri/src/btp/model.rs`: `start_date` aus dem Snapshot (Setting
    gegen Mitschnitt verifizieren; Fallback Erstsichtungs-Datum).
  - `src-tauri/assets/tablet.html`: `rallyLog`, Resync, Verlauf-Overlay
    (SVG aus lokalen Daten). `assets/tl.html`: Overlay + SVG-Renderer.
  - `src/`: Command-Anbindung, `TimelineChart.tsx`,
    `FieldOverviewPage.tsx` (Felderübersicht + Beendet-Tabelle).
- **Architekturregeln:** R1 Desktop nur über Tauri-Command
  (`match_timeline`). R2 unberührt — der Verlauf ist additive Anzeige,
  BTP bleibt die Wahrheit für Ergebnisse; bei Abweichung gewinnt BTP,
  der Graph trägt einen Hinweis. R3 beide Wege: LAN-Route am
  eingebetteten Server, Cloud über Relay-Request/Response; Tablet
  rendert lokal (offline-fähig). R4/R6 unverändert; Ingest nur vom
  aktiven Court-Halter. R5-Analogie: Rally-Frames werden gegen das
  Court-Match gefiltert (HM-03-Muster) und hart gedeckelt.
- **Konfiguration & Abwärtskompatibilität:** keine neuen Config-Felder.
  Alle neuen Wire-Felder `#[serde(default)]`; unbekannte Frames werden
  von altem Relay/Host still verworfen. **Rollout: Relay vor Client.**
- **Datenschutz:** Der gespeicherte Verlauf enthält **keine Namen** —
  nur Turnier-/Match-Kennungen und Punktfolgen. Namen kommen zur
  Laufzeit aus dem Turnierstand. Kein Geburtsjahr. Keine Löschpflichten
  an der Datei (kein Personenbezug); Dateien bleiben bewusst liegen.
- **Abhängigkeiten:** keine neue Cargo-/npm-Dependency (SVG handgerollt).
  BTP-Startdatum-Setting ist zu verifizieren (Fallback definiert).

## Akzeptanzkriterien

Kern:

- [ ] **AK-1** Am Tablet zählt jeder `point()`-Aufruf einen Ballwechsel in
      den lokalen `rallyLog`; ein Undo kürzt ihn. Der Graph am Tablet
      entsteht aus diesen lokalen Daten und funktioniert **offline**.
- [ ] **AK-2** Das Tablet meldet jeden Ballwechsel als `Rally`-Frame und
      sendet nach Undo, Satz-Wiedereröffnung, Reconnect, Seiten-Reload
      und Geräte-Übernahme einen **Komplett-Resync** (`RallySync`), der
      den Host-Stand des Matches vollständig ersetzt.
- [ ] **AK-3** Der Host akzeptiert Rally-/Sync-Frames nur vom aktiven
      Court-Halter und nur für das dem Court zugewiesene Match; fremde
      und überzählige Frames werden verworfen (Caps: Rallies je Satz,
      Sätze je Match, Sync-Größe).
- [ ] **AK-4** TL-Web und Desktop zeigen den Graphen als Overlay: TL-Web
      an Feldkachel (laufend) und Beendet-Zeile, Desktop an
      Felderübersicht und Beendet-Tabelle — **nur** wenn `has_timeline`
      gesetzt ist (kein Klick ins Leere bei Papier-Spielen).
- [ ] **AK-5** Der Verlauf wird **on-demand** geladen (LAN-Route und
      Relay-Route, gleicher Pfad `tl/api/timeline/{match_id}`), nie im
      regulären `TlState`-Push; bei offenem Overlay zieht der bestehende
      Poll-Takt nach.
- [ ] **AK-6** Verläufe werden je Turnier unter
      `punktverlauf/<slug>.json` gespeichert (Schlüssel Turniername +
      Startdatum, GUID im Header falls konfiguriert), Schreiben
      best-effort, debounced, atomar; beim Start wird die Datei des
      aktuellen Turniers geladen.
- [ ] **AK-7** Ein Spiel mit Zwischenstand-Einstieg (`midGameSetup`)
      bekommt einen Teilverlauf ab Einstiegsstand mit Kennzeichnung
      „ab Zwischenstand aufgezeichnet".
- [ ] **AK-8** Weicht der Verlauf vom gewerteten BTP-Ergebnis ab
      (nachträgliche Korrektur), bleibt der Graph abrufbar und trägt den
      Hinweis „weicht vom gewerteten Ergebnis ab".

Randfälle (aus dem Grill, alle testbar):

- [ ] **AK-9** (Host-Neustart) Nach einem Neustart mitten im Spiel lädt
      der Host die Datei; die Lücke seit dem letzten Schreiben füllt der
      nächste `RallySync` des Tablets.
- [ ] **AK-10** (Geräte-Übernahme) Nach `state_restore` auf einem neuen
      Gerät bleibt der Verlauf vollständig — der `rallyLog` wandert im
      persistierten Tablet-Zustand mit, das neue Gerät resynct.
- [ ] **AK-11** (Stale-Frames) Rally-Frames eines nicht mehr dem Court
      zugewiesenen Matches werden verworfen (HM-03-Muster) und
      verschmutzen keinen fremden Verlauf.
- [ ] **AK-12** (Verlagerung) Wird ein laufendes Spiel auf ein anderes
      Feld verlegt, hängt der Verlauf an der `match_id` und läuft dort
      nahtlos weiter.
- [ ] **AK-13** (Aufgabe/Disqualifikation) Ein Sonderausgang mitten im
      Satz finalisiert den Verlauf mit Kennzeichnung; der Teil-Satz
      bleibt sichtbar.
- [ ] **AK-14** (Offline-Nachzügler) Ein nach Reconnect eintreffender
      `RallySync` füllt die Offline-Lücke; für ein inzwischen
      abgeschlossenes und neu besetztes Court-Match wird er verworfen,
      wenn er nicht mehr zum Court-Match passt — der zuletzt
      finalisierte Stand bleibt.

## Tests

TDD, `cargo test` grün, `npm run build` fehlerfrei:

- `relay-proto`: Serde-Roundtrips aller neuen Frames; Alt-Frames ohne
  neue Felder bleiben lesbar; Punktfolgen-Validierung (nur `A`/`B`,
  Caps).
- `tablet/timeline.rs`: Reihenfolge/Lücken-Erkennung · Sync ersetzt
  vollständig (Undo) · Fremd-Match verworfen · Persist/Reload über
  Neustart · match_id-stabil bei Court-Wechsel · Zwischenstand-Marker ·
  Aufgabe-Finalisierung · Nachzügler-Sync · Turnierwechsel öffnet neue
  Datei · Dateinamen-Slug (Path-Traversal).
- `tablet/server.rs`: Ingest nur vom aktiven Halter;
  `process_result` finalisiert.
- `relay/`: Durchreichung 1:1, überlanger Sync verworfen;
  Timeline-Request↔-Antwort, Host offline → 503, Timeout räumt Pending,
  fremder Bearer → 401.
- Manueller Turniertest: Live-Kurve am TL-Web während ein Tablet zählt;
  Undo sichtbar korrigiert; Reload des Tablets ändert den Graphen nicht.

## Risiken & Rollback

- Schreiben stört das Zählen nie (best-effort wie `persist_scores`);
  Datenträgerfehler kosten den Graphen, nie das Ergebnis.
- Version-Skew (Auto-Update nicht atomar): alte Tablet-Seite →
  `has_timeline` bleibt aus; neue Tablet-Seite an altem Relay → Frames
  still verworfen; neuer Relay an altem Host → Timeout mit klarer
  Meldung; alte `tl.html` → kennt den Knopf nicht.
- Cloud-Missbrauch: harte Caps + Verwerfen statt Wachsen; GET hinter
  Bearer + Slot-/Pending-Limit.
- Rollback: vollständig additiv — Frames werden wieder ignoriert, Routen
  404, Knöpfe verschwinden; Verlaufsdateien bleiben gefahrlos liegen.

## Offene Fragen / Annahmen

- BTP-Setting-ID des Turnier-Startdatums ist gegen einen echten
  Mitschnitt zu verifizieren; bis dahin gilt der definierte Fallback
  (Host stempelt Erstsichtungs-Datum in den Datei-Header).
- Namenskollision `<Turniername+Datum>` zweier verschiedener Turniere
  wird bewusst akzeptiert (gleiche Datei wird weitergeführt).
- TL-Web zeigt nur die letzten 30 Beendeten (`FINISHED_LIMIT`) — der
  Graph ist dort nur für diese erreichbar (Desktop zeigt alle).

## Betroffene Doku-Dateien

- **`docs/punktverlauf.md` (neu)** — Funktionsweise, Datenfluss,
  Speicherort, Grenzen; CLAUDE.md-Tabellenzeile dazu.
- `docs/cloud-relay.md` (neue Frames + Route), `docs/tablet.md`
  (rallyLog, Overlay), `docs/turnierleitung-web.md` (Overlay),
  `docs/btp_protocol.md` (Startdatum-Setting), `docs/changelog.md`
  (beim Release).

## Umsetzungs-Hinweise

Reihenfolge und Details: [How-To (Phase 3)]-Ergebnis, verdichtet:

1. `relay-proto`-Wire-Typen (+ Serde-Tests) → 2. BTP-Startdatum →
3. `TimelineStore` (+ Kern-Tests) → 4. Ingest LAN/Cloud + Relay-
Durchleitung → 5. Tablet-Seite (rallyLog, Resync, Overlay) → 6. Lese-
Pfade Host (Command, LAN-Route, `has_timeline`) → 7. Relay-Route →
8. TL-Web-Overlay → 9. Desktop-Overlay → 10. Doku-Abschluss.

Reviews: `code-reviewer` nach jeder Änderung; `security-reviewer` für
Wire-Eingaben/Caps (1, 4), Dateipfad aus Turniernamen (3) und die neuen
Routen (6, 7). Rendering überall handgerolltes SVG (keine Dependency).
Kein Versions-Bump (kommt mit dem Release). **Rollout: Relay vor
Client.** ADRs: 0014 (expliziter Rally-Frame), 0015 (Datei je Turnier).
