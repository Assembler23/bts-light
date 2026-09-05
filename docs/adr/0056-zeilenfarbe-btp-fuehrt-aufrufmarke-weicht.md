# 0056 — Zeilenfarbe: BTP führt, die Aufrufmarke weicht der Handfarbe

- **Status:** accepted
- **Datum:** 2026-09-05
- Spec: [features/tl-zeilenfarbe.md](../features/tl-zeilenfarbe.md)

## Kontext

BTP hält je Spiel ein Feld `Highlight` (0 = keine, 1–6 = eine von sechs
Farben aus dem Menü „Hervorheben"). bts-light benutzt dasselbe Feld seit
v0.9.160 als **Aufrufmarke**: Vorbereitungs-Aufruf → `1`, Aufruf-Ende → `0`
(P1, `sync.rs::reconcile_highlights`). Jetzt soll die Turnierleitung die
Farbe auch aus der Web-Sicht setzen und sehen. Zwei Schreiber, ein Feld:
Wer gewinnt, wenn ein von Hand gefärbtes Spiel gerufen wird — und wenn der
Aufruf endet?

Gemessen (05.09.2026): BTP nimmt jeden Wert 0–6 per `SENDUPDATE` an, zeigt
ihn sofort und behält ihn über Bedienschritte im Planer hinweg.

## Entscheidung

1. **BTP ist die Wahrheit für die Farbe** (R2). bts-light hält keinen
   eigenen Farbspeicher; die Seite zeigt, was der Snapshot liefert. Das
   einzige Lokale ist ein **Echo von 20 s** nach dem eigenen Write, damit
   die Farbe nicht erst mit dem nächsten Abruf erscheint — und das Echo
   erlischt, sobald BTP den Wert bestätigt oder die Frist abläuft.
2. **Die Handfarbe hat Vorrang vor der Aufrufmarke.** Der P1-Abgleich
   schreibt `1` nur an Spiele, die in BTP `0` tragen **und** die die
   Turnierleitung nicht über die Web-Sicht angefasst hat (Hand-Marke im
   `TabletState`, auch bei „keine"); er löscht am Aufruf-Ende nur, wenn BTP
   noch `1` liefert. Alles andere lässt er stehen. **Gelb an einem
   gerufenen Spiel gilt immer als Aufrufmarke** — auch wenn der Merkbestand
   des Syncs nach einem Neustart leer ist; sonst bliebe eigenes Gelb nach
   jedem „Speichern" der Einstellungen für immer stehen.
3. Die Farbe bleibt **intern** (Turnierleitungs-Seite, Feldkachel). Kein
   Monitor, kein Tablet, kein Liveticker.

## Alternativen

- **Aufrufmarke abschaffen**, `Highlight` ganz der Hand überlassen. Verworfen:
  Die Marke ist seit Monaten im Einsatz und der einzige Weg, einen
  bts-light-Aufruf im BTP-Planer zu sehen. Sie stört nur an Spielen, die
  ohnehin schon eine Farbe haben — genau dort weicht sie jetzt.
- **Aufrufmarke bekommt eine eigene Farbe** (z. B. immer Gelb, Hand darf
  Gelb nicht) und der Abgleich unterscheidet „unser Gelb" von „Hand-Gelb".
  Verworfen: BTP unterscheidet nicht, wer geschrieben hat; „unser Gelb" ließe
  sich nach einem Neustart nicht mehr von einem Hand-Gelb trennen. Ein
  Merkbestand in einer Datei wäre die nächste Wahrheit neben BTP.
- **Eigener Farbspeicher in bts-light** mit Rücksync. Verworfen: zweite
  Wahrheit (R2), und BTP hat die Farbe ja bereits.

## Konsequenzen

- Wer ein Spiel von Hand gelb färbt (Wert 1) und es dann ruft, sieht am
  Aufruf-Ende das Gelb verschwinden — Gelb an einem gerufenen Spiel gilt
  als Aufrufmarke, die Marke kann Hand-Gelb nicht von ihrem eigenen Gelb
  unterscheiden. Bewusst hingenommen: Ein Aufruf endet fast immer mit dem
  Ruf aufs Feld, und dort trug die Zeile im Planer bisher auch schon keine
  Farbe mehr. Wer eine Farbe über den Aufruf retten will, nimmt eine der
  fünf anderen.
- Der Merkbestand des P1-Abgleichs (`highlight_written`) lebt weiterhin nur
  in der `SyncEngine`; ein Neustart des Syncs vergisst ihn (bekanntes
  Muster). Deshalb die Gelb-Regel oben: Sie macht den Abgleich unabhängig
  davon, ob er sich an sein eigenes Gelb erinnert. Für die übrigen
  Handfarben ist der Neustart unerheblich — sie stehen in BTP; nur die
  Hand-Marke „bewusst auf keine gestellt" geht mit einem **App**-Neustart
  verloren (dann würde ein noch gerufenes, farbloses Spiel wieder gelb).
- Das 20-s-Echo liegt **einmal je Abruf** in `run_once` über dem Stand,
  bevor er an Anzeige und Abgleich geht — beide sehen denselben Wert. Die
  Frist läuft ab dem geglückten Write, nicht ab Eingang der Aktion, und
  eine rückwärts springende Uhr beendet das Echo statt es zu verlängern.
- Bei zwei Turnierleitungs-Geräten gewinnt der letzte Write — dieselbe
  Regel wie im BTP-Planer selbst.
