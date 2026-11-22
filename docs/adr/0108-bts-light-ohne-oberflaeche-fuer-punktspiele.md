# 0108 — bts-light ohne Oberfläche für Punktspiele: derselbe Kern als Serverprozess

- **Status:** accepted
- **Datum:** 2026-11-22

## Kontext

Punktspiele sollen sich ohne Windows-PC und ohne eigenen Router zählen lassen (badhub ADR-0092 Schritt 4). Die
Tablet-Seite, die Monitore und das Turnierleitungs-Web sprechen über den Cloud-Relay mit einem Host — und dieser Host
ist bts-light. Tilo hat am 22.11.2026 entschieden, dafür bts-light selbst auf dem Server laufen zu lassen, statt in
badhub eine zweite, einfachere Zählseite zu bauen.

Befund am Code: Tauri wird nur in `lib.rs` und `commands.rs` benutzt. Die Oberfläche bekommt keine Ereignisse aus der
Übertragung. `start_sync_kern` braucht die Tauri-App nur für drei Verzeichnisse und um den Zustand wiederzufinden.

## Entscheidung

1. **Ein Schalter `--kopflos <ordner>`** am bestehenden Programm, ausgewertet vor Tauri. Keine zweite Binary, kein
   eigenes Crate: derselbe Kern, dieselben Tests, dieselbe Auslieferung.
2. **`start_sync_kern` ist über einen `Ort` parametrisiert** (Verzeichnisse und Zustand). Am Desktop ist das die
   Tauri-App, ohne Oberfläche ein fester Ordner. Der Ablauf dahinter ist derselbe.
3. **Selbst koppeln aus einem Auftrag**, den badhub in den Ordner legt. Das Gerätetoken entsteht im Prozess und liegt
   verschlüsselt in seinem Ordner — badhub muss das Dateiformat nicht nachbauen.
4. **Kein Liveticker** in diesem Betrieb (`liveticker_aus`, geschlossenes Tor). Der Punktestand laufender Spiele geht
   mit den Meldungen des Punktspiels an badhub. Das gilt für jede Punktspiel-Übertragung, auch am Desktop.
5. **Nur die Quelle Punktspiel** läuft ohne Oberfläche.

## Alternativen

- **Eigene Zählseite in badhub** ohne bts-light. Einfacher im Betrieb, aber ein zweites Zählwerk mit eigenem Verhalten
  (Aufschlag, Seitenwechsel, Korrekturen) und ohne Monitore. Von Tilo verworfen.
- **Zweite Binary ohne Tauri.** Sauberer zur Laufzeit (keine GTK-Bibliotheken auf dem Server), verlangte aber, den
  Kern aus dem Tauri-Crate herauszulösen. Zu groß für diesen Schritt, bleibt möglich.
- **Das Desktop-Programm unter einem virtuellen Bildschirm.** Kein Umbau, aber ein Browser je Begegnung im Speicher
  und eine Einzelinstanz-Sperre, die mehrere Begegnungen verhindert.
- **Token-Dateien von badhub schreiben lassen.** Verworfen: zwei Stellen müssten dasselbe Verschlüsselungsformat halten.

## Konsequenzen

- Der Server braucht die Laufzeit-Bibliotheken von GTK und WebKit, obwohl kein Fenster entsteht.
- Je Begegnung läuft ein Prozess mit eigenem Ordner. Start, Ende und Aufräumen gehören einem Dienst auf dem Server,
  der noch fehlt.
- `start_sync_kern` ist generisch. Am Desktop bleibt das Verhalten gleich, die Rust-Suite hält es fest.
- Die `install_id` der Begegnung ist am Relay das Geheimnis für die Tablet-Links. Sie muss je Begegnung neu und
  zufällig sein und darf nur in Mein Badhub erscheinen.
- Ein Programm-Update auf dem Server wirkt erst für neu gestartete Begegnungen.
