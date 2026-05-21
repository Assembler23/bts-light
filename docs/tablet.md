# Digitaler Tablet-Spielzettel

Schiedsrichter zählen am Tablet statt auf Papier. bts-light betreibt dafür
einen eingebetteten Server, an den sich die Tablets im Hallen-WLAN hängen.
Am Spielende wird das Ergebnis nach BTP zurückgeschrieben.

## Architektur

bts-light ist der zentrale Hub – wie der Server der Original-BTS-Software.

```
Tablet Court 1 ─┐
Tablet Court 2 ─┼─ WS/HTTP ─▶ bts-light ──┬─▶ BTP    (SENDUPDATE bei Spielende)
Tablet Court 3 ─┘          (axum :8088)   ├─▶ badhub Liveticker (live)
                                          └─▶ Felder-Übersicht im bts-light-Fenster
```

- **Eingebetteter Server** – `axum` auf `0.0.0.0:8088`, läuft mit der
  Liveticker-Sync-Schleife (startet/stoppt mit „Starten"/„Stoppen").
- **Court → Match automatisch** – das Tablet ist an einen BTP-Court-Namen
  gebunden und zeigt das Spiel, das BTP gerade auf diesem Court hat. Keine
  manuelle Zuweisung – BTP ist die Quelle.
- **Score-Quelle pro Court** – zählt an einem Court ein Tablet, treibt es
  den Live-Score; sonst weiter das BTP-Polling. So überschreibt der
  5-Sekunden-Poll nie den Tablet-Stand.

## Verbindungsart: LAN oder Cloud

Die Tablets erreichen bts-light auf zwei Wegen – umschaltbar im
Setup-Wizard unter „Tablet-Verbindung":

- **LAN** – der hier beschriebene eingebettete Server. Schnell und
  offline, braucht aber den freigegebenen eingehenden Port 8088.
- **Cloud** – über einen Relay auf badhub.de; funktioniert auch hinter
  gesperrten Firmen-Firewalls (nur ausgehende Verbindungen). Details:
  [cloud-relay.md](cloud-relay.md).

Dieses Dokument beschreibt den LAN-Modus. Im Cloud-Modus sind Daten- und
BTP-Schreibweg identisch – nur die Strecke Tablet ↔ bts-light läuft über
den Relay statt direkt.

## Endpunkte des Tablet-Servers

| Route | Zweck |
|---|---|
| `GET /` | Landing-Page mit allen Court-Adressen |
| `GET /court/{name}` | Tablet-Spielzettel-UI für einen Court |
| `GET /qr/{name}` | QR-Code (SVG) zur Court-URL |
| `GET /ws` | WebSocket (Match-Zuweisung, Live-Score) |
| `POST /result` | Endergebnis vom Tablet → `SENDUPDATE` nach BTP |
| `GET /health` | Status-Schnappschuss |

## Datenfluss

1. **Match-Zuweisung** – der Server prüft alle 2 s `match_for_court` und
   schickt dem Tablet `match_assigned` / `match_cleared`.
2. **Live-Score** – jeder Punkt am Tablet → `score_update` → bts-light baut
   ein `tupdate_match` und pusht es an den Liveticker.
3. **Endergebnis** – „Ergebnis übermitteln" → `POST /result` → bts-light
   meldet sich per LOGIN an und schreibt das Match mit `SENDUPDATE` zurück
   nach BTP (siehe [btp_protocol.md](btp_protocol.md)).

## Einrichtung im Turnier

1. In bts-light den Liveticker starten (BTP muss verbunden sein) – der
   Tablet-Server startet automatisch mit.
2. „Tablet-Spielzettel" öffnen → pro Court QR-Code/Adresse.
3. Am Spielfeld das Tablet mit dem Hallen-WLAN verbinden, die Court-URL
   öffnen (oder QR scannen).
4. BTP muss das Spiel dem Court zugewiesen haben – dann erscheint es
   automatisch auf dem Tablet.

## Voraussetzungen

- Tablet und bts-light-PC im **selben WLAN**.
- **Windows-Firewall**: beim ersten Start fragt Windows, ob der Zugriff
  erlaubt werden soll – „Zugriff zulassen" (private Netze). Ohne Freigabe
  erreichen die Tablets bts-light nicht. Auf gesperrten Turnier-PCs ohne
  Admin-Rechte hilft stattdessen der Cloud-Modus ([cloud-relay.md](cloud-relay.md)).
- In **BTP müssen Netzwerk-Edits erlaubt** sein, sonst lehnt BTP den
  `SENDUPDATE` ab – das Tablet zeigt dann einen Fehler.

## Fehlersuche

- bts-light läuft pro Rechner **nur einmal** (Single-Instance). Ein
  zweiter Start würde sonst den Tablet-Server-Port 8088 blockieren.
- Der Server protokolliert ausgelieferte Tablet-Seiten, Tablet
  verbunden/getrennt und Match-Zuweisungen ins Log – siehe
  [logging.md](logging.md). Steht dort kein „Tablet verbunden", erreicht
  das Tablet den Server nicht (WLAN/Firewall prüfen).

## Bekannte Vereinfachungen

- Spielsystem ist fest **Best-of-3 bis 21** (BTP liefert das Format nicht
  zuverlässig im aktuellen Parser).
- Liga-Matches (`PlayerMatches`) sind noch nicht abgedeckt.
