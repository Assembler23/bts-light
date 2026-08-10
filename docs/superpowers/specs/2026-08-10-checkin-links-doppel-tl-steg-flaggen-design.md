# Design: Check-In-Links & Doppel-Darstellung, TL-Web-Trennsteg, Flaggen-Fix

Stand: 10.08.2026 · freigegeben vom Nutzer im Gespräch („Passt alles so, leg los")

Vier Arbeitspakete auf einem Branch (`feat/checkin-links-doppel-tl-steg`),
je eigener Commit.

## 1. Flaggen-Fix (Bug, zugleich Ursache des „Hüpfens")

**Befund:** Die Cloud-TL-Seite liegt bewusst ohne Namespace unter
`/bts-relay/tl` (ADR 0012). Sie leitet die Flaggen-Basis aus ihrem Pfad ab
(`/bts-relay/flags/GER.svg`) — das Relay serviert Flaggen aber nur unter
`/{ns}/flags/{file}`. Jede Flagge läuft in ein 404; das `onerror` im
`nationBadge` tauscht das 20×14-`<img>` gegen den schmaleren Kürzel-Span.
Da der Poll die Listen alle 2 s neu aufbaut, wiederholt sich
Platzhalter → Fehler → Tausch bei jedem Zyklus — die Sicht „hüpft".

**Fix:**
- Relay: zusätzliche ns-lose Route `/flags/{file}`. Die Flaggen sind
  statische Länder-SVGs ohne Turnierbezug; ein Namespace ist nicht nötig.
  Dateiabruf als pure Helfer-Funktion (`flag_lookup`), von beiden Routen
  genutzt, mit Unit-Test (gefunden · unbekannt · Pfad-Traversal abgelehnt).
- tl.html (Verteidigungslinie): fehlgeschlagene Kürzel in einem `Set`
  merken; beim nächsten Rendern sofort den Kürzel-Span ausgeben statt
  erneut ein scheiterndes `<img>` einzusetzen. Damit hüpft auch bei einer
  echt fehlenden Flagge (unbekanntes Kürzel) nichts mehr.
- LAN ist nicht betroffen (dort gibt es `/flags/{file}` bereits).

## 2. Check-In: Links zur badhub-Seite

- `CheckinView` (Rust) bekommt `public_url` (`<basis>/checkin/<GUID>`) und
  `poster_url` (`…/aushang`). URL-Helfer neben `tl_url` in
  `checkin_state.rs`, Unit-Test u. a. für Basis mit/ohne Slash. Gefüllt in
  `commands.rs::checkin_state` aus `checkin_zugang`; leer, wenn nicht
  eingerichtet. Das Frontend baut keine URLs selbst (eine Wahrheit).
- `types.ts` nachziehen.
- `CheckinPanel`-Kopf (nur `ready`): „Check-In-Seite öffnen"
  (`openExternal`), „Link kopieren" (Zwischenablage, kurze
  „kopiert"-Rückmeldung), „Aushang (QR) öffnen".

## 3. Check-In: Doppel als eine Zeile je Paarung

- Pure Funktion `src/io/checkinPairs.mjs` (+ `.d.mts`), Muster wie
  `hallGrid.mjs`: Spieler einer Klasse nach `entry_id` gruppieren.
  **Paar nur, wenn `entry_id > 0` und genau zwei Spieler sie teilen** —
  Schutz gegen ein badhub ohne `entry_id` (dann 0 bei allen) und gegen
  Datenfehler (3+ Träger). Unvollständige Doppel bleiben Einzelzeilen.
  Reihenfolge: Position des ersten Partners in der Serverliste.
- Test `scripts/test-checkin-pairs.mjs` + CI-Schritt in `ci.yml`.
- `CheckinPanel`: eine `<li>` je Meldung; bei Paaren zwei Spieler-Zellen,
  durch „/" getrennt — jede Zelle mit Name, Verein, Zustand und eigenen
  Knöpfen (da/zurücksetzen/entsperren). Ein-/Auschecken bleibt je Spieler.
  Zählungen bleiben spielerbasiert.

## 4. TL-Web: Spielliste per Trennsteg ziehbar

- Trennsteg zwischen Felder-Sektion und Spielliste: nebeneinander
  (Zeilen-Layout) senkrecht → zieht Listen-**Breite**; gestapelt waagerecht
  → zieht Listen-**Höhe**. Pointer-Events, `touch-action: none` nur auf dem
  Steg (Technik wie beim Zieh-Griff).
- Speicherung je Gerät in `localStorage`: `bts-tl-liste-breite` /
  `bts-tl-liste-hoehe` (getrennt je Anordnung), bestehendes
  `bts-tl-*`-Muster.
- Grenzen: Liste nie unter 320 px Breite bzw. `--liste-min` Höhe; Felder
  nie auf null; Klammern bei Fenster-Resize.
- Ohne gespeicherten Wert Automatik wie heute; ein gesetzter Wert geht
  `fitCourts()` vor. Doppelklick/Doppeltipp auf den Steg löscht den Wert
  und stellt die Automatik wieder her.

## Doku-Pflege (je im selben Commit)

| Paket | Doku |
|---|---|
| 1 | `docs/cloud-relay.md` (Flaggen-Route), `docs/turnierleitung-web.md` |
| 2, 3 | `docs/spieler-check-in.md` |
| 4 | `docs/turnierleitung-web.md`, `docs/features/turnierleitung-web.md` |

Kein Versions-Bump (kommt mit dem nächsten Release).
