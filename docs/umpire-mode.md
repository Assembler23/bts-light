# Schiri-Modus am Zähltablett

Hilfe für **Vereins-/Verleih-Turniere**: Ein Helfer am Zähltablett bekommt die
**vorzulesenden Ansagen** angezeigt und kann **Karten/Verwarnungen** vergeben.
(Bundesliga & offizielle Turniere laufen über das Original-BTS, nicht bts-light.)

Reine Tablet-Funktion in `src-tauri/assets/tablet.html` — greift **nicht** in die
geprüfte Zähl-Logik ein. Karten werden **nur lokal** protokolliert (kein Versand
an Server/badhub).

## Aktivieren
Tablet → **⚙ (Header) → PIN** (Standard `0000`, einstellbar) → **„Schiri-Modus: an"**.
Opt-in pro Tablet, lokal gespeichert (`localStorage`). Es erscheint eine **immer
sichtbare Ansage-Leiste**.

## Ansagen (Deutsch/DBV)
Aus dem aktuellen Spielstand erzeugt, **Aufschlägerstand zuerst**:

| Situation | Ansage |
|---|---|
| Eröffnung | „Meine Damen und Herren: zu meiner Rechten {rechts}, zu meiner Linken {links}. {Aufschläger} schlägt auf {Rückschläger}. Null beide – bitte spielen." |
| Punkt | „{Aufschläger}:{Rückschläger}" |
| Gleichstand | „{n} beide" |
| Aufschlagwechsel | „Aufschlagwechsel {Stand}" |
| 11-Pause | „{Stand} – Pause." |
| Satzende | „Satz. Den {n}. Satz gewinnt {Sieger} mit {Stand}. Bitte die Seiten wechseln." |
| Satzbeginn | „{N}. Satz. Null beide – bitte spielen." |
| Spielende | „Spiel. Das Spiel gewinnt {Sieger}, {x} Sätze zu {y}: {Satzstände}." |

Badges: **Satzball** / **Matchball**.

## Karten / Verwarnungen
Button **„Karte / Verwarnung"** → Spieler wählen → Farbe:

| Karte | Wirkung | Ansage |
|---|---|---|
| 🟨 Gelb | Verwarnung (kein Punkt) | „{Name}, Verwarnung wegen unsportlichen Verhaltens. {Stand}" |
| 🟥 Rot | **Gegner bekommt +1** (regulärer Punkt) | „{Name}, Fehler wegen unsportlichen Verhaltens. {Stand}" |
| ⬛ Schwarz | Disqualifikation (Anzeige/Protokoll) | „{Name}, disqualifiziert." |

Vergebene Karten erscheinen als **Chips** in der Leiste; lokal je `matchId`
gespeichert, bei neuem Match zurückgesetzt.

## Formulierungen anpassen
Alle Texte stehen gebündelt in den `ump*`-Funktionen in `tablet.html`
(`umpOpeningSpoken`, `umpScoreSpoken`, `umpSetEndSpoken`, `umpMatchEndSpoken`,
`applyCard`). Reine Strings — leicht zu ändern.

## Stand / offen
- v1: Ansagen + Karten (Deutsch, lokal). Logik via Node-Harness verifiziert.
- Bewusst **nicht** gebaut: Spielzettel-Export, Übertragung an badhub, weitere
  Sprachen — bei Bedarf später.
