# 0057 — Update-Ablauf im Rust-Kern, Wiederanlauf per Marker, stiller Installer beim Beenden

- **Status:** accepted
- **Datum:** 2026-09-06

## Kontext

Das Auto-Update lief bis v0.9.278 komplett im WebView über das Tauri-
Updater-Plugin: prüfen, `downloadAndInstall`, fertig. Drei Dinge fehlten,
und alle drei brauchen Wissen, das nur der Rust-Kern hat:

1. Ob die Übertragung lief, als das Update ausgelöst wurde — damit sie nach
   dem Neustart **von selbst** wieder anläuft.
2. Wie viele Felder belegt sind — damit die Turnierleitung den Moment des
   Neustarts bewusst wählen kann.
3. Der Schließpfad der App (`lib.rs`) — damit ein vorgemerktes Update beim
   Beenden eingebaut wird.

Dazu eine Eigenheit des Plugins: Der NSIS-Installer wird unter Windows immer
mit `/R` gestartet, die App geht danach also **wieder auf**. Für „beim
Beenden einbauen" ist das genau falsch.

## Entscheidung

- Der Update-Ablauf sitzt im Rust-Kern: `update.rs` (Tauri-frei: Marker,
  Installer-Argumente, Phasenmaschine) und `update_*`-Commands in
  `commands.rs`. Das Frontend stößt an und liest (`update_info`) — Regel R1.
- **Wiederanlauf per Marker** (`update-resume.json`, 15 Minuten gültig,
  einmal gelesen und gelöscht), nicht per dauerhaftem Autostart-Schalter.
  Der Wiederanlauf ist an das Update gebunden; nach einem Absturz oder
  Handstart bleibt das Verhalten wie bisher.
- **Beim Beenden** ruft die App den abgelegten Installer selbst auf —
  `/S /UPDATE`, ohne `/R`. Der Plugin-Weg (`/P /R`) bleibt für „jetzt neu
  starten".
- Das Paket wird **sofort nach dem Prüfen** geladen und im Speicher gehalten
  (etwa 10 MB) — der Einbau ist damit eine Sekundensache.

## Alternativen

- **Autostart bei jedem Start** („Übertragung lief beim Beenden" merken):
  verworfen. Nach einem Absturz oder einem bewussten Neustart soll die
  Turnierleitung entscheiden; ein Autostart, der einen kaputten Stand sofort
  weiterpusht, wäre die schlechtere Überraschung.
- **Relaunch nach dem Exit-Install stillschweigend beenden** (Marker + App
  beendet sich beim Start selbst): verworfen. Ein Fenster, das kurz aufgeht
  und wieder verschwindet, sieht wie ein Absturz aus; außerdem hinge das an
  der Sichtbarkeits-Reihenfolge von Tauri.
- **Alles im WebView lassen und den Marker per Command schreiben**:
  verworfen. Der Schließpfad in `lib.rs` kommt nicht ans Plugin-Objekt des
  WebViews; zwei Wahrheiten über „liegt ein Paket bereit?" wären die Folge.
- **Prozess-Übergabe / Dienst + Oberfläche (echtes Zero-Downtime)**: nicht
  verworfen, aber verschoben — Roadmap „Geplant". Die verbleibende Lücke ist
  Sekunden lang und wird von allen Geräten überbrückt.

## Konsequenzen

- `flush_live_scores` als Command bleibt registriert, wird vom Frontend aber
  nicht mehr gebraucht — der Flush sitzt jetzt in `update_install_now`.
- `AppState` hält das Update-Paket im Speicher; ein zweites Prüfen mit
  derselben Version lädt nicht erneut.
- Der stille Installer-Aufruf verlässt sich darauf, dass der Installer
  per-user läuft (siehe `installer/firewall-hooks.nsh`). Würde das Setup
  einmal auf per-machine umgestellt, bräuchte der Aufruf `ShellExecute`
  mit UAC — der Plugin-Weg macht das bereits so.
- Die Feldstempel-Übernahme (`reconcile_on_court`, erster Lauf) ändert das
  Verhalten nur beim Prozessstart; im laufenden Betrieb stempelt ein
  Feldwechsel weiterhin „jetzt".
