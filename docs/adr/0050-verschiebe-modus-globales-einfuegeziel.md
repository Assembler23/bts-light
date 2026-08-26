# 0050 — Das Einfügeziel im Verschiebe-Modus bleibt global, nicht filterbewusst

- **Status:** proposed
- **Datum:** 2026-08-26

Gehört zu [docs/features/tl-spielliste-sprung-umsortieren.md](../features/tl-spielliste-sprung-umsortieren.md).
Ergänzt [ADR 0026](0026-spielliste-eine-globale-reihenfolge-eine-liste.md).

## Kontext

Die Spielliste der Turnierleitungs-Weboberfläche bekommt einen Verschiebe-Modus: Ein Spiel wird
gemerkt, danach scrollt man normal und tippt eine Zielzeile an — „hierhin, vor dieses Spiel".

Zwei Eigenschaften des Bestands stoßen dabei zusammen.

**Erstens speichert der Turnier-PC keine Positionen, sondern einen Präfix.**
`QueueOrderStore::reorder` (`src-tauri/src/tablet/queue_order.rs`) legt die Liste aller Match-IDs
vom Anfang der wirksamen Reihenfolge **bis einschließlich der neuen Position** ab. Alles davor wird
damit mit eingefroren — auch Spiele, die niemand angefasst hat. Alles dahinter folgt weiter der
Reihenfolge, die BTP vorgibt.

**Zweitens ist die Liste in TL-Web gefiltert.** `visibleQueue()` zeigt bei gesetztem Hallenfilter
nur die Spiele einer Halle. Die manuelle Reihenfolge selbst ist dagegen **turnierweit und global**
(ADR 0026) — `assign::ready_queue` kennt keinen Hallenfilter.

Beides zusammen heißt: Tippt jemand mit gesetztem Hallenfilter auf die dritte sichtbare Zeile,
kann das global Position 30 sein. Der Präfix friert dann alle 30 Spiele davor ein, darunter
Spiele der anderen Halle, die die Turnierleitung nie gesehen hat.

**Das ist heute schon so** — beim Ziehen gilt exakt dieselbe Mechanik. Neu ist nur, dass der
Verschiebe-Modus **Weitwürfe zum Normalfall macht**: Er existiert ja gerade dafür, ein Spiel über
fünfzig Zeilen hinweg zu versetzen. Was beim Ziehen ein seltener Nebeneffekt war, wird damit zur
Regel. Deshalb ist die Frage jetzt zu entscheiden, statt sie weiter mitlaufen zu lassen.

## Entscheidung

**Eine Einfügemarke bedeutet „vor diese Zeile in der globalen Reihenfolge" — genau wie beim
Ziehen.** Der Modus erfindet keine filterbewusste Semantik, und der Präfix nimmt unsichtbare
Spiele der anderen Halle weiterhin mit.

Die Wirkung wird stattdessen **dokumentiert**: `docs/turnierleitung-web.md` erklärt im Abschnitt
zum Umsortieren, dass eine Handsortierung alles vor der Zielposition festschreibt, und dass das
bei gesetztem Hallenfilter auch Spiele betrifft, die gerade nicht zu sehen sind.

## Alternativen

**B — Der Modus verlangt „Alle Hallen".** Bei gesetztem Hallenfilter wäre der Verschiebe-Modus
gesperrt oder würde den Filter aufheben.

Überraschungsfrei, und die Regel wäre leicht zu erklären. Verworfen, weil sie im Zwei-Hallen-Betrieb
genau dort stört, wo der Modus am meisten nützt: Wer eine Halle disponiert, filtert auf sie — und
müsste den Filter für jedes Umsortieren aufgeben und danach wiederherstellen. Zudem entstünde ein
**Unterschied zum Ziehen**, das bei gesetztem Filter weiterhin erlaubt wäre und dieselbe Wirkung
hätte. Zwei Bedienwege mit verschiedenen Regeln für dieselbe Fachlichkeit sind schlimmer als eine
überraschende Regel, die für beide gilt.

**C — Der Turnier-PC löst die Marke filterbewusst auf.** Die Aktion würde die Halle mitschicken,
und `reorder` würde den Präfix nur innerhalb der gefilterten Sicht bilden.

Fachlich am saubersten — und trotzdem verworfen, aus drei Gründen. Erstens bräuchte es einen neuen
`TlAction`-Vertrag in `relay-proto`, womit die Änderung die Wire-Ebene erreicht und einen
Relay-Deploy **und** einen App-Release-Tag braucht; LAN-Geräte hingen bis zum Tag hinterher, der in
diesem Projekt regelmäßig mehrere Versionen zurückliegt und nur von einem Admin gesetzt werden kann.
Zweitens widerspräche ein hallenbewusster Präfix ADR 0026, das die manuelle Reihenfolge bewusst als
**eine** globale Liste festgelegt hat — zwei Hallen könnten dann widersprüchliche Präfixe schreiben.
Drittens wäre die Semantik dann davon abhängig, welcher Filter auf welchem Gerät gerade steht;
dasselbe Spiel an dieselbe Stelle zu setzen ergäbe je nach Gerät ein anderes Ergebnis.

## Konsequenzen

**Positiv.** Ziehen und Verschiebe-Modus haben exakt dieselbe Wirkung — es gibt eine Fachlichkeit,
nicht zwei. Die Änderung bleibt vollständig in `src-tauri/assets/tl.html` und `src/io/`: kein
Rust, kein `relay-proto`, kein neuer Command, keine Konfigurationsfelder, keine Migration. Damit
ist sie durch bloßes Installieren einer älteren Version zurückrollbar, und der Cloud-Weg ist mit
dem nächsten Relay-Deploy live.

**Negativ, und bewusst getragen.** Bei gesetztem Hallenfilter bleibt eine Handsortierung
überraschend weitreichend: Sie schreibt Spiele fest, die gerade nicht sichtbar sind. Der
Verschiebe-Modus verschärft das, weil er Weitwürfe erst praktikabel macht. Dagegen hilft nur die
Dokumentation — und der bereits vorhandene Reset-Knopf im Panelkopf, der die Handsortierung des
ganzen Turniers verwirft.

**Offen gelassen.** Sollte sich im Betrieb zeigen, dass der Präfix bei Weitwürfen tatsächlich
stört, ist Weg C weiterhin baubar; er ist eine Erweiterung, keine Umkehr. Diese Entscheidung
verbaut ihn nicht.
