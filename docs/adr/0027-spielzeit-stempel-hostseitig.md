# 0027 — Spielzeit-Stempel entstehen host-seitig, nicht auf dem Tablet

- **Status:** accepted
- **Datum:** 2026-08-16

## Kontext

Die Spielzeiten-Messung (Spec `docs/features/spielzeiten-prognose.md`)
braucht drei Zeitpunkte je Match: erste Feldzuweisung (Bruttostart),
erster Punkt (Nettostart) und Spielende. Kandidaten für die Quelle der
letzten beiden waren das Tablet (es weiß exakt, wann gezählt wird) und
der Host (er sieht alle Score-/Ergebnis-Eingänge). Die Tablet-Uhr driftet
(deshalb rechnen schon die Satzpausen in Server-Zeit, `serverNow()` in
`tablet.html`), und Ergebnisse können nach Verbindungsabriss Minuten
verspätet zugestellt werden (ADR 0018).

## Entscheidung

Der **Host stempelt alle drei Zeitpunkte selbst** (`tablet/match_times.rs`,
persistiert in `match-times.json` nach dem ADR-0022-Muster):

- **Bruttostart** beim Sync-Poll, der das Match erstmals OnCourt sieht —
  „nur wenn leer", damit Feldwechsel und App-Neustart nichts umstempeln.
  Reset erst, wenn drei aufeinanderfolgende Snapshots das Match wieder als
  `Scheduled` ohne Feld führen (bestätigte Abnahme; filtert Sync-Flackern).
- **Nettostart** beim ersten beim Host eingehenden Punktestand > 0
  (`handle_score`, hinter Finalisiert-Gate und Stale-Filter — der
  gemeinsame Trichter von LAN und Cloud, deckt auch Zähltafeln ab).
- **Spielende** beim Host-Eingang des Ergebnisses (alle Pfade: Tablet,
  Desktop-Backend, TL-Web) — „nur wenn leer": Korrekturen und
  Wiederholungs-POSTs ändern weder Zeit noch Einstufung.

## Alternativen

- **Tablet liefert Zeitstempel in Server-Zeit mit:** exakter bei
  gepufferter Zustellung (ADR 0018), aber ein neues eingehendes Datum mit
  Validierungspflicht (R5), und Zähltafel-/tablet-lose Spiele bräuchten
  trotzdem den Host-Weg. Verworfen: zwei Quellen für dieselbe Wahrheit.
- **`on_court_since` weiterverwenden:** lebt nur im RAM und wird bei
  Feldwechsel/Neustart neu gestempelt — genau die Fehlerquellen, die die
  Messung ausschließen soll. Bleibt als Fallback-Zubringer bestehen.

## Konsequenzen

- Eine Uhr für alles; kein Uhren-Drift, keine neue Angriffsfläche.
- Verspätet zugestellte Ergebnisse machen Brutto/Netto minimal zu lang —
  bewusst toleriert (Spec, E3).
- Die BTP-`Duration` wird neustartfest: die früheren 0-Pfade (manuelles
  Ergebnis, TL-Web, App-Neustart) bedienen sich aus dem Store; Walkover
  bleibt bewusst 0.
- Alte Versionen ignorieren `match-times.json` — Rollback gefahrlos.
