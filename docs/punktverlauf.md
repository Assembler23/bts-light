# Punktverlauf-Graph

Zu jedem **tablet-gezählten** Spiel entsteht je Satz ein Liniendiagramm
des Punktverlaufs: x-Achse = gespielte Ballwechsel, y-Achse = erreichte
Punkte, beide Parteien als zwei Linien. Abrufbar per Klick/Fingertipp —
für laufende **und** beendete Spiele.

Spezifikation: [features/punktverlauf-graph.md](features/punktverlauf-graph.md) ·
Entscheidungen: [ADR 0014](adr/0014-punktverlauf-expliziter-rally-frame.md)
(Datenerfassung) · [ADR 0015](adr/0015-punktverlauf-datei-je-turnier.md)
(Speicherformat).

## Wo der Graph zu finden ist

| Oberfläche | laufend | beendet |
|---|---|---|
| **Tablet** (Zähltafel) | „📈 Verlauf"-Knopf am Score-Board — rendert aus den **lokalen** Daten, funktioniert offline | — (nach der Ergebnis-Abgabe verlässt das Tablet das Spiel) |
| **TL-Web** | 📈 im ⋯-Menü der Feldkachel (bis 17.08.2026 direkt auf der Kachel) | 📈 an der Beendet-Zeile |
| **Desktop** | 📈 an der Feld-Kachel der Felderübersicht | 📈 in der Beendet-Tabelle |

Der Klick erscheint **nur**, wenn es wirklich einen Verlauf gibt
(`has_timeline`) — Papier-/Dialog-Ergebnisse haben keinen, und rückwirkend
(vor Einführung) existiert nichts.

## Wie die Daten fließen

```
Tablet ──rally (je Ballwechsel)──▶ Host (TimelineStore) ──punktverlauf/<slug>.json
   │        rally_sync                     ▲    │
   └── (Komplett-Resync nach Undo/         │    └─▶ Tauri-Command match_timeline (Desktop)
        Reconnect/Übernahme/Reopen)        │    └─▶ GET /tl/api/timeline/{match_id}
                                           │         (LAN-Server UND Relay, on-demand)
                              Relay reicht 1:1 durch (Briefträger)
```

- **Expliziter Rally-Frame** (ADR 0014): Der Host leitet nichts aus
  Schnappschüssen ab. Jeder Ballwechsel kommt einzeln (`rally`); nach
  Undo, Satz-Wiedereröffnung, Reconnect, Reload und Geräte-Übernahme
  ersetzt ein `rally_sync` den Host-Stand des Matches **komplett** — der
  Verlauf heilt sich selbst (Offline-Lücken, Host-Neustart).
- **Verwerfen statt raten:** Ein Frame, der nicht lückenlos passt
  (Satzfolge, laufende Nummer, Stand-Plausibilität) oder nicht zum
  aktuellen Court-Match gehört (HM-03-Muster), wird verworfen; der
  nächste Sync ersetzt. Harte Deckel (`MAX_RALLIES_PER_SET` u. a.)
  verteidigen den Cloud-Weg.
- **On-Demand statt Push:** TL-Web/Desktop laden den Verlauf erst beim
  Öffnen des Overlays und ziehen ihn dann im 2-s-Takt nach. Er ist NIE
  Teil des `TlState`-Pushes (Mobilfunk-Budget); der Relay hält keine
  Verläufe vor, sondern reicht Anfrage und Antwort durch
  (`timeline_request`/`timeline_data`, Muster TL-Kommando).

## Speicherung (dauerhaft, ohne Namen)

`<app_data>/punktverlauf/<slug>.json` — **eine Datei je Turnier**
(ADR 0015): Kopf (Turniername, Erstsichtungs-Datum, turnier.de-GUID falls
konfiguriert) + `matches`-Map (match_id → Sätze als Punktfolgen
`"ABBA…"`). Bewusst **ohne Spielernamen** — die Anzeige holt Namen zur
Laufzeit aus dem Turnierstand; die Datei ist zugleich das fertige
Dokument für den badhub-Push (`tverlauf`, siehe unten).

- Slug aus dem Turniernamen: Whitelist `[a-z0-9-]` (Path-Traversal
  ausgeschlossen); Namenskollisionen führen dieselbe Datei weiter.
- BTP liefert **kein** Startdatum (Befund in
  [btp_protocol.md](btp_protocol.md)) — der Host stempelt die
  Erstsichtung selbst und behält sie über Neustarts.
- Geschrieben wird best effort (atomar, debounced ~3 s; Resync und
  Finalisierung sofort) — ein Schreibfehler kostet den Graphen, nie das
  Zählen. Alte Turnier-Dateien bleiben liegen.

## badhub-Push: `tverlauf` (seit v0.9.309, badhub ADR-0060)

Im selben 5-s-Takt wie der Score-Push, aber danach und mit eigenem
Fehlerpfad (`Sync::send_verlauf`, `src-tauri/src/badhub/verlauf.rs`), geht je
Match der **ganze** Punktverlauf an `live_update.php`:

```json
{"type":"tverlauf","rid":7,"tournament_uuid":"<GUID>","matches":[{
  "_id":"btp_4711","doubles":true,
  "fmt":{"target":21,"cap":30,"bestOf":3,"intervalAt":11},
  "timeline":{"sets":[{"startA":0,"startB":0,"points":"ABBA"}],"midGame":false,"retired":false,"finished":false},
  "serve":[{"kind":"serve_start","set":1,"afterN":0,"team":0,"player":0,"receiverTeam":1,"receiverPlayer":0}],
  "pause":{"kind":"eleven","endsAt":1793450000000}}]}
```

- **Ersetzen statt anhängen**: badhub ersetzt je Match den gespeicherten
  Verlauf — Undo, Host-Neustart und verlorene Pushes heilen sich.
- **Nur Geändertes**: `VerlaufStand` hält je Match den Hash des zuletzt
  **angenommenen** Eintrags; abgelehnte gehen im nächsten Takt erneut, höchstens
  64 Matches je Nachricht (der Rest folgt gestaffelt). Neues Turnier → alles neu.
- **Vom Zettel nur `serve_start`** (nicht zurückgenommene) — keine Karten,
  keine Disqualifikation, keine Verletzung (`is_sanction`-Regel). Pausen nur
  `eleven`/`game` aus dem Tablet-Zustand. Keine Namen.
- Aufschläger, Aufschlagfeld und Annehmer je Ballwechsel rechnet badhub aus
  `serve_start` + Punktfolge nach; deshalb gehen keine Aufstellungen je
  Ballwechsel mit.
- Altes badhub (400/404 auf den Typ): Pause wie beim `sched`
  (`CHECKIN_UNSUPPORTED_RETRY`), danach erneut.
- **Mannschaften berichtigt** (seit v0.9.318, ADR 0065): badhub kennt nur
  `serve_start`. `aufschlaege()` liefert sie deshalb schon in Endkoordinaten
  (`tablet/berichtigung.rs` `in_endkoordinaten`); die Punktfolge kippt das
  Tablet selbst und schickt sie per `rally_sync` neu.

## Kennzeichnungen

- **„ab Zwischenstand aufgezeichnet"** — die Zähltafel übernahm mit
  eingetipptem Stand (`midGameSetup`); der Satz beginnt beim
  Einstiegsstand, frühere Sätze erscheinen als reine Endstände (Punkte).
- **Aufgabe/Disqualifikation** — Verlauf finalisiert, letzter Satz
  bewusst unvollständig.
- **„weicht vom gewerteten Ergebnis ab"** — nachträgliche Korrektur in
  BTP; der Graph bleibt abrufbar, aber BTP bleibt die Wahrheit (R2).

## Grenzen

- **Mannschaften getauscht am Tablet** (⇄, seit v0.9.318): Das Tablet kippt den
  Verlauf A↔B und schickt ihn per `rally_sync` komplett neu. Hatte vorher ein
  anderes Tablet übernommen, kannte es frühere Sätze nur als Endstand — deren
  Ballwechsel-Details gehen dann beim Host verloren (die AK-12-Verschmelzung
  greift bei gekipptem Endstand nicht mehr). „Seiten tauschen" ändert am
  Verlauf nichts.

- Kein Verlauf ohne Tablet-Zählung; TL-Web zeigt Beendete nur bis
  `FINISHED_LIMIT` (30) — ältere erreicht man im Desktop.
- Versionsschiefstand ist vorgesehen: alte Tablets senden keine Frames
  (`has_timeline` bleibt aus), ein alter Relay verwirft sie still, ein
  alter Host lässt den Relay-Abruf in einen klaren 503-Hinweis laufen.
  **Rollout-Regel: Relay vor Client** (passiert beim Release automatisch).
- Der Verlauf entsteht zentral beim **Master** (Fern-Hallen-Tablets
  hängen am Master-Relay); Slaves persistieren nichts.

## Im Code

- Wire: `relay-proto` (`TabletMsg::Rally`/`RallySync`,
  `RelayFrame::TimelineRequest`, `HostFrame::TimelineData`,
  `MatchTimeline`).
- Host: [`tablet/timeline.rs`](../src-tauri/src/tablet/timeline.rs)
  (`TimelineStore`), Ingest in `tablet/server.rs` (LAN) und
  `tablet/relay_client.rs` (Cloud), Finalisierung in `process_result`,
  `has_timeline` in `state.rs`/`tl.rs`, Command `match_timeline`.
- Relay: Durchleitung + Route `GET /tl/api/timeline/{match_id}`
  (`tl_timeline_route`, Muster `tl_forward`).
- Anzeige: `timelineSetSvg` in `tablet.html` (**kanonische Fassung**;
  `tl.html` trägt die markierte Inline-Kopie),
  [`TimelineChart.tsx`](../src/components/TimelineChart.tsx) (Desktop).

## Tests

`relay-proto` (Serde/Deckel) · `tablet/timeline.rs` (13 Kern-Tests:
Lücken, Undo-Sync, Fremd-Match, Neustart-Reload, Turnierwechsel,
Zwischenstand, Aufgabe, Nachzügler, Slug) · `tablet/server.rs`
(Finalisierung) · `tablet/tl.rs` (`has_timeline`-Wahrheit) · `relay/`
(Durchleitung, Deckel, Anfrage↔Antwort, 404/503).

## Zwei Projektionen, eine Erfassung

Seit dem Schiedsrichterzettel (Spec
[features/schiedsrichterzettel-druck.md](features/schiedsrichterzettel-druck.md),
ADR 0037) gibt es **zwei** Lesesichten auf dasselbe Spiel:

| | Punktverlauf-Graph | Schiedsrichterzettel |
|---|---|---|
| Quelle | `MatchTimeline` (Ballwechsel) | `MatchTimeline` **+** `MatchEvent` (Ereignisse) |
| Datei | `punktverlauf/<slug>.json` | `zettel/<slug>.json` |
| Sanktionsdaten | **nie** | ja (das ist sein Zweck) |
| Abgleich vom Tablet | **ersetzt** (`rally_sync`) | **vereinigt** (`match_event_sync`) |

**Der Punktverlauf bleibt unverändert.** `MatchTimeline`, `TimelineSet`, die
Datei, die Graph-Route und `timelineSetSvg` sind nicht angefasst worden; ein
Golden-String-Test (`graph_dto_bleibt_byte_gleich`) friert die Graph-Antwort ein.
Wer sie ändert, bricht dort — nicht erst auf einer älteren `tl.html` im Turnier.

Der Grund für die Trennung ist Datenschutz, nicht Geschmack: Karten sind
personenbezogene Sanktionsdaten. In `MatchTimeline` gelegt, kippten sie die
Begründung aus ADR 0015 („kein Personenbezug auf Platte, also keine
Lösch-Pflichten") und vererbten dem späteren badhub-Push die Falle. Zusätzlich
haben beide Ströme **getrennte Deckel** — ein Ereignis-Schwall kann den
Punktverlauf strukturell nicht mehr verdrängen.
