# Spielzeiten & Startzeit-Prognose — Bedienung

> Spec: [features/spielzeiten-prognose.md](features/spielzeiten-prognose.md) ·
> ADR [0027](adr/0027-spielzeit-stempel-hostseitig.md) ·
> Technik BTP-Seite: [btp_protocol.md](btp_protocol.md) (Duration) ·
> TL-Web-Bedienung: [turnierleitung-web.md](turnierleitung-web.md)

## Was gemessen wird

bts-light misst je Spiel drei Zeitpunkte — automatisch, ohne Bedienschritt:

| Zeitpunkt | Bedeutung |
|---|---|
| **Bruttostart** | Erste Feldzuweisung durch BTP (Feldwechsel und App-Neustart ändern nichts; nimmt BTP das Spiel wieder ganz vom Feld, wird neu gemessen) |
| **Nettostart** | Der erste beim Turnier-PC eingehende Punkt |
| **Ende** | Eingang des Ergebnisses (eine spätere Korrektur ändert die Messung nicht) |

Daraus entstehen **Brutto** (Zuweisung → Ende), **Netto** (1. Punkt → Ende,
Satzpausen zählen mit) und die **Anlaufzeit** (Brutto − Netto: Weg zum Feld,
Einspielen, Anschreiben). Gespeichert wird turniergebunden in
`match-times.json` — ein App-Neustart verliert nichts, ein Turnierwechsel
beginnt frisch. In die **Statistik** zählen nur regulär über ein Tablet zu
Ende gespielte Partien (kein Walkover, keine Aufgabe/Disqualifikation, keine
von Hand nachgetragenen Ergebnisse); Ausreißer über 6 Stunden werden
verworfen.

**BTP** bekommt die Bruttozeit wie bisher im Feld `Duration` — jetzt auch
nach einem App-Neustart mitten im Spiel, beim Backend-Ergebnis und bei der
TL-Web-Wertung (früher stand dort 0). Kampflose Spiele melden weiterhin 0.

## Die Prognose („wann bin ich dran?")

Die TL-Web-Spielliste zeigt an jedem wartenden Spiel den voraussichtlichen
Aufruf, z. B. **„🕐 14:32"**. Gerechnet wird je Klasse × Disziplin mit dem
**Median** der gemessenen Bruttozeiten (eigene Werte ab 3 Spielen, davor
Klasse gesamt → Turnier gesamt → eingestellter Startwert), simuliert über
freie und belegte Felder, die Spielreihenfolge, Hallen-Regeln,
Spieler-Mindestpausen und 2 Minuten Übergangszeit je Feldwechsel.

- **„~14:32"** (mit Tilde): Hinter dem Wert steht nur der Startwert aus den
  Einstellungen — noch keine Messwerte. Entsprechend grob ist die Prognose.
- **„gleich"**: Das Spiel ist als Nächstes dran, sobald ein Feld frei ist.
- Von der automatischen Feldvergabe **ausgenommene** Spiele bekommen keine
  Prognose (die Vergabe überspringt sie ja tatsächlich).

Einstellungen: SetupWizard → **„Startzeit-Prognose"** (Anzeige an/aus,
Startwert in Minuten; Standard: an, 25 min).

## Live-Restzeit laufender Spiele (Etappe D)

Sobald ein Feld **live zählt** (Tablet oder Zähltafel verbunden bzw. schon
Punkte gemeldet), schätzt der Host die Restzeit des laufenden Spiels aus dem
**Satzstand** statt nur aus „Median minus verstrichene Zeit":

- Ein Spiel bei **14:6 im dritten Satz** blockiert sein Feld nur noch wenige
  Minuten — die Prognosen aller wartenden Spiele rücken entsprechend vor.
- Ein Spiel, das **bei 0:0 steht**, hält sein Feld die volle erwartete
  Nettodauer plus restliche Anlaufzeit — es wird nicht mehr „freigerechnet",
  nur weil die Zuweisung schon eine Weile her ist.
- Gerechnet wird mit dem **Eigentempo** des Spiels (gemessene Sekunden je
  Punkt, anfangs mit dem Gruppen-Median geglättet) und dem Zählsystem des
  Matches (Best-of, Zielpunkte, Deckel). Ein möglicher
  **Entscheidungssatz** zählt mit seiner **Wahrscheinlichkeit** hinein,
  geschätzt aus Satzstand und Punktstärke: Wer den ersten Satz 15:5
  gewonnen hat und im zweiten 10:6 führt, macht fast sicher in zwei
  Sätzen zu (dritter Satz ≈ 2 %); liegt derselbe Spieler 7:11 hinten,
  wird der dritte Satz sehr wahrscheinlich (≈ 80 %); bei 13:13 zwischen
  Gleichstarken zählt etwa der halbe Satz. Je erwartetem weiteren Satz
  kommen 2 Minuten Satzpause dazu. Friert ein Stand ein (Tablet
  ausgefallen), ist das Tempo auf das Doppelte des Normalwerts gedeckelt;
  die Restzeit insgesamt auf das Doppelte des Brutto-Medians.

Felder ohne Live-Zählung (Papier-Anschreiben) behalten das bisherige
Modell. Die Warteliste profitiert automatisch; zusätzlich kann die Kachel
jedes belegten Felds die Schätzung anzeigen („~12 min Rest"): TL-Web →
Profil bearbeiten → Anzeige → **„Restzeit laufender Spiele zeigen"**
(Standard: aus).

## Das Panel „Spielzeiten" (TL-Web)

Ein eigenes Panel (über das Profil ein-/ausblendbar) zeigt je
Klasse × Disziplin die Mediane von Brutto, Netto und Anlaufzeit sowie die
Zahl der Messungen — die Anlaufzeit ist dabei eine **Feld-Logistik-Metrik**
(wie lange dauert es vom Aufruf bis gespielt wird), keine Bewertung von
Spielern. Beendete Spiele tragen ihre Ist-Zeiten in der Beendet-Zeile
(„43 min (netto 37)").

## Grenzen

- Die Prognose ist eine **Anzeige-Hilfe der Turnierleitung** — kein
  verbindlicher Zeitplan und bewusst nicht auf Monitoren oder im
  badhub-Ticker sichtbar.
- Bei BTP-Zeitfenster-Planung (alle Vormittagsspiele „9:00") ersetzt die
  Prognose die BTP-Zeiten, sie nutzt sie nicht.
- Erfolgsmaß (Spec E12): am Testturnier ≥ 70 % der Spiele innerhalb
  ±10 Minuten — beim Bruttostart-Stempel schreibt der Host die zuletzt
  publizierte Prognose ins Diagnose-Log, daraus lässt sich das auswerten.
