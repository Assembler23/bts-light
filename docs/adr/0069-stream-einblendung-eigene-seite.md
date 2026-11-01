# 0069 — Stream-Einblendung als eigene Seite auf dem Feld-Zustand je Feld

- **Status:** accepted
- **Datum:** 2026-10-31

## Kontext

Für Livestreams braucht es eine transparente Score-Einblendung als OBS-Browser-Quelle (Spec
[monitor-stile-und-stream](../features/monitor-stile-und-stream.md), Etappe E2), für ein Feld
und für 2–3 Felder, in LAN und Cloud.

Die Bausteine im Bestand:

- `monitor.html` kann nur ein Feld und trägt Werbung, Offline-Hinweis, Identifizieren,
  Kopplungsseite und Gerätemodus. Im Stream ist all das verboten.
- `combo.html` und `/combo/state` gibt es nur im LAN (ADR 0049), ohne ETag und ohne
  `seq`/Herzschlag.
- Die Cloud-`/health` liefert keinen Aufschlag (`serving_team: null`).
- Die feste Cloud-Route `/{ns}/court/{id}/state` antwortet ohne ETag.

## Entscheidung

1. Eine **eigene Seite** `stream.html` unter `/stream` (LAN) und `/{ns}/stream` (Cloud).
2. **Daten** je Feld über den vorhandenen Feld-Zustand `/court/{id}/state` bzw.
   `/{ns}/court/{id}/state`, höchstens 3 Felder, dazu eine `monitor-ws` ohne Feldfilter als Nudge.
   Es entsteht keine neue Daten-Route.
3. Die feste Cloud-State-Route bekommt ETag/304 (`monitor_state_response`, wie die Gerätestrecke).
4. Im Bild nie Werbung, Pause, Identifizieren oder Fehlertext. Nach 60 s ohne Daten wird die Seite
   durchsichtig. Leeres `courtLabel` vom Relay zählt nicht als Daten.
5. Die Stream-URL legt in der Cloud den Namespace offen (Bezug R6). Das ist **akzeptiert**, das
   Handbuch sagt „nur ans eigene Stream-Team weitergeben". Es gibt keinen eigenen Lese-Schlüssel.
6. Ein Drift-Test `scripts/test-inline-kopien.mjs` hält alle Inline-Kopien von
   `monitorSeq`/`pushHealth` wortgleich zum Modul, weil `stream.html` die vierte Kopie wird.

## Alternativen

- **Stil von `monitor.html`/`combo.html`:** Jeder verbotene Baustein müsste je Stil abgeschaltet
  werden, und `combo` ist nur LAN. Verworfen.
- **`/{ns}/health?court=` (schmaler Abruf S7):** nur ein Feld je Abruf und ohne Aufschlag. Ihn
  nachzurüsten veränderte die Marke aller Übersichtsmonitore. Verworfen.
- **Neue `/{ns}/combo/state` am Relay:** ein Abruf statt drei, aber eine neue Projektion mit ETag
  und Cache und ein Bruch mit ADR 0049. Vorbehalten als Ausbau, falls die Lastmessung es verlangt.
- **Eigener nur-lesender Schlüssel:** sicherer, aber ein neuer Relay-Mechanismus mit eigenem
  Umfang. Vom Nutzer abgelehnt.

## Konsequenzen

- Bis zu 12 Abrufe/s je Stream-Quelle bei 250 ms Takt, gemildert durch 304. Wird mit
  `last-monitor.mjs` gemessen.
- Der Cloud-Stream läuft schon mit dem Relay-Deploy, auch mit einem alten Host.
- Relay-Rollback: `/{ns}/stream` antwortet 404, OBS zeigt eine nicht transparente Fehlerseite.
- Über die Slave-Brücke gibt es keinen Stream. Ferne Hallen nehmen die Cloud-Adresse.
