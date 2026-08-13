# 0016 — Monitor-Push-Transport: WebSocket-Nudge statt SSE oder Poll

- **Status:** proposed
- **Datum:** 2026-08-13

Gehört zu [docs/features/turnier-robustheit.md](../features/turnier-robustheit.md)
(Paket A / A1).

## Kontext

Court-Monitor (`monitor.html`) und Feld-Übersicht (`overview.html`) zeigen den
Spielstand mit **bis 1 s bzw. 5 s Verzögerung**, weil sie **pollen**
(`monitor.html:1032`, `overview.html:544`), während Tablet→Server/Relay bereits
**Push** (WebSocket) ist. Für einen niedrig-latenten Weg (<200 ms) braucht es
einen Server→Anzeige-Push. Der muss LAN (eingebetteter Server) **und** Cloud
(Relay hinter nginx `/bts-relay/`) können (R3), im laufenden Turnier robust sein
und darf keinen Regress erzeugen.

## Entscheidung

**WebSocket-„Nudge".** Server und Relay erhalten je eine neue Monitor-WS-Route
(LAN `/monitor-ws?court={id}` bzw. Cloud `/{ns}/monitor-ws?court={id}`; die
CourtID steht im Query, nicht im Pfad — fehlt sie, abonniert der Client alle
Felder für die Übersicht) und eine Subscriber-Registry je Court.
Bei Score-/Zustandsänderung (`record_score` / `forward_score`) wird an die Subs
des betroffenen Courts ein **winziger Nudge** gesendet: „Court X geändert, seq N".
Der Client löst daraufhin **seinen bestehenden `fetch`** auf die schon vorhandene
Poll-Route aus.

Damit bleibt der Poll-Endpunkt die **einzige** Datenquelle (eine Serialisierung,
ein Renderpfad); es gibt keinen zweiten Datenpfad, der flackern könnte. Der Client
führt einen monotonen `lastSeq` je Court (veraltete Nudges verwerfen) und
**pausiert das Intervall-Poll, solange Nudges eintreffen** — Poll bleibt reiner
Stille-/Ausfall-Fallback (~250 ms).

## Alternativen

- **SSE (Server-Sent Events):** unidirektional, bräuchte `proxy_buffering off`
  je Push-Location in der nginx-Config — ein Ops-Eingriff außerhalb des Repos mit
  Deploy-Risiko im laufenden Turnier; tote Verbindungen schwerer zu erkennen
  (kein Client-Herzschlag). Kein bestehendes Muster im Code. **Verworfen.**
- **Nur schnelleres Poll (250 ms, kein Push):** null neue Infrastruktur, aber
  250 ms bleibt sichtbar und vervierfacht die Cloud-/Nebenhallen-Requests.
  **Als Interim/Fallback behalten, nicht als Zielarchitektur.**
- **Score inline im Push (statt Nudge):** spart einen `fetch`, erzwingt aber eine
  zweite Serialisierung + Last-Write-Wins-Ordnung über zwei Kanäle (Flacker-
  Risiko). **Zurückgestellt** als spätere Optimierung.

## Konsequenzen

- WS ist durch den Proxy erprobt (Tablet-WS) → R3 ohne Ops-Änderung erfüllt;
  Reconnect-Muster aus `tablet.html` wiederverwendbar.
- Minimaler Neubau: Route + Registry + Broadcast an den Stellen, die Score/
  Zuweisung **schon heute** schreiben; der Renderpfad der Anzeigen bleibt.
- **Negativ:** ein neuer eingehender WS-Kanal → Security-Review nötig
  (Namespace-/CourtID-Validierung, Sub-Limits, Fan-out/DoS). Zusätzliche
  Verbindungen je TV; bei sehr instabilem WLAN häufige Reconnects — abgefedert
  durch den Poll-Fallback (kein Regress).
