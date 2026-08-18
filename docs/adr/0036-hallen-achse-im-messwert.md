# 0036 — Hallen-Achse: Halle im Messwert stempeln statt zur Anzeigezeit nachschlagen

- **Status:** accepted
- **Datum:** 2026-08-18

## Kontext

Die Spielzeiten-Statistik (ADR 0027, `match-times.json`) misst je Match drei
Stempel und wertet sie zu Medianen aus. Ausgewertet wurde bisher **eine**
Achse: Klasse × Disziplin. Die Turnierleitung eines Zwei-Hallen-Turniers will
zusätzlich wissen, ob eine **Halle** systematisch langsamer läuft als die
andere (Spec [`tl-sicht-feinschliff`](../features/tl-sicht-feinschliff.md),
Punkt 1).

Der Messwert `MatchTimeEntry` trug `class_label` und `discipline` — beide
werden beim **ersten Feldzuweisungs-Stempel** (E4) mitgeschrieben, damit die
Statistik ohne Snapshot-Lookup rechnen kann. Eine Halle stand nirgends.

Damit standen zwei Wege offen. Die Entscheidung ist nicht folgenlos: Sie
bestimmt, welche Halle ein umgezogenes Spiel behält, ob alte Messwerte
mitzählen und was ein Rollback kostet.

Erschwerend kommt hinzu, dass es im System **drei** Hallen-Begriffe gibt: die
Halle des Felds, die Halle des Vorbereitungs-Aufrufs und die der
Auto-Vorverteilung (ADR 0029/0030). Genau einer davon wird gemessen, und das
muss festgelegt sein.

## Entscheidung

**Die Halle wird beim E4-Stempel in den Messwert geschrieben** — als
zusätzliches Feld `hall` an `MatchTimeEntry`, mit `#[serde(default)]`.

Gemessen wird die **Halle des Felds bei der ersten Feldzuweisung**, aufgelöst
über `BtpSnapshot::court_location_name` (`court.location_id → locations.name`).
Nicht die Halle des Vorbereitungs-Aufrufs und nicht die der
Auto-Vorverteilung — die Statistik misst, wo **gespielt** wurde, nicht wohin
gerufen wurde.

Der Stempel folgt damit demselben Vertrag wie `class_label` und `discipline`:
einmal beim Erststempel gesetzt, danach **immun gegen Feldwechsel**. Wechselt
ein Spiel die Halle, bleibt es in der Zeile seiner ersten Halle.

Als Schlüssel dient der **getrimmte Hallenname**, nicht die `location_id`.

Die Hallen-Achse bleibt **reine Anzeige**: Die Prognose-Fallback-Kette
(Klasse × Disziplin → Klasse → Turnier → Default) bleibt unverändert.

## Alternativen

**Halle zur Anzeigezeit aus dem BTP-Snapshot nachschlagen.** Verworfen.
Sie wäre immer aktuell und bräuchte keine Schema-Änderung — aber sie ist
genau dann weg, wenn man sie braucht: Sobald BTP ein beendetes Spiel vom Feld
nimmt, ist die Zuordnung Match → Feld → Halle nicht mehr auflösbar. Die
Statistik ist per Definition Rückschau über beendete Spiele; ein Verfahren,
das die Rückschau-Daten verliert, taugt dafür nicht. Der Nebeneffekt wäre
zudem, dass dieselbe Auswertung an zwei Tagen verschiedene Ergebnisse
liefert.

**`location_id` statt Hallenname als Schlüssel.** Verworfen, obwohl stabiler
gegen Umbenennung. Die Anzeige braucht ohnehin den Namen, und der Messwert
trägt bereits Klasse und Disziplin als **Text** — ein numerischer Fremd-
schlüssel neben zwei Textfeldern müsste zur Anzeigezeit wieder aufgelöst
werden, und zwar aus einem Snapshot, der die alte Halle womöglich nicht mehr
kennt. Das ist derselbe Fehler wie bei der ersten Alternative, nur kleiner.

**Halle bei jedem Stempel aktualisieren statt nur beim ersten.** Verworfen.
Sie widerspräche dem ausdrücklichen Vertrag des E4-Stempels („immun gegen
Feldwechsel und App-Neustart") und machte die Messung davon abhängig, wann
ein Spiel zufällig zuletzt angefasst wurde.

## Konsequenzen

**Positiv**

- Die Hallen-Zeile ist so belastbar wie die Klassen- und Disziplin-Zeile:
  Sie stützt sich auf dieselben Messwerte und dieselbe Erstzuweisungs-Logik.
- Kein Snapshot-Lookup zur Anzeigezeit — die Auswertung bleibt im
  bestehenden Cache-Vertrag (einmal je Messwert-Generation, nie je Poll).
- Alte `match-times.json` bleiben lesbar (`#[serde(default)]`).

**Negativ — bewusst in Kauf genommen**

- **Alte Messwerte tragen keine Halle.** Wird mitten im Turnier
  aktualisiert, sammeln sich die Spiele davor in einer Zeile „ohne Halle".
  Die Hallen-Achse startet faktisch neu.
- **Ein umgezogenes Spiel bleibt in seiner alten Hallenzeile.** Die Kehrseite
  der Feldwechsel-Immunität und in der Praxis der seltenere Fall.
- **Eine BTP-Umbenennung mitten im Turnier spaltet die Zeile** in zwei
  Hallen mit demselben Ort.
- **Ein Rollback ist nicht verlustfrei.** Eine ältere App-Version liest die
  Datei weiter, ignoriert `hall` aber und schreibt sie **ohne** das Feld
  zurück — alle bis dahin gestempelten Hallen sind dann verloren. Gehört in
  den PR-Text.
- Bei Ein-Hallen-Turnieren ist das Feld immer leer; die Achse wird dort
  ausgeblendet statt eine sinnlose Ein-Zeilen-Tabelle zu zeigen.

## Verweise

- Spec: [`docs/features/tl-sicht-feinschliff.md`](../features/tl-sicht-feinschliff.md)
- Berührt: [ADR 0027](0027-spielzeit-stempel-hostseitig.md) (Stempel-Quelle),
  [ADR 0029](0029-hallen-vorverteilung-eigener-store.md) /
  [ADR 0030](0030-halle-bindet-die-feldvergabe.md) (die anderen beiden
  Hallen-Begriffe)
- [`docs/multi-hall.md`](../multi-hall.md),
  [`docs/spielzeiten-prognose.md`](../spielzeiten-prognose.md)
