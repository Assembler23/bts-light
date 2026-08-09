# 0012 — Ergebniskorrektur nur, wo nichts daran hängt

- **Status:** accepted (vorläufig — der entscheidende Versuch steht aus)
- **Datum:** 2026-08-09

## Kontext

Die Turnierleitungs-Oberfläche soll ein bereits gewertetes Spiel
überschreiben können — ein Zahlendreher beim Eintragen ist der häufigste
Grund, an den Turnier-PC zurückzulaufen. Die Spec ließ genau eine Frage
offen ([features/turnierleitung-web.md](../features/turnierleitung-web.md),
offener Punkt 1): **Was macht BTP mit dem Turnierbaum, wenn der Sieger einer
KO-Paarung nachträglich wechselt?**

Die Frage ist nicht akademisch. Ein Mitschnitt aus einem laufenden Turnier
(878 echte Paarungen) zeigt: Von den neun bereits gewerteten Spielen hatten
**alle neun** ein Folgespiel im selben Draw. Eine Regel „nur korrigieren,
wenn nichts dranhängt" sperrt dort also 100 % der Fälle — die Funktion wäre
praktisch wirkungslos, wenn die Regel bliebe.

Ein Experiment an einem Test-BTP (`src-tauri/tests/btp_overwrite_experiment.rs`)
hat geliefert:

1. **BTP nimmt Überschreib-Requests an** (`Result=1`, keine Ablehnung).
2. **Sie wirken nicht immer.** Bei einem von BTP selbst gewerteten Spiel
   wechselte `Winner` von 1 auf 2; bei einem, das derselbe Versuch kurz
   zuvor gewertet hatte, blieb er stehen — **trotz `Result=1`**.
3. **Die Kernfrage blieb offen**, weil in diesem Turnier gar nichts
   umzurechnen war: Das Folgespiel wurde nie besetzt, auch nicht nachdem
   beide Vorgänger gewertet waren. Damit ist auch die bisherige Annahme
   widerlegt, eine beendete Paarung werde selbst zum Feeder-Slot.

## Entscheidung

**Überschreiben ist erlaubt, wo es nichts umzurechnen gibt, und sonst
nicht** — mit je eigener Begründung statt einer Sammelabsage:

| Lage | Verhalten |
|---|---|
| Kein Folgespiel (Finale, Gruppenspiel, Draw ohne weitere Runde) | erlaubt |
| Folgespiel läuft bereits | abgelehnt — eine Korrektur zöge ein laufendes Feld mit |
| Folgespiel ist gewertet | abgelehnt — aus einem gültigen Ergebnis würde ein Rätsel |
| Folgespiel steht bereit, Sieger eingesetzt | abgelehnt, **bis das Experiment abgeschlossen ist** |

Ohne ausdrückliches `overwrite` bleibt es bei „bereits gewertet"
(`AlreadyScored`) — so ersetzt niemand versehentlich ein Ergebnis.

Das Folgespiel wird über die rohe Baumkante gefunden (`Match.From1`/`From2`
→ `PlanningID` im **selben** Draw; die Positionen sind nur je Draw
eindeutig). Umgesetzt in `tl::correction_blocker`.

## Alternativen

**(a) Alles erlauben, BTP macht das schon.** Verworfen: Punkt 2 des
Experiments zeigt, dass ein Überschreiben stillschweigend wirkungslos sein
kann. Wer darauf baut, meldet der Turnierleitung Erfolg, während in BTP das
alte Ergebnis steht — die schlimmste Art von Rückmeldung, weil sie beruhigt.

**(b) Alles verbieten, Korrektur nur am Turnier-PC.** Verworfen: Das Finale
und jedes Gruppenspiel sind unstrittig, und Gruppen sind der Löwenanteil
eines Breitensport-Turniers. Ein pauschales Verbot verschenkt sie.

**(c) Nachlesen statt vorher prüfen** — schreiben, dann kontrollieren, ob
der Sieger wirklich wechselte, und sonst zurückmelden. Nicht verworfen,
sondern **verschoben**: Das ist die richtige Ergänzung, sobald der Baumfall
geklärt ist. Für die heute erlaubten Fälle (nichts hängt dran) bringt es
wenig; für die gesperrten wäre es die Absicherung.

## Konsequenzen

- Die Korrektur ist im KO-Bereich vorerst kaum nutzbar — das ist der Preis
  dafür, den Turnierbaum nicht auf Verdacht anzufassen. Die Turnierleitung
  wechselt dort weiterhin an den PC, und die Ablehnung sagt ihr das.
- **Der Versuch bleibt offen** und braucht ein Turnier, in dem BTP die
  nächste Runde nachweislich füllt. Anleitung und Zwischenstand:
  [btp_protocol.md](../btp_protocol.md). Der Test ist wiederholbar und läuft
  standardmäßig nicht mit.
- Sobald die Antwort da ist, ändert sich genau ein Zweig
  (`CorrectionBlocker::Untested`) — plus, je nach Ergebnis, die
  Nachlese-Prüfung aus Alternative (c). Dieses ADR wird dann ersetzt.
- `Result=1` von BTP gilt in diesem Repo **nicht** als Beleg dafür, dass
  eine Änderung angekommen ist. Das betrifft nur den Korrektur-Pfad; die
  regulären Wertungen schreiben Werte, die vorher nicht dastanden.
