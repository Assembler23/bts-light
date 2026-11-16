# 0103 — Pi-Paketmodell und Rückfall: eine Quelle, Versionsverzeichnisse mit Symlink, fester Kern, Probe über Prozesse

- **Status:** accepted
- **Datum:** 2026-11-16

## Kontext

Die Pi-Dateien stecken heute als Heredocs im Installer
`pi/setup-monitor.sh`, zusammen mit `apt`, `raspi-config` und Änderungen an
der Boot-Partition. Ein Fernupdate (Spec `docs/features/pi-fernupdate.md`)
braucht ein Paket, das aus derselben Quelle entsteht wie das Image. Die Sets
werden hart vom Strom genommen; ein Update darf einen Pi in keiner Phase
unbrauchbar machen. Niemand steht am Pi, wenn ein Paket nicht läuft.

## Entscheidung

- **Eine Quelle:** Die Dateien liegen als echte Dateien unter `pi/paket/`.
  Image und Paket entstehen aus demselben Baum.
- **Paket:** deterministisches, unkomprimiertes tar plus Textmanifest mit
  eigener Ganzzahl-Nummer und `image-min` (benötigte Image-Generation).
- **Tausch:** Versionsverzeichnisse `/opt/bts/pakete/<nr>` und atomarer
  Wechsel des Symlinks `aktiv`. Der Probe-Zustand wird vor dem Umschalten
  geschrieben. Aufbewahrt werden aktiv, vorher und der Werksstand.
- **Fester Kern:** Der Updater liegt nur im Image und ist nie Paketinhalt.
  Units und Einstiegspunkte sind fest und zeigen auf den Symlink.
- **Außerhalb des Pakets:** `config.txt`/`cmdline.txt`, Systempakete, neue
  Units, der Kern, die Schlüssel.
- **Neustart** nach jedem Update.
- **Probe in zwei Stufen, nur über Prozesse:** Kiosk-Skript stabil, dann
  Chromium stabil. Ein fehlender PC löst nie einen Rückfall aus. Drei Starts
  ohne Kiosk führen zum Rückfall.
- **Sperre:** Eine zurückgefallene Nummer spielt der Pi nie wieder ein; die
  Sperre endet mit einem bestätigten höheren Paket.
- **Kanal:** eigene Route `/pi-update` und eigener Dienst, unabhängig vom
  CEC-Dienst. Der Auftrag lebt nur im Arbeitsspeicher der App.

## Alternativen

- **Ganzes Image mit zwei Systemhälften:** deckt alles ab, aber 667 MB je
  Pi über das Hallen-WLAN und neu aufgeteilte Karten.
- **Dateien am Ort überschreiben:** nicht stromausfallfest.
- **Boot-Dateien oder Units im Paket:** Die FAT-Partition kennt keinen
  atomaren Tausch; Units sind Systemzustand außerhalb des Symlinks.
- **Dienste neu starten statt Neustart:** Die Probe braucht einen
  definierten Anfang.
- **Updater aktualisiert sich selbst:** Ein kaputter Updater könnte sich
  nicht mehr reparieren.
- **Mitfahren auf `/pi-tv`:** Der CEC-Dienst läuft nicht auf jedem Pi, und
  die Antwort ist auf 64 Byte gedeckelt.
- **Lebenszeichen-Datei des Kiosks:** ein neuer Vertrag zwischen tauschbarem
  Paket und festem Kern.
- **„PC erreichbar" als Gesundheitsmaß:** Ein ausgeschalteter PC würde
  gesunde Pakete zurückrollen.
- **App-Version als Paketnummer:** Nicht jede App-Version ändert das Paket;
  der Vergleich in der Shell wäre unnötig schwer.
- **Rückfall von der App gesteuert:** hilft nicht, wenn das Paket die
  Verbindung zum PC bricht.

## Konsequenzen

- Ein letztes Mal müssen alle Karten neu bespielt werden.
- Ein Fehler im Kern kostet eine weitere Kartenrunde; der Kern bleibt
  deshalb klein und wird gegen ein Wurzelverzeichnis in der CI getestet.
- Benennt ein Paket das Kiosk-Skript um oder wechselt den Browser, fällt es
  zurück.
- `setup-monitor.sh` ist nicht mehr als Einzeldatei vollständig; es lädt
  `pi/` bei Bedarf nach.
- `build.rs` ruft Node auf.
- Ein neues Image bleibt nötig für Betriebssystem, Systempakete,
  Startparameter, Units und den Kern.
