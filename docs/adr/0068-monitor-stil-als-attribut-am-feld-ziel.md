# 0068 — Monitor-Stil als Attribut am Feld-Ziel, in-band übertragen

- **Status:** accepted
- **Datum:** 2026-10-31

## Kontext

Der Feld-Monitor bekommt wählbare Stile (Spec
[monitor-stile-und-stream](../features/monitor-stile-und-stream.md)), je Monitor im Panel
gewählt und in LAN **und** Cloud wirksam. Für Anzeige-Varianten gibt es im Repo schon zwei
Muster: eine eigene Geräte-Datei ohne Wire-Änderung (ADR 0049, Kombi-Ausrichtung) und eine
eigene `MonitorTarget`-Variante (ADR 0055, Zähltafel). Die Variante hat einen bekannten
Preis: Nach einem Downgrade verwirft `read_assignments` den Eintrag, und ein altes Relay
lehnt den ganzen `MonitorControl` mit 422 ab. Dann frieren alle Zuweisungen ein.

Geprüft am 31.10.2026: Weder `relay-proto`, `relay/` noch `src-tauri` nutzen
`deny_unknown_fields`. Ein **zusätzliches Feld** an einer bekannten Variante wird von alten
Lesern ignoriert. Gefährlich ist nur ein **unbekannter Wert** in einem geschlossenen Enum.

## Entscheidung

1. `MonitorTarget::Court` bekommt ein optionales Feld
   `stil: MonitorStil` (`#[serde(default, skip_serializing_if = "MonitorStil::ist_standard")]`).
   Die Standardform `{"kind":"court","court_id":5}` bleibt bytegleich.
2. `MonitorStil` ist ein geschlossenes Enum mit `#[serde(other)] Unbekannt`. Es folgt dem Muster
   `AnsageArt`. `Unbekannt` wirkt wie `Standard` und wird nicht zurückgeschrieben.
3. Transportiert wird der Stil **in-band** über `MonitorState.stil`. Die Zustands-Marke ändert
   sich, also wechselt die Seite ohne Neuladen. Es gibt keine Umleitungs-URL.
4. Wirksamer Stil: im Gerätemodus nur `MonitorState.stil`, auf festen URLs nur `?stil=`. Beides
   läuft über eine Allowlist im Browser.
5. `CourtMonitorConfig.layout` bleibt lesbar, wird als veraltet markiert und nicht mehr angeboten.
   Es wird **nicht** zum turnierweiten Standard-Stil umgedeutet, damit keine zweite
   Wahrheitsquelle entsteht.

## Alternativen

- **Eigene Geräte-Datei (Muster ADR 0049):** Der Stil überlebt Zielwechsel. Die Cloud bräuchte
  trotzdem eine neue Wire-Karte in `MonitorControl`. Dazu kämen Schreibreihenfolge, eigener
  Command, Aufräumen beim Vergessen eines Geräts und ein eigener Cache, also etwa doppelt so
  viele bewegliche Teile. Das Argument aus 0049 („der Relay wertet es nicht aus") gilt hier nicht.
- **Neue Varianten je Stil (Muster ADR 0055):** verworfen. Downgrade macht Geräte unzugewiesen,
  ein altes Relay friert alle Zuweisungen ein.
- **`CourtMonitorConfig.layout` je Gerät:** verworfen. Die Config ist global, wird vom
  Setup-Assistenten komplett zurückgeschrieben und bräuchte Schutz in `keep_host_managed_fields`.
- **Stil als `String`:** überlebt einen Downgrade mit Rückschreiben. Dafür fehlt jede Typprüfung,
  Länge und Zeichensatz müssten in Host und Relay begrenzt werden. Verworfen.
- **Umleitungs-URL statt in-band:** verworfen wie in ADR 0049 (Schwarzbild beim Wechsel,
  positionsabhängiger URL-Vergleich).

## Konsequenzen

- Ein altes Relay ignoriert `stil`, die Cloud zeigt bis zum Relay-Deploy den Standard-Stil.
- App-Downgrade: Zuweisungen bleiben, Stile gehen verloren.
- Der Stil verfällt beim Umstellen auf Tafel, Info, Werbung oder Kombi (bewusst akzeptiert). Das
  Panel muss ihn beim Feldwechsel mitnehmen (`zielMitStil`).
- Am Relay gibt es keinen Nudge bei `monitor_control_upload`. Der Wechsel greift nach höchstens
  einem Fallback-Takt.
