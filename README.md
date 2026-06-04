# BTS Light

Plug-and-play Liveticker-Brücke zwischen **BTP** (Badminton Tournament Planner,
tournamentsoftware.com) und **badhub.de**.

BTS Light läuft als kleine Windows-App auf demselben Rechner wie BTP. Sie liest
über das TP-Network-Protokoll (TCP) den aktuellen Turnierstand aus und schickt
ihn an `badhub.de`, wo er als öffentlicher Liveticker erscheint. Zielgruppe sind
Turnierleiter ohne technischen Hintergrund: installieren, BTP verbinden,
Badhub-Passwort eintragen – fertig.

## Status

**App funktionsfähig.** Funktionskern und Oberfläche stehen:

- **BTP-Anbindung** – TP-Network-Protokoll (Wire-Codec, VISUALXML,
  TCP-Client), gegen echte Turnier-Mitschnitte verifiziert. Spezifikation:
  [docs/btp_protocol.md](docs/btp_protocol.md).
- **Badhub-Payload** – Übersetzung in das `tset`/`tupdate_match`-Format
  inkl. Snapshot-Diff.
- **HTTP-Push** – Versand an `live_update.php`, end-to-end gegen
  badhub.de verifiziert.
- **Sync-Engine** – kompletter Poll-Push-Zyklus mit Resend-on-failure.
- **Oberfläche** – Setup-Wizard (BVBB-Preset oder manuell), Dashboard
  mit Live-Status, System-Tray mit Hintergrundbetrieb.
- **Auto-Update** – die App prüft beim Start und per Dashboard-Button auf
  neue Versionen und installiert sie auf Wunsch selbst (signierte
  Tauri-Updater-Artefakte, Hosting auf badhub.de). Release- und
  Update-Ablauf: [docs/release.md](docs/release.md).
- **Tablet-Spielzettel** – eingebetteter Server, an dem Schiedsrichter-
  Tablets im Hallen-WLAN hängen: Punkte zählen, Live-Score an den
  Liveticker, Endergebnis zurück nach BTP. Details:
  [docs/tablet.md](docs/tablet.md).
- **Cloud-Relay** – die Tablets erreichen bts-light wahlweise direkt im
  LAN oder über einen Relay auf badhub.de. Der Cloud-Weg funktioniert
  auch hinter gesperrten Firmen-Firewalls (nur ausgehende Verbindungen) –
  umschaltbar im Setup. Details: [docs/cloud-relay.md](docs/cloud-relay.md).
- **Diagnose-Logs** – tägliche Logdatei lokal (Dashboard-Button „Logs
  öffnen"); optional ein opt-in automatischer Upload an badhub.de zur
  zentralen Fehlerauswertung über alle Installationen. Details:
  [docs/logging.md](docs/logging.md).
- **Single-Instance** – bts-light läuft pro Rechner nur einmal; ein
  zweiter Start holt das bestehende Fenster nach vorn (sonst kollidierte
  der Tablet-Server-Port).
- **Court-Monitor (Raspberry Pi)** – ein Pi im Hallen-WLAN zeigt
  Live-Score bzw. Feldnummer als Kiosk auf einem TV. Ein **gemeinsames
  Image** bedient sowohl Tilos BTS als auch bts-light (Auto-Discovery per
  Subnetz-Scan, kein Karten-/Image-Tausch). Fertiges Image zum Schreiben
  mit Raspberry Pi Imager + Anleitung:
  [docs/pi-dual-image.md](docs/pi-dual-image.md) · Download (empfohlen, klein &
  auto-wachsend):
  <https://badhub.de/download/bts-light/pi-image/bts-light-pi.img.xz>.

Noch nicht signiert: Der Windows-Installer hat kein Code-Signing-
Zertifikat, daher zeigt Windows beim ersten Start eine SmartScreen-Warnung
– über „Weitere Informationen → Trotzdem ausführen" bestätigen. Das
Auto-Update ist davon unabhängig (eigenes Signaturschlüsselpaar).

Offene Punkte und geplante Arbeiten: [docs/roadmap.md](docs/roadmap.md).

## Stack

- **Tauri 2** – Windows-App mit nativem WebView
- **Rust** – App-Kern (BTP-Protokoll, HTTP-Push)
- **React 19 + Vite + TypeScript + Tailwind 4** – Setup-UI im WebView

## Entwicklung

Voraussetzungen: [Node.js](https://nodejs.org) (LTS) und die
[Rust-Toolchain](https://rustup.rs).

```bash
npm install          # Frontend-Abhängigkeiten
npm run tauri dev    # App im Dev-Modus starten
npm run tauri build  # Produktions-Build (Installer)
```

## Projektstruktur

```
bts-light/          # Cargo-Workspace
├── src/            # React-Frontend (WebView-Inhalt)
├── src-tauri/      # Rust-Kern + Tauri-Konfiguration (die App)
├── relay/          # Cloud-Relay-Dienst (Binary bts-relay)
├── relay-proto/    # geteilte JSON-Wire-Typen App ↔ Relay
├── pi/             # Kiosk-Launcher für den Court-Monitor-Pi (shared BTS/bts-light)
├── ops/            # systemd-/nginx-Vorlagen für den Relay
├── tools/          # Entwicklungs-Werkzeuge (BTP-Capture-Skript)
├── docs/           # Protokoll- & Feature-Doku
└── .github/        # CI
```

Der Relay baut sich separat: `cargo build --release -p bts-relay`.

## Arbeitsweise

Dieses Repo nutzt das Skill-Framework [obra/superpowers](https://github.com/obra/superpowers)
als Entwicklungsmethodik (Spec-First, TDD). Aktivierung via
`claude /plugin add obra/superpowers`.

## Lizenz & Herkunft

Privates Repo. Das TP-Network-Protokoll wird als eigenständige
Clean-Room-Implementierung nachgebaut – Details in [NOTICE.md](NOTICE.md).
