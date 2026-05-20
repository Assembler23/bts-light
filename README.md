# BTS Light

Plug-and-play Liveticker-Brücke zwischen **BTP** (Badminton Tournament Planner,
tournamentsoftware.com) und **badhub.de**.

BTS Light läuft als kleine Windows-App auf demselben Rechner wie BTP. Sie liest
über das TP-Network-Protokoll (TCP) den aktuellen Turnierstand aus und schickt
ihn an `badhub.de`, wo er als öffentlicher Liveticker erscheint. Zielgruppe sind
Turnierleiter ohne technischen Hintergrund: installieren, BTP verbinden,
Badhub-Passwort eintragen – fertig.

## Status

**Phase 4 – App-Oberfläche (in Arbeit).** Der gesamte Funktionskern
steht und ist getestet:

- **BTP-Anbindung** – TP-Network-Protokoll (Wire-Codec, VISUALXML,
  TCP-Client), gegen echte Turnier-Mitschnitte verifiziert. Spezifikation:
  [docs/btp_protocol.md](docs/btp_protocol.md).
- **Badhub-Payload** – Übersetzung in das `tset`/`tupdate_match`-Format
  inkl. Snapshot-Diff.
- **HTTP-Push** – Versand an `live_update.php`, end-to-end gegen
  badhub.de verifiziert.
- **Sync-Engine** – kompletter Poll-Push-Zyklus mit Resend-on-failure.

In Arbeit ist die Bedien-Oberfläche: Setup-Wizard, Dashboard,
System-Tray, Hintergrund-Polling.

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
bts-light/
├── src/            # React-Frontend (WebView-Inhalt)
├── src-tauri/      # Rust-Kern + Tauri-Konfiguration
├── tools/          # Entwicklungs-Werkzeuge (BTP-Capture-Skript)
├── docs/           # Protokoll-Spezifikation
└── .github/        # CI
```

## Arbeitsweise

Dieses Repo nutzt das Skill-Framework [obra/superpowers](https://github.com/obra/superpowers)
als Entwicklungsmethodik (Spec-First, TDD). Aktivierung via
`claude /plugin add obra/superpowers`.

## Lizenz & Herkunft

Privates Repo. Das TP-Network-Protokoll wird als eigenständige
Clean-Room-Implementierung nachgebaut – Details in [NOTICE.md](NOTICE.md).
