# Mitwirken an bts-light

Kurzer Mensch-Einstieg. Architektur, Coding-Standards und Subagent-Regeln stehen in
[`CLAUDE.md`](CLAUDE.md); Feature-Doku in [`docs/`](docs/); Architektur-Entscheidungen in
[`docs/adr/`](docs/adr/).

## ⛔ Harte Regeln

- **Keine Secrets/Tokens** ins Repo (das Repo ist **public**).
- BTP/Relay-Wire-Protokoll ist eine Clean-Room-Implementierung — Herkunft/Lizenz siehe
  [`NOTICE.md`](NOTICE.md).

## Quick Start

```sh
git clone https://github.com/Assembler23/bts-light.git && cd bts-light
npm ci
git config core.hooksPath .githooks   # lokales Pre-Commit/Pre-Push-Gate aktivieren
npm run tauri dev                      # App lokal starten
```

Der Hook prüft vor jedem Commit `cargo fmt --check`; `pre-push` ergänzt `cargo clippy`. Beide
spiegeln die CI — Notfall-Bypass: `git commit --no-verify`.

## Dev-Workflow

1. **Branch** pro Thema: `feature/…`, `fix/…`, `docs/…`.
2. **Pull Request** öffnen → Template ausfüllen. Die CI (`build`: fmt + clippy + `cargo test` +
   `npm run build`) läuft — das ist zugleich die **Regressions-Suite**: welche Verhaltensgarantien
   sie festnagelt und welche Regeln für neue Änderungen gelten, steht in
   [`docs/regression-suite.md`](docs/regression-suite.md).
3. **Self-Merge**, sobald `build` grün ist — `main` ist geschützt (kein Direkt-/Force-Push;
   Merge-Button erst bei grüner CI). Keine Approval-Pflicht.

## Release (separat vom Merge)

Ein Merge nach `main` shipped **nicht** automatisch. Release = manueller Tag:
```sh
git tag v0.9.x && git push origin v0.9.x   # loest release.yml aus (MSI + latest.json + Auto-Update)
```
Details: [`docs/release.md`](docs/release.md). `relay-deploy.yml` deployt Relay-Änderungen beim
Merge nach `main` automatisch.
