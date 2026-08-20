# 0043 — Das Zettelblatt folgt dem DBV-Bogen, nicht dem eigenen Raster

- **Status:** accepted
- **Datum:** 2026-08-20

Gehört zu [docs/features/schiedsrichterzettel-autodruck.md](../features/schiedsrichterzettel-autodruck.md).
Ändert das Layout aus [schiedsrichterzettel-druck.md](../features/schiedsrichterzettel-druck.md).

## Kontext

Das bisherige Blatt ist eine Eigenkonstruktion: ein Block je Satz, 60 Spalten, Ballwechsel 61–120
als „Fortsetzungsgruppe" darunter, Protokolltabelle unter dem Raster. Solange der Zettel nur
**nach** dem Spiel als Archivbeleg gedruckt wurde, war das ausreichend.

Mit dem Vorabdruck wird das Blatt zum Arbeitsmittel **während** des Spiels. Schiedsrichter führen
den Bogen des Deutschen Badminton Verbands: sechs Blöcke à 33 Spalten, vier Zeilen je Block,
A/R-Spalte, Satzergebniskasten im Kopf, Unterschriftszeilen „Schiedsrichter" und „Referee". Wer
darauf schreiben soll, darf nicht erst ein fremdes Raster deuten müssen.

Die Vorlage (`Schiedsrichterzettel.pdf`, Blankobogen) wurde vermessen: A4 quer, Raster 275 mm,
Zellbreite 6,41 mm, Zeilenhöhe 5,27 mm, Blockraster 23,2 mm.

## Entscheidung

**Ein Layout für beide Zettelarten** — der ausgefüllte Archivzettel und der leere Vorabzettel sind
dasselbe Blatt, einmal mit und einmal ohne Einträge. Maße wie vermessen, als Konstanten in
`blatt.rs`.

Ein Satz beginnt in einem neuen Block und läuft bei mehr als 33 Ballwechseln im nächsten weiter;
reichen sechs Blöcke nicht, folgt eine zweite Seite mit verkürztem Kopf.

Drei Anpassungen gegenüber dem Vorbild und dem Bestand:

1. **Keine Verbandsmarke.** Logo und Schriftzug des DBV sind geschützt; im Kopf steht das
   Turnierlogo und der Turniername.
2. **Zeichenkonvention des Vorbilds:** `W` Warnung, `F` Fault, `R` Referee gerufen, dazu `D` für
   Disqualifikation — statt der hausgemachten `V`/`F`/`D`.
3. **Der Vermerk „Internes Turnier-Archiv — kein amtlicher Beleg" entfällt.** Er passt nicht auf
   ein Blatt, das während des Spiels geführt wird, und die Zusage wird damit ausdrücklich
   zurückgenommen — in dieser Spec, in `schiedsrichterzettel-druck.md` und in `umpire-mode.md`.
   Der Statussatz „offizielle Turniere laufen über das Original-BTS" bleibt davon unberührt.

Ebenfalls entschieden: **Der Verein wird gedruckt, wenn BTP ihn kennt** — unabhängig vom Schalter
`show_club_names`. Das Vorbild hat eine Vereinszeile, und der Verein steht ohnehin auf Aushang
und Meldeliste.

## Alternativen

- **Zwei Layouts** — Archivzettel bleibt, DBV-Blatt kommt als zweite Druckform daneben.
  Verworfen: zwei Renderer, zwei Budgets, zwei Wahrheiten, die auseinanderlaufen.
- **Bestehendes Raster beibehalten und nur leer drucken.** Verworfen: Der Zweck des Vorabdrucks
  ist ein Bogen, den Schiedsrichter ohne Erklärung führen können.
- **Verbandslogo einstellbar machen**, damit Berechtigte es hinterlegen. Verworfen für die erste
  Fassung: verlagert eine Rechtefrage auf den Anwender, ohne dass jemand danach gefragt hat.

## Folgen

- `SPALTEN_JE_GRUPPE` und die Fortsetzungsgruppe verschwinden; der Blockbegriff in
  `sheet_grid` ändert sich von „ein Block = ein Satz" zu „ein Block = 33 Spalten".
  Die Tests der Vorgänger-Spec zu Zellenzahl und Umbruch werden entsprechend umgeschrieben,
  die Wächter-Tests zu Datenschutz und Escaping bleiben unverändert grün.
- **Die Höhe wird erstmals zum Budget.** Sechs Blöcke plus Kopf und Fuß füllen das Blatt fast
  aus (199,2 von 200 mm) — es braucht neben `breitenbudget_geht_auf` zwingend
  `blatt_passt_in_die_hoehe`. Ohne ihn wiederholt sich der Überlauf von v0.9.246 in der anderen
  Achse.
- Schon gedruckte Zettel sehen anders aus als künftige. Papier ist unberührt, ein Nachdruck
  erscheint im neuen Layout — für ein internes Turnierdokument unkritisch.
