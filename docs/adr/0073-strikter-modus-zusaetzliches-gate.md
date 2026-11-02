# 0073 — Strikter Modus als zusätzliches Gate der Feld-Automatik

- **Status:** accepted
- **Datum:** 2026-11-02
- Ergänzt [ADR 0030](0030-halle-bindet-die-feldvergabe.md) und [ADR 0046](0046-wunschfeld-reserviert-ab-spielbereitschaft.md).

## Kontext

Mit „nur spielbereite Partien" vergibt die Automatik ohne Aufruf kein Feld mehr.
Heute ersetzt eine Hallenzuordnung den Aufruf (ADR 0030), Ein-Hallen-Turniere
brauchen gar keinen. Ein Wunschfeld hält sein Feld frei, sobald die Partie nach
Spieler-Verfügbarkeit bereit ist.

## Entscheidung

`require_call`/`halle_statt_aufruf` bleiben unverändert. Ist
`strict_auto_assign` an, kommt ein weiterer Filter hinzu: Partie muss
meetingpoint-bereit sein und (Mehr-Hallen) Aufruf-Halle = Feld-Halle. Das
Wunschfeld reserviert im strikten Modus erst ab Bereitschaft. Keine Kopplung an
die Aufruf-Automatik; stattdessen TL-Warnung bei freiem Feld ohne bereite Partie.
Hand-Vergabe bleibt frei.

## Alternativen

- Strikt ersetzt die Aufruf-Pflicht-Logik — größerer Eingriff, Regressionen in 0030.
- Strikt nur mit Aufruf-Automatik wählbar — bevormundet TL, die von Hand ruft.

## Konsequenzen

Ohne Aufrufe stehen Felder still; die Warnung macht das sichtbar. Schalter aus →
Verhalten bitgleich zu heute.
