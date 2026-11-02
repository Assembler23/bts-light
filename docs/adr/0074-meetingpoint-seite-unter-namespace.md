# 0074 — Meetingpoint-Seite unter /{ns}/ mit schreibendem Request/Ack-Pfad

- **Status:** accepted
- **Datum:** 2026-11-02
- Abgrenzung zu ADR 0012 (Namespace = Tablet-Zugang).

## Kontext

Die Meetingpoint-Seite schreibt (Check-in) und soll in LAN, Cloud und über den
Slave-PC der fernen Halle laufen. In der Cloud ist die Namespace-Kennung
zugleich der Zugang der Zähl-Tablets.

## Entscheidung

Seite und Routen unter `/{ns}/meetingpoint` wie Tafel/Anzeige; das Risiko, dass
die Kennung am Hallen-Gerät ablesbar ist, ist vom Nutzer **bewusst hingenommen**.
Check-in als HTTP-Request/Ack (`RelayFrame::MeetingpointCheckin` →
`HostFrame::MeetingpointAck`, Muster `result`, `MAX_PENDING_PER_NS`). Eine
Prüffunktion im Host für alle Wege (Halle, aktiver Aufruf, Platz). Das Zähl-Tablet
nutzt dieselbe Route mit `ort=feld`, nur wenn `MatchBrief.feldCheck` gesetzt
ist. Slave: 303 auf den Relay bzw. Durchreichen im Träger-Betrieb.

## Alternativen

- Eigener Gerätezugang wie TL — sicherer, aber zusätzliche Einrichtung am Kiosk;
  vom Nutzer abgelehnt.
- Nur LAN wie `/ansage` — ferne Cloud-Hallen ohne Meetingpoint.
- WebSocket — schneller, aber neue Socket-Verwaltung in drei Komponenten.
- Neue `TabletMsg` für den Feld-Check — alter Relay verwirft still, mehr Wire.

## Konsequenzen

Wer die Adresse kennt, kann Personen ein-/auschecken; Schaden begrenzt
(TL-Korrektur, strikt wirkt nur auf die Automatik). Relay-Deploy vor App-Release.
