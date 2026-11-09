# 0081 — Personensuche der Turnierleitung läuft am Turnier-PC

- **Status:** accepted
- **Datum:** 2026-11-09

## Kontext

Die Turnierleitung möchte in der TL-Oberfläche nach Spielern und
Schiedsrichtern suchen — am Feld, in der Warteliste und in **allen**
beendeten Spielen (Spec `tl-spielersuche`). Der TL-Zustand (`TlState`) ist
dafür nicht gebaut: Er trägt nur die letzten 30 beendeten Spiele, die
Warteliste gedeckelt (120, Cloud 40/20/10/5), beendete Spiele ohne
SR-Namen, und über das Relay höchstens 64 KiB. Relay und Turnier-PC werden
zu verschiedenen Zeitpunkten ausgerollt (Relay bei jedem Merge, Turnier-PC
erst nach Tag und Auto-Update).

## Entscheidung

1. Die Suche läuft **am Turnier-PC** über den ganzen BTP-Snapshot:
   `POST /tl/api/search` (LAN) und dieselbe Route am Relay, das sie mit
   einem eigenen Frame-Paar (`TlSearchRequest`/`TlSearchResult`, opakes
   JSON, eigene Pending-Map, eigener Größendeckel) nur durchreicht — wie
   Punktverlauf, Zettel und Schiedsrichter-Details.
2. Die Suchantwort ist eine **Positivliste** und trägt als einziger Weg
   **SR-/AR-Namen an beendeten Spielen** (nur bei eingeschaltetem
   Schiedsrichter-Betrieb). Der Suchbegriff reist im Body und wird nie
   protokolliert.
3. Die Suche ist ein Personen-Abfragewerkzeug hinter dem TL-Gerätezugang.
   Der Profil-Schalter „Spielersuche zeigen" blendet nur aus und ist
   **kein** Zugriffsschutz — freigegeben durch die Turnierleitung am
   09.11.2026.
4. Der Turnier-PC meldet die Fähigkeit per `TlState.can_search`; ohne das
   Flag zeigt die Seite kein Suchfeld und schickt keine Anfrage.

## Alternativen

- **`TlState` erweitern und im Browser filtern** — verworfen: sprengt
  Deckel und 64 KiB über das Relay, schickt den ganzen Bestand an jedes
  Gerät bei jedem Push, obwohl selten gesucht wird.
- **Generischer TL-Abfrage-Frame** (`TlQuery {kind, body}`) — verworfen:
  untypisierte Deckel, beliebiger Abfragetunnel über eine Internet-Route,
  heute kein zweiter Anwendungsfall (YAGNI).
- **Suche im Relay** — unmöglich ohne den ganzen Personenbestand auf einen
  Server zu spiegeln, der viele Turniere sieht; das Relay liest den
  TL-Zustand bewusst nie.
- **Profil-Schalter am Turnier-PC durchsetzen** — verworfen: TL-Geräte sind
  angemeldet und vertrauenswürdig; der Aufwand (Profilbindung je Anfrage)
  steht in keinem Verhältnis.
- **Ohne Fähigkeits-Flag** — verworfen: Nach jedem Merge hinge jede Suche
  gegen einen alten Turnier-PC 20 s am Relay-Timeout.

## Konsequenzen

- Neue Relay-Route und neue Frames; Relay-Deploy bei Merge (automatisch).
- Eigene Wächter-Tests für die Suchantwort (`search_answer_*`), getrennt
  vom `TlState`-Wächter. Jede Erweiterung der Treffer muss die Positivliste
  bewusst ändern.
- CLAUDE.md-Datenschutzabschnitt nennt die Suche.
- Kein Rate-Limit je Gerät; Last wird über Client-Disziplin, Deckel und
  `spawn_blocking` begrenzt.
