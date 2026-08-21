# Aushang für die Halle

Ein A4-Blatt zum Ausdrucken und Aufhängen. Es trägt zwei QR-Codes, die
Spielerinnen, Spieler und Zuschauer auf die beiden öffentlichen
badhub-Seiten des Turniers führen:

| Code | Ziel | Wofür |
|---|---|---|
| links (grün) | `…/live/<kürzel>/teilnehmer` | Teilnehmerliste → eigener Name → **persönliches Spielerprofil**: in welcher Halle man spielt (sobald es feststeht), wie viele Spiele noch vor einem liegen, wann das nächste Spiel voraussichtlich dran ist, dazu die eigenen Ergebnisse |
| rechts (orange) | `…/live?t=<kürzel>` | **Liveticker**: alle Felder, Punkt für Punkt in Echtzeit |

Dazu Turnierlogo und Turniername im Kopf, eine Kurzanleitung zum Scannen
und die beiden Adressen im Klartext — für alle, die lieber tippen.

## Bedienung

Hauptbildschirm → Abschnitt **„Aushang für die Halle"** → **Aushang
drucken**. Es öffnet sich eine Vorschau des Blattes; der Knopf **Drucken**
gibt es an den Drucker. Über den Druckdialog lässt sich das Blatt auch als
PDF speichern (Systemdialog des Druckers) — praktisch, um es in einem
Copyshop größer ausdrucken zu lassen.

Der Aushang hängt **nicht** am laufenden Liveticker: Er lässt sich vor dem
Turnier drucken und aufhängen. Was er braucht, ist nur die öffentliche
Live-Seite (siehe unten). Der Turniername kommt aus BTP, sobald die
Verbindung stand — vorher bleibt der Kopf ohne Namen, das Blatt
funktioniert trotzdem.

### Voraussetzung: die öffentliche Live-Seite

Einstellungen → **Badhub-Liveticker** → *Öffentliche Live-Seite*, z. B.
`https://badhub.de/live?t=bvbb`. Aus dieser einen Angabe leitet die App
beide Adressen ab. Fehlt sie, sagt die Vorschau genau das — geraten wird
nichts, ein falsches Kürzel führte die halbe Halle auf ein fremdes
Turnier.

Akzeptiert werden beide Schreibweisen, die auf badhub vorkommen:
`…/live?t=<kürzel>` und `…/live/<kürzel>`. Schema und Host übernimmt die
App unverändert, damit Testaufbauten (`http://localhost:8080/live?t=cup`)
auf derselben Installation bleiben.

### Turnierlogo

Dasselbe Logo wie auf dem Schiedsrichterzettel und im Liveticker:
Einstellungen → **Turnierlogo**. Ohne Logo trägt der Kopf nur die
badhub-Marke — das Blatt bleibt vollständig.

## Wie es gebaut ist

`src-tauri/src/aushang.rs` erzeugt das fertige HTML, der Tauri-Command
`aushang_html` reicht es an die Oberfläche, `AushangOverlay.tsx` zeigt es
in einem `iframe srcdoc` und druckt über den WebView. Das ist dasselbe
Muster wie beim Schiedsrichterzettel (**ADR 0039**): Das Dokument ist
skriptfrei, das `iframe` ist entsprechend sandboxed.

Die QR-Codes entstehen lokal (`qrcode`-Crate, Fehlerkorrektur **H**) —
kein Dienst im Netz sieht, welches Turnier hier läuft. Stufe H, weil das
Blatt tagelang in der Halle hängt, blass kopiert und geknickt wird.

### Zwei Fallen, die im Layout stecken

1. **Hintergrundfarben druckt der WebView nicht mit.** Deshalb steht das
   Blatt auf dunkler Schrift auf Weiß, Farbe liegt nur in Rahmen und Text;
   `print-color-adjust: exact` ist zusätzlich gesetzt. Dieselbe Falle hat
   in v0.9.250 das Raster des Schiedsrichterzettels gekostet.
2. **Das Blatt ist auf 297 mm gerechnet, und das Logo frisst die Reserve.**
   Karten wachsen mit dem Text; die beiden QR-Felder werden über
   `margin-top: auto` nach unten geschoben und bleiben so auf gleicher
   Höhe. Ohne Logo bleiben rund **13 mm** Luft, mit Logo nur noch **4 mm** —
   ein Logo macht die Kopfzeile 14 mm hoch. Deshalb ist der Turniername auf
   11,5 pt gesetzt: So bleibt er auch zweizeilig *innerhalb* der Logohöhe
   und schiebt nichts nach unten.

### Nach jeder Textänderung: Muster ansehen

Rust-Tests prüfen Inhalt, Adressen und Escaping — die **Höhe** prüft nur
der Browser:

```text
cd src-tauri
cargo run --example aushang_probe -- probe.html            # ohne Logo
cargo run --example aushang_probe -- probe-logo.html --mit-logo   # enger Fall
```

Beide Dateien im Browser öffnen und mit Strg+P als A4 hoch prüfen: Der
Inhalt muss auf **eine** Seite passen. Der Logo-Fall ist der maßgebliche —
er hat die kleinere Reserve.

## Grenzen

- **Ein Turnier je Blatt.** Mehrere parallele Turniere brauchen mehrere
  Ausdrucke; die App kennt immer nur die eine eingetragene Live-Seite.
- **Keine Halle im Code.** Beide Codes zeigen auf das ganze Turnier, nicht
  auf eine einzelne Halle. Für Halleninfos gibt es die Monitor-Seiten
  (`docs/court-monitor.md`).
- **Kein stiller Druck.** Anders als der Schiedsrichterzettel geht der
  Aushang bewusst über den normalen Druckdialog: Er wird ein-, zweimal pro
  Turnier gedruckt, meist auf einem anderen Papier oder in einem Copyshop.
