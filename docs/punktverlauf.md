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
- **Nachzügler nach dem Spielende** (seit v0.9.375, Spec
  [tablet-stand-robust](features/tablet-stand-robust.md), Regel D; ADR 0085):
  Der `rally_sync` reist über den WebSocket, das Ergebnis über HTTP — die
  beiden haben keine Ordnung. Das Tablet schickt den kompletten Verlauf
  deshalb an drei weiteren Stellen: bei jedem `match_assigned` für das
  schon geladene Spiel (heilt Host-Neustart und Abriss Turnier-PC↔Relay),
  unmittelbar vor dem Ergebnis und einmal nach dessen Annahme. Der Host
  nimmt einen **kompletten** Abgleich für das zuletzt auf dem Feld beendete
  Spiel noch **120 s** an — auch wenn das Feld schon neu belegt ist
  (`TabletState::sync_ziel`, `NACHZUEGLER_FENSTER`). Ein solcher Nachzügler
  darf nur **ergänzen** (`TimelineStore::apply_nachzuegler`, `ergaenzt`):
  Was der Host schon hat, muss unverändert der Anfang bleiben — je Satz
  derselbe Startstand und die vorhandene Punktfolge als Präfix. Kürzer
  („Wieder öffnen" am Tablet), gedreht (Mannschaften getauscht) oder anders
  gezählt wird verworfen; einzige Ausnahme ist ein Satz, den der Host nur
  als Endstand kennt und dessen Ballwechsel genau auf diesen Endstand
  führen. `finished` bleibt gesetzt, `retired` kommt aus dem Bestand.
  Das Fenster gilt **ab dem Ergebnis**, auch in den Sekunden, in denen BTP
  das Spiel noch auf dem Feld führt (`sync_ziel` prüft es zuerst). Kehrt
  das Spiel nach einer Ergebniskorrektur aufs Feld zurück, fällt der Merker
  nach 15 s Karenz (`spiel_wieder_auf_dem_feld`), und das Tablet ist wieder
  die Wahrheit. Wechselt das Spiel am Tablet, schickt es vor dem Laden des
  neuen einen letzten Abgleich des alten. Einzelne `rally`-Frames bleiben
  nach dem Ergebnis verworfen. Ein Abgleich ohne Neues schreibt nichts auf
  die Platte. Das Fenster liegt nur im RAM: Startet der Turnier-PC
  innerhalb der zwei Minuten neu, ist es weg.
  Der badhub-Push (`tverlauf`) schiebt den ergänzten Verlauf über seinen
  Diff von selbst nach.
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
Fehlerpfad (`PushTeil::send_verlauf` in `sync.rs`, `src-tauri/src/badhub/verlauf.rs`), geht je
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
- **Nur Geändertes** (seit v0.9.383 über Änderungsmarken): `TimelineStore`
  und `SheetStore` führen je Match eine Marke, die bei jedem Schreiben steigt
  (`marke()`); dazu kommt ein Hash über Pausenzustand, Zählweise und
  Einzel/Doppel (die ändern sich, ohne dass ein Speicher schreibt). `VerlaufStand` hält je Match die
  Marke des zuletzt **angenommenen** Eintrags. Gebaut — also geklont und
  serialisiert — wird nur, was sich geändert hat (`verlauf::kandidaten` →
  `VerlaufStand::auswahl` → `eintrag_fuer`). Bis v0.9.382 baute und hashte
  jeder 5-s-Takt alle Einträge aller Spiele. Höchstens 64 Matches je
  Nachricht (der Rest folgt gestaffelt). Neues Turnier → alles neu.
- **badhubs Antwort zählt** (seit v0.9.383): badhub verwirft einzelne
  Einträge mit Status 200 — etwa den Verlauf eines Spiels, das es noch nicht
  aus `tset` oder `sched` kennt — und nennt nur ihre **Zahl**
  (`{"type":"ok","rows":n,"verworfen":n}`). Bei `verworfen > 0` gilt das
  Paket nicht als angenommen; jeder Eintrag des Pakets zählt einen Versuch.
  Nach **drei** Versuchen wird ein Eintrag geparkt, bis er sich ändert oder
  ein neuer voller `tset` bestätigt ist: sofort, wenn der `tset` wegen einer
  Änderung oder nach einer Störung hinausging; bei einem bloßen
  Lebenszeichen höchstens alle zehn Minuten (sonst ginge ein Verlauf, den
  badhub auf Dauer verwirft, dreimal je Minute hinaus). Bis v0.9.382 galt ein
  verworfener Verlauf als gesendet und kam nie wieder.
- **Ganz-Ablehnung** (5xx, 413, 409): Die nächste Auswahl beginnt hinter der
  abgelehnten — sonst hielten dieselben ersten 64 alle übrigen auf.
- **Nur mit bestätigtem Stand** (seit v0.9.382): Scheitert der `tset` eines
  Durchgangs, geht kein `tverlauf`.
- **Vom Zettel nur `serve_start`** (nicht zurückgenommene) — keine Karten,
  keine Disqualifikation, keine Verletzung (`is_sanction`-Regel). Pausen nur
  `eleven`/`game` aus dem Tablet-Zustand. Keine Namen.
- Aufschläger, Aufschlagfeld und Annehmer je Ballwechsel rechnet badhub aus
  `serve_start` + Punktfolge nach; deshalb gehen keine Aufstellungen je
  Ballwechsel mit.
- Altes badhub (400/404 auf den Typ): 30 Minuten Pause des Kanals wie beim
  `sched` (`strecke::KANAL_PAUSE`), danach erneut. Alle Fehlerklassen:
  [liveticker-push.md](liveticker-push.md).
- **Mannschaften berichtigt** (seit v0.9.319, ADR 0067): badhub kennt nur
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

- **Mannschaften getauscht am Tablet** (⇄, seit v0.9.319): Das Tablet kippt den
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

`relay-proto` (Serde/Deckel) · `tablet/timeline.rs` (Kern-Tests:
Lücken, Undo-Sync, Fremd-Match, Neustart-Reload, Turnierwechsel,
Zwischenstand, Aufgabe, Nachzügler, Slug; `nachzuegler_*` für den Abgleich
hinter dem Ergebnis) · `tablet/state.rs` (`nachzuegler_*`,
`einzel_rally_nach_ergebnis_bleibt_verworfen`: Fenster, fremdes Spiel, neu
belegtes Feld) · `scripts/test-verlauf-sync.mjs` (die drei Sende-Stellen am
echten Tablet-Code) · `tablet/server.rs` (Finalisierung) · `tablet/tl.rs` (`has_timeline`-Wahrheit) · `relay/`
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
