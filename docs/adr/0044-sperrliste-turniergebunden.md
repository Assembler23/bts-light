# 0044 — Die Sperrliste gilt für ein Turnier

- **Status:** accepted
- **Datum:** 2026-08-23

## Kontext

Gesperrte Felder stehen als `config.locked_courts: Vec<i64>` in der
`config.json` — eine nackte Liste von CourtIDs ohne jeden Turnierbezug. Beim
Start der Übertragung werden sie in den Laufzeit-Zustand übernommen
(`commands.rs`), wo Auto-Vergabe und Hallen-Vorverteilung sie lesen.

**BTP vergibt CourtIDs je Turnier neu.** Eine Sperre, die am Samstag Feld 7
einer Halle meinte, trifft am Sonntag im nächsten Turnier ein beliebiges
anderes Feld. Es gibt keine Stelle, die sie beim Turnierwechsel zurücksetzt.

Bisher war das ein Randfall: Gesperrt wurde nur am Turnier-PC, von einer
Person, die sich meist erinnerte. Mit der Bedienung aus der Halle
([Spec](../features/tl-web-felder-sperren.md)) sperren mehrere Menschen von
mehreren Geräten — die Wahrscheinlichkeit, dass eine Sperre einfach
stehenbleibt, steigt deutlich. Und sie fällt niemandem auf: Ein gesperrtes
Feld sieht aus wie ein Feld, auf dem gerade nichts los ist.

Vergleichbare Zustände im Projekt sind längst turniergebunden — der
Schiedsrichter-Stand (ADR 0022), die manuelle Spielreihenfolge, die
Auto-Hallen (ADR 0029), die Spielzeiten. Alle nach demselben Muster: eigene
Datei mit dem Turnier im Kopf, Inhalt wird beim Wechsel verworfen.

## Entscheidung

Die Sperrliste bleibt in der `config.json`, bekommt aber einen Turnierbezug
(`locked_courts_tournament`) und wird **beim Turnierwechsel verworfen**.

Der `TabletState` merkt sich den Turniernamen und leert die Sperren in
`set_snapshot`, sobald er wechselt — an derselben Stelle, an der auch die
turniergebundenen Stores ihr `set_tournament` bekommen.

Die Config kann dadurch kurzzeitig eine Sperre des Vortags tragen: Sie wird
beim Start in den Laufzeit-Zustand geladen und beim **ersten Snapshot** des
neuen Turniers sofort verworfen. Das ist unkritisch, weil ohne Snapshot
ohnehin nichts vergeben wird; der nächste Sperr-Vorgang bereinigt auch die
Datei.

## Alternativen

**Turnierübergreifend lassen (Status quo).** Verworfen: Ein dauerhaft kaputtes
Feld über Turniere hinweg gesperrt zu halten klingt nützlich, funktioniert
aber nicht — die CourtID meint am nächsten Turnier etwas anderes. Der Nutzen
ist also nur scheinbar, der Schaden real.

**Eigener turniergebundener Store nach Muster ADR 0022/0029**
(`locked-courts.json` mit Turnier im Kopf). Das sauberste Muster und
konsistent mit den Nachbarn. Verworfen als Overkill: Die Sperrliste ist eine
Handvoll Zahlen ohne eigene Lebensdauer, ohne Nebendaten und ohne
Schreiblast. Ein eigener Store brächte eine Migration aus der bestehenden
Config, eine zweite Wahrheit während der Übergangszeit und mehr Code als der
Zustand wiegt. Die Konsistenz-Anforderung ist mit dem Turnierfeld erfüllt.

## Konsequenzen

- **Gut:** Ein Turnierwechsel räumt automatisch auf. Niemand sucht am
  Folgetag nach dem Grund, warum ein intaktes Feld leer bleibt.
- **Gut:** Alte Configs bleiben lesbar (`#[serde(default)]`, leeres Feld =
  „Turnier unbekannt"), neue für ältere Versionen ebenso — das unbekannte
  Feld wird ignoriert. Rollback bleibt möglich.
- **Preis:** Wer ein dauerhaft defektes Feld hat, muss es je Turnier neu
  sperren. Bewusst in Kauf genommen — die Alternative ist eine Sperre, die auf
  das falsche Feld zeigt.
- **Preis:** Die Sperre liegt weiter in der `config.json` und ist damit an das
  gemeinsame Speichern gebunden. Das verlangt, dass
  `keep_host_managed_fields` sie schützt — was bis zu dieser Spec **nicht** der
  Fall war und dort mitbehoben wird.
- **Abgrenzung:** Sollte die Sperrliste je mehr tragen als CourtIDs (Grund,
  Zeitfenster, Urheber), ist der eigene Store nach ADR 0022 der richtige
  nächste Schritt. Dann ist diese Entscheidung abzulösen.
