# 0032 — Hallen-Farben: deterministische Auto-Palette über die sortierte Hallenliste

- **Status:** accepted
- **Datum:** 2026-08-16

## Kontext

Ohne manuelle Übersteuerung müssen alle Anzeigen (Desktop, TL-Web,
Monitore, badhub-Aushang) dieselbe Farbe je Halle zeigen — auch nach
BTP-Neustart oder `SENDTOURNAMENTINFO`-Neuladen. Die Reihenfolge der
Hallen im BTP-Snapshot ist dafür nicht stabil genug: springt sie, springen
mitten im Turnier die Farben.

## Entscheidung

Die Auto-Palette wird **deterministisch über die getrimmte,
case-insensitiv alphabetisch sortierte Hallenliste** vergeben
(`palette[i % 16]`; Sortierung wie `distinct_halls`, das Dedup ist hier
zusätzlich getrimmt und case-insensitiv). Persistierte Overrides
gewinnen immer und beeinflussen die Auto-Vergabe der übrigen Hallen
bewusst **nicht** (keine Umsortierung durch fremde Übersteuerung).
Bei weniger als zwei Hallen liefert der Resolver nichts — das Feature ist
bei Ein-Hallen-Turnieren strukturell unsichtbar.

## Alternativen

- **BTP-Snapshot-Reihenfolge:** instabil über Neustarts. Verworfen.
- **Namens-Hash:** stabil, aber Kollisionen zweier Hallen auf demselben
  Ton sind wahrscheinlich und für die Turnierleitung nicht vorhersagbar.
  Verworfen.

## Konsequenzen

- Eine mitten im Turnier neu auftauchende Halle kann die Auto-Farben
  alphabetisch nachfolgender Hallen verschieben — akzeptiert: die
  Hallen-Topologie ist im Turnier praktisch konstant, und Overrides
  persistieren.
- Farbdopplung per Override (zwei Hallen gleicher Ton) ist eine sichtbare
  Nutzerentscheidung und wird nicht technisch verhindert.
