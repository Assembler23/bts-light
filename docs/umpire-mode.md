# Schiri-Modus am Zähltablett

Hilfe für **Vereins-/Verleih-Turniere**: Ein Helfer am Zähltablett bekommt die
**vorzulesenden Ansagen** angezeigt und kann **Karten/Verwarnungen** vergeben.
(Bundesliga & offizielle Turniere laufen über das Original-BTS, nicht bts-light.)

Reine Tablet-Funktion in `src-tauri/assets/tablet.html` — greift **nicht** in die
geprüfte Zähl-Logik ein.

> **Seit dem Schiedsrichterzettel (Spec [features/schiedsrichterzettel-druck.md](features/schiedsrichterzettel-druck.md), ADR 0037) reisen Karten zum Turnier-PC.**
> Die frühere Zusage „Karten werden nur lokal protokolliert" gilt nicht mehr:
> Sie liegen nur lokal, überlebten keinen Gerätewechsel und waren nach dem
> Turnier verloren. Nun gehen sie an den Host, landen dort in
> `zettel/<slug>.json` und erscheinen auf dem gedruckten Zettel.
>
> **Unverändert gilt:** Sie gehen **nie zu badhub**, **nie in den Liveticker**
> und **nie in den Anzeige-Zustand** der Turnierleitungs-Seite — ein
> Wächter-Test erzwingt das (`sanktionsdaten_erreichen_den_anzeige_zustand_nie`).
> In der Datei stehen **keine Namen**, nur `team`/`player`; die Namen kommen
> erst beim Drucken aus dem BTP-Snapshot.

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
| Punktepause | „{Stand} – Pause." — die **Überschrift** der Pause nennt die Schwelle der jeweiligen Zählweise („Pause bei 11 Punkten", bei 15ern „Pause bei 8 Punkten"). |
| Satzende | „Satz. Den {n}. Satz gewinnt {Sieger} mit {Stand}. Bitte die Seiten wechseln." |
| Satzbeginn | „{N}. Satz. Null beide – bitte spielen." — steht es **Satz gegen Satz** (allgemein: beide eine Partie vor dem Sieg), heißt es „**Entscheidungssatz.**" statt der Nummer. |
| Spielende | „Spiel. Das Spiel gewinnt {Sieger}, {x} Sätze zu {y}: {Satzstände}." |

Badges: **Satzball** / **Matchball**.

## Karten / Verwarnungen
Button **„Karte / Verwarnung"** → Spieler wählen → Farbe:

| Karte | Wirkung | Ansage |
|---|---|---|
| 🟨 Gelb | Verwarnung (kein Punkt) | „{Name}, Verwarnung wegen unsportlichen Verhaltens. {Stand}" |
| 🟥 Rot | **Gegner bekommt +1** (regulärer Punkt) | „{Name}, Fehler wegen unsportlichen Verhaltens. {Stand}" |
| ⬛ Schwarz | Disqualifikation (Anzeige/Protokoll) | „{Name}, disqualifiziert wegen unsportlichen Verhaltens." |

Vergebene Karten erscheinen als **Chips** in der Leiste; je `matchId`
gespeichert, bei neuem Match zurückgesetzt.

**Die rote Karte ist nur wählbar, wenn ihr Punkt auch zählen kann** (nicht in
einer Pause, nicht nach Spielende, nicht im 700-ms-Cooldown nach dem letzten
Punkt). Sonst ist sie ausgegraut, mit Hinweis. Grund: Vorher konnte der Punkt
still verschluckt werden, während die Karte trotzdem protokolliert wurde — auf
dem Zettel stünde dann eine rote Karte ohne den Punkt, den sie erzeugt.

## Weitere Vorgänge (seit dem Schiedsrichterzettel)

Derselbe Knopf führt jetzt zuerst auf eine Art-Auswahl:

| Vorgang | Personenbezug | Wann |
|---|---|---|
| Karte / Verwarnung | Spieler | wie oben |
| Behandlung beginnt / endet | — | auch in der Behandlungspause |
| Unterbrechung | — | jederzeit |
| Überstimmung | — | jederzeit |
| Oberschiedsrichter gerufen | — | jederzeit |

Erfassbar sind sie **auch in Pausen und vor dem ersten Ballwechsel** — dort
passieren die meisten davon. Ein Undo nimmt Ereignisse jenseits des neuen
Schnitts ausdrücklich zurück; sie verschwinden nicht, sondern erscheinen auf dem
Zettel **durchgestrichen**. Für einen Archivbeleg ist das ehrlicher.

## Formulierungen anpassen
Alle Texte stehen gebündelt in den `ump*`-Funktionen in `tablet.html`
(`umpOpeningSpoken`, `umpScoreSpoken`, `umpSetEndSpoken`, `umpMatchEndSpoken`,
`applyCard`). Reine Strings — leicht zu ändern.

## Stand / offen
- v1: Ansagen + Karten (Deutsch, lokal). Logik via Node-Harness verifiziert.
- **Spielzettel-Export gibt es seit 08/2026** — siehe
  [features/schiedsrichterzettel-druck.md](features/schiedsrichterzettel-druck.md)
  und [features/schiedsrichterzettel-autodruck.md](features/schiedsrichterzettel-autodruck.md).
- **Der Zettel folgt seit v0.9.249 dem DBV-Bogen** (ADR 0043). Damit ist auch
  der Vermerk „Internes Turnier-Archiv — kein amtlicher Beleg" **zurückgenommen**:
  Er passt nicht auf ein Blatt, das während des Spiels geführt wird. Der Satz
  oben bleibt davon unberührt — offizielle Turniere laufen weiter über das
  Original-BTS.
- Die Karten erscheinen auf dem Blatt in der gewohnten Konvention: **W**
  Warnung (gelb), **F** Fehler (rot), **D** Disqualifikation; dazu **R** für
  „Oberschiedsrichter gerufen".
- Bewusst **nicht** gebaut: Übertragung an badhub, weitere Sprachen — bei
  Bedarf später.
