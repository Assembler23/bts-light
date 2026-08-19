# 0039 — Der Zettel wird einmal am Host als HTML gerendert und im WebView gedruckt

- **Status:** proposed
- **Datum:** 2026-08-19

Gehört zu [docs/features/schiedsrichterzettel-druck.md](../features/schiedsrichterzettel-druck.md).

## Kontext

Der Zettel soll aus zwei Oberflächen druckbar sein: der Desktop-App am Turnier-PC (auch
stapelweise für eine ganze Runde) und der Turnierleitungs-Seite im Browser.

Im Repo gibt es dafür bisher nichts: keinen PDF- oder Druck-Code, keine PDF-Abhängigkeit in
`src-tauri/Cargo.toml` oder `package.json`, keine eingebettete Schrift. `capabilities/default.json`
erlaubt nur `dialog:allow-open` — **`dialog:allow-save` fehlt**, und ein `fs`-Plugin gibt es
nicht. „Datei speichern" wäre also nicht ohne neue Abhängigkeit **oder** neue Permission zu haben.

Dazu eine Warnung aus dem Bestand: `timelineSetSvg` existiert **dreifach** — kanonisch in
`assets/tablet.html`, als Inline-Kopie in `assets/tl.html` und als JSX in `TimelineChart.tsx`.
Ein Zettel-Renderer ist deutlich umfangreicher als ein Verlaufsgraph; dieselbe Duplizierung
noch einmal einzugehen, wäre teuer.

## Entscheidung

**Genau eine Rust-Funktion `scoresheet::render_html(&[SheetDoc]) -> String` erzeugt ein
vollständiges, selbstgenügsames HTML-Dokument.** Inline `<style>` mit
`@page { size: A4 landscape; margin: 8mm }`, keine externen Ressourcen, **keine `<script>`-Tags**.

Zwei Aufrufer, null Kopie:
- **Route** `GET /tl/api/scoresheet/{ids}` — unter **identischem Pfad** am eingebetteten Server
  und am Relay, wie schon bei `tl/api/timeline`, damit `tl.html` in beiden Modi denselben String
  aufruft (R3).
- **Tauri-Command** `match_scoresheet_html` für den Desktop (R1 — kein `fetch` aus React auf
  `127.0.0.1:8088`, das bräche zusätzlich den Cloud-Only-Betrieb).

Beide Oberflächen zeigen das Ergebnis in einem `<iframe srcdoc>` und rufen
`contentWindow.print()`. „Als PDF speichern" geht über den Systemdialog des Druckers.

**Stapeldruck:** `render_html` nimmt eine Liste; jedes Match wird ein `<section>` mit
`page-break-after: always`, also ein Druckauftrag für eine ganze Runde. Harter Deckel
`MAX_SHEETS_PER_DOC = 40`.

Damit: **keine neue Cargo- oder npm-Abhängigkeit, keine neue Tauri-Permission**, und der Zettel
wird in **keiner** Client-Datei nachgebaut — das bewusste Gegenteil von `timelineSetSvg`.

## Alternativen

- **PDF-Crate in Rust.** Verworfen für die erste Fassung: neue Abhängigkeit samt Pflege- und
  Lizenzprüfung, dazu Schrifteinbettung, ohne die Umlaute unzuverlässig werden. Der Gewinn wäre
  ein Dateiartefakt — das der Systemdialog ebenfalls liefert.
- **PDF-Export aus dem WebView mit `dialog:allow-save`.** Verworfen: neue Permission für einen
  Effekt, den der Druckdialog schon hat.
- **Renderer im Client** (JS in `tablet.html`/`tl.html` plus JSX im Desktop). Verworfen: erbte
  die Dreifach-Duplizierung von `timelineSetSvg` in größerem Maßstab, und die Projektionslogik
  (Aufschlagfolge, Zellenraster) müsste dreimal identisch gepflegt werden.
- **SVG statt HTML** (wie bup es tut). Verworfen: HTML mit CSS-`@page` bringt Seitenumbruch,
  Wiederholungskopf und Textumbruch mit; bei SVG müsste jede Position von Hand gerechnet werden.

## Folgen

- Das Seitenbild hängt an der Druck-Implementierung des jeweiligen WebViews. **Erster Prüfpunkt
  der Umsetzung** ist ein Druck-Test unter Windows-WebView2 **und** Android-Chrome; fällt der
  aus, ist dieser ADR neu zu bewerten.
- Kein Dateiartefakt ohne Benutzerinteraktion — automatisiertes Archivieren („alle Zettel des
  Tages in einen Ordner") ist damit **nicht** möglich. Bewusst: bisher hat niemand danach gefragt.
- **Erstmals im Projekt wird HTML aus BTP-Fremdeingaben erzeugt** (Turnier-, Spieler- und
  Vereinsnamen) und in einem `iframe srcdoc` im Desktop-WebView angezeigt. Escaping ist
  zwingend, mit eigenem Test und `security-reviewer`.
- Das Dokument enthält bewusst kein Skript — dadurch bleibt es auch außerhalb des WebViews
  (etwa als gespeicherte Datei) harmlos und unverändert darstellbar.
- Die Projektionslogik liegt als reine Rust-Funktion vor und ist damit unabhängig vom Layout
  testbar (Struktur-Assert statt Pixelvergleich).
