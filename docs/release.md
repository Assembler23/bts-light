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

## Release-Seite (Downloads + Änderungen je Version)

`https://badhub.de/download/bts-light/` zeigt alle Versionen mit
Download-Link, Datum und den Kompakt-Änderungen aus
[changelog.md](changelog.md); die neueste Version steht prominent oben
(plus der stabile Link). Das **Datum je Version** kommt aus dem
Erstell-Datum des Git-Tags (`git for-each-ref … refs/tags` →
`--dates`-Datei; der publish-Job checkt dafür mit `fetch-depth: 0` +
`fetch-tags` aus). Die Seite wird bei **jedem Tag-Release
automatisch** neu erzeugt (`scripts/build-release-page.mjs` im
publish-Job) und zusammen mit Installer + `latest.json` hochgeladen.
Download-Knöpfe erscheinen nur für Versionen, deren Installer wirklich
auf dem Server liegt. Der Changelog-Auszug landet zugleich in
`latest.json → notes` — das Update-Fenster der App zeigt damit
„Was ist neu". **Konsequenz:** `docs/changelog.md` muss VOR dem Taggen
den Abschnitt `## vX.Y.Z` der neuen Version enthalten (ist ohnehin
Commit-Pflicht laut CLAUDE.md).

### Die Notes umfassen die ganze Strecke, nicht nur den Tag

Zwischen zwei Tags liegen hier regelmäßig **mehrere** Versionssprünge —
von v0.9.214 auf v0.9.223 waren es neun. Wer aktualisiert, springt genau
über diese Strecke. Deshalb ermittelt der publish-Job den zuletzt
veröffentlichten Tag (`git describe --tags --abbrev=0 "$TAG^"`) und
reicht ihn als `--notes-since` weiter; die Notes decken dann alle
Versionen **danach** bis zur getaggten ab, neueste zuerst.

Bis zum 2026-08-18 war das nicht so: Das Update-Fenster zeigte nur den
Abschnitt der getaggten Version. Acht ausgelieferte Änderungen — Push-Kanal
statt Poll, Panel „Anfangszeiten", Schriftgröße pro Gerät und weitere —
waren für den Nutzer schlicht unsichtbar, und nichts meldete das: Die
`latest.json` war ja gültig.

Zwei Ausgabeformen, die Grenze liegt bei 4000 Zeichen:

- **Passt es**, steht je Version eine Kopfzeile `vX.Y.Z` mit ihren
  vollständigen Stichpunkten da.
- **Passt es nicht**, kürzt der Generator auf **eine Zeile je Version**
  (die fett ausgezeichnete Kernaussage) und verweist auf die Release-Seite.
  Ein Dialog ist kein Dokument — neun lesbare Zeilen schlagen eine Textwand.

Ohne `--notes-since` bleibt es bei genau der getaggten Version. Das ist
bewusst die sichere Richtung: Ein vergessenes Flag liefert zu wenig statt
den kompletten Changelog seit v0.4.0. Dasselbe gilt für jeden Zweifelsfall —
unbrauchbarer Wert (kein Versionsformat) oder `since >= version` (Tag auf
demselben Commit, Tag ausserhalb der Ahnenlinie) führen zum selben Rückfall.
**Wichtig dabei:** Der Rückfall behält die Stichpunkte der getaggten Version.
Ohne ihn bliebe der Bereich leer, und das Update-Fenster zeigte nur noch
„BTS Light X.Y.Z" — schlechter als der Zustand vorher.

Deshalb steht im Workflow `--match 'v[0-9]*'`: Ohne die Einschränkung findet
`git describe` irgendeinen erreichbaren Tag, dessen Name dann als
`--notes-since` ankommt und dort unbrauchbar ist.

Festgehalten von `scripts/test-release-notes.mjs` (läuft in der CI).

Lokal testen:

```sh
# Nur die getaggte Version
node scripts/build-release-page.mjs --changelog docs/changelog.md --out /tmp/index.html

# So, wie der Release-Workflow es tut — Strecke seit dem letzten Tag
node scripts/build-release-page.mjs --changelog docs/changelog.md \
  --out /tmp/index.html --notes-out /tmp/notes.txt \
  --notes-since 0.9.214 --notes-version 0.9.223
```

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

## Wenn das Taggen vergessen wird

Der Versionssprung passiert inzwischen **innerhalb** der Feature-Commits
(`… (v0.9.199)`); es gibt keinen eigenen „Release vX.Y.Z"-Commit mehr, der zum
Taggen auffordert. Ohne Tag fällt nichts auf: `main` ist grün, kein Job schlägt
fehl — nur auf dem Turnier-PC kommt nichts an. Zweimal passiert: einmal blieb
`v0.9.186` liegen, dann zwölf Versionen (`0.9.188`–`0.9.199`) **in 19 Stunden**.

Dagegen läuft der Workflow **`release-faellig.yml`** (bei Push auf `main` und
täglich 06:20 UTC). Er meldet — er blockiert nichts, ist kein Required-Check und
setzt **keine** Tags:

| Grenze | Wert | fängt |
|---|---|---|
| Uhr | ältester unveröffentlichter Commit ≥ **24 h** | „liegt liegen" |
| Menge | ≥ **5** unveröffentlichte Versionssprünge | „hat sich gestapelt" |

**Warum zwei Grenzen:** Je eine allein hätte den echten Vorfall verschluckt, und
das ist gemessen, nicht geschätzt. Eine Grenze auf das Alter der *aktuellen*
Version greift nie, weil bei schnellen Sprüngen jede Version von der nächsten
überschrieben wird (`0.9.199` war zum Zeitpunkt der Meldung 1,4 h alt). Und das
Alter des *ältesten* offenen Commits lag bei 19 h, also unter 24 h. Nur die
Menge (zwölf Sprünge) schlug an.

Reine Doku-, ADR-, Workflow- und Test-Änderungen zählen nicht als
auslieferbar (`istAuslieferbar()` in `scripts/check-version-tagged.mjs`) —
sonst wäre der Check nach jeder Doku-Ergänzung rot und würde zu Rauschen.

Lokal nachsehen:

```bash
node scripts/check-version-tagged.mjs   # exit 0 = nichts fällig
```

**Kein Auto-Tag, bewusst:** Der Tag löst ein Auto-Update auf allen laufenden
Turnier-PCs aus. Ob das gerade passt, weiß nur ein Mensch — deshalb bleibt der
Ablauf, wie er ist: Tilo meldet, wann getaggt wird, der Check ist nur das Netz.

**Grenze des Checks:** Er macht das Vergessen sichtbar, ersetzt aber keine
Zustellung. Wer das Repo zwei Tage nicht öffnet, sieht auch das rote Kreuz nicht.

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
