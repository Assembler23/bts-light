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

1. Zwei turnierweite **Plätze** am Namen in `CourtMonitorConfig` (Nachtrag 01.11.2026, vor dem
   ersten Release; ersetzt die zwei Häkchen `show_nations`/`show_clubs`):
   - `name_davor`: `flagge` | `logo` | `ohne`, Standard **`flagge`**, damit sich das heutige
     Verhalten nicht ändert.
   - `name_darunter`: `verein` | `nation` | `ohne`, Standard **`ohne`**.

   Die Plätze legen die Anordnung am Feld-Monitor fest **und** den Datenbedarf:
   `nation_an()` = ein Platz zeigt die Nation, `verein_an()` = ein Platz zeigt den Verein. Nur
   diese abgeleiteten Werte steuern den Filter; auf dem Draht reisen sie weiter als
   `MonitorConfig.showNations`/`showClubs`, die Plätze zusätzlich als `nameDavor`/`nameDarunter`
   (leer bei älteren Absendern → die Seite leitet aus den Schaltern ab). Getrennt vom
   Tablet/TL-Schalter `DisplayConfig.show_club_names`.
2. Gefiltert wird **serverseitig** an jeder Anzeige-Projektion:
   - LAN `build_monitor_state`, `uebersicht_json`, `combo_state`
   - Relay `monitor_player`, `overview_health`
   - Ohne Host-Upload gilt am Relay der Default (Nation an, Verein aus).
   - Die Stream-Einblendung (`?zweck=stream`) hat eigene Schalter `stream_nations`/`stream_clubs`
     (Nachtrag 01.11.2026) und wird mit ihnen statt mit den Plätzen gefiltert — ebenfalls
     serverseitig (LAN `fuer_stream`, Relay `build_stream_state`).
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
- **Plätze je Monitor** (wie der Stil an der Zuweisung): flexibler, aber der Datenfilter müsste je
  Gerät laufen, und Tafel/Übersicht/Kombi bekämen keinen eindeutigen Bedarf. Verworfen zugunsten
  turnierweiter Plätze.
- **Logo in der Cloud direkt vom badhub-Resolver laden** (wie `tl.html`): Die Seiten müssten LAN
  und Cloud unterscheiden, und die Slave-Brücke ginge leer aus. Verworfen zugunsten der
  Relay-Weiterleitung.

## Konsequenzen

- CLAUDE.md (Datenschutz) wird präzisiert: Auf Monitoren ist die Nation standardmäßig an und
  abschaltbar, der Verein standardmäßig aus.
- Relay-Rollback nach dem Release: Ein altes Relay ignoriert „Nation aus", in der Cloud stehen
  dann wieder Flaggen. Das wird in `docs/cloud-relay.md` dokumentiert.
- Ein Datenschutz-Wächter-Test sichert die Filter strukturell ab.
