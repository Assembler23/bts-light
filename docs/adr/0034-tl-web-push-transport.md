# 0034 — TL-Web-Push: WebSocket-Nudge mit In-Band-Auth

- **Status:** proposed
- **Datum:** 2026-08-18

Gehört zu [docs/features/tl-web-push.md](../features/tl-web-push.md).

## Kontext

Die Turnierleitungs-Seite fragt ihren Zustand alle 2 s per HTTP ab
(ETag/304). Das kostet zweierlei, was der ETag **nicht** abschöpft:
~43.000 Anfragen je Gerät und Turniertag (Akku/Funk auf Tablets) und —
schwerer wiegend — eine volle Zustands-Rechnung am Turnier-PC **je Gerät
und Anfrage**: BTP-Snapshot-Deep-Clone, Sortierung, zwei JSON-
Serialisierungen (eine für den Fingerprint, eine für die Antwort), dazu
zwei Config-Lesevorgänge von Platte. Bei acht Geräten sind das rund vier
Snapshot-Clones und acht Serialisierungen **pro Sekunde**, dauerhaft,
auch im völlig ruhigen Turnier.

Zwei Dokumente widersprachen sich zur Transportfrage:
[ADR 0016](0016-monitor-push-transport.md) verwarf SSE zugunsten von
WebSocket (nginx puffert; `proxy_buffering off` wäre ein Ops-Eingriff
außerhalb des Repos), während das How-To der TL-Web-Spec notierte, „SSE
bliebe später eine reine Ergänzung ohne Protokolländerung". Dieser ADR
löst den Widerspruch für TL-Web ausdrücklich auf.

## Entscheidung

**WebSocket-Nudge, Zugang im ersten Frame.** LAN-Server und Relay
bekommen je eine Route `/tl-ws`. Der Kanal trägt ausschließlich
`{"rev":n}` — **nie** Zustandsdaten. Die Seite holt nach jedem Anstoß
über ihren bestehenden `GET /tl/api/state` (Auth, ETag, Kürzungsleiter,
`X-Tl-Active-Profile` unverändert). Der Intervall-Poll bleibt als
Rückfallebene, gedrosselt auf 30 s, solange der Kanal steht.

Ergänzend am Turnier-PC: ein **zentraler Erkennungstakt** (1 s) baut den
Zustand einmal, legt die fertige Antwort in einen Cache, den alle
LAN-Anfragen lesen, und nudgt bei neuer Revision.

**Zugang in-band** (erste Nachricht `{"token":…}`), nicht im Pfad und
nicht im Query: Browser-WebSockets können keine Kopfzeilen setzen, und
Adressen landen in Zugriffsprotokollen (dieselbe Regel wie bei den
HTTP-Routen). Vor erfolgreicher Prüfung sendet der Server nichts — auch
keinen Ablehnungsgrund.

## Alternativen

- **SSE.** Erneut verworfen, aus demselben Grund wie ADR 0016: nginx vor
  dem Relay puffert ohne `proxy_buffering off`; der Eingriff läge
  außerhalb des Repos und wäre im laufenden Turnier riskant. WebSocket
  läuft dort erprobt (`Upgrade`-Map und 3600-s-Timeouts stehen bereits
  in `ops/nginx-bts-relay.conf`).
- **Daten im Push.** Verworfen: Es gäbe eine zweite Wahrheit für Auth,
  ETag, Kürzungsleiter und Profil-Header — und der Zustand ist bis
  64 KiB groß. Der Nudge kostet 12 Bytes.
- **Zugang im Query-Parameter.** Verworfen (Protokollierung).
- **Poll weiter, nur seltener.** Verworfen: Verschlechtert die Latenz,
  ohne die Rechnung am Turnier-PC zu senken.
- **Reiner Cache ohne Push.** Wäre die halbe Ersparnis (Rechnung), ließe
  aber die Anfragen und die Latenz unangetastet. Beides zusammen kostet
  kaum mehr.

## Folgen

- Zwei neue Routen (`/tl-ws` je Server und Relay), eine
  Abonnenten-Registry je Seite (Deckel 16 = doppelter Geräte-Cap), ein
  Erkennungstakt am Host.
- **Kein Bruch:** Ohne erreichbaren Kanal (älterer Turnier-PC, älterer
  Relay, Proxy ohne WebSocket) verhält sich die Seite exakt wie heute
  (2-s-Poll). Ältere Seiten gegen neuen Host sind unberührt.
- Der 8-Geräte-Platz im Relay bleibt an den **Poll** gebunden; der
  30-s-Fallback frischt ihn innerhalb der 60-s-TTL auf. Ein Kanal allein
  belegt keinen Platz.
- Der Zugangs-Entzug wirkt weiterhin über den Poll (401); ein offener
  Kanal überlebt ihn höchstens bis zum nächsten Verbindungsaufbau.
