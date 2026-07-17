# Release & Auto-Update

BTS Light aktualisiert sich selbst. Diese Datei beschreibt, wie ein neuer
Release veröffentlicht wird und wie das Auto-Update funktioniert.

## Wie das Auto-Update funktioniert

- Beim App-Start und über den Dashboard-Button „Nach Update prüfen" fragt
  die App das Manifest `https://badhub.de/download/bts-light/latest.json`
  ab.
- Ist dort eine höhere Version eingetragen, erscheint oben ein Banner.
  Klick auf „Herunterladen & neu starten" lädt das signierte Update,
  installiert es und startet die App neu.
- Jedes Update-Artefakt ist mit einem eigenen Tauri-Signaturschlüssel
  signiert (getrennt vom Windows-Code-Signing). Die App akzeptiert nur
  Artefakte, die zum eingebauten Public Key in `tauri.conf.json` passen.
- Offline ist kein Fehler – ohne Internet bleibt das Banner einfach aus.

Der Windows-Updater nutzt den **NSIS-Installer** (`*-setup.exe`); dessen
`.sig`-Signatur steht inline im `latest.json`.
Die `.msi` bleibt nur für manuelle Installationen.

## Stabiler Download-Link

Für Aushänge, QR-Codes und Mails an Vereine gibt es einen **festen** Link,
der immer auf die neueste Version zeigt:

    https://badhub.de/download/bts-light/BTS.Light-setup.exe

Der `publish`-Job legt bei jedem Release zusätzlich zur versionierten Datei
(`BTS.Light_X.Y.Z_x64-setup.exe`) eine Kopie unter diesem festen Namen ab.
Der **Updater** nutzt weiterhin ausschließlich die versionierte URL aus
`latest.json` — der stabile Link ist rein für Menschen. Das SD-Karten-Image
des Court-Monitors hat ohnehin einen festen Namen
(`bts-light-pi.img.xz`, siehe [pi-master-image.md](pi-master-image.md)).

## Einen Release veröffentlichen

1. Version in **drei** Dateien identisch hochsetzen:
   - `src-tauri/tauri.conf.json` → `"version"`
   - `package.json` → `"version"`
   - `src-tauri/Cargo.toml` → `version` (liefert `CARGO_PKG_VERSION`,
     das der Updater für den Versionsvergleich nutzt)
2. Änderungen committen und pushen.
3. Tag setzen und pushen:
   ```bash
   git tag v0.3.0
   git push origin main --tags
   ```
4. Der GitHub-Actions-Workflow `release.yml` erledigt den Rest:
   - baut den signierten Installer (`build`-Job, Windows),
   - legt ein GitHub-Release `v0.3.0` an,
   - erzeugt `latest.json` und lädt Installer + Updater-Artefakt +
     `latest.json` nach `badhub.de/download/bts-light/` (`publish`-Job).

Installierte Clients sehen das Update beim nächsten Start (oder per
Button) innerhalb weniger Sekunden.

`workflow_dispatch` (Actions-Tab → „Run workflow") baut nur zum Test und
veröffentlicht **nicht**.

## Benötigte GitHub-Secrets (einmalig eingerichtet)

| Secret | Zweck |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | privater Updater-Signaturschlüssel |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Passwort dieses Schlüssels |
| `SSH_DEPLOY_KEY` | SSH-Key für den Upload nach badhub.de |
| `SSH_KNOWN_HOSTS` | Host-Fingerprint des badhub.de-Servers |

Das **Updater-Schlüsselpaar** wurde einmalig mit
`npx tauri signer generate` erzeugt. Der Public Key steht in
`src-tauri/tauri.conf.json` (`plugins.updater.pubkey`). Der private
Schlüssel und sein Passwort liegen ausschließlich in den GitHub-Secrets
und in einem Passwort-Manager.

> **Wichtig:** Geht der private Updater-Schlüssel verloren, kann sich
> **kein installierter Client** mehr automatisch aktualisieren – dann
> hilft nur eine manuelle Neuinstallation auf allen Geräten. Sicher
> aufbewahren.

## Code-Signing (offen)

Der Installer ist noch **nicht** Windows-code-signiert; beim ersten Start
erscheint die SmartScreen-Warnung „Unbekannter Herausgeber". Das
Auto-Update ist davon unabhängig und funktioniert bereits. Optionen für
das Code-Signing sind im Projektplan beschrieben.
