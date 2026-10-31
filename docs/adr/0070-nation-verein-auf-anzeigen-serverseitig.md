# 0070 — Nation und Verein auf Anzeigen serverseitig gefiltert

- **Status:** accepted
- **Datum:** 2026-10-31

## Kontext

Die Monitore zeigen die Flagge heute **immer**, sobald BTP eine Nation liefert. Einen Schalter
gibt es nicht. Den Verein zeigen sie nie, obwohl er am Relay schon vorliegt
(`PlayerBrief.club`). LAN-`/health` und `/combo/state` liefern `team*_clubs` ungefiltert, ohne
dass eine Seite sie liest. Die neuen Stile (Nur Spieler, International) und die
Stream-Einblendung sollen Nation und Verein zeigen können (Spec
[monitor-stile-und-stream](../features/monitor-stile-und-stream.md)).

Ein Schalter, der nur im Browser wirkt, wäre wirkungslos: Jeder mit der URL sähe die Daten.

## Entscheidung

1. Zwei turnierweite Schalter in `CourtMonitorConfig`: `show_nations` (Standard **an**, damit
   sich das heutige Verhalten nicht ändert) und `show_clubs` (Standard **aus**, gilt für Name und
   Logo). Sie sind getrennt vom Tablet/TL-Schalter `DisplayConfig.show_club_names`.
2. Gefiltert wird **serverseitig** an jeder Anzeige-Projektion:
   - LAN `build_monitor_state`, `uebersicht_json`, `combo_state`
   - Relay `monitor_player`, `overview_health`
   - Ohne Host-Upload gilt am Relay der Default (Nation an, Verein aus).
3. URL-Parameter (`nation=0`, `verein=0`) können nur **abschalten**, nie einschalten.
4. Vereinslogo einheitlich über `BASE + "info/club-logo?name="`. LAN wie heute. Cloud über die
   neue Relay-Route `/{ns}/info/club-logo`, ein relativer 302 auf den badhub-Resolver
   `/api/v1/club-logo` (badhub #402). Fester Pfad, Name kodiert und gekappt, kein offener Redirect.

## Alternativen

- **Nation standardmäßig aus** (Datenschutz-Linie des TL-Web): Nach dem Update verschwänden alle
  Flaggen auf allen TVs. Vom Nutzer abgelehnt. Die Nation steht ohnehin auf Aushang und
  Meldeliste.
- **Bestehende `DisplayConfig.show_club_names` mitbenutzen:** Damit wäre die Wahl für Tablet und
  TL-Web an die Hallen-TVs gekoppelt, obwohl das unterschiedliche Öffentlichkeiten sind.
  Verworfen.
- **Filter nur im Browser / URL mit Vorrang:** hebelt den zentralen Schalter aus. Verworfen.
- **Logo in der Cloud direkt vom badhub-Resolver laden** (wie `tl.html`): Die Seiten müssten LAN
  und Cloud unterscheiden, und die Slave-Brücke ginge leer aus. Verworfen zugunsten der
  Relay-Weiterleitung.

## Konsequenzen

- CLAUDE.md (Datenschutz) wird präzisiert: Auf Monitoren ist die Nation standardmäßig an und
  abschaltbar, der Verein standardmäßig aus.
- Relay-Rollback nach dem Release: Ein altes Relay ignoriert „Nation aus", in der Cloud stehen
  dann wieder Flaggen. Das wird in `docs/cloud-relay.md` dokumentiert.
- Ein Datenschutz-Wächter-Test sichert die Filter strukturell ab.
