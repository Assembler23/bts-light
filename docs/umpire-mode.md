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
Tablet → **⚙ (Header) → PIN** (Standard `0000`, einstellbar; innerhalb der
Fünf-Minuten-Freigabe nach einer Eingabe entfällt sie) → **„Schiri-Modus: an"**.
Opt-in pro Tablet, lokal gespeichert (`localStorage`). Es erscheint eine **immer
sichtbare Ansage-Leiste**.

## Ansagen (Deutsch/DBV)
Aus dem aktuellen Spielstand erzeugt, **Aufschlägerstand zuerst**:

| Situation | Ansage |
|---|---|
| Eröffnung | „Meine Damen und Herren: zu meiner Rechten {rechts}, **und** zu meiner Linken {links}. {Aufschläger} schlägt auf {Rückschläger}. Null beide – bitte spielen." Die Reihenfolge ist fest: **immer erst rechts**, unabhängig davon, wer aufschlägt. |
| Punkt | „{Aufschläger}:{Rückschläger}" |
| Gleichstand | „{n} beide" |
| Satzpunkt / Spielpunkt | „{Aufschläger} **Satzpunkt** {Rückschläger}" — z. B. „20 Satzpunkt 6", bei Gleichstand „29 Satzpunkt beide". „**Spielpunkt**", wenn der Satz zugleich das Spiel entschiede. Das Wort steht immer zwischen Aufschläger- und Rückschlägerstand (DBV-Vokabular, Anhang 4, 3.1.10–3.1.13 und 7.1.3). Erkannt über die echte Satzregel samt Verlängerung und Obergrenze (21er: ab 20, bis 29:29; 15er: ab 14, bis 20:20; 11er: ab 10, bis 14:14). **Angesagt wird das Wort nur, wenn der Satzpunkt mit dem letzten Ballwechsel neu entsteht** (DBV Anhang 4, 7.1.1/7.1.2: „Sobald … erreicht"): 20:15 → „20 Satzpunkt 15", danach 20:16 → „20:16". Nach einem Gleichstand in der Verlängerung entsteht er neu (21:20, 22:21 …), an der Obergrenze für die zweite Mannschaft dazu („29 Satzpunkt beide"). Das gilt auch für die Freigabe nach einer Pause und die Ansage nach einer Karte. Fehlt der Stand davor (Einstieg mitten im Spiel), wird angesagt. Bis v0.9.400 stand das Wort bei **jedem** Ballwechsel mit Satzpunkt (Schiri-Rückmeldung 14.11.2026). |
| Aufschlagwechsel | „Aufschlagwechsel, {Stand}" |
| Nach jeder Pause | „{Stand}, bitte spielen." — die Freigabe gehört dazu; sie gilt bis zum nächsten Ballwechsel. |
| Punktepause | „{Stand} – Pause." — mit „Aufschlagwechsel, " davor, wenn der Punkt zur Pause ein Aufschlagwechsel war. **Nur im Entscheidungssatz** folgt „**Feldseiten wechseln.**" — nur dort wechseln die Spieler bei der Punktepause die Seiten. Die **Überschrift** der Pause nennt die Schwelle der jeweiligen Zählweise („Pause bei 11 Punkten", bei 15ern „Pause bei 8 Punkten"). |
| Satzende | „Satz. Der {n}. Satz wurde gewonnen von {Sieger} mit {Stand}." Ein „Seiten wechseln" wird nach dem Satz **nicht** angesagt (bis v0.9.350 stand dort „Bitte die Seiten wechseln."). Steht es danach **gleich**, folgt der Zwischenstand: „Einen Satz beide." (bzw. „Zwei Sätze beide.") — bei Führung sagt der Ablaufplan nichts, wir auch nicht. |
| Satzbeginn | „{N}. Satz. Null beide – bitte spielen." — steht es **Satz gegen Satz** (allgemein: beide eine Partie vor dem Sieg), heißt es „**Entscheidungssatz.**" statt der Nummer. |
| Spielende | „Spiel. Das Spiel wurde gewonnen von {Sieger} mit {Satzstände}." — Stände aus Siegersicht, **„und" vor dem letzten**. Die Satzbilanz („zwei Sätze zu eins") gehört nicht in die Ansage; die Stände sagen sie ohnehin. |

Badges: **Satzpunkt** / **Spielpunkt** (bis v0.9.350 „Satzball"/„Matchball";
Schiri-Feedback vom 08.11.2026 — das Vokabular sagt „Satzpunkt").
Das Abzeichen steht, **solange** ein Satzpunkt besteht — auch an den
Ständen, an denen die Ansage das Wort weglässt. Die Regel liegt in
`src/io/satzpunkt.mjs` (Test `scripts/test-satzpunkt.mjs`, Inline-Kopie im
Tablet).
<!-- pruef: "Spielpunkt" in src-tauri/assets/tablet.html -->
<!-- pruef: "Feldseiten wechseln." in src-tauri/assets/tablet.html -->

**Einspielzeit.** Ist die Wahl abgeschlossen (Aufstellung bestätigt), aber noch
kein Ballwechsel gespielt, steht über der Eröffnungsansage groß
„**Einspielzeit** M:SS" — gezählt ab der Bestätigung. Nach **2 Minuten** wird
sie rot. Mit dem ersten Punkt verschwindet sie; danach läuft die Spieldauer
klein in der Kopfzeile weiter. Die Einspielzeit erscheint **auch ohne
Schiri-Modus** — dann allein in der Leiste, ohne Ansage und Karten.
<!-- pruef: "Einspielzeit" in src-tauri/assets/tablet.html -->
<!-- pruef: /EINSPIEL_SEKUNDEN = 120;/ in src-tauri/assets/tablet.html -->

## Karten / Verwarnungen
Button **„Karte / Verwarnung"** → Spieler wählen → Farbe:

| Karte | Wirkung | Ansage |
|---|---|---|
| 🟨 Gelb | Verwarnung (kein Punkt) | „{Name}, Verwarnung wegen unsportlichen Verhaltens. {Stand}" |
| 🟥 Rot | **Gegner bekommt +1** (regulärer Punkt) | „{Name}, Fehler wegen unsportlichen Verhaltens. {Stand}" |
| ⬛ Schwarz | Disqualifikation (Anzeige/Protokoll) | „{Name}, disqualifiziert wegen **grober Unsportlichkeit**." — bewusst ein anderer Grund als bei Gelb und Rot. |

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

Nach **⇄ Seiten tauschen / Mannschaften tauschen** (seit v0.9.319) startet die
Ansage neutral mit dem Stand — ohne „Aufschlagwechsel" —, wie nach einem Undo.
Die Namen in der Ansage folgen der neuen Zuordnung von selbst.

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
