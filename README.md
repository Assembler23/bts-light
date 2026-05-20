# BTS Light

Plug-and-play Liveticker-Brücke zwischen **BTP** (Badminton Tournament Planner,
tournamentsoftware.com) und **badhub.de**.

BTS Light läuft als kleine Windows-App auf demselben Rechner wie BTP. Sie liest
über das TP-Network-Protokoll (TCP) den aktuellen Turnierstand aus und schickt
ihn an `badhub.de`, wo er als öffentlicher Liveticker erscheint. Zielgruppe sind
Turnierleiter ohne technischen Hintergrund: installieren, BTP verbinden,
Badhub-Passwort eintragen – fertig.

## Status

**Phase 0 – Skeleton.** Lauffähiges Tauri-Grundgerüst, noch ohne
BTP-Anbindung. Siehe Roadmap im Implementierungsplan.

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
└── .github/        # CI
```

## Arbeitsweise

Dieses Repo nutzt das Skill-Framework [obra/superpowers](https://github.com/obra/superpowers)
als Entwicklungsmethodik (Spec-First, TDD). Aktivierung via
`claude /plugin add obra/superpowers`.

## Lizenz & Herkunft

Privates Repo. Das TP-Network-Protokoll wird als eigenständige
Clean-Room-Implementierung nachgebaut – Details in [NOTICE.md](NOTICE.md).
