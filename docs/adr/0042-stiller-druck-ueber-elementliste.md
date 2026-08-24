# 0042 — Stiller Druck über eine Elementliste, nicht über den WebView

- **Status:** accepted
- **Datum:** 2026-08-20

Gehört zu [docs/features/schiedsrichterzettel-autodruck.md](../features/schiedsrichterzettel-autodruck.md).
Ergänzt [ADR 0039](0039-zettel-html-im-webview.md), hebt ihn nicht auf.

## Kontext

Der Zettel soll bei der Feldvergabe **selbsttätig** auf Papier gehen: ohne Klick, ohne Dialog, an
einen in den Einstellungen gewählten Drucker. Der bisherige Weg (`iframe srcdoc` +
`contentWindow.print()`, ADR 0039) kann beides grundsätzlich nicht — er öffnet immer den
Systemdialog und kennt keinen Zieldrucker. Im Repo gibt es keinen Druckcode, keine PDF-Bibliothek
und keine Windows-Abhängigkeit.

Zugleich wird das Blatt mit dieser Spec zu einem **exakten Formularraster** in Millimetern
([ADR 0043](0043-zettelblatt-nach-dbv-vorbild.md)) — Positionen, die ohnehin gerechnet und nicht
vom Textfluss gefunden werden.

## Entscheidung

**Das Blattlayout wird einmal als reine Funktion in eine Elementliste gerechnet** —
`blatt(doc) -> Vec<Seite>`, jede Seite eine Liste aus Linien, Kästen, Texten und (optional) dem
Turnierlogo, alle Maße in Millimetern. Die Seitenaufteilung gehört damit in die reine Funktion,
nicht in die Treiber. Zwei dünne Treiber geben dieselbe Liste aus:

- **HTML** (`scoresheet::render_html`) für Bildschirm, TL-Web und den von Hand ausgelösten Druck —
  der Weg aus ADR 0039 bleibt damit unverändert bestehen.
- **GDI** (`print/windows.rs`: `CreateDCW(printer)`, `StartDoc`, `Rectangle`, `TextOutW`,
  `EndDoc`) für den stillen Druck an einen benannten Drucker. Die Druckerliste kommt aus
  `EnumPrintersW`.

Neue Cargo-Abhängigkeit: `windows` mit `Win32_Graphics_Gdi` und `Win32_Graphics_Printing` —
MIT/Apache-2.0, von Microsoft gepflegt, transitiv ohnehin im Baum.

## Alternativen

- **WebView2 `ICoreWebView2_16::PrintAsync`** mit `PrinterName` und `Landscape`, aus einem
  versteckten Fenster. Behielte HTML als einzige Ausgabe, hängt aber an COM-Interop über
  `with_webview`/`webview2-com` (0.38.2 liegt transitiv vor, ist im Projekt unerprobt), am
  Fensterzustand und am Ladezeitpunkt der Seite — und ist praktisch nicht unit-testbar.
  **Nicht gewählt, aber als Rückfallpfad benannt**, falls GDI den Textsatz im Feldtest nicht trägt.
- **PDF erzeugen und mit einem Fremdprogramm drucken** (SumatraPDF, Adobe Reader). Verworfen:
  GPL-Bündelung in einer signierten App ist heikel, ein PDF-Handler ist nicht garantiert
  vorhanden, und für Turnierleiter ohne IT-Kenntnisse wäre Plug-and-play dahin.
- **Rohdruck in Druckersprache** (PCL/PostScript). Verworfen: hängt am Druckermodell.

## Folgen

- **Das Layout ist testbar, ohne zu drucken.** Zellbreite, Blockfolge, Namenskürzung und beide
  Blattbudgets sind Asserts auf einem reinen Wert. Das war beim HTML-only-Weg nur indirekt
  möglich — und genau dort ist v0.9.246 mit 16 mm Überlauf durchgerutscht.
- **Zwei Ausgaben, ein Layout.** Kleine Abweichungen bleiben: HTML kürzt lange Namen über
  `text-overflow`, der GDI-Treiber rechnet die Kürzung selbst (`GetTextExtentPoint32W`). Beide
  Wege werden gegen dasselbe Kriterium geprüft.
- **Der Druck lebt im Kern**, wo auch der Auslöser sitzt — kein Fenster, kein WebView, keine
  Frontend-Beteiligung. Damit gilt R1 ohne Sonderfall.
- **Windows-only.** Das Projekt ist eine Windows-Desktop-App; der Treiber steht hinter
  `#[cfg(windows)]`, damit Tests und Prüfläufe plattformunabhängig bleiben.
- Ein Druckername aus der Config geht in eine Win32-Funktion — `security-reviewer` in der
  Druck-Etappe.
