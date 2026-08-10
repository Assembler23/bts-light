# Feld-Raster je Halle — Mini-Spezifikation

> Status: Umgesetzt 2026-08-10 (Teil des Pakets „TL-Web-Ausbau").
> Betroffene Crates: `src-tauri/`, `src/`.

## Problem

Die Felderübersicht (App) und die Turnierleitungs-Weboberfläche (TL-Web)
zeigten Felder bisher in **Formularreihenfolge** — BTP liefert keine
Positionsangabe. In der Halle stehen sie aber in Reihen; „das Feld links
hinten" lässt sich aus einer Fließliste nicht ablesen. Wer helfen soll, ein
bestimmtes Feld zu finden, muss erst die Nummer suchen.

## Scope

- **Konfiguration je Halle**, am Turnier-PC gesetzt: Spaltenzahl, Start-Ecke
  (welche Ecke Feld 1 ist, aus Sicht der Turnierleitung auf die Halle
  geschaut), Schlangen-/Zick-Zack-Nummerierung an/aus.
- **Mapping** von Feld-Index → Bildschirm-Zelle (Spalte/Reihe), rein
  geometrisch, ohne Turnierlogik.
- Anwendung in **beiden** Oberflächen: App (`FieldOverviewPage.tsx`) und
  TL-Web (`tl.html`) — dieselbe Konfiguration, dieselbe Anordnung.
- **Listen-Position je Gerät** in TL-Web: Spielliste rechts neben oder
  unter den Feldern, eine reine Anzeigefrage ohne Turnierbezug.

**Bewusst verschoben:** Drag-&-Drop-Anordnung der Felder direkt in der
Oberfläche. Für den ersten Wurf reicht Spaltenzahl + Start-Ecke +
Schlange, um die allermeisten Hallen (rechteckiges Raster, durchgezählt
oder in Schlangenlinien) abzubilden. Frei ziehbare Positionen wären die
komfortablere Lösung für unregelmäßige Hallen, sind aber deutlich
aufwendiger (Persistenz je Feld statt je Halle, Speicher-UI, Zusammenspiel
mit wechselnden Feldzahlen) — siehe `docs/roadmap.md`.

## Datenmodell

Host-Konfiguration (`src-tauri/src/config.rs`):

```rust
pub struct HallLayoutConfig {
    pub hall: String,
    pub columns: u8,
    pub origin: LayoutOrigin,   // BottomLeft | BottomRight | TopLeft | TopRight
    pub serpentine: bool,
}
```

`AppConfig.hall_layouts: Vec<HallLayoutConfig>` — eine Halle ohne Eintrag
bleibt in der bisherigen Fließ-Darstellung. Verwaltet über die
Tauri-Commands `set_hall_layout`/`remove_hall_layout` (`commands.rs`,
Validierung: Spaltenzahl 1–12), aufgerufen aus dem Zahnrad an der
Hallenüberschrift in `FieldOverviewPage.tsx`.

Für TL-Web reicht dieselbe Konfiguration als **Wire-Kopie** in den
Anzeige-Zustand: `TlState.layouts: Vec<TlHallLayout>` (`tablet/tl.rs`),
`origin` dort bereits als snake_case-String (`"bottom_left"` usw.), damit
die Seite kein Rust-Enum kennen muss. Reine Geometrie, keine
Personendaten — auf der Allowlist ohne weitere Diskussion.

## Mapping

Kanonische Implementierung: `src/io/hallGrid.mjs`, Funktion
`gridPositions(count, { columns, origin, serpentine })` → Liste von
`{ col, row }` (Bildschirm-Koordinaten, `row 0` = oben, `col 0` = links).
Getestet in `scripts/test-hallgrid.mjs`.

`tl.html` kann keine ES-Module laden (statische Datei ohne Bundler) — dort
liegt eine **Inline-Kopie** derselben Funktion, mit Herkunfts-Kommentar,
nach demselben Muster wie die vorhandene Kopie von `gamePoint.mjs`.
Änderungen an der Mapping-Logik gehören **zuerst** in `hallGrid.mjs` und
werden von dort in `tl.html` gespiegelt — sonst laufen App und TL-Web
irgendwann auseinander.

Beide Verbraucher setzen das Ergebnis identisch um: der Kachel-Container
wird `display:grid` mit `grid-template-columns: repeat(columns, minmax(0,
1fr))`, jede Kachel bekommt `grid-column`/`grid-row` aus `col+1`/`row+1`
(CSS-Grid zählt ab 1). Ohne Layout bleibt die bisherige Fließanordnung
(`flex-wrap` bzw. `grid-template-columns: repeat(auto-fill, …)`)
unverändert.

## Vergleichsregel: Hallenname

Ein Layout gilt für eine Halle, wenn `hall.trim().toLowerCase() ===
konfigurierter_name.trim().toLowerCase()` — getrimmt und
groß-/kleinschreibungsunabhängig, dieselbe Regel wie beim
Disziplin/Klasse→Halle-Mapping. Umgesetzt an drei Stellen:

- `FieldOverviewPage.tsx` (`findHallLayout`) — JavaScript `toLowerCase()`.
- `tl.html` (`findHallLayout`, Inline-Kopie derselben Logik) — ebenfalls
  JavaScript `toLowerCase()`.
- Host-seitig beim Speichern/Entfernen (`AppConfig::upsert_hall_layout`,
  `remove_hall_layout`) — Rust `eq_ignore_ascii_case()`.

**Bekannte Einschränkung, bewusst nicht behoben:** JavaScripts
`toLowerCase()` ist Unicode-bewusst und faltet auch Umlaute korrekt
(`"Ä".toLowerCase() === "ä"`), Rusts `eq_ignore_ascii_case()` dagegen ist
reines ASCII und lässt `Ä`/`ä` als verschieden gelten. Für eine Hallenname
wie „Neuköln“ vs. „neuköln“ stimmen Client- und Host-Vergleich trotzdem
überein (beide sehen die Zeichen als gleich oder beide als verschieden,
je nach Groß-/Kleinschreibung *des Umlauts selbst*) — divergieren können
sie nur, wenn sich Groß-/Kleinschreibung **ausschließlich** an einem
Umlaut-Buchstaben unterscheidet (z. B. „Aussenplatz Ä“ vs. „aussenplatz
ä“ an einer Stelle, an der sonst nichts variiert — ein in der Praxis
seltener Fall, da Hallennamen aus BTP meist einheitlich geschrieben
ankommen). Diese Divergenz ist dieselbe, die bereits beim
Disziplin/Klasse→Halle-Mapping besteht; sie wird hier nicht neu
eingeführt, nur mitgeerbt. Sollte sie an einem echten Turnier zuschlagen,
ist die Behebung eine host-seitige Unicode-Casefold-Funktion statt
`eq_ignore_ascii_case` — bewusst nicht vorab investiert, ohne belegten
Bedarf.

## Listen-Position je Gerät (TL-Web)

Zwei Radio-Knöpfe im Anzeige-Menü, **rechts** (Standard) / **darunter**,
in `localStorage` (`bts-tl-liste`) persistiert — je Gerät, kein
Turnierstand, kein Wire-Feld. Umgesetzt als Körperklasse
(`body.liste-unten`), die die vorhandene Zweispalten-Regel für `main`
(≥1100 px) auf eine Spalte umschaltet und das Kleben der Feldspalte
(`position: sticky`) dabei aufhebt — geklebt neben einer gestapelten Liste
ergäbe keinen Sinn.

## Siehe auch

- [turnierleitung-web.md](../turnierleitung-web.md) — Bedienung
  („Anordnung wie in der Halle“, Anzeige-Menü).
- [multi-hall.md](../multi-hall.md) — Hallen-Gruppierung, in die das
  Raster einsortiert wird.
