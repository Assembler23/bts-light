# 0060 — TP-Upload sperrt BTP-Writes kurz (Schreib-Gate)

- **Status:** accepted
- **Datum:** 2026-10-26

## Kontext

Die Spec [tp-datei-upload](../features/tp-datei-upload.md) kopiert die
**offene** TP-Datei, während BTP sie hält. Zwei Schreiber gibt es: BTP selbst
nach Eingaben des Turnierleiters (unsichtbar für uns) und BTP nach unseren
eigenen `SENDUPDATE`s. Die zweite Quelle kennen wir: Alle BTP-Writes laufen
durch vier Funktionen in `tablet/server.rs` (`write_result_to_btp`,
`write_courts_to_btp`, `write_officials_to_btp`, `write_highlight_to_btp`),
die nur `&AppConfig` entgegennehmen, keinen `TabletState`. Nach einem
bestätigten Write schreibt Jet die Seiten verzögert auf die Platte
(Nachlauf, ungemessen). Der vorhandene Merker `note_direct_btp_write` stempelt
nur Ergebnis-Writes nach dem Erfolg und weiß nichts von laufenden Writes.

Gebraucht wird eine **garantierte Lücke**: kein Write läuft, und seit dem
letzten Write ist genug Zeit für den Nachlauf vergangen, dann wird gelesen.

## Entscheidung

Ein prozessweites `WriteGate` in `btp/write_gate.rs` auf
`tokio::sync::RwLock<()>` mit einem `AtomicU64` für das Ende des letzten
Writes:

- Jeder BTP-Write hält für seine Dauer die **Lesesperre** (`schreiben()`,
  RAII-Guard; beim Drop wird der Zeitstempel gesetzt). Writes untereinander
  blockieren sich nicht, solange keine Kopie wartet.
- Die Kopie nimmt die **Schreibsperre** (`fuer_kopie(ruhe, max_wartezeit)`):
  wartet höchstens `max_wartezeit = 20 s` auf laufende Writes, hält dann
  `ruhe = 3 s` lang alle neuen Writes zurück und gibt erst danach den Guard
  frei, unter dem zweimal gelesen wird (Abstand 2 s, Byte-Vergleich). Nach
  dem Lesen wird die Sperre freigegeben, erst dann gesendet.
- Eine Instanz `GATE: LazyLock<WriteGate>` für den Prozess; Tests bauen eigene
  Instanzen (Unit-Tests teilen den Prozess, wie beim Testsystem-Schalter).

`tokio::sync::RwLock` ist schreiber-bevorzugt: Sobald die Kopie wartet,
reihen sich neue Writes hinter ihr ein. Das ist gewollt, denn genau das liefert
die garantierte Lücke. Wartezeiten ehrlich: Ein Write, der während der Kopie
eintrifft, wartet die Kopie ab (3 s Ruhe + zweimal Lesen mit 2 s Abstand,
typisch 5 s; mit der einmaligen Wiederholung bis etwa 9 s). Ein Write, der
eintrifft, während die Kopie noch auf einen langen Write wartet, wartet
Rest-Wartephase (höchstens 20 s) plus Kopie. Im Normalfall ohne langen Write
also nur die Kopie. Nichts wird verworfen; der Nachschub
(`flush_btp_retries`) läuft ohnehin verzögert. Die Konstanten stehen
kommentiert im Modul und werden nach der Nachlauf-Messung angepasst.

## Alternativen

- **Zähler `AtomicUsize` + Polling** (Vorschlag aus dem Grill): sichert nur
  gegen *laufende* Writes, nicht gegen einen, der in der Ruhezeit beginnt;
  braucht eine Schleife mit Abbruch. Die RwLock-Variante liefert die Lücke
  ohne Polling.
- **Nur Doppel-Lesen mit Byte-Vergleich:** billig, erkennt aber den
  Jet-Nachlauf nach unserem eigenen Write nicht und fängt einen Write
  zwischen den beiden Lesevorgängen nur zufällig. Bleibt als zweite Schicht
  Teil der Lösung.
- **Merker am `TabletState`:** scheitert daran, dass die Write-Funktionen
  keinen `TabletState` haben; eine Signaturänderung an 16 Aufrufstellen für
  einen Merker lohnt nicht.

## Konsequenzen

- Eine neue Stelle, an der ein BTP-Write kurz warten kann; dokumentiert in
  `docs/btp_protocol.md`. Im Normalbetrieb ohne Upload ist die Lesesperre
  frei und kostet nichts Messbares.
- Das Gate senkt das Risiko einer zerrissenen Kopie, beseitigt es nicht:
  Eingaben direkt in BTP sieht niemand. Dagegen stehen Byte-Vergleich im
  Client sowie Jet-Kopf, Parser und 50-%-Plausibilität im Server. Das steht
  so im Handbuch.
- Ein Upload während eines sehr langen Writes (bis ~30 s im Extremfall)
  scheitert nach 20 s mit „BTP ist gerade beschäftigt" statt den Write zu
  verdrängen. Bewusst: der Write geht vor.

## Nachtrag 2026-10-26 (PR L1, v0.9.300)

Umgesetzt wie beschrieben in `btp/write_gate.rs` (`WriteGate`, `GATE`,
`schreiben()`, `fuer_kopie()`), Guard-Zeile in den vier `write_*_to_btp`
nach den Frühausgängen für leere Listen. Abweichung: Der im Entwurf genannte
Zeitstempel des letzten Write-Endes (`AtomicU64`) entfällt — die Ruhe wird
immer voll gewartet, weil die Schreibsperre erst kommt, wenn der letzte
Reader gefallen ist; der Nachlauf beginnt also spätestens dann, und ein
Zeitstempel brächte nur Uhrsprünge ins Spiel. Konstanten unverändert
(`RUHE` 3 s, `MAX_WARTEZEIT` 20 s, `LESEABSTAND` 2 s) bis zur
Nachlauf-Messung. Tests mit pausierter Tokio-Zeit und ein Textwächter über
die vier Write-Funktionen in `tablet/server.rs`.
