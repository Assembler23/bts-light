# 0055 — Zähltafel: Anzeige-Hülle als iframe-Container, Tafel als eigenes Zuweisungsziel

- **Status:** accepted
- **Datum:** 2026-09-04

## Kontext

Ein zweites Tablet am Feld soll nur den Spielstand zeigen — groß, ohne Namen, wie eine klassische
Zähltafel — und dabei das zählende Tablet nicht stören (R4: ein aktives Tablet je Feld). Dieselbe
Tafel soll auch auf Pi-TVs laufen, die der Turnierleiter über das Court-Monitor-Panel zuweist.
Ein Tablet braucht dafür Dinge, die ein TV nicht braucht: Zahnrad mit PIN, Weg zurück zum
Zählen, Spiegel-Schalter, Vollbild, Wake-Lock. Spec:
[features/zaehltafel-anzeige-huelle.md](../features/zaehltafel-anzeige-huelle.md).

Zwei Entscheidungen hängen daran.

## Entscheidung 1 — Hülle und Layout trennen

Die Tafel ist ein **reines Layout** (`tafel.html`, `/court/{id}/tafel`) nach dem Vorbild von
`monitor.html`. Die Tablet-Funktionen sitzen in einer **eigenen Hülle** (`anzeige.html`,
`/anzeige`), die das gewählte Layout in einem seitenfüllenden iframe derselben Herkunft einbettet
und den Pfad ausschließlich aus einer Allowlist baut. Die Hülle kann damit auch die vorhandenen
Layouts (Feld-Monitor, Hallen-Übersicht, Spiele in Vorbereitung) auf einem Tablet zeigen, ohne
dass diese Seiten angefasst werden.

### Alternativen

- **Anzeigemodus in `tablet.html`.** Die Zähl-Seite kennt Seiten und Aufschlag schon. Verworfen:
  sie meldet sich als Tablet am `/ws` an; ein reiner Zuschauer bräuchte eine neue Rolle im
  Protokoll und berührte die Platz-Vergabe in Server und Relay. Zudem verdeckt das Belegt-Overlay
  auf einem besetzten Feld das Zahnrad.
- **Weiteres Layout in `monitor.html`.** Das Config-Feld `layout` ist dafür vorbereitet, und der
  Verbindungsblock käme gratis. Verworfen: `layout` gilt global für alle Monitore; und Zahnrad,
  PIN, Spiegel-Schalter und Wake-Lock gehören nicht auf eine TV-Seite, die schon groß ist.
- **Nur Links im Tablet-Menü** auf die vorhandenen Seiten. Verworfen: kein Weg zurück, kein
  Wake-Lock, kein Spiegeln.

## Entscheidung 2 — neue `MonitorTarget`-Variante `CourtTafel` trotz ADR 0049

Das Zuweisungsziel „Zähltafel – Feld X" wird eine **neue Variante** `MonitorTarget::CourtTafel
{ court_id }` (Serde-Tag `court_tafel`). `redirect_path()` liefert `/court/{id}/tafel`,
`court_id()` liefert das Feld, damit Geräteliste und Panel das Gerät wie ein Feld-Gerät führen.
Die Umleitungs-Allowlist im Relay wird um die Variante erweitert.

ADR 0049 hat für die Kombi-Ausrichtung eine neue Variante **verworfen**, weil `read_assignments`
unbekannte `kind`-Tags still verwirft und ein Downgrade die Zuweisung verlöre. Das gilt hier
genauso — und wird bewusst in Kauf genommen: Die Ausrichtung war ein **Attribut** eines
bestehenden Ziels, die Tafel ist ein **eigenes Anzeige-Ziel** mit eigener Seite. Sie in ein
Attribut von `Court` zu pressen hätte den Feld-Monitor mit einem zweiten Zustand belastet und
`redirect_path()` für `Court` verzweigt.

### Alternativen

- **Attribut am `Court`-Ziel** (`Court { court_id, tafel: bool }`): downgrade-sicher, aber
  `Court` wäre nicht mehr „Feld-Monitor", sondern „Feld-Monitor oder Tafel"; jeder Konsument von
  `Court` müsste das Attribut kennen. Verworfen.
- **Eigene Geräte-Datei** wie in ADR 0049 (`tafel-devices.json`): downgrade-sicher, aber die
  Datei müsste jede Zuweisung spiegeln und im Relay nachgezogen werden. Für ein Ziel, das
  ohnehin eine eigene Seite hat, unverhältnismäßig. Verworfen.

## Konsequenzen

- Zwei neue Assets, zwei neue Routen in LAN und Relay; keine Config-Felder.
- Nach einem **Downgrade** stehen Tafel-TVs auf der Kopplungsseite und müssen neu zugewiesen
  werden; alle anderen Zuweisungen bleiben.
- Ein **altes Relay** lehnt den gesamten Zuweisungs-Upload mit unbekanntem `kind` ab (422): Der
  Host lädt `assignments` und `targets` in einem Body hoch (`relay_client.rs`), also sieht das
  alte Relay die CourtID-Karte der Tafel nie und behält den zuletzt akzeptierten Stand — **für
  alle Geräte**. Neue Zuweisungen und Fernbefehle des ganzen Turniers frieren ein, solange
  irgendein Gerät auf `court_tafel` steht, nicht nur die Tafel selbst. Die Regel „Relay-Deploy
  beim Merge, App-Tag danach" deckt das; ein Relay-Rollback nicht.
- Der iframe-Weg hängt an Safari: Der iPad-Feldtest ist Abnahmekriterium; Rückfall ist ein Menü
  direkt in `tafel.html`.
- Das Belegt-Overlay bleibt der einzige Übernahme-Weg; Hülle und Tafel öffnen nie `/ws`.
