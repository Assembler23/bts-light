# 0072 — SR-Vorab-Zuteilung als vorläufige Reservierung

- **Status:** accepted
- **Datum:** 2026-11-02
- Ergänzt [ADR 0021](0021-officials-ruecksync-eigenstaendiger-write.md) / ADR 0022.

## Kontext

Die SR-Rotation teilt heute erst beim Aufs-Feld-Kommen zu. Der Meetingpoint soll
den SR schon beim Aufruf kennen (nur freie SR; frei werdende SR gehen an schon
gerufene Partien). `officials_entries` schreibt jede lokale Zuweisung sofort nach
BTP, auch an geplanten Spielen; `Some(0)` bedeutet „ausdrücklich keiner".

## Entscheidung

Die Vorab-Zuteilung ist eine vorläufige Reservierung im Meetingpoint-Eintrag,
**nicht** in `OfficialsStore.assignments`. Ein zustandsbasierter Schritt nach
`track_officials` vergibt sie; beim Aufs-Feld-Kommen wird sie zur echten
Zuweisung (in `auto_assign` vor `officials_for_write`, sonst zustandsbasiert in
`track_officials`). Erst dann BTP-Write. BTP- oder Hand-SR hat Vorrang. „Ohne
Schiedsrichter" = vorhandenes `OfficialClear{Sr}`. Nur SR, nicht AR.

## Alternativen

- Direkt in `assignments` — schreibt vor dem Feld nach BTP; Rücknahme hinterließe
  `Some(0)` und blockierte die spätere Feldrotation.
- Keine Vorab-Zuteilung — widerspricht der Anforderung.

## Konsequenzen

Belegt-Prüfung der Rotation muss vorläufige SR einschließen. Kantenbasierte
Übernahme wäre nach Sync-Neustart verloren → zustandsbasiert Pflicht.
