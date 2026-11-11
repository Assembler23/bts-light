# 0088 — Anzeigen bei Host-Verlust: Karenz, dann 503

- **Status:** accepted
- **Datum:** 2026-11-11
- Spec: [`relay-haerten`](../features/relay-haerten.md), Abschnitt e).
- Zusammen mit [ADR 0089](0089-lebensdauer-des-namespace-zustands-ohne-host.md) (dieselbe
  Uhr „Host weg seit").

## Kontext

Die Stand-Routen des Relays (`court/{id}/state`, `monitor/state`, `/{ns}/health`,
`info/preparation/state`, `meetingpoint/state`, `info/announce/state`) prüfen nicht, ob der
Turnier-PC („Host") verbunden ist. Hängt noch ein Tablet oder ein TL-Tab am Namespace,
liefern sie den eingefrorenen Stand als aktuell. Fehlt der Namespace ganz, kommt 200 mit
leerem Stand — das Feld erscheint frei. Die Seiten haben eine Offline-Anzeige, die nur auf
HTTP-Fehler reagiert.

Ein sofortiges 503 wäre die kleinste Änderung, aber jeder Sekunden-Abriss des Hosts
(Reconnect, WLAN-Wackler, Verbindungswechsel bei Cloudflare, Update-Neustart) ließe dann
alle Hallen-TVs die Störung zeigen, und jeder Feld-Monitor lüde zweimal ein Log hoch. Ein
Relay-Deploy leert außerdem den RAM: Danach fehlt der Namespace, bis der Host zurück ist
(üblich 1–4 s). Im LAN+Cloud-Betrieb ohne Cloud-Tablets räumt der Relay den Namespace heute
sofort beim Host-Abgang — es gäbe nichts weiterzuzeigen.

Entschieden vom Nutzer: Anzeigen zeigen den letzten Stand 15–20 s weiter, dann die Meldung;
dieselbe Meldung, wenn der Turnier-PC noch nie verbunden war; die Kopplungsseite eines
unzugewiesenen Monitors bleibt sichtbar.

## Entscheidung

1. **Eine Uhr je Namespace:** `host_weg_seit`. Gesetzt beim Freigeben des Host-Slots auf das
   **letzte Lebenszeichen** der Verbindung, gelöscht bei erfolgreicher Übernahme. Ein
   Namespace, den ein Tablet anlegt, startet mit „weg seit jetzt".
2. **Karenz 20 s** (eine Konstante). Darunter antworten die Stand-Routen 200 mit dem
   gehaltenen Stand, danach 503 mit `no-store`, `Retry-After: 2` und dem Rumpf
   `{"grund":"host_weg"}`. Die Lage wird vor Cache-Blick, Bau und Markenvergleich
   entschieden — gesperrt gibt es weder 304 noch Cache-Treffer.
3. **Der Namespace überlebt den Host-Abgang** für die Dauer der Uhr; ein Sweeper (alle 5 s)
   entfernt ihn und stößt am Karenz-Ende und bei der Rückkehr danach alle Monitor-Abos
   einmal an. Beim Host-Abgang werden die TL-Abos geschlossen.
4. **Fehlender Namespace ist gesperrt** (503). Ausnahme **Start-Karenz:** In den ersten 20 s
   nach Prozessstart bekommt ein Abruf mit `If-None-Match` für einen fehlenden Namespace
   304.
5. **Kopplungsseite bleibt:** `monitor/state?device=` liefert für ein Gerät ohne bekanntes
   Ziel weiter 200 mit der Kopplungsseite und dem neuen Feld `hostOffline`.
6. **Ausnahmen:** `info/announce/state` sperrt **sofort** (ein 200 ohne Schlüssel liest der
   Ansage-Slave als „Sprachdienst abgeschaltet"). `courts`, `info/ad/state`, Bilder, Logo
   und die HTML-Seiten bleiben unverändert.
7. **Seiten:** Feld-Monitor und Zähltafel dunkeln bei `grund = host_weg` den Stand ab,
   zeigen „Keine Verbindung zum Turnier-PC" und laden kein Log hoch. Übersicht, Kombi und
   Vorbereitung zeigen ihr Banner dann schon beim ersten 503. Die Stream-Einblendung zeigt
   nie einen Fehlertext.

## Verworfene Alternativen

- **503 sofort:** einfachste Regel, aber jeder Host-Wackler und jeder Relay-Deploy stünde
  auf allen Fernsehern.
- **200 mit Kennzeichen „Host weg seit …" im Stand, Blende in der Seite:** hält Stand und
  Hinweis getrennt, braucht aber Änderungen an jeder Seite und hilft alten Seiten (ferne
  Halle mit alter App, seit Tagen offene Kiosks) nicht — die zeigten den eingefrorenen
  Stand weiter als aktuell.
- **Fehlender Namespace sofort 503, ohne Start-Karenz:** einfacher, aber jeder
  Relay-Neustart zeigte auf allen Fernsehern kurz die Meldung.
- **Grund als eigener Antwort-Kopf:** Die Slave-Brücke reicht nur `Content-Type`, `ETag` und
  `Cache-Control` durch; die ferne Halle bekäme den Grund nicht.
- **Am Feld-Monitor nur den Text ändern:** kleinste Änderung, aber ein voll sichtbarer,
  eingefrorener Stand sieht weiter aktuell aus.

## Folgen

- Ein kurzer Abriss des Hosts und ein Relay-Neustart bleiben auf den Anzeigen unsichtbar.
- Ein stumm gestorbener Host ist rund 20 s nach seinem letzten Lebenszeichen gesperrt.
- In der Start-Karenz bestätigt der Relay bis zu 20 s lang „unverändert", ohne einen
  eigenen Stand zu haben.
- Fehlt der Namespace, sieht auch ein zugewiesener Monitor nach der Start-Karenz die
  Kopplungsseite mit Hinweis — der Relay kennt die Zuweisung nicht mehr.
- Alte Seiten behandeln 503 wie jeden Fehler: richtiges Verhalten, alter Wortlaut, alte
  Log-Uploads.
- Der Relay bekommt erstmals einen Zeit-Task (Sweeper); Zeit-Tests laufen mit pausierter
  Uhr.
- Das Verhalten ist für Zuschauer sichtbar und gehört ins Handbuch
  (`docs/court-monitor.md`, `docs/oberflaechen.md`, mit Anker).
