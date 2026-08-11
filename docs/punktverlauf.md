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
| **TL-Web** | 📈 an der Feldkachel | 📈 an der Beendet-Zeile |
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
Dokument für den späteren badhub-Push (Folge-Feature).

- Slug aus dem Turniernamen: Whitelist `[a-z0-9-]` (Path-Traversal
  ausgeschlossen); Namenskollisionen führen dieselbe Datei weiter.
- BTP liefert **kein** Startdatum (Befund in
  [btp_protocol.md](btp_protocol.md)) — der Host stempelt die
  Erstsichtung selbst und behält sie über Neustarts.
- Geschrieben wird best effort (atomar, debounced ~3 s; Resync und
  Finalisierung sofort) — ein Schreibfehler kostet den Graphen, nie das
  Zählen. Alte Turnier-Dateien bleiben liegen.

## Kennzeichnungen

- **„ab Zwischenstand aufgezeichnet"** — die Zähltafel übernahm mit
  eingetipptem Stand (`midGameSetup`); der Satz beginnt beim
  Einstiegsstand, frühere Sätze erscheinen als reine Endstände (Punkte).
- **Aufgabe/Disqualifikation** — Verlauf finalisiert, letzter Satz
  bewusst unvollständig.
- **„weicht vom gewerteten Ergebnis ab"** — nachträgliche Korrektur in
  BTP; der Graph bleibt abrufbar, aber BTP bleibt die Wahrheit (R2).

## Grenzen

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
