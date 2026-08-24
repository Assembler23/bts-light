# 0046 — Das Wunschfeld reserviert ab Spielbereitschaft

- **Status:** accepted
- **Datum:** 2026-08-24

## Kontext

Die Turnierleitung soll einem Spiel ein Feld zuweisen können, auf das die
automatische Vergabe wartet — der Anlass ist das Endspiel, das aufs Hauptfeld
gehört (Spec [tl-wunschfeld](../features/tl-wunschfeld.md)).

Der naheliegende Entwurf war ein Filter in der Vergabe: „Dieses Spiel bekommt
nur dieses Feld." Der Grill hat gezeigt, dass das den Zweck verfehlt. Die
Vergabe läuft über **freie Felder** und gibt jedes davon dem ersten passenden
Spiel. Der Filter schließt nur das Wunschspiel von anderen Feldern aus — er
hält das Wunschfeld nicht frei.

Der reale Ablauf: Das Hauptfeld wird frei. Das Endspiel steckt im selben
Moment in der Pflichtpause nach dem Halbfinale — die Lage, in der ein Endspiel
praktisch immer ist. Die Suche läuft weiter, ein anderes Spiel bekommt das
Feld, und es ist für vierzig Minuten weg. Das Feature hätte still nichts
bewirkt, und niemand hätte gesehen, warum.

Damit stand die eigentliche Frage: Soll der Wunsch auch das **Feld** binden?
Das ist keine Implementierungsfrage — eine Reservierung kostet Feldkapazität,
und in einem vollen Turnier ist ein leer stehendes Hauptfeld eine spürbare
Entscheidung.

## Entscheidung

Das Wunschfeld wird für sein Spiel freigehalten, **sobald das Spiel
spielbereit ist**: nicht von der Automatik ausgenommen, kein Spieler steht auf
einem Feld oder in der Pflichtpause.

Konkret bekommt die Vergabe zwei Filter statt einem:

1. Ein Spiel mit Wunschfeld bekommt **kein anderes** Feld.
2. Ein reserviertes Feld geht **nur** an sein Wunschspiel.

Zusätzlich legt das Wunschfeld die **Halle** des Spiels mit fest (als
`Manual`-Quelle in der Kaskade), damit Wunsch und Hallenbindung nach ADR 0030
nicht auseinanderlaufen können.

## Alternativen

**Nur das Spiel binden (reiner Filter).** Billig und ohne Kapazitätskosten —
aber wirkungslos, siehe oben. Verworfen.

**Immer reservieren, sobald der Wunsch gesetzt ist.** Erfüllt den Zweck
zuverlässig, hält aber das Hauptfeld möglicherweise stundenlang leer: Wer das
Wunschfeld am Morgen setzt, blockiert es den ganzen Tag. In einem vollen
Turnier ist das teuer, und der Preis wäre für die Turnierleitung schwer
abzuschätzen. Verworfen (Nutzer-Entscheid).

**Das Wunschfeld zusätzlich sperren.** Der naheliegende Bedienweg („ich sperre
Feld 1, damit es fürs Finale frei bleibt") funktioniert nicht: Gesperrte Felder
überspringt die Vergabe vollständig, auch für das Wunschspiel. Deshalb lehnt
das Setzen eines Wunschfelds auf ein gesperrtes Feld jetzt ausdrücklich ab —
mit dem Hinweis, dass die Reservierung die Sperre ersetzt.

**Eigene Kaskadenstufe `HallSource::Wish`.** Sauberer in der Benennung, aber
sie brächte eine neue Quelle auf die Wire, deren Bedeutung ältere Anzeigen
nicht kennen — für einen Unterschied, den die Turnierleitung ohnehin an der
Wunschfeld-Marke sieht. Verworfen zugunsten von `Manual`, was der Wunsch
inhaltlich auch ist: ein Hand-Eingriff für dieses eine Spiel.

## Konsequenzen

- **Gut:** Das Feature tut, was sein Name verspricht — das Endspiel bekommt
  sein Feld.
- **Gut:** Der Kapazitätspreis fällt nur an, solange er nötig ist. Ein
  Endspiel, dessen Spieler noch im Halbfinale stehen, blockiert nichts.
- **Preis:** Ein Feld kann leer stehen, während ein anderes Spiel wartet —
  genau für die Dauer, die das Wunschspiel zum Antreten braucht. Das ist
  gewollt und der Kern der Entscheidung.
- **Preis:** Die Regel „ab Spielbereitschaft" ist komplexer als beide
  Extreme und braucht eigene Tests — insbesondere den Fall, dass ein **nicht**
  spielbereites Wunschspiel sein Feld freigibt.
- **Folge für die Prognose:** Wunschspiele bekommen keine Startzeit. Die
  Simulation kennt nur Hallen; sie würde systematisch zu früh rechnen und die
  Startzeiten aller nachfolgenden Spiele mitverschieben. Lieber keine Zahl als
  eine falsche.
- **Abgrenzung:** Mehrere erlaubte Felder je Spiel („eines der beiden Center
  Courts") sind damit nicht abgedeckt. Wenn das gebraucht wird, ist es eine
  Erweiterung dieser Entscheidung, keine Korrektur.
