# 0053 — Offene Spiele nehmen an der manuellen Spielreihenfolge teil

- **Status:** accepted
- **Datum:** 2026-08-30
- **Ergänzt:** [ADR 0023](0023-manuelle-spielreihenfolge-praefix-je-halle.md) ·
  [ADR 0026](0026-spielliste-eine-globale-reihenfolge-eine-liste.md) ·
  [ADR 0050](0050-verschiebe-modus-globales-einfuegeziel.md)

## Kontext

Mit der Spec [`tl-offene-paarungen`](../features/tl-offene-paarungen.md)
erscheinen Spiele ohne feststehende Teilnehmer in der TL-Spielliste. Damit
stellt sich die Frage, ob sie auch gezogen und als Ziel eines Zuges gewählt
werden dürfen.

Der Bestand sagt dazu heute dreierlei:

- `assign.rs::ready_queue` — die Ziel-Liste jeder `QueueReorder` — filtert
  Spiele ohne vollständige Mannschaften weg.
- `sync.rs::reconcile_queue_order` behält im Präfix ebenfalls nur Matches mit
  vollständigen Mannschaften; ein offenes Spiel im Präfix würde beim nächsten
  Sync-Takt gelöscht.
- `QueueOrderStore::reorder` verwirft einen Zug **still**, wenn das gezogene
  Spiel oder das Ziel nicht in der effektiven Liste steht
  (`queue_order.rs:150`, `:161`) — und `tl.rs` verwirft den Rückgabewert und
  antwortet unbedingt `ok: true`. Die Seite meldet dann „Reihenfolge geändert",
  obwohl nichts geschah.

Ohne Erweiterung wäre also **jeder** Zug eines offenen Spiels und jeder Zug
**vor** ein offenes Spiel eine grüne Erfolgsmeldung ohne Wirkung — die
Oberfläche löge, sichtbar, bei jeder Benutzung.

## Entscheidung

Offene Spiele nehmen **voll** an der globalen manuellen Reihenfolge teil.
`ready_queue` **und** `reconcile_queue_order` werden entsprechend erweitert:
Beide behalten Matches, denen nur die Teilnehmer fehlen, solange sie
`Scheduled` und nicht gerufen sind.

Eine manuell gesetzte Position bleibt damit erhalten, wenn ein offenes Spiel
durch ein BTP-Ergebnis zu einem vollständigen Spiel wird — die Reihenfolge
überlebt den Übergang, statt beim Bekanntwerden der Paarung zurückzuspringen.

Die übrigen Sperren bleiben: keine Feldzuweisung, keine automatische Vergabe,
kein Vorbereitungs-Aufruf.

## Alternativen

- **Nicht ziehbar, kein Zielplatz** — offene Spiele als reine Anzeige an ihrer
  Sortierposition. Ließe ADR 0023/0026/0050 unangetastet und wäre der kleinere
  Eingriff. Verworfen auf ausdrückliche Nutzer-Entscheidung: Wer den kommenden
  Verlauf sieht, will ihn auch ordnen; und die Position ginge beim Feststehen
  der Paarung ohnehin verloren.
- **Ziehbar, aber ohne Erweiterung von `reconcile_queue_order`** — der Zug
  wirkte bis zum nächsten Sync-Takt und verschwände dann. Verworfen: ein
  Verhalten, das sekundenlang funktioniert und sich dann selbst zurücknimmt,
  ist schlimmer als eine gesperrte Aktion.

## Konsequenzen

- **Negativ, bewusst getragen:** Der Präfix, den ein einzelner Zug nach
  ADR 0050 einfriert, wird größer — er reicht bis zur Zielposition, und
  zwischen den echten Wartenden stehen nun offene Spiele. Ein Zug weit nach
  unten friert entsprechend mehr Zeilen ein.
- **Negativ:** Über die Cloud kann ein Ziel serverseitig in `ready_queue`
  stehen, dem Gerät aber wegen der Kappung
  ([ADR 0051](0051-offene-spiele-eigene-gedeckelte-liste.md)) nie übertragen
  worden sein. `state.rs` dehnt das Fenster in diesem Fall bis zur
  Zielposition aus — der Zug wirkt, friert aber Zeilen ein, die das Gerät nie
  gesehen hat. Bereits heute so, durch dieses ADR nur häufiger.
- Der stille No-Op bei unbekannter ID bleibt bestehen; ein ehrlicher
  Ablehnungspfad ist eigener Arbeit vorbehalten.
- `src-tauri/tests/queue_order_consistency.rs` braucht eine ausdrückliche
  Ausnahme: Offene Spiele gehören **nicht** zum Reihenfolge-Vergleich zwischen
  TL-Web, Desktop-Vorbereitung und Liveticker, weil sie nur in TL-Web
  erscheinen.
