# TL-Web-Push: Anstoß-Kanal statt 2-Sekunden-Poll — Spezifikation

> Status: **Entwurf 2026-08-17** (Feldtest-Nachgang „Performance im Blick",
> Schritt 5 des Performance-Plans; Freigabe ausstehend).
> Betroffene Crates: `src-tauri/` (server.rs, state.rs, tl.rs), `relay/`,
> `assets/tl.html`.
> ADR: 0034 (WS-Nudge für TL-Web; löst den Widerspruch zwischen ADR 0016
> und dem How-To der TL-Web-Spec auf).
> Verwandt: [turnierleitung-web.md](turnierleitung-web.md) ·
> `docs/adr/0016-monitor-push-transport.md` (das Transport-Vorbild).

## Kontext / Problem

Die TL-Web-Seite fragt den Zustand alle 2 s per HTTP ab (ETag/304). Das
hat zwei Kosten, die der ETag **nicht** abschöpft:

1. **Anfragen:** ~43.000 HTTP-Roundtrips je Gerät und Turniertag — Akku
   und Funk auf Tablets, auch wenn die Antwort nur „304, nichts Neues"
   ist.
2. **Rechnung am Turnier-PC:** Jede LAN-Anfrage baut den kompletten
   TL-Zustand neu (BTP-Snapshot-Deep-Clone, Sortierung, **zwei** volle
   JSON-Serialisierungen — eine für den Fingerprint, eine für die
   Antwort) plus zwei Platten-Lesevorgänge der Config. Bei 8 Geräten ×
   0,5 Hz sind das ~4 Snapshot-Clones und ~8 Serialisierungen **pro
   Sekunde**, dauerhaft, auch im völlig ruhigen Turnier. Der
   Relay-TICK rechnet zusätzlich alle 2 s `state_for_relay`, bevor sein
   Rev-Gate greift.

Dazu die Latenz: Eine Änderung erreicht ein Gerät im Mittel nach 1 s,
im schlechtesten Fall nach 2 s (Cloud: bis 4 s — Host-TICK + Poll).

## Zielbild

Ein **Anstoß-Kanal** (WebSocket-Nudge) sagt der Seite „es gibt eine
neue Revision" — die Seite holt daraufhin **sofort über den bestehenden
Poll-Pfad** (Auth, ETag, `X-Tl-Active-Profile`, Fehlerbehandlung
unverändert). Der Intervall-Poll wird zur Rückfallebene (30 s statt
2 s, solange der Kanal steht). Zusätzlich rechnet der Turnier-PC den
LAN-Zustand **einmal zentral pro Änderung** statt je Gerät und Anfrage.

**Erfolgskriterien:**

1. Latenz Änderung → Anzeige im LAN < 1 s (heute Ø 1 s, max. 2 s);
   Cloud ≤ heutigem Stand (Host-TICK bleibt der Taktgeber).
2. HTTP-Anfragen je Gerät sinken im ruhigen Betrieb um ≥ 90 %
   (30-s-Fallback + 1 Abruf je echter Änderung statt 0,5 Hz immer).
3. Turnier-PC: höchstens **eine** Zustands-Rechnung pro Änderung (plus
   1-s-Erkennungstakt) statt einer je Gerät und Anfrage.
4. **Kein Verhaltensbruch ohne Push:** Gegen einen alten Host/Relay
   (kein `/tl-ws`) fällt die Seite geräuschlos auf den heutigen
   2-s-Poll zurück. Alte Seiten gegen neuen Host funktionieren
   unverändert.
5. Der 8-Geräte-Platz (Relay, TTL 60 s) verfällt nicht, solange ein
   Gerät nur zuhört: Der Fallback-Poll (30 s) frischt ihn auf.
6. Schreibaktionen bleiben unverändert POST (`/tl/api/command`).

## Nicht-Ziele

- **Kein Daten-Push.** Der Kanal trägt nur `{"rev": n}` — nie den
  Zustand. Eine Wahrheit für Auth/ETag/Kürzung/Profile-Header: der
  bestehende GET-Pfad (Muster ADR 0016, „Anstoß ohne Fracht").
- **Kein SSE.** Erneut verworfen aus demselben Grund wie ADR 0016:
  nginx vor dem Relay puffert (`proxy_buffering off` fehlt und ist ein
  Ops-Eingriff außerhalb des Repos); WebSocket läuft dort erprobt
  (`/{ns}/ws`, `/{ns}/monitor-ws`, `Upgrade`-Map + 3600-s-Timeouts
  stehen schon in `ops/nginx-bts-relay.conf`).
- Kein Ersatz des Host→Relay-Weges (TICK 2 s bleibt der Cloud-Takt).
- Keine neue Client-Klasse in R4: TL-WS-Zuhörer sind **dieselben**
  TL-Geräte (gleicher Zugang, gleicher 8er-Cap), nur ein zweiter,
  lesender Kanal.

## Architektur

### Transport: WebSocket-Nudge, Auth im ersten Frame

- **Route `/tl-ws`** auf LAN-Server **und** Relay (Relay ohne
  Namespace-Pfad — die Zuordnung läuft wie bei allen TL-Routen über den
  Zugang via `tl_index`).
- Browser-WebSockets können keine Header setzen, und der Zugang darf
  nie in eine URL (Log-Hygiene, bestehende Regel „nur Header/Fragment").
  Deshalb **In-Band-Auth**: Der Client sendet als erste Nachricht
  `{"token":"…"}`; der Server prüft (`tl::authorize` bzw. `tl_tokens`)
  **bevor** er abonniert, sonst schließt er die Verbindung. Bis zur
  erfolgreichen Prüfung wird nichts gesendet.
- Danach: Server → Client `{"rev": n}` bei jeder neuen Revision;
  15-s-Ping (LAN) / 30-s-Heartbeat (Relay, wie `monitor_conn`) hält
  nginx-Timeouts und tote Verbindungen im Griff. Client-Nachrichten
  nach der Auth werden ignoriert.
- Fan-out-Deckel je Namespace/Host: 16 (2× Geräte-Cap — Reserve für
  Reconnect-Überlappung); ältester fliegt, wie `subscribe_monitor`.

### Host: zentraler Erkennungstakt + Antwort-Cache

Heute erfährt `tl_revision` eine Änderung erst, wenn jemand den
Fingerprint rechnen lässt. Neu: eine **Host-Task** (Tick 1 s, nur bei
laufender Übertragung und `tl_web.enabled`):

1. baut den Zustand **einmal**, rechnet den Fingerprint, holt die Rev;
2. legt `(rev, etag, json, profil-los)` in einen **Antwort-Cache**
   (`ServerCtx`/`TabletState`);
3. bei neuer Rev: Nudge an alle LAN-`/tl-ws`-Zuhörer.

`GET /tl/api/state` bedient sich aus dem Cache (ETag-Vergleich wie
heute; `X-Tl-Active-Profile` bleibt je Gerät aus der Config). Damit
kostet ein Turnier mit 8 Geräten **eine** Rechnung pro Sekunde statt
vier pro Sekunde — und die Anfragen selbst werden zu Cache-Reads.
Fällt der Cache aus irgendeinem Grund aus (kalt, Übertragung gestoppt),
rechnet der Handler wie heute selbst — der Cache ist Beschleuniger,
nicht Wahrheit.

### Relay: Nudge beim TlState-Empfang

Der Host pusht den Zustand wie heute (TICK 2 s, Rev-Gate). Neu: Wenn
`HostFrame::TlState` mit neuer Rev ankommt, nudgt der Relay seine
`tl_subs` (neue Registry je Namespace, Muster `monitor_subs`).
Host-Abriss (`forget_tl_access`) schließt die Zuhörer **nicht** — ihr
nächster Poll bekommt wie heute 503 und die Seite zeigt „Turnier-PC
nicht verbunden".

### Seite: Kanal auf, Poll gedrosselt

- Beim Start (und nach jedem Abriss mit Backoff 1→2→5→10 s, Muster
  `monitor.html`): `/tl-ws` verbinden, Token-Frame senden.
- Nudge empfangen → `poll()` sofort (durch den ETag ist ein
  überflüssiger Nudge ein 304, harmlos; ein Rev-Sprung um mehrere
  Stufen ist automatisch zusammengefasst).
- Kanal steht → Intervall-Poll auf **30 s** (hält zugleich den
  Relay-Geräteplatz frisch, TTL 60 s); Kanal weg → zurück auf 2 s
  (bzw. 10 s im Hintergrund, wie heute). `visibilitychange` sichtbar →
  sofort poll + Kanal sicherstellen; im Hintergrund darf der Browser
  den Kanal schließen, der Fallback deckt es.
- Verbindungsanzeige: „live" verlangt künftig Kanal **oder** frischen
  Poll-Erfolg — die heutige `failures`-Logik bleibt, nur die
  Erwartungsfrequenz hängt am Modus (30-s-Takt darf nicht als
  „Daten sind 28 s alt" alarmieren, solange der Kanal steht und keine
  Rev verpasst ist).
- Kein `/tl-ws` erreichbar (alter Host, alter Relay, Firmen-Proxy ohne
  WS): geräuschloser Dauer-Fallback auf den heutigen Poll — ein
  einziger Wiederholungsversuch je 60 s, kein Konsolen-Rauschen.

## Sicherheit

- Zugang **nie** in der URL (In-Band-Auth); vor der Auth kein Abo,
  keine Information — auch nicht „Namespace existiert".
- Der Nudge trägt nur die Revisionsnummer — keine Turnierdaten. Ein
  erschlichener Kanal ohne gültigen Zugang existiert nicht; einer mit
  Zugang erfährt nichts, was der Poll nicht auch sagte.
- Deckel: 16 Zuhörer je Namespace/Host; In-Band-Auth-Timeout 10 s
  (Verbindung ohne Token-Frame wird geschlossen); Frame-Größe wie
  bestehende WS-Limits.
- R5 unberührt: Es gibt keinen neuen Schreibweg.

## Akzeptanzkriterien

- [ ] AK-1 LAN: Feld-Ereignis → Nudge → Anzeige in < 1 s (gemessen mit
      `tlRenderMessen` + Netzwerk-Log).
- [ ] AK-2 Ruhiger Betrieb, Kanal steht: höchstens 2 Requests/min je
      Gerät (30-s-Fallback), 0 Nudges.
- [ ] AK-3 Alte Seite ↔ neuer Host und neue Seite ↔ alter Host/Relay:
      Verhalten exakt wie heute (2-s-Poll), keine Fehlerflut in der
      Konsole (max. 1 WS-Versuch je 60 s).
- [ ] AK-4 Falscher/fehlender Token-Frame: Verbindung wird ohne jede
      Server-Aussage geschlossen; gültiger Zugang nach `tl_device_remove`
      wird beim nächsten Auth-Versuch abgewiesen (Kanal überlebt einen
      Entzug höchstens bis zum nächsten Reconnect/Heartbeat-Zyklus;
      Poll-Pfad 401 bleibt die verbindliche Sperre).
- [ ] AK-5 Relay: 9. Gerät wird weiterhin per 429 am **Poll** abgewiesen;
      der WS-Kanal allein hält keinen Platz (der 30-s-Poll tut es).
- [ ] AK-6 Host rechnet bei 8 verbundenen LAN-Geräten im ruhigen Betrieb
      ≤ 1 Zustands-Rechnung/s (Log-/Trace-Beleg), Antworten kommen aus
      dem Cache.
- [ ] AK-7 Host-Neustart: `process_tag`-ETag-Wechsel + Nudge nach
      Neuverbindung führen ohne Nutzereingriff zum frischen Stand.
- [ ] AK-8 nginx unangetastet: Der Kanal läuft über die bestehende
      `Upgrade`-Konfiguration; kein `proxy_buffering`-Eingriff.

## Umsetzungsschritte

1. ADR 0034 (Transportentscheid, Widerspruch 0016 ↔ How-To auflösen).
2. Host: Antwort-Cache + 1-s-Erkennungstask + Nudge-Registry
   (`subscribe_tl`/`notify_tl` in state.rs, Muster Monitor-Nudge) —
   Rust-Tests: Fingerprint-Task nudgt genau bei Rev-Wechsel; Cache
   liefert identische Antwort wie Direktbau; Auth-Gate im WS-Handshake.
3. LAN-Route `/tl-ws` (server.rs, Muster `monitor_socket` + In-Band-Auth).
4. Relay: `tl_subs`-Registry, `/tl-ws`-Route (In-Band-Auth über
   `tl_index`/`tl_tokens`), Nudge aus dem `HostFrame::TlState`-Pfad —
   Relay-Tests nach dem Muster der Monitor-Fanout-Tests.
5. Seite: `tlWsUrl()`, Verbinden/Auth/Backoff/Fallback-Drossel
   (Muster `monitor.html`), Verbindungsanzeige-Anpassung.
6. Doku: `docs/turnierleitung-web.md` (Verhalten), `docs/cloud-relay.md`
   (Route, Frames, Deckel), Changelog; Feldtest-Messpunkte (AK-1/2/6).

## Risiken

| Risiko | Wirkung | Gegenmaßnahme |
|---|---|---|
| Firmen-Proxy blockt WS zum Relay | kein Push im Cloud-Modus | Fallback-Poll bleibt vollwertig (AK-3); Verhalten = heutiger Stand |
| Browser drosselt Hintergrund-WS/Timer | verpasste Nudges | 30-s-Fallback-Poll + Sofort-Poll bei `visibilitychange` |
| Nudge-Sturm bei Punkteregen | Poll je Nudge | ETag macht Überholtes zum 304; Client bündelt Nudges über ein laufendes `poll()` (kein paralleler zweiter Abruf) |
| Cache liefert veraltet nach Config-Änderung | Anzeige hinkt ≤ 1 s | Erkennungstask liest Config je Tick (wie der Relay-TICK heute) |
| WS-Zuhörer-Leck | Speicher | Deckel 16 + Heartbeat-Abräumung, Muster `monitor_conn`/`subscribe_monitor` |
