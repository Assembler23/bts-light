# 0033 — Hallen-Farben: Hex-String auf dem Draht, Palettenzwang nur am Schreibpunkt

- **Status:** accepted
- **Datum:** 2026-08-16

## Kontext

Die Hallenfarbe reist zu Gegenstellen mit **eigenem Release-Zyklus**: über
das opake `TlState`-JSON (TL-Web), drei typisierte relay-proto-Frames
(`CourtBrief`, `PreparedMatch`, `MonitorState` → Cloud-Monitore) und den
badhub-`tset` (Aushang, Deploy nur durch Kollegen). Die Farbwahl selbst
ist auf eine kuratierte ~10-Ton-Palette beschränkt.

## Entscheidung

Auf dem Draht reist die Farbe als **Hex-String `#rrggbb`** (lowercase),
überall `#[serde(default)]`/`skip_serializing_if` — alte Gegenstellen
ignorieren das Feld und bleiben farblos. Der **Palettenzwang gilt
ausschließlich am einzigen Schreibpunkt** (`upsert_hall_color` validiert
„Ton ∈ Palette"); Konsumenten rendern den Wert direkt.

## Alternativen

- **Palette-Index:** kompakt und strukturell palettentreu — aber jeder
  Konsument (badhub-Repo, tl.html, drei Monitor-Seiten, Relay) bräuchte
  eine Paletten-Kopie. Bei Deploy-Skew (Relay vor App, badhub Wochen
  später) zeigen zwei Anzeigen **verschiedene Farben für dieselbe
  Halle** — exakt der Fehlerfall aus dem Erfolgskriterium. Verworfen.
- **`{bg, fg}`-Objekt je Theme:** löst Hell/Dunkel-Lesbarkeit per
  Datenform, verdoppelt aber die Wire-Fläche; überflüssig, weil die
  Palette als Marke (nicht Textfarbe) auf beiden Gründen kuratiert ist.
  Verworfen.

## Konsequenzen

- Kein Versions-Skew zwischen Host, Relay, Monitor-Seiten und badhub
  möglich; die Palette kann weiterentwickelt werden, ohne Draht oder
  gespeicherte Configs zu brechen.
- Gespeicherte Overrides bleiben auch dann gültig, wenn ein Ton später
  aus der Palette fällt (sie tragen den Hex-Wert selbst).
