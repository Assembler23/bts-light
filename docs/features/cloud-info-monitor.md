# Cloud-Info-Monitore — Übersicht/Vorbereitung/Werbung übers Relay

**Status:** In Umsetzung. Ziel-Version-Reihe ab v0.9.191.

## Problem

Court-Monitore funktionieren im Cloud-Modus; **Info-/Werbe-Monitore nicht**.
Weist man einem Cloud-Gerät „Court-Übersicht", „In Vorbereitung", „Werbung"
oder „Siegerehrung" zu, bleibt es auf dem bts-light-Logo — der Relay
transportierte nur die Feld-Zuweisung (CourtID), und die Info-Seiten
(`overview.html`/`preparation.html`/`ad.html`/`winners.html`) sind **LAN-gebaut**:
absolute Fetch-Pfade (`/health`, `/monitor/state`, `/info/*/state`, `/ads/`,
`/flags/`, `/pi-log`) und vom Relay gar nicht ausgeliefert.

## Ist-Bestand des Relays (gemessen)

Das Relay hält je Namespace bereits: `court_labels`, `courts` (CourtBrief),
`court_matches` (CourtID→MatchBrief), `court_scores`, `court_state`,
`prepared` (PreparedMatch), `monitor` (Ads + Config + Logo). Damit sind
**Übersicht, Vorbereitung und Werbung** cloud-fähig baubar. **Siegerehrung**
fehlen die Daten (Ergebnisse) → **Nicht-Ziel dieser Iteration** (bleibt
LAN-only; im Cloud fällt sie sauber auf „unassigned" zurück, kein 404).

## Umsetzung

### Fundament (alle Seiten)

1. **Volles Ziel zum Relay** (erledigt): `MonitorControl.targets`
   (Geräte-ID→MonitorTarget). Der Relay bevorzugt `targets`, `assignments`
   (CourtID) bleibt als Alt-Relay-Kompat. `monitor_device_state` setzt für
   Nicht-Court-Ziele `redirect_to = target.redirect_path()`.
2. **BASE-fähige Info-Seiten**: `overview.html`/`preparation.html`/`ad.html`
   bekommen — wie `monitor.html` — einen `__BASE__`-Platzhalter; **alle**
   Fetches laufen über `BASE + "…"`. Der LAN-Server templatet `BASE="/"`, der
   Relay `BASE="/bts-relay/{ns}/"`.
3. **BASE-relativer Redirect**: `monitor.html` löst `redirectTo` gegen `BASE`
   auf (`BASE + redirectTo.replace(/^\//,"")`), damit die Umleitung im Cloud
   unter dem Namespace-Präfix landet.

### Je Seite

- **Vorbereitung**: Relay serviert `/{ns}/info/preparation` (HTML) +
  `/{ns}/info/preparation/state` (aus `prepared`, gleiche JSON-Form wie LAN).
- **Werbung** ✅ (v0.9.192): Relay serviert `/{ns}/info/ad` (HTML) und
  `/{ns}/info/ad/state` mit vollem `ads`-Array (Indizes, Rotation) zusätzlich
  zu `barAds`/`hasLogo`. Redirect-Gate um `AdRotation` erweitert. Werbe-
  Einzelbild (`AdSingle`, dateinamenbasiert) bleibt LAN-only.
- **Übersicht**: Relay serviert `/{ns}/info/overview` (HTML) +
  `/{ns}/info/overview/state` (aus `courts`+`court_matches`+`court_scores`).
  `overview.html` von `/health` auf diesen dedizierten State umstellen (LAN
  liefert ihn ebenso), damit LAN und Cloud dieselbe Quelle nutzen.

### Reihenfolge

1. Fundament + **Vorbereitung** (der gemeldete Fall) — eine PR, end-to-end
   testbar. 2. **Werbung**. 3. **Übersicht**. Jede mit Relay-Routing-Test.

## Nicht-Ziele

- **Siegerehrung** im Cloud (Relay hat keine Ergebnisdaten) — LAN-only.
- `pi-log` im Cloud (Diagnose) — im Cloud No-Op.

## Tests / Doku

Relay-Routing-Tests je Seite (Ziel→Redirect, State-Endpunkt liefert Daten).
`docs/court-monitor.md` Endpunkt-Tabelle (Cloud-Spalten füllen). Version je
Schritt bumpen.
