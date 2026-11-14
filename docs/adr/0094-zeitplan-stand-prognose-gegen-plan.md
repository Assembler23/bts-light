# 0094 — Zeitplan-Stand: Prognose gegen Plan je Zeitfenster, gerechnet am Host

- **Status:** accepted
- **Datum:** 2026-11-14

## Kontext

Das TL-Web soll auf einem Panel zeigen, ob das Turnier im Zeitplan liegt und um
wie viele Minuten es davor oder dahinter ist (Spec
[tl-zeitplan](../features/tl-zeitplan.md)). Drei Fragen haben mehrere
tragfähige Antworten:

1. **Woran wird gemessen?** BTP-Planzeiten sind Zeitfenster — alle Spiele eines
   Fensters tragen dieselbe Zeit. Die oberste Zeile der Warteliste ist zudem oft
   gerufen, von Hand vorgezogen oder von einem späteren Tag.
2. **Wo wird gerechnet?** Die Cloud kürzt die Warteliste auf 40 Zeilen, die
   Planzeit ist Wandzeit des Turnier-PCs ohne Zone, und ein zeitabhängiger
   Host-Wert bewegt die Revision des Zustands.
3. **Wie bleibt ein neues Panel in Bestandsprofilen aus?** Das Panelsystem
   ergänzt fehlende Schlüssel als sichtbar; der Host kennt Panel-Schlüssel nicht.

## Entscheidung

1. **Gemessen wird Prognose gegen Plan am nächsten unbegonnenen Zeitfenster von
   heute.** Fenster = Spiele gleicher Planzeit mit heutigem Datum. Begonnen =
   zwei Spiele des Fensters (bei einem Ein-Spiel-Fenster eines) sind gerufen,
   auf dem Feld oder beendet, Walkover ausgenommen. Bezugswert ist die
   **früheste** Prognose der wartenden, nicht gerufenen Spiele dieses Fensters.
   Hat das Fenster keine Prognose, bleibt die Kachel neutral.
2. **Der Host wählt Fenster und Bezugswert und liefert sie als kleines Objekt
   `schedule`; die Seite bildet nur die Differenz.** Sie rechnet gegen die Uhr
   des Turnier-PCs und klemmt eine alte Prognose auf „jetzt". Die
   zeitabhängigen Teile sind aus dem Fingerabdruck genullt. Das Objekt trägt
   keine Match-ID.
3. **„Standardmäßig unsichtbar" ist eine Regel der Seite** (`PANEL_STANDARD_AUS`):
   Ein fehlender Schlüssel dieser Menge wird als unsichtbar ergänzt. Keine
   Host-Migration.

## Alternativen

- **Planzeit des obersten Spiels gegen die Uhr.** Verworfen: meldet mitten im
  Zeitfenster Stunden Rückstand und springt bei jedem Umsortieren oder Aufruf.
- **Fensterlogik ohne Prognose** („dahinter", sobald ein Fenster fällig ist und
  das vorige noch wartet). Verworfen: kennt keinen Vorsprung und meldet erst,
  wenn der Rückstand schon eingetreten ist.
- **Frühestes wartendes Spiel überhaupt.** Verworfen: dauerhaft „dahinter",
  sobald ein Fenster mehr Spiele hat als Felder.
- **Fenster ab dem ersten Spiel begonnen.** Verworfen: Ein einzeln vorgezogenes
  Spiel ließe die Kachel am übernächsten Fenster messen.
- **Erstes Spiel der Liste statt Minimum.** Verworfen: Ein blockiertes Spiel
  meldete Verzug, obwohl das Fenster pünktlich anläuft.
- **Bei fehlender Prognose zum nächsten Fenster mit Prognose springen.**
  Verworfen: zeigt einen Wert, der am eigentlichen nächsten Fenster vorbeisieht.
- **Offene Paarungen in der Prognose-Simulation mitführen.** Fachlich richtig,
  aber ein eigener Eingriff in `predict.rs`; als Folge-Etappe vorgemerkt.
- **Alles auf der Seite rechnen.** Verworfen: scheitert an der 40er-Kürzung der
  Cloud und an der unbekannten Zeitzone.
- **Fertiger Minutenwert vom Host.** Verworfen: bewegt die Revision im
  Minutentakt (TL-Aktionen liefen auf „überholtem Stand") oder ist veraltet,
  solange sich am Brett nichts ändert.
- **Host-Migration aller Profile für das neue Panel.** Verworfen: Der Host
  müsste Panel-Schlüssel kennen.
- **Marke in der Kopfzeile statt Panel.** Verworfen: Gewünscht ist eine große
  Kachel.

## Konsequenzen

- Die Kachel braucht die eingeschaltete Spielzeiten-Prognose; ohne sie steht
  ein Hinweis.
- In der Endphase (nächstes Fenster nur offene Paarungen) und bei sehr groben
  Planzeiten zeigt die Kachel häufig einen neutralen Zustand.
- Ein Wert für das ganze Turnier; Hallen mit eigenen Zeitplänen werden nicht
  getrennt.
- Bestandsprofile schreiben `zeitplan` mit `visible: false` erst bei ihrer
  nächsten ohnehin gewollten Änderung aus.
- Die Umrechnung Wandzeit → Unix-ms zieht aus `badhub/payload.rs` nach
  `btp/model.rs`, weil Liveticker und TL-Zustand sie teilen.
- Der Profil-Schalter für die Planzeit an den Zeilen braucht das Relay vor dem
  Client; das Panel selbst nicht.
