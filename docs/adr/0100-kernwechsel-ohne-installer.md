# 0100 — Kernwechsel ohne Installer: Umbenennen-Tausch am Ort, signierter Kopf, Wächter aus dem alten Image

- **Status:** accepted
- **Datum:** 2026-11-16

## Kontext

Ein Update läuft heute über den NSIS-Installer (ADR 0057): Paket vorgeladen,
Installer-Lauf, Neustart, Wiederanlauf der Übertragung über das WebView. Die
Lücke wird auf rund 20 s geschätzt, gemessen ist sie nicht. Für einen Fix im
laufenden Turnier ist das zu lang, und es gibt keinen Weg zurück.

Befunde am Code:

- Die Programmdatei ist die ganze App (keine `bundle.resources`, `dist/`
  eingebettet); die Firewall-Regeln sind port-, nicht programmgebunden.
- `tauri_plugin_single_instance` hält einen benannten Mutex; ein zweiter
  Prozess beendet sich sofort.
- Ein gescheitertes Binden von Port 8088 wird nur geloggt.
- Das Relay weist einen zweiten Host im Namespace ab (R4) und hält 20 s ohne
  Host aus.
- BTP verbindet je Abruf neu; es gibt nichts zu übergeben.
- Das einzige Update-Artefakt ist der Installer; `latest.json` ist unsigniert.

## Entscheidung

- **Artefakt:** die nackte `bts-light.exe`, im Release-Job aus dem fertigen
  Installer entnommen. Signiert wird ein kleiner **Kern-Kopf** (Version,
  SHA-256, Größe, `rueckwaerts_lesbar_ab`); die exe hängt per Hash daran.
- **Tausch am Ort:** `bts-light.neu.exe` ins Installationsverzeichnis, dann
  `bts-light.exe` → `bts-light.vorher.exe` und `neu` → `bts-light.exe`.
- **Probelauf vor dem Angebot:** `bts-light.neu.exe --selbsttest`. Schlägt er
  fehl, wird der Tausch nicht angeboten.
- **Strikte Reihenfolge:** alte App hält die Übertragung an und schließt die
  Relay-Verbindung, tauscht, startet den Wächter und endet; erst danach
  startet die neue App.
- **Wächter** ist die alte Programmdatei mit `--waechter`, ohne Tauri und
  ohne Mutex. Er wartet auf das Ende der alten App, startet die neue und
  stellt die alte wieder her, wenn die neue vor ihrer Ankunftsmeldung endet
  oder die Frist abläuft.
- **Wiederanlauf im Kern** (`.setup()`) statt über das WebView.
- **Messung eingebaut:** Die App loggt „Port zu → Port wieder offen".
- **Rückfall auf den Installer-Weg**, wenn der Tausch nicht möglich ist
  (kein Schreibrecht, nicht Windows, Probelauf gescheitert).
- **„Beim Beenden einbauen" bleibt der stille Installer** (ADR 0057).

## Alternativen

- **Versionsordner im App-Datenverzeichnis plus Starter-Programm.**
  Verworfen: Verknüpfungen und pfadgebundene Firewall-Ausnahmen brechen, es
  braucht ein zweites Binary.
- **Beim Installer bleiben, nur den Rückweg ergänzen.** Verworfen: Die
  Laufzeit des Installers bleibt die Untergrenze der Lücke.
- **Neue App wartet selbst auf das Ende der alten, ohne Wächter.** Verworfen:
  Kein Rückfall möglich, wenn die neue Datei gar keinen Code ausführt
  (Virenscanner, beschädigte Datei).
- **Alte App bleibt als Wächter am Leben.** Verworfen: Sie hält Mutex und
  WebView und verlängert die Lücke.
- **Die exe direkt signieren.** Verworfen: Eine minisign-Signatur über eine
  nackte exe ließe sich Altversionen über ein gefälschtes `latest.json` als
  Installer unterschieben.
- **Socket-Übergabe oder Kern als Dienst** (Roadmap Stufe 3). Verworfen für
  diese Spec: großer Umbau, offene WebSockets reißen trotzdem ab.

## Konsequenzen

- ADR 0057 gilt weiter für Vorladen, Marker und „Beim Beenden"; der
  Wiederanlauf wandert in den Kern.
- Mehrere Annahmen zum Windows-Verhalten (laufende exe umbenennbar, Wächter
  überlebt das Beenden, Virenscanner, Reste des Deinstallierers) sind im Repo
  nicht prüfbar. Ein Messschritt (`tests/uebergabe_probe.rs`) läuft, **bevor**
  der Tausch scharf geschaltet wird; sein Ergebnis und die Wächter-Frist
  werden hier nachgetragen.
- Zwischen den zwei Umbenennungen gibt es ein Millisekunden-Fenster, in dem
  ein Stromausfall die Installation ohne `bts-light.exe` zurücklässt.
- Die Versionsanzeige unter „Apps & Features" bleibt bis zum nächsten
  Installer-Lauf alt.
- Der Kernwechsel gilt nur für Windows mit per-user-Installation; Linux und
  MSI behalten den bisherigen Weg.
- Der Release-Job veröffentlicht zwei zusätzliche Artefakte und einen Index.
- Der Deinstallierer braucht einen Hook, der die Hilfsdateien entfernt.
