# Monitor-Livestand per Push — Spezifikation

> Status: **abgestimmt 2026-08-18** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Idee vom 18.08.2026. Betroffene Crates: `src-tauri`, `relay`, `relay-proto`, `src/io`.
> ADR: [0035 — Monitor-Livestand: schmaler Abruf, Ordnung über `seq`, additive Nudges](../adr/0035-monitor-livestand-ordnung.md)

## Kontext / Problem

Ein gezählter Punkt ist das häufigste Ereignis im Turnierbetrieb und damit der Haupttreiber der
Last. Der Weg vom Tablet zum Bildschirm ist heute in drei Schritten unterschiedlich gut gelöst:

1. **Tablet → Server: gezielt.** Pro Punkt ein `score_update` über die Tablet-WS,
   `handle_score` (`src-tauri/src/tablet/server.rs:3039`) validiert und schreibt.
2. **Server → Anzeigen: gezielt adressiert, aber datenlos.** `notify_monitor`
   (`state.rs:2090`) sendet nur `{"court":7,"seq":42}`. Feste Court-Monitore abonnieren ihr
   Feld (`?court=`), Übersichts-TVs und Geräte-Monitore alle Felder.
3. **Anzeige: Voll-Abruf und Voll-Render.** Jeder Nudge löst einen kompletten HTTP-Fetch aus;
   `render()` in `overview.html:503` macht `board.textContent = ''` und baut alle Feldkarten neu.

Für den festen Court-Monitor ist das billig — er holt nur sein eigenes Feld, ~0,3 Abrufe/s.
**Der Schmerz liegt bei der Feld-Übersicht:** sie wird von jedem Punkt jedes Feldes geweckt und
zieht dabei den Zustand aller Felder. Bei 20 Feldern und 20 Übersichts-TVs sind das grob
1,6–8 MB/s Hallen-WLAN und ein voller DOM-Neuaufbau je Punkt auf jedem Pi — für eine Information
von ~20 Byte.

Die Analyse (Grill, 18.08.2026) hat zwei weitere Posten freigelegt, die der Transport gar nicht
berührt und die zusammen schwerer wiegen als der Fanout:

- **Rechnung je Abruf:** `overview()` (`state.rs:3276`) macht **je Feld** einen linearen Scan
  über alle Matches **und** ein `serde_json::from_str` des `display_court_state`, dazu
  `hall_colors::paint` — bei jedem einzelnen `/health`.
- **Plattenschreibvorgang je Punkt:** `record_score` (`state.rs:1918`) schreibt über
  `persist_scores` die **komplette** `live-scores.json` synchron im async-Handler.

Betroffen sind Turnierleiter (träger Turnier-PC bei vielen Anzeigen), Zuschauer (ruckelnde TVs)
und der Hallenbetrieb (WLAN-Sättigung). Vorbild und Zwillingsfall ist
[TL-Web-Push](tl-web-push.md), das dieselbe Frage vor einer Woche für die TL-Seite gelöst hat.

## Zielbild & Erfolgskriterien

Ein gezählter Punkt kostet nur noch, was er wert ist: eine kleine Nachricht an die betroffenen
Anzeigen, ein schmaler Abruf nur des betroffenen Feldes, ein punktuelles Update auf dem
Bildschirm. Für den Turnierleiter ändert sich **nichts an der Bedienung** — die Anzeigen
reagieren gleich schnell oder schneller, und die Pis bleiben ruhig.

Erfolg wird an der Messung aus Etappe S0 festgemacht (Vorher/Nachher, 20 Felder × 20 Monitore):

| Kennzahl | Ziel |
|---|---|
| `/health`-Requests/s gesamt, getrennt nach `push`/`poll` | −80 % |
| `/health`-Bytes/s | −90 % |
| `overview_build_ns` p95 je Abruf | ≥95 % der Abrufe treffen den Cache |
| `persist_calls`/s | −90 % |
| Latenz Punkt → Anzeige p95 | nicht schlechter als heute |
| Latenz Zuweisung → Anzeige p95 | ≤ 1 s (trotz langsamerem Fallback) |
| Pi: verlorene Frames/min in `overview.html` | −90 % |

Die Nachmessung läuft mit **aktiviertem** `court_monitor.push_fallback_slow` — die Zielwerte
beziehen sich auf den Vollausbau, nicht auf den ausgelieferten Default. Zusätzlich wird einmal
mit Default-Einstellung gemessen, um zu belegen, dass eine frisch aktualisierte Installation
nicht langsamer ist als vorher.

Dazu die Bestätigung an einem echten Turnier: keine hängenden, springenden oder eingefrorenen
Anzeigen über einen vollen Turniertag.

## Nicht-Ziele

- **TL-Web** (`tl.html`). Hat seit v0.9.221 einen eigenen Push-Kanal mit Host-Cache (ADR 0034).
- **Inkrementelle Übertragung** irgendeines Wertes. Alles transportiert absolute Werte (ADR 0014).
- **Nutzlast im Nudge.** Zurückgestellt als messbedingte Ausbaustufe, siehe „Ausbaustufe" unten.
- **`court_state` als StateRestore-Träger** (`state.rs:3471`). Bleibt unverändert.
- Deltas oder Sonderpfade für alles außer dem Anzeige-Livestand: Aufruf, Spielende, Werbung und
  Geräte-Zuweisung laufen weiter über den bestehenden Voll-Abruf.
- Das Tablet selbst (es ist der Sender), der Liveticker-Push zu badhub, `slave_bridge.rs`.
- Eine Protokollversion für den Nudge-Kanal (bewusst nicht, siehe ADR 0035 c).

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  `src-tauri/src/tablet/state.rs` (Antwortcache, `persist_scores`-Entprellung, `seq`-Seeding,
  Zuweisungs-Nudge in `set_snapshot`, gemeinsamer Helfer `anzeige_livestand`) ·
  `src-tauri/src/tablet/server.rs` (`health` mit `?court=`/`?src=`, `monitor_state`, Heartbeat
  auf der Monitor-WS, Perf-Zähler) · `src-tauri/src/sync.rs` (1-s-Flush-Takt) ·
  `relay/src/main.rs` (`overview_health` mit `?court=`/`seq`, `court_display`-Projektion,
  Zuweisungs-Nudge, Heartbeat) · `relay-proto/src/lib.rs` (`MonitorState.seq`) ·
  `src-tauri/assets/overview.html` + `monitor.html` · **neu** `src/io/pushHealth.mjs`,
  `src/io/courtPatch.mjs`, `src/io/monitorSeq.mjs` · **neu** `scripts/last-monitor.mjs`,
  `scripts/test-push-health.mjs`, `scripts/test-court-patch.mjs`, `scripts/test-monitor-seq.mjs`.
- **Architekturregeln:**
  **R1** unberührt — keine neuen Tauri-Commands; die Monitor-Seiten sind HTTP-Clients.
  **R2** gewahrt — weder Abruf noch Patch erfinden je eine Court→Match-Zuordnung; der Patch
  verweigert bei abweichender `match_id`; `set_snapshot` bleibt die einzige Quelle, die
  Nudge-Erweiterung hängt sich dort **lesend** an.
  **R3 zentral** — jede Etappe hat einen Host- **und** einen Relay-Teil; in einer fernen Halle
  hinter `slave_bridge.rs` (reiner Redirect, `/health` ohne Felder) gilt ausschließlich der
  Cloud-Pfad.
  **R4** unberührt — Anzeigen sind keine Tablets, `MAX_MONITOR_SUBS = 256` bleibt.
  **R5** gewahrt und zwar **negativ**: es entsteht **kein neuer Schreibweg**, alles hier ist
  lesend. Neu validiert wird ausschließlich der Selektor `?court=`.
  **R6** unberührt.
- **Konfiguration & Abwärtskompatibilität:** genau ein neues Feld,
  `court_monitor.push_fallback_slow: bool`, **Default `false`** — bestehende Installationen
  verhalten sich nach dem Auto-Update exakt wie vorher, bis der Schalter gesetzt wird. Alte
  `config.json` bleibt lesbar (serde-Default). `identifier` und Updater-Pfad unangetastet.
- **Datenschutz:** kein neues Datum. Der schmale Abruf liefert eine Teilmenge dessen, was die
  Voll-Route ohnehin liefert; die `court_display`-Projektion im Relay enthält **ausschließlich
  Zahlen** (Aufschlag-Team, Aufschlag-Spielerindex, Pausen-Zeitstempel) — nie Namen, nie
  Lizenznummern, nie Geburtsjahre. Die Perf-Zähler und `/debug/perf` enthalten nur Zahlen; das
  ist per Wächter-Test abgesichert.
- **Abhängigkeiten:** keine neue Cargo- oder npm-Abhängigkeit. `scripts/last-monitor.mjs` nutzt
  nur Node-Bordmittel (`node:http`, globales `WebSocket`). nginx `/bts-relay/` unverändert
  (WebSocket-Upgrade und 3600-s-Timeouts stehen bereits in `ops/nginx-bts-relay.conf`).
  Pi-Kiosk unverändert.

## Umsetzung in Etappen

Jede Etappe ist ein eigener PR und für sich auslieferbar. Reihenfolge ist bindend:
**S0 → S1 → S2 → S3 → S4 → S5 → S6 → Nachmessung → S7.**

**S0 — Messung.** (a) Perf-Zähler aus `AtomicU64` in `TabletState`: `health_push`/`health_poll`
+ Bytes, `court_state_*`, `overview_build_ns`, `persist_calls`/`_ns`/`_bytes`, `nudges_sent`.
Die Trennung nudge-getrieben ↔ Fallback liefert der Client über `&src=push|poll` am Fetch —
additiv im vorhandenen `DeviceHeartbeat`-Query (alte Seiten senden nichts → zählen als `poll`).
Ausgabe: aggregierte `tracing::info!`-Zeile alle 10 s (kommt über den Log-Upload auch aus einem
echten Turnier zurück) plus `GET /debug/perf` **nur am LAN-Server**.
(b) `scripts/last-monitor.mjs`: 20 simulierte Tablets (`score_update` **und** `state_sync`, weil
heute jeder Punkt zwei Nudges erzeugt), 20 simulierte Übersichten mit 60-ms-Coalescing wie im
Original, optional 20 Court-Monitore; gegen Host und Relay identisch fahrbar. Kein CI-Schritt.
(c) Pi-Messung `ovRenderMessen` nach dem Muster `tlRenderMessen` aus `tl.html`.

**S1 — Antwortcache für `/health`.** `OverviewCache { rev, etag, courts_json, gebaut_ms }` neben
`tl_state_cache`, Muster `set_tl_state_cache`. **Kein Ticker** (anders als TL-Web):
ereignisgetriebene Invalidierung über `overview_rev` in `notify_monitor`, `set_snapshot` und beim
Config-Schreiben, plus **250-ms-Hart-TTL** gegen vergessene Quellen. Umschlag (`serverNowMs`,
`callTimer`) bleibt je Abruf. ETag → 304 für den Fallback-Poll. Der Cache ist **Beschleuniger,
nicht Wahrheit**: kalt oder abgelaufen → Direktbau wie heute.

**S2 — `persist_scores` entprellen.** `record_score` und die zwei weiteren Aufrufer setzen nur
noch `mark_scores_dirty()`. Ein 1-s-Takt im **Sync-Loop** (läuft in LAN und Cloud) flusht;
**synchroner Flush** bei `process_result`/Ergebnis-Eintrag, Match-Räumung, Stoppen der
Übertragung und App-Ende. Geschrieben wird in `spawn_blocking`, nicht im async-Handler; ein
Fingerabdruck verhindert das Schreiben unveränderter Stände. Bewusst akzeptiert: bei einem
Absturz fehlt bis zu ~1 s auf Platte — die Tablets sind die Wahrheit und heilen das beim
Reconnect per `state_sync`.

**S3 — Zuweisungs-Nudge (schließt die beiden offenen A1-TODOs).** Host: `set_snapshot` vergleicht
die vorherige Projektion `court_id → (match_id, on_court_since)` mit der neuen und nudgt jede
Abweichung — deckt Zuweisung, Räumung, Feldwechsel und den stillen BTP-Sprung in einem Griff.
Relay: `notify_monitor` in den Armen `MatchAssigned`/`MatchCleared`, **nach** dem Cache-Insert.
Kein Nudge bei unverändertem Match.

**S4 — `seq` in den Voll-Antworten.** `CourtOverview.seq` und `MonitorState.seq`
(`serde(default)`, 0 = alter Server), gespeist aus der vorhandenen Pro-Court-Sequenz. Diese wird
je Feld mit `now_ms()` geseedet statt mit 0 (Muster `set_monitor_command`), damit sie über
Prozess-Neustarts monoton bleibt. Client-Regel: Push bei `seq > gezeigt`, Voll-Antwort bei
`seq >= gezeigt`.

**S5 — Render-Patch.** `overview.html` patcht nur die betroffene Karte, `monitor.html` nur die
betroffenen Stellen. Die Zuständigkeitsgrenze ist eine reine, getestete Funktion (siehe
Akzeptanzkriterien). Zusätzlich ein Zwangs-Voll-Render mindestens alle 30 s.

**S6 — „Push-Kanal ist gesund" neu definieren + Fallback 4 s.** Heute verwechselt
`pushHealthy() = mwsOpen && letzter Nudge < 1,2 s` „Kanal lebt" mit „es passiert etwas" — bei
langsamerem Fallback wäre eine ruhige Halle nicht von einem toten Kanal unterscheidbar, und ein
halbtoter Socket meldet minutenlang `OPEN`. Neu: (1) **sichtbarer Heartbeat** `{"hb":<ms>}` alle
10 s auf der Monitor-WS (der bestehende 15-s-WS-Ping ist für JS unsichtbar); (2) Gesundheit =
`mwsOpen && (now − lastServerFrameAt) < 25 s && letzterFetchOk && failures === 0`, wobei
`lastServerFrameAt` von Nudge **und** Heartbeat gesetzt wird und ein **einziger** Fehlversuch
sofort auf 250 ms zurückschaltet; (3) Heartbeat länger als 25 s aus → aktives `close()` +
Reconnect (bewusste Umkehr des heutigen „KEIN Force-Close bei Stille", der nur galt, weil der
250-ms-Poll alles abfing). Takt: `FALLBACK_MS = gesund ? 4000 : 250`, Startwert 250 ms.
Gesteuert über `court_monitor.push_fallback_slow`, **Default aus**.

**S7 — Schmaler Abruf je Feld.** `GET /health?court=<id>` am Host und
`GET /{ns}/health?court=<id>` am Relay: dieselbe Struktur, nur das eine Feld, aus dem Cache.
Der Nudge bleibt datenlos. Unbekannte oder ungültige ID → `courts: []`, HTTP 200.

**Ausbaustufe (nicht Teil dieser Spec).** Nutzlast im Nudge wird gebaut, **wenn** die
Nachmessung zeigt: Übersichts-TVs ziehen weiterhin mehr als 20 Requests/s je Gerät **oder** der
Pi verliert mehr als 10 Frames/min. Dann als eigener ADR, mit der Vorgabe, die Nutzlast **am
Sender** aus derselben Quelle abzuleiten, aus der die Voll-Route liest (im Relay also erst nach
Stale-Verwerfen, „leerer Spiegel überschreibt nicht" und Größengrenze).

## Vorher-Messung (S0)

**Erster Lauf: 19.08.2026**, v0.9.235, LAN, laufende Übertragung mit belegten Feldern
(26 Felder, Antwortgröße 15,9 KB), 20 simulierte Feld-Übersichten, 61 s, `--trocken`.

| Kennzahl | Gemessen | Bemerkung |
|---|---|---|
| `/health`-Abrufe/s (20 Anzeigen) | **72,7/s** | 3,6/s je Anzeige — der 250-ms-Fallback, praktisch ungebremst |
| `/health`-Bytes/s | **1,13 MB/s** | 69 MB in 61 s |
| Antwortgröße | **15,9 KB** | Schätzung war 10–30 KB — trifft |
| `overview`-Bauten/s | **74,0/s** | 4524 Bauten, davon 76 **nicht** von `/health` (Desktop, Kombi) |
| `overview_build_ns` Ø / p95 / max | **1,20 ms / 4,19 ms / 16,7 ms** | je Abruf, jedes Mal neu |
| `persist_scores` Ø | **20,0 ms** | je Punkt, **synchron im async-Handler** |
| `health_push` / Latenz Punkt → Anzeige | — | Trockenlauf, siehe unten |

**Was die Zahlen tragen — und was nicht:**

- Die Kernaussage der Spec ist belegt: Zwanzig Anzeigen kosten **1,13 MB/s und 74
  Vollberechnungen je Sekunde** für eine Information, die sich um wenige Byte ändert.
  Der Antwortcache (S1) zielt genau auf die 74 Bauten, der schmale Abruf (S7) auf die
  15,9 KB.
- **`persist_scores` ist mit 20 ms je Punkt der teuerste Einzelposten** und läuft
  synchron im async-Handler — die Annahme hinter S2 ist damit bestätigt, sogar
  deutlicher als erwartet.
- ⚠️ **Debug-Build.** Gemessen wurde ein `tauri dev`-Lauf; ein Release-Build ist bei
  Rust typisch um ein Vielfaches schneller. Die **Zeiten** (1,20 ms Bau, 20 ms
  Schreibvorgang) sind deshalb pessimistisch, die **Zählwerte** (Abrufe, Bytes,
  Bauten) nicht — die hängen nicht am Optimierungsgrad. Die Nachmessung muss im
  selben Modus laufen, sonst vergleicht sie zwei verschiedene Dinge.
- ⚠️ **Trockenlauf, deshalb keine Push-Spalte.** Es war kein simuliertes Tablet
  verbunden (das hätte Felder belegt und erfundene Stände in den öffentlichen
  Liveticker geschrieben). Damit fehlen `health_push`, die Nudge-Rate und die
  Latenz Punkt → Anzeige. Sie brauchen einen **Probeaufbau ohne Liveticker**.
- ⚠️ **Loopback, kein WLAN.** Die 1,13 MB/s sind gerechnet, nicht im Hallen-Funk
  gemessen; die Sättigung aus dem Zielbild bleibt offen.
- ⚠️ **Leicht zu hoch.** Beim Messlauf sendeten `tablet.html` (Uhr-Synchronisation,
  alle 30 s je Tablet) und `tv.html` (Hallen-Menü) noch kein `src` und landeten damit
  samt vollem Rumpf in `health_poll` — obwohl sie keine Anzeige aktualisieren. Seit
  v0.9.238 senden beide `src=check` und zählen gar nicht mehr mit. Bei zwanzig Tablets
  waren das grob 0,7 Abrufe/s der oben gezeigten 72,7.

Ablauf für den nächsten (vollen) Messlauf:

1. bts-light starten, BTP verbinden, Übertragung starten, Felder belegen lassen.
2. `node scripts/last-monitor.mjs --base http://<turnier-pc>:8088/ --dauer 120`
   (LAN) und einmal gegen `https://badhub.de/bts-relay/<install-id>/` (Cloud).
3. Auf einem Pi zusätzlich `localStorage.ovRenderMessen = "1"` setzen und die
   Konsolen-Sammelzeile mitschreiben.
4. Die Werte unten eintragen — Client-Sicht aus der Skript-Zusammenfassung,
   Server-Sicht aus `/debug/perf` bzw. der Log-Zeile `Perf-Anzeigen (…)`.

| Kennzahl | LAN vorher | Cloud vorher | Quelle |
|---|---|---|---|
| `/health`-Abrufe/s gesamt | **72,7** (20 Anz., Debug) | | LAN: `/debug/perf` · Cloud: **nur** Skript |
| davon `push` / `poll` | 0 / 72,7 *(Trockenlauf)* | | LAN: `/debug/perf` · Cloud: — |
| `/health`-Bytes/s | **1,13 MB/s** | | Skript (beide) |
| `/court/{id}/state`-Abrufe/s | 0 *(keine Court-Monitore)* | | LAN: `court_state_*` · Cloud: **nur** Skript |
| `overview_build_ns` p95 | **4,19 ms** (Ø 1,20, max 16,7) | | `/debug/perf` (beide) |
| `persist_calls`/s | Ø **20,0 ms** je Vorgang | | `/debug/perf` (beide) |
| Nudges/s | *(Trockenlauf)* | | `nudges_sent` (beide) |
| Latenz Punkt → Anzeige p50/p95 | *(Trockenlauf)* | | Skript (beide) |
| Pi: Renders/s, Ø, über 16 ms | | | `ovRenderMessen` |

**Grenze der Host-Zähler im Cloud-Betrieb:** Dort bedient der Relay `/health` und
`/court/{id}/state`, nicht der Turnier-PC — `health_*` und `court_state_*` bleiben
strukturell null, während `nudges_sent`, `persist_calls` und `overview_builds` normal
weiterzählen. Die Cloud-Spalte dieser beiden Zeilen füllt deshalb **nur** das Lastskript
(Client-Sicht, gegen die Relay-Adresse gefahren). Den Relay selbst zu instrumentieren ist
bewusst nicht Teil von S0; sollte sich die Cloud-Seite als der interessantere Fall
erweisen, wäre das eine eigene kleine Etappe.

Beim Ablesen zu beachten: `overview_builds` ist absichtlich **größer** als die Zahl der
`/health`-Abrufe — `overview()` speist auch die Kombi-Anzeige (`/combo/state`), die
Desktop-Oberfläche (`tablet_info`) und die Hallen-Kurzlinks. Kein Messfehler, sondern
der Punkt: Der Antwortcache aus S1 entlastet all diese Aufrufer, nicht nur `/health`.

Ergibt der Lauf ein deutlich anderes Bild als die Schätzung (4–7 Punkte/s turnierweit,
10–30 KB je `/health`-Antwort), werden die Zielwerte oben einmalig nachgezogen und die
Änderung hier vermerkt — so in „Offene Punkte / Annahmen" festgelegt.

## Akzeptanzkriterien

**Messung (S0)**
- [x] Die Vorher-Messung liegt als Tabelle in dieser Spec vor, getrennt nach `health_push`,
      `health_poll`, `overview_build_ns` p95, `persist_calls`/s, Bytes/s und Latenz p50/p95.
      *(Lauf vom 19.08.2026, LAN. Offen bleiben die vier Werte, die ein sendendes Tablet
      brauchen — `health_push`, Nudge-Rate, Latenz — sowie die Cloud-Spalte und die
      Pi-Zeile; sie brauchen einen Probeaufbau ohne Liveticker.)*
- [x] Ein Abruf ohne `src`-Parameter (alte Seite) wird als `poll` gezählt, nie als `push`.
      *(`Quelle::aus_query`, Test `zaehler_ohne_src_zaehlt_als_poll`; auch `""` und
      Unbekanntes zählen als `poll`.)*
- [x] `/debug/perf` enthält ausschließlich Zahlen — keine Namen, keine Match- oder Spielerdaten.
      *(Wächter `debug_perf_enthaelt_keine_personendaten` prüft die Struktur des
      serialisierten Snapshots; mit einem eingebauten Textfeld gegengeprüft.)*
- [x] `/debug/perf` existiert nur am LAN-Server, nicht am Relay.
      *(Route nur in `tablet/server.rs`; an `relay/` wurde nichts geändert.)*

**Antwortcache (S1)** — umgesetzt v0.9.236
- [x] Zwei `/health`-Abrufe ohne dazwischenliegende Änderung bauen den Zustand genau einmal.
      *(`zwei_abrufe_ohne_aenderung_bauen_den_zustand_nur_einmal`, gemessen über den
      S0-Zähler `overview_builds`; mit abgeschalteter TTL gegengeprüft.)*
- [x] Die Cache-Antwort ist zeichengleich mit dem Direktbau.
      *(`die_cache_antwort_traegt_dieselben_felder_wie_der_direktbau` — beide Wege bauen
      denselben Umschlag, nur `serverNowMs` ist naturgemäß neu.)*
- [x] Ein Nudge, ein neuer BTP-Snapshot und ein Config-Schreibvorgang invalidieren je sofort.
      *(je ein Test; die Config-Quelle meldet an drei Stellen — `ServerCtx::mutate_app_config`,
      `commands::mutate_config`, `save_config`.)*
- [x] Nach 250 ms ohne Invalidierung wird trotzdem neu gebaut (Hart-TTL).
- [x] Bei kaltem Cache liefert `/health` denselben Inhalt wie heute (kein Leerstand, kein Fehler).
      *(der kalte Weg **ist** der Direktbau — der Cache sitzt davor, nicht dazwischen.)*
- [x] Ein unveränderter Zustand beantwortet den Fallback-Poll mit HTTP 304.
      *(`ein_unveraenderter_stand_wird_mit_304_bestaetigt`, samt Gegenprobe: nach einem
      Nudge gilt die alte Marke nicht mehr.)*

**Zwei Dinge, die an S1 hingen und in derselben Etappe mitkamen:**

- **Der Uhr-Versatz wird beim Empfang gesetzt, nicht im Render.** `render` läuft auch mit
  gemerkten Daten (Hallen-Rotation), deren `serverNowMs` längst alt ist — der Versatz
  liefe sonst mit jedem solchen Render weiter zurück.
- **Die Aufruf-Uhr bekam einen eigenen Sekundentakt.** Sie lief bisher beiläufig mit,
  weil viermal je Sekunde ein voller Stand kam und die Seite komplett neu zeichnete.
  Sobald ein unveränderter Stand mit `304` beantwortet wird, gibt es keinen Anlass mehr
  zu zeichnen — die Minutenzahl wäre stehengeblieben. Der Takt läuft nur, solange
  überhaupt eine Uhr sichtbar ist, und ein Render je Sekunde bleibt deutlich sparsamer
  als die vier davor.

Der Relay bleibt in dieser Etappe unberührt: Er beantwortet `/health` im Cloud-Betrieb
selbst und kennt weder Marke noch Cache. Eine Seite, die `If-None-Match` schickt, bekommt
dort weiterhin die volle Antwort ohne Marke und verhält sich wie bisher.

**Entprellung (S2)** — umgesetzt v0.9.237
- [x] Drei Punkte innerhalb einer Sekunde erzeugen genau einen Schreibvorgang.
      *(`drei_punkte_in_einer_sekunde_ergeben_einen_schreibvorgang`; der Punkt selbst
      schreibt gar nicht mehr — `ein_gezaehlter_punkt_schreibt_nicht_mehr_sofort`.)*
- [x] Ein Ergebnis-Eintrag, eine Match-Räumung und das Stoppen der Übertragung schreiben
      **synchron**, bevor sie zurückkehren.
      *(`eine_match_raeumung_schreibt_synchron`; alle Ergebnis-Pfade — `enter_result`,
      `disqualify_match`, der Tablet-Weg in `server.rs` und der TL-Weg in `tl.rs` —
      laufen über `clear_court`. `stop_sync` flusht **vor** dem Abbrechen des
      Sync-Tasks, das App-Ende über `beenden()` in `lib.rs`.)*
- [x] Ein inhaltsgleicher Stand schreibt gar nicht.
      *(`ein_inhaltsgleicher_stand_schreibt_gar_nicht` — Fingerabdruck über das fertige
      JSON; ohne ihn schriebe der Sekundentakt einen ganzen Turniertag durch.)*
- [x] `live-scores.json` wird weiterhin atomar geschrieben (Temp-Datei, dann Rename) und ist
      nach einem App-Neustart bitgleich lesbar wie vor der Änderung.
      *(`die_datei_bleibt_neustartfest_lesbar` — Format unverändert, keine Temp-Datei
      bleibt liegen. Rollback bleibt damit zustandsfrei.)*
- [x] Nach einem simulierten Absturz mit verlorenem Puffer stellt der Tablet-Reconnect
      (`state_sync`) den Stand wieder her.
      *(`ein_verlorener_puffer_wird_vom_tablet_geheilt` — der Test hält den akzeptierten
      Verlust ausdrücklich fest und zeigt die Heilung.)*

**Wo der Takt sitzt:** im Sync-Loop (`commands.rs`), der die Wartezeit bis zum nächsten
BTP-Abruf in Sekundenschritten absitzt und nach jedem Schritt flusht. Dort statt in
`run_once`, weil dieser Loop in **beiden** Betriebsarten läuft und der Takt so unabhängig
von den BTP-Frühausstiegen bleibt. Geschrieben wird in `spawn_blocking`, damit die
gemessenen 20 ms keinen Async-Worker belegen.

**Zuweisungs-Nudge (S3)** — umgesetzt v0.9.238
- [x] Eine neue Court→Match-Zuordnung nudgt genau das betroffene Feld, kein anderes.
      *(`ein_snapshot_mit_neuer_zuweisung_nudgt_genau_dieses_feld` — beim Feldwechsel
      sind es genau zwei: das frei gewordene und das neu belegte.)*
- [x] Ein unveränderter Snapshot löst keinen Nudge aus.
      *(`ein_unveraenderter_snapshot_nudgt_nicht` — sonst weckte jeder BTP-Poll alle
      Anzeigen aller Felder.)*
- [x] Eine Räumung nudgt. *(`eine_raeumung_im_snapshot_nudgt`)*
- [x] Ein BTP-Satzstand-Sprung ohne Tablet-Beteiligung nudgt (kein stiller Sprung mehr).
      *(`ein_btp_satzstand_sprung_nudgt`. Deshalb steht der Satzstand mit in der
      Projektion, nicht nur `match_id`: Ein von Hand in BTP eingetragener Stand ändert
      sonst nichts, was der Vergleich sähe. Ein Punkt **vom Tablet** taucht dort nie auf —
      `set_snapshot` legt den rohen BTP-Stand ab, `apply_tablet_scores` arbeitet danach
      auf einer eigenen Kopie, und das Tablet nudgt ohnehin selbst.)*
- [x] Am Relay nudgen `MatchAssigned` und `MatchCleared`, ein erneutes gleiches Match nicht.
      *(`match_assigned_nudgt_die_anzeigen` · `match_cleared_nudgt_die_anzeigen` ·
      `dasselbe_match_erneut_nudgt_nicht` — jeweils **nach** dem Cache-Insert bzw. der
      Räumung, damit die geweckte Anzeige nicht den Stand von vorher holt.)*
- [ ] Zuweisung → sichtbare Anzeige dauert p95 ≤ 1 s, auch bei aktivem 4-s-Fallback.
      *(Messwert — der 4-s-Fallback kommt erst mit S6. Gehört in die Nachmessung.)*

**Damit sind beide offenen A1-TODOs geschlossen** (an `monitor_socket` im Host und
`monitor_conn` im Relay): Die Match-Zuweisung war der letzte Anzeige-Wechsel, für den
allein der Poll-Fallback die Latenz abdeckte.

**Ordnung (S4)** — umgesetzt v0.9.239
- [x] `/health` und `/court/{id}/state` tragen je Feld ein `seq`; die Cloud-Pendants ebenso.
      *(`die_ordnungszahlen_stehen_neben_der_feldliste`, `MonitorState.seq` über
      `MonitorCourt`, und im Relay dieselbe Form wie in der LAN-Antwort — geprüft in
      `cloud_overview_health_lists_courts_with_match_and_score`.)*

      **In `/health` steht die Zahl neben der Feld-Liste, nicht darin** (`seqs`:
      CourtID → Zahl). Die Marke der Antwort ist ein Streuwert über die Liste, und die
      Zahlen steigen bei **jedem** Anstoß — auch bei einem, der die Anzeige nicht
      verändert (etwa ein Tablet-Abgleich mit unverändertem Anzeige-Stand). Steckten sie
      in der Liste, wechselte die Marke jedes Mal, und die Bestätigung ohne Nutzdaten aus
      S1 wäre wirkungslos — auf genau der Strecke, die sie entlasten soll. Ein
      Regressionstest hält fest, dass ein Anstoß ohne Inhaltsänderung die Feld-Liste
      zeichengleich lässt.

      Gelesen werden die Zahlen **vor** dem Inhalt und im selben Bau zwischengespeichert.
      So ist eine Zahl höchstens *älter* als der Stand, zu dem sie ausgeliefert wird —
      die harmlose Richtung. Umgekehrt merkte sich die Anzeige eine Zahl für einen Stand,
      den sie nie gesehen hat, und verwürfe den zugehörigen Nudge als „schon bekannt".
- [x] Eine Voll-Antwort mit kleinerem `seq` als dem angezeigten wird verworfen.
      *(`monitor.html`: `applyState` bricht ab. In `overview.html` **noch nicht** —
      sie rendert bis S5 immer alle Felder auf einmal, eine feldweise Entscheidung ist
      erst mit dem Teil-Patch möglich. Bis dahin führt sie die Zahlen nur mit.)*
- [x] Eine Voll-Antwort mit **gleichem** `seq` wird angewendet (BTP-Rückfall-Fall).
      *(`anwenden` nutzt `>` für Push und `>=` für Abruf —
      `scripts/test-monitor-seq.mjs`, eigener CI-Schritt.)*
- [x] Ein doppelter oder veralteter Push wird verworfen.
- [x] Nach einem Server-Neustart springt `seq` nicht zurück (Seeding über `now_ms()`).
      *(`die_feld_sequenz_startet_neustart_fest`; im Relay dasselbe Seeding, damit ein
      Deploy die Cloud-Anzeigen nicht hängen lässt.)*
- [x] Serde-Roundtrip: ein `MonitorState` **ohne** `seq` (alter Server) deserialisiert
      fehlerfrei zu `seq = 0`. *(`ein_monitor_state_ohne_seq_bleibt_lesbar`.)*

**Warum die Regel für Push und Abruf verschieden ist:** Ein Push gilt nur bei
`seq > gezeigt` — ein doppelter Nudge löste sonst einen zweiten Abruf ohne neuen Inhalt
aus. Eine Voll-Antwort gilt schon bei `seq >= gezeigt`, weil sie denselben Stand
berichtigen darf, den der Nudge angekündigt hat: Nimmt jemand in BTP einen Satzstand von
Hand zurück, ändert sich der Inhalt, ohne dass die Zahl steigt. `seq = 0` heißt „keine
Ordnung bekannt" (älterer Absender) und blockiert nie — eine eingefrorene Anzeige wäre
der schlimmere Fehler.

Die Regel liegt als kanonisches Modul in `src/io/monitorSeq.mjs` mit eigenem CI-Schritt;
beide Anzeige-Seiten tragen eine Inline-Kopie (die Assets durchlaufen keinen Build).

**Render-Patch (S5)** — umgesetzt v0.9.240
- [x] Eine reine Punktänderung patcht nur die betroffene Karte; das übrige Board wird nicht
      angefasst (nachweisbar über eine unveränderte DOM-Referenz einer Nachbarkarte).
      *(`patcheKarten` tauscht ausschließlich die beiden `.trow__sets`-Spalten der
      betroffenen Karten; alle übrigen Knoten bleiben dieselben Objekte.)*
- [x] Nicht gepatcht, sondern voll gerendert wird bei: Satzwechsel, Match-Wechsel, geänderter
      Feld-Menge oder -Reihenfolge, geänderter Halle oder Hallen-Farbe, gewechselter
      Sichtbarkeit des Aufruf-Chips, Wechsel von `isLive`/`injury`/`official_call`, sowie wenn
      die Karte gar nicht im DOM ist (Hallen-Rotation).
      *(`istPatchbar` in `src/io/courtPatch.mjs` — je Bedingung ein Testfall, dazu
      Feldname, Runden-/Gruppen-Beschriftung, Spielernamen und Nationen. `patcheKarten`
      prüft zusätzlich `isConnected`. 32 Prüfungen, eigener CI-Schritt.)*
- [x] Bei Hallen-Rotation zeigt die Karte nach dem Umschalten den gepushten Stand
      (`lastData` wurde fortgeschrieben), nicht den Stand vom letzten Voll-Render.
      *(Der Patch-Zweig schreibt `lastData` selbst fort.)*
- [x] Spätestens alle 30 s läuft ein voller Render aus einem vollen Abruf.
      *(`VOLL_RENDER_SPAETESTENS_MS`; dazu erzwingen Hallen-Rotation und der
      Sekundentakt der Aufruf-Uhr je einen Neubau — die Uhr steht im Kopf der Karte
      und bliebe sonst stehen.)*
- [x] In `monitor.html` wird nicht gepatcht, solange `redirectTo` gesetzt, die Werbeansicht
      aktiv, das Gerät unzugewiesen oder die Match-ID abweichend ist.
      *(Strukturell erfüllt und bewusst **nicht** umgebaut: `monitor.html` wirft nie ein
      Board weg — `renderMatch` setzt Texte an bestehenden Elementen und erzeugt auf
      107 Zeilen sieben neue Knoten. Die genannten Sonderzustände verlassen `applyState`
      **vor** `renderMatch`. Ein Teil-Patch brächte dort nichts: Der feste Court-Monitor
      ist laut Analyse der billige Fall, der Schmerz liegt bei der Feld-Übersicht.)*

**Die Zuständigkeitsgrenze ist bewusst streng.** Ein zu vorsichtiges „nein" kostet einen
Neubau, den es vorher ohnehin bei jedem Abruf gab; ein zu großzügiges „ja" hinterlässt eine
Karte, die dauerhaft etwas Falsches zeigt — und das fällt im Turnier niemandem auf, der es
nicht weiß. Deshalb gehen auch Spielernamen und Nationen in den Vergleich ein, obwohl sie
sich bei gleicher Match-ID nicht ändern *sollten*.

**Gesundheit und Fallback (S6)**
- [ ] Bei gesetztem `push_fallback_slow` und gesundem Kanal beträgt der Fallback-Takt 4 s.
- [ ] Eine ruhige Halle ohne jeden Punkt bleibt dank Heartbeat gesund — die Anzeige friert nicht.
- [ ] Bleibt der Heartbeat länger als 25 s aus, gilt der Kanal als tot: Takt sofort 250 ms und
      aktiver Reconnect.
- [ ] Ein **einziger** fehlgeschlagener Abruf schaltet sofort auf 250 ms.
- [ ] Der Server sendet den Heartbeat mindestens alle 10 s; das Frame enthält **kein**
      `court`-Feld, damit alte Seiten es folgenlos verwerfen.
- [ ] Default `push_fallback_slow = false`: eine frisch aktualisierte Installation verhält sich
      exakt wie vorher.
- [ ] Das Monitor-Lebenszeichen (`MONITOR_ONLINE_WINDOW_MS = 20 s`) bleibt auch bei 4-s-Takt
      erhalten — kein Gerät erscheint fälschlich offline.

**Schmaler Abruf (S7)**
- [ ] `/health?court=<id>` liefert genau ein Feld, inhaltlich identisch zu dessen Eintrag in der
      vollen Antwort.
- [ ] `/health` ohne Parameter ist unverändert.
- [ ] Unbekannte, negative oder nicht-numerische ID → `courts: []` mit HTTP 200; die Antwort
      unterscheidet sich nicht danach, ob das Feld existiert (kein Existenz-Leck).
- [ ] Am Relay respektiert der Selektor die Namespace-Isolation: ein Feld eines fremden
      Namespace ist nicht abrufbar.

**Verträglichkeit (alle Etappen)**
- [ ] Alter Relay + neue Seite: datenloser Nudge, kein Heartbeat, `?court=` ignoriert → die
      Seite arbeitet wie heute weiter.
- [ ] Neuer Relay + alte Seite: Zusatzfelder und Heartbeat werden ignoriert, keine Fehlfunktion.
- [ ] LAN und Cloud zeigen bei identischem Zustand identische Inhalte (je Etappe geprüft).
- [ ] Eine Anzeige in einer fernen Halle hinter `slave_bridge.rs` verhält sich identisch zur
      Cloud-Anzeige der Haupthalle.

**Erfolg**
- [ ] Die Nachmessung erreicht die Zielwerte aus „Zielbild & Erfolgskriterien"; Abweichungen
      werden in dieser Spec begründet.
- [ ] Ein voller Turniertag ohne hängende, springende oder eingefrorene Anzeige.

## Tests

**Rust-Unit-Tests (TDD, jeweils vor der Umsetzung geschrieben)**

| Etappe | Tests |
|---|---|
| S0 | `zaehler_trennt_push_und_poll` · `zaehler_ohne_src_zaehlt_als_poll` · `overview_build_ns_steigt_je_direktbau` · `debug_perf_enthaelt_keine_personendaten` (Wächter, Muster `the_state_never_carries_personal_data_beyond_its_purpose` in `tl.rs`) |
| S1 | `cache_liefert_identisches_json_wie_direktbau` · `zwei_abrufe_bauen_nur_einmal` · `nudge_invalidiert` · `snapshot_wechsel_invalidiert` · `config_wechsel_invalidiert` · `ttl_250ms_invalidiert` · `kalter_cache_faellt_auf_direktbau_zurueck` · `unveraenderter_stand_liefert_304` |
| S2 | `record_score_schreibt_nicht_sofort` · `flush_schreibt_genau_einmal_fuer_drei_punkte` · `inhaltsgleicher_stand_schreibt_nicht` · `ergebnis_eintrag_flusht_synchron` · `stop_flusht` · `datei_bleibt_atomar_temp_dann_rename` |
| S3 | Host: `snapshot_mit_neuer_zuweisung_nudgt_genau_dieses_feld` · `unveraenderter_snapshot_nudgt_nicht` · `raeumung_nudgt` · `btp_satzstand_sprung_nudgt`. Relay: `match_assigned_nudgt` · `match_cleared_nudgt` · `gleiches_match_erneut_nudgt_nicht` |
| S4 | `health_traegt_seq_je_feld` · `monitor_state_traegt_seq` · `seq_steigt_mit_jedem_nudge` · `seq_startet_neustart_fest_ueber_now_ms` · `relay_overview_health_traegt_seq` · Serde-Roundtrip `MonitorState` mit und ohne `seq` |
| S6 | `monitor_ws_sendet_heartbeat_alle_10s` · `heartbeat_frame_hat_kein_court_feld` · `relay_monitor_conn_sendet_heartbeat` · `push_fallback_slow_default_false` |
| S7 | `health_mit_court_liefert_genau_ein_feld` · `health_mit_unbekanntem_court_liefert_leere_liste` · `health_ohne_court_unveraendert` · `nicht_numerischer_court_wird_abgewiesen_ohne_leck` · `relay_health_mit_court_respektiert_namespace_isolation` |

**Client-Tests.** Es gibt keinen JS-Testrahmen; das Haus-Muster ist: kanonische Fassung in
`src/io/*.mjs`, `node:assert`-Test unter `scripts/test-*.mjs`, eigener CI-Schritt,
**Inline-Kopie im HTML-Asset** mit Verweis-Kommentar (belegt an `gamePoint.mjs`/`hallGrid.mjs`,
die Assets durchlaufen keinen Build und können keine Module laden). Drei Extraktionen:

1. `src/io/pushHealth.mjs` — `pushGesund({wsOpen, lastServerFrameMs, lastFetchOk, failures}, nowMs)`.
   Höchstes Risiko im ganzen Vorhaben: eine falsche Regel friert alle Anzeigen ein.
   Test: ruhige Halle mit Heartbeat bleibt gesund · Heartbeat-Ausfall > 25 s → ungesund + 250 ms ·
   ein Fehlversuch → sofort ungesund · `wsOpen=false` → ungesund · Kaltstart → 250 ms.
2. `src/io/courtPatch.mjs` — `istPatchbar(vorher, nachher, brettSignatur)`, alle
   Zuständigkeitsbedingungen als reine Funktion; Test je Bedingung ein Fall.
3. `src/io/monitorSeq.mjs` — `anwenden(gezeigtSeq, seq, quelle)` mit `>` für Push, `>=` für Fetch.

`scripts/check-asset-syntax.mjs` deckt die Inline-Kopien syntaktisch ab (läuft bereits).

**Pflichtläufe:** `cargo test` grün, `cargo clippy --workspace --all-targets` sauber,
`cargo fmt --check`, `npm run build` fehlerfrei, die drei neuen `node`-Tests im CI.
**Manuell:** `scripts/last-monitor.mjs` vor und nach der Umsetzung (in `docs/regression-suite.md`
als manueller Lauf vermerken, Muster ADR 0019), dazu ein Turnier-Feldtest.

## Risiken & Rollback

| Risiko | Wirkung im laufenden Turnier | Gegenmaßnahme |
|---|---|---|
| Falsche Gesundheits-Definition (S6) | alle Anzeigen frieren ein, bei halbtotem Socket unbegrenzt | Heartbeat + „ein Fehlversuch → 250 ms" + Force-Close nach 25 s; **Config-Schalter, Default aus** |
| Antwortcache liefert veraltet (S1) | falscher Stand auf allen TVs gleichzeitig | ereignisgetriebene Invalidierung **plus** 250-ms-Hart-TTL; Cache ist Beschleuniger, nicht Wahrheit |
| Entprellung verliert Stand (S2) | bis ~1 s Punktstand nicht auf Platte | Tablets sind die Wahrheit, `state_sync` heilt; synchroner Flush an allen kritischen Punkten |
| Teil-Patch zeigt Zombie-Stand (S5) | eine Karte hängt | Zuständigkeit als getestete reine Funktion; Zwangs-Voll-Render alle 30 s |
| Nudge-Sturm bei jedem BTP-Sync (S3) | Nudge-Flut | nur bei tatsächlich geänderter Projektion nudgen; 60-ms-Coalescing im Client greift ohnehin |
| Mischstand Relay/App | eingefrorene oder falsche Anzeige | additiver Kontrakt (ADR 0035 c) + Verträglichkeits-AK je Etappe |

**Rollback ist zustandsfrei:** kein Plattenformat ändert sich (`live-scores.json` bleibt
bitgleich), keine Wire-Semantik wird umgedeutet, alles Neue ist optional. App per Auto-Update auf
den Vorgänger-Tag; Relay per Revert-Merge (deployt automatisch); S6 per Config-Schalter ohne
beides. Relay-Merges nur außerhalb von Spielzeiten; die App wird nie während eines Turniers
aktualisiert.

## Auslieferung

**Reihenfolge zwingend Relay → App**, weil der Relay bei jedem main-Merge deployt und die App
erst mit dem Release-Tag: zuerst die Relay-Teile von S3/S4/S6/S7 samt Aufschlag-Füllung (jeder
gegen alte App **und** alte Seiten lauffähig), dann die App-Etappen mit Tag (neue Seiten gegen
alten Relay lauffähig). Version je app-relevanter Etappe gemeinsam bumpen
(`src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `package.json`; aktuell 0.9.225); reine
Relay-Etappen brauchen keinen App-Bump.

## Reviews

`code-reviewer` nach **jeder** Etappe (Schwerpunkte: S1 Invalidierungslücken und
Lock-Reihenfolge, S2 Flush-Vollständigkeit, S4 Serde-Rückwärtsverträglichkeit, S5
Zuständigkeits-Matrix, S6 Einfrier-Gefahr). **`security-reviewer` für S6 und S7:** der Relay
steht im Internet, `?court=` ist erstmals ein von außen gesteuerter Selektor auf der bislang
parameterlosen `overview_health`-Route, und der Heartbeat ist ein neuer serverinitiierter
Fan-out über bis zu 256 Abonnenten je Namespace.

## Doku-Pflicht im selben Commit

`CLAUDE.md` (neue Tabellenzeile) · [`docs/court-monitor.md`](../court-monitor.md) (Heartbeat,
Gesundheitsdefinition, Fallback, Antwortcache, `?court=`) ·
[`docs/cloud-relay.md`](../cloud-relay.md) (Heartbeat-Frame, `seq`, gefüllter Aufschlag,
Zuweisungs-Nudge) · [`docs/adr/0035-monitor-livestand-ordnung.md`](../adr/0035-monitor-livestand-ordnung.md)
und Statuswechsel in [`0016`](../adr/0016-monitor-push-transport.md) ·
`docs/regression-suite.md` · `docs/changelog.md` · `docs/roadmap.md` (beide A1-TODOs streichen) ·
[`docs/multi-hall.md`](../multi-hall.md) (ferne Halle = nur Cloud) · `docs/logging.md` (Perf-Zeile).

## Offene Punkte / Annahmen

- **Annahme:** Die Zielwerte in „Erfolgskriterien" sind aus einer Schätzung abgeleitet
  (4–7 Punkte/s turnierweit, 10–30 KB je `/health`-Antwort). Ergibt die Vorher-Messung ein
  deutlich anderes Bild, werden die Zielwerte einmalig nachgezogen und die Änderung hier vermerkt.
- **Bekannte Restlücke:** `injury` und `official_call` stammen aus `record_alert` und erreichen
  den Relay überhaupt nicht; im Cloud bleiben sie `false`. Das ist Bestandsverhalten, wird von
  dieser Spec nicht behoben und ist nicht Teil des Kontrakts.
- **Offen bis zur Nachmessung:** ob die Ausbaustufe „Nutzlast im Nudge" gebaut wird. Das
  Auslösekriterium steht oben; die Entscheidung bekommt dann einen eigenen ADR.
