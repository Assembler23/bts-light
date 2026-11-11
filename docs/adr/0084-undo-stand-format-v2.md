# 0084 — Tablet-Zustand: Undo-Format v2, Mischversionen und Deckel

- **Status:** accepted
- **Datum:** 2026-11-11
- Berührt [ADR 0038](0038-ereignisse-append-only.md) (Ereignisse werden nicht
  zurückgerollt) und [ADR 0067](0067-mannschaften-berichtigt-kipp-ereignis.md) (Korrektur als eigener
  Undo-Schritt).
- Spec: [`tablet-stand-robust`](../features/tablet-stand-robust.md).

## Kontext

Der Tablet-Zustand wird je Punkt gespiegelt. 96 % davon sind 50 Undo-Stände, jeder mit
Vollkopien von Punktverlauf und Zettel-Ereignissen. Gemessen: 61–66 KB am Ende eines
normalen Dreisätzers, bis 590 KB im Schiri-Modus. Das Relay verwirft über 64 KiB still;
der LAN-Server hat keinen Deckel. Die Ereignis-Kopie hat keinen Leser.

## Entscheidung

1. **Format v2** (`fmt: 2`): Undo-Stände ohne Ereignis-Kopie; statt der Verlaufs-Vollkopie
   die Längen des Verlaufs je Satz. Wiederherstellen kürzt den aktuellen Verlauf. Der
   Korrektur-Schritt „Mannschaften tauschen" behält die Vollkopie, weil er den Verlauf
   kippt. Gemessen: 24 KB normal, 35 KB bei 63 Ereignissen.
2. **Die vollen 50 Undo-Schritte werden weiter gespiegelt** — ein Übernahme-Gerät braucht
   sie für Undo und korrektes „Wieder öffnen".
3. **Mischversionen:** Die neue Seite liest beide Formate und wandelt alte Stände beim
   Laden um. Eine alte Seite, die v2 liest, verliert nach einem Undo den Verlauf des
   laufenden Satzes („ab Zwischenstand"); das wird hingenommen.
4. **Das Tablet hält den Deckel selbst ein:** Über 56 KB kürzt es nur für den Spiegel die
   ältesten Undo-Stände. Host und Relay verwerfen über `MAX_STATE_LEN` (64 KiB, eine
   Konstante in `relay-proto`) mit gedrosselter Warnung ohne Inhalt.
5. **Monitore** erhalten in LAN und Cloud nur die schlanke Fassung ohne `history`,
   `rallyLog` und `events`.

## Verworfene Alternativen

- **Undo gar nicht oder nur wenige Schritte spiegeln:** macht den Review-Fix vom 18.08.2026
  rückgängig (kein Undo, fehlerhaftes „Wieder öffnen" nach Übernahme).
- **Nur die Ereignis-Kopie streichen:** 35–46 KB; reicht knapp, lässt aber keinen Puffer.
- **Delta-Sync je Punkt:** kleinste Last, aber ein zweites Protokoll mit eigener
  Lücken-Heilung.
- **Oberster Undo-Stand immer als Vollkopie** (alte Seiten verlustfrei für einen Schritt):
  dauerhaft zweiter Schreibpfad für einen Übergangsfall.
- **Host/Relay entpacken für alte Seiten:** koppelt beide an das Tablet-Format.
- **Server lehnt ab und meldet es dem Tablet:** neues Frame, wirkt nicht gegen ein altes
  Relay; die Selbstbegrenzung macht „nie still verworfen" per Konstruktion wahr.

## Folgen

- Der Zustand passt in allen gemessenen Szenarien mit Abstand unter den Deckel.
- Cloud-Monitore sehen keine Sanktions-Ereignisse mehr.
- Die Längen-Regel hängt daran, dass der Verlauf zwischen zwei Undo-Ständen nur wächst;
  jede künftige Operation, die den Verlauf umschreibt, braucht wie die Korrektur eine
  Vollkopie.
