# Release & Auto-Update

BTS Light aktualisiert sich selbst. Diese Datei beschreibt, wie ein neuer
Release veröffentlicht wird und wie das Auto-Update funktioniert.

## Wie das Auto-Update funktioniert

- Beim App-Start und über den Dashboard-Button „Nach Update prüfen" fragt
  die App das Manifest `https://badhub.de/download/bts-light/latest.json`
  ab.
- Ist dort eine höhere Version eingetragen, lädt die App das signierte
  Paket **sofort im Hintergrund** und zeigt oben ein Banner, sobald es
  bereitliegt — mit der Zahl der gerade belegten Felder. Zwei Wege
  (Spec [features/update-im-turnierbetrieb.md](features/update-im-turnierbetrieb.md),
  ADR 0057):
  - **„Jetzt neu starten"**: Live-Stand sichern, Installer starten, App
    kommt als neue Version zurück. Lief die Übertragung, **startet sie von
    selbst wieder** (Wiederanlauf-Marker `update-resume.json`, 15 min
    gültig); die Aufruf-Uhr belegter Felder läuft weiter. Die Lücke beträgt
    etwa 20 s — Tablets zählen offline weiter, Monitore und TL-Web kommen
    über ihre Reconnect-Wächter zurück.
  - **„Beim Beenden einbauen"**: Vormerkung; beim Schließen der App läuft
    der Installer **stumm** (`/S /UPDATE`, ohne Neustart der App). Die neue
    Version steht beim nächsten Start bereit.
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

Ebenso fest ist der Link für die **Tablet-Kiosk-App**:

    https://badhub.de/download/bts-light/bts-light-tablet.apk

Der `publish`-Job legt diesen festen Namen **nur** für eine SIGNIERTE APK an
(sobald die beiden Android-Keystore-Secrets hinterlegt sind). Eine
Debug-APK (Secrets fehlen) ist debuggable und würde später kein signiertes
Update mehr annehmen — sie bleibt deshalb ausschließlich unter ihrem
versionierten `-debug.apk`-Namen erreichbar, nie unter dem festen Link
(siehe „Benötigte GitHub-Secrets" unten und
[tablet-android-app.md](tablet-android-app.md)). Auf der Release-Seite
steht trotzdem eine APK zum Download: die neueste **signierte**, sonst die
neueste Debug-APK unter ihrem versionierten Namen und mit Hinweis.

## Handbuch (`/download/bts-light/handbuch/`)

Die öffentliche Anleitung unter
`https://badhub.de/download/bts-light/handbuch/index.html` wird aus den
Markdown-Dateien dieses Repos erzeugt — es gibt **keine** zweite Textfassung,
die auseinanderlaufen könnte. Die Release-Seite ist aus Sicht des Handbuchs
ein Kapitel („Downloads & Versionshinweise"); beide verlinken sich gegenseitig.

**Was veröffentlicht wird, steht in [`handbuch.json`](handbuch.json)** — eine
Whitelist. Nur eingetragene Dateien gehen online; ADRs, Specs, Roadmaps und die
Server-Einrichtung des Relays bleiben draußen. Halb-interne Dateien (Bedienung
UND Architektur in einem Dokument) müssen nicht zerschnitten werden:

- `"aus": ["Endpunkte", …]` im Manifest entfernt genannte Abschnitte samt
  Unterabschnitten bis zur nächsten gleichrangigen Überschrift.
- `<!-- handbuch:aus -->` … `<!-- handbuch:an -->` im Markdown selbst deckt
  Stellen mitten in einem Abschnitt ab.

Ein `aus`-Titel, den es nicht (mehr) gibt, lässt den Bau **fehlschlagen**.
Ohne diese Härte würde eine umbenannte Überschrift einen internen Abschnitt
still wieder öffentlich machen — und die Seite sähe dabei völlig in Ordnung aus.
Ebenso meldet `scripts/test-handbuch.mjs` jede `docs/*.md`, die weder im
Manifest noch unter `"intern"` steht: eine neue Anleitung bleibt so nicht
unbemerkt liegen.

**Veröffentlicht wird bei jedem Push auf `main`**, der `docs/`, `README.md`
oder den Generator anfasst (`.github/workflows/handbuch.yml`), zusätzlich per
`workflow_dispatch` — **nicht** beim Tag-Release. Doku ändert sich häufiger als
die Versionsnummer; wäre es an den Tag gekoppelt, bliebe eine Korrektur bis zum
nächsten Release unsichtbar. Der Workflow prüft erst
(`node scripts/test-handbuch.mjs`), baut dann und lädt nach
`…/download/bts-light/handbuch/` — mit `--delete`, damit ein aus dem Manifest
entferntes Kapitel auch vom Server verschwindet. Das `--delete` gilt
**ausschließlich** diesem Unterordner; im Elternverzeichnis liegen Installer und
`latest.json` (siehe die Warnung in `badhub/docs/ops/deployment.md`).

**`index.html` gehört in die Adresse.** Der nginx-vHost löst diese
Verzeichnis-URL nicht auf einen Index auf: gemessen am 06.09.2026 direkt nach
dem ersten Deploy liefert `…/handbuch/` eine **403**, `…/handbuch/index.html`
dagegen 200. Für das Elternverzeichnis `/download/bts-light/` gibt es
offenbar eine eigene Regel, für neue Unterordner nicht. Die kürzere Adresse
braucht eine nginx-Änderung mit eigener Freigabe (in `roadmap.md` vermerkt) —
bis dahin verlinkt die Release-Seite den vollen Pfad, und
`scripts/test-release-notes.mjs` hält das fest.

Lokal ansehen:

```bash
node scripts/build-handbuch.mjs --out dist/handbuch
open dist/handbuch/index.html
```

Der Generator nutzt `marked` (reine devDependency, keine transitiven Pakete,
Audit 05.09.2026). Sie läuft nur in Node/CI und wird **nicht** in die
ausgelieferte App gebündelt.

## Release-Seite (Downloads + Änderungen je Version)

`https://badhub.de/download/bts-light/` zeigt alle Versionen mit
Download-Link, Datum und den Kompakt-Änderungen aus
[changelog.md](changelog.md); die neueste Version steht prominent oben
(plus der stabile Link). Direkt darunter steht der Block
**„Court-Monitore: Raspberry-Pi-Image für die SD-Karten“** mit Link auf
`pi-image/bts-light-pi.img.xz` (passwortgeschützt, weil das Image das
WLAN-Passwort des Verleih-Routers enthält), Prüfsumme und Kurzanleitung (seit November
2026 ein Image für Pi Zero 2 W und Pi 3 B+; das frühere Test-Image
`bts-light-pi-v2.img.xz` ist entfallen) — die Karten werden
beim Hallenaufbau beschrieben, oft ohne Zugriff auf dieses Repo. Der Block ist
im Generator fest verdrahtet (das Image kommt **nicht** aus dem
Release-Workflow, sondern per rsync, siehe [pi-master-image.md](pi-master-image.md));
`scripts/test-release-notes.mjs` hält fest, dass er auf der Seite steht. Der
Workflow `release-seite` prüft nach dem Hochladen zusätzlich die Live-Seite; er
lädt sie dafür erst vollständig und sucht dann (ein `curl | grep -q` meldete
unter `pipefail` fälschlich einen Fehler, erster Lauf am 10.10.2026).
Darunter der Block **„Zähl-Tablets: Android-App für Fire-Tablets“** mit der
neuesten Tablet-APK: Der publish-Job holt per ssh die APK-Dateien, die auf
dem Server liegen (`--apks`), plus die frisch gebaute; der Kopf-Knopf nimmt
die neueste **signierte** APK (fester Name `bts-light-tablet.apk`), und nur
wenn es keine signierte gibt, die neueste versionierte `-debug.apk` mit dem
Hinweis auf die Debug-Signatur — signiert vor Version, damit der feste Name
nie unter einem Debug-Etikett steht. Zusätzlich hat jede Version, für die eine APK auf dem
Server liegt, neben dem Installer-Knopf einen Knopf „Tablet-APK“ (signiert
vor Debug). `scripts/test-release-page-apk.mjs` prüft beide Fälle und dass
ohne `--apks` nichts versprochen wird. Das **Datum je Version** kommt aus dem
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

## Nur die Seite neu veröffentlichen (ohne Release)

Eine reine Änderung an der Seite — ein neuer Link, ein Textfehler, ein
Changelog-Nachtrag — braucht **keinen** Release. Actions-Tab →
**„Release-Seite neu veröffentlichen"** → *Run workflow*
(`.github/workflows/release-seite.yml`, nur `workflow_dispatch`).

Der Job baut `index.html` aus `docs/changelog.md` genau wie der publish-Job
(Installer-Liste per ssh vom Server, Tag-Daten aus `git for-each-ref`) und
lädt **ausschließlich** diese eine Datei hoch — keine Installer, kein
`latest.json`. Er kann das Auto-Update damit auch bei einem Fehlaufruf nicht
beschädigen. Anschliessend prüft er die Seite per `curl` gegen badhub.de.

Er nutzt denselben Deploy-Zugang wie `release.yml` (`SSH_DEPLOY_KEY_V2`) und
dieselbe `concurrency`-Gruppe, damit nie zwei Läufe gleichzeitig in das
Verzeichnis auf badhub.de schreiben.

> Warum es das gibt: Bis zum 2026-08-20 entstand die Seite nur im publish-Job
> von `release.yml`, der an einem Versions-Tag hängt. Für den Link auf das
> Pi-Image der Court-Monitore hätte es einen kompletten Release samt
> signiertem Windows-Build gebraucht.

## Einen Release veröffentlichen

1. Version in **drei** Dateien identisch hochsetzen:
   - `src-tauri/tauri.conf.json` → `"version"`
   - `package.json` → `"version"`
   - `src-tauri/Cargo.toml` → `version` (liefert `CARGO_PKG_VERSION`,
     das der Updater für den Versionsvergleich nutzt)
2. Änderungen committen und pushen.
3. Tag setzen — per Knopf über den Workflow **„Release auslösen"**
   (siehe [unten](#release-per-knopf-auslösen)):
   ```bash
   gh workflow run release-ausloesen.yml --ref main -f version=0.3.0
   ```
   Von Hand (`git tag -a v0.3.0 -m "BTS Light v0.3.0" && git push origin v0.3.0`)
   kann den Tag nur ein Admin setzen — das Ruleset „Release-Tags nur durch
   Admins" sperrt `v*` für alle anderen.
4. Der GitHub-Actions-Workflow `release.yml` erledigt den Rest:
   - baut den signierten Installer (`build`-Job, Windows),
   - legt ein GitHub-Release `v0.3.0` an,
   - erzeugt `latest.json` und lädt Installer + Updater-Artefakt +
     `latest.json` nach `badhub.de/download/bts-light/` (`publish`-Job).

Installierte Clients sehen das Update beim nächsten Start (oder per
Button) innerhalb weniger Sekunden.

`workflow_dispatch` **von `release.yml`** (Actions-Tab → „Run workflow") baut
nur zum Test und veröffentlicht **nicht**.

### Release per Knopf auslösen

Der Workflow **`release-ausloesen.yml`** setzt den Versions-Tag auf `main`.
Starten kann ihn jeder mit Schreibrecht auf das Repo — Admin-Recht braucht es
dafür nicht:

- **Im Browser:** Actions → „Release auslösen" → „Run workflow" → Branch
  `main`, im Feld die Version eintippen (`0.9.422`).
- **Im Terminal:** `gh workflow run release-ausloesen.yml --ref main -f version=0.9.422`

Die getippte Version ist die Bestätigung: Ein versehentlicher Klick soll kein
Auto-Update auf allen Turnier-PCs auslösen. Getaggt wird nur, wenn **alles**
davon zutrifft (`scripts/check-release-freigabe.mjs`, Tests in
`scripts/test-release-freigabe.mjs`):

| Prüfung | Warum |
|---|---|
| Lauf wurde auf `main` gestartet | es gilt die Regel, die auf `main` steht — nicht eine umgeschriebene auf einem Branch |
| `main` hat sich seit dem Start nicht bewegt | getaggt wird der Commit des Laufs; die grüne CI soll zu ihm gehören |
| Version in `Cargo.toml`, `tauri.conf.json`, `package.json` identisch, Form `X.Y.Z` | sonst ist der Tag zwangsläufig falsch |
| getippte Version = Version auf `main` | bewusste Bestätigung |
| Tag `vX.Y.Z` existiert noch nicht | ein Tag wird nie verschoben |
| jüngster CI-Lauf auf `main` ist fertig, grün **und gehört zu diesem Commit** | kein Release aus einem roten oder ungeprüften Stand |

Zur letzten Zeile: Direkt nach einem Merge ist der CI-Lauf für den neuen
Commit noch nicht angelegt; die Abfrage fände den grünen Lauf des Vorgängers.
Deshalb vergleicht die Regel den Commit des Laufs mit dem zu taggenden. Ein
älterer Lauf genügt nur, wenn seither ausschließlich Dateien geändert wurden,
für die `ci.yml` auf `main` gar nicht startet (`paths-ignore`: Markdown,
`docs/`) — ein Test hält die beiden Listen deckungsgleich.

Schlägt eine Prüfung fehl, nennt der Lauf **alle** Gründe und setzt keinen Tag.
Geht alles durch, pusht der Job `taggen` einen annotierten Tag — und der löst
wie bisher `release.yml` aus.

**Weiterhin kein Auto-Tag:** Den Zeitpunkt bestimmt ein Mensch. Der Workflow
nimmt nur die Übergabe an den Admin heraus, an der Versionen liegen blieben.

#### Wie das ohne Admin-Recht geht

Der Tag wird nicht mit dem Konto dessen gepusht, der den Knopf drückt, sondern
mit einem **Deploy-Key**, der im Ruleset als Ausnahme (Bypass) eingetragen ist.
Der private Schlüssel liegt als Secret `RELEASE_TAG_KEY` im **Environment
`release-tag`**, das nur Läufe von `main` bedient. Darauf kommt es an: Ein
gewöhnliches Repo-Secret könnte jeder mit Schreibrecht über einen umgeschriebenen
Workflow auf einem eigenen Branch auslesen. So aber gilt: Wer taggen will, muss
durch die Regel auf `main`.

#### Worauf der Schutz beruht — und was er nicht leistet

- **Er ist so stark wie der Schutz von `main`.** Wer eine Änderung an
  `release-ausloesen.yml` oder der Regel nach `main` bringt, kann dort auch
  einen Schritt einbauen, der den Schlüssel preisgibt. Stand 15.11.2026
  verlangt `main` die Checks `build` und `secret scan`; ein zweites Paar Augen
  (Pflicht-Review) ist **nicht** Teil dieses PRs. Wer das will, hat zwei
  Stellschrauben: Pflicht-Review auf `main` (am besten mit `CODEOWNERS` für
  `.github/workflows/` und `scripts/check-release-freigabe.mjs`) oder
  „Required reviewers" am Environment — letzteres hält auch dann, wenn `main`
  kompromittiert ist.
- **Der Schlüssel selbst kann mehr als der Workflow.** Er steht in der
  Bypass-Liste des Tag-Rulesets und darf `v*`-Tags deshalb anlegen,
  **verschieben und löschen**; „ein Tag wird nie verschoben" gilt für den
  Workflow, nicht für den Schlüssel. Wer ihn in die Hand bekommt, kann einen
  beliebigen Commit taggen — und `release.yml` läuft dann in der Fassung
  **dieses** Commits, mit Signaturschlüssel und Upload nach badhub.de. Ein
  Leck des Schlüssels ist also gleichbedeutend mit einem fremden Auto-Update
  für alle Installationen. Deshalb: nur im Environment, nie als Repo-Secret,
  lokale Kopien löschen.
- **Neu ist dieses Risiko nur zum Teil.** Die Release-Secrets
  (`TAURI_SIGNING_PRIVATE_KEY`, `SSH_DEPLOY_KEY_V2`) sind Repo-Secrets;
  `release.yml` lässt sich per `workflow_dispatch` von jedem Branch starten.
  Sie ebenfalls in ein auf `main` und `v*`-Tags beschränktes Environment zu
  legen, wäre ein eigener, sinnvoller Schritt.

Warum nicht der `GITHUB_TOKEN`: Ein damit gesetzter Tag löst `release.yml` nicht
aus (GitHub verhindert Workflow-Ketten), und am Ruleset scheitert er ohnehin.

#### Einmalige Einrichtung (nur Admin, ca. 10 Minuten)

Bis das erledigt ist, bricht der Job `taggen` mit „Secret RELEASE_TAG_KEY
fehlt" ab; es passiert nichts weiter.

1. **Schlüsselpaar erzeugen** (lokal, ohne Passwort) und den öffentlichen Teil
   als Deploy-Key **mit Schreibrecht** eintragen:
   ```bash
   ssh-keygen -t ed25519 -C "bts-light release-tag" -f release-tag -N ""
   gh repo deploy-key add release-tag.pub --repo Assembler23/bts-light \
     --title "Release-Tag (Workflow)" --allow-write
   ```
2. **Environment anlegen:** Settings → Environments → „New environment" →
   Name exakt `release-tag`.
   - „Deployment branches and tags" → **Selected branches and tags** → „Add
     deployment branch or tag rule" → Typ **Branch**, Muster exakt `main`
     (kein `main*`, kein Typ „Tag" — einen Tag namens `main` dürfte jeder mit
     Schreibrecht anlegen). **Nicht auslassen** — das ist der eigentliche
     Schutz. Danach darf in der Liste nur dieser eine Eintrag stehen.
   - „Environment secrets" → `RELEASE_TAG_KEY` = Inhalt der Datei
     `release-tag` (der **private** Schlüssel, komplett mit den
     `-----BEGIN/END-----`-Zeilen).
   - Gegenprobe: Unter Settings → Secrets and variables → Actions darf
     `RELEASE_TAG_KEY` **nicht** als Repository-Secret auftauchen.
   - *Optional:* „Required reviewers" → Admin eintragen. Dann wartet jeder
     Lauf auf einen Klick des Admins (geht auch am Handy) — für den Fall, dass
     der Zeitpunkt doch abgestimmt bleiben soll.
3. **Ruleset öffnen:** Settings → Rules → Rulesets → „Release-Tags nur durch
   Admins" → „Bypass list" → „Add bypass" → **Deploy keys** → speichern.
4. Die beiden lokalen Dateien `release-tag` und `release-tag.pub` löschen.

> **Falls „Deploy keys" in der Bypass-Liste nicht angeboten wird:** Dann eine
> eigene kleine GitHub-App anlegen (nur Berechtigung „Contents: Read and
> write", nur in diesem Repo installiert), sie in die Bypass-Liste aufnehmen
> und im Job `taggen` statt `ssh-key` ein App-Token verwenden
> (`actions/create-github-app-token`). App-ID und privater Schlüssel gehören
> dann ebenfalls ins Environment `release-tag`.

Auf Branches kann der Schlüssel nicht mehr als jedes Konto mit Schreibrecht;
für `main` gelten dessen Schutzregeln weiter (die Ausnahme gilt nur im
Tag-Ruleset).

**Schlüssel tauschen oder zurückziehen:** Deploy-Key unter Settings → Deploy
keys löschen — ab dann kann der Workflow nicht mehr taggen. Für einen Tausch
die Schritte 1, 2 (nur das Secret) und 4 wiederholen.

### CI vor dem Release (`ci.yml`)

Drei Jobs je Push/PR: **`build`** auf `windows-latest` (Zielplattform —
Frontend, Asset-Syntax, JS-Regeltests, `cargo fmt`/Clippy/Tests),
**`android`** (Kiosk-App: JVM-Tests + Debug-APK) und seit 15.09.2026
**`linux`** auf `ubuntu-22.04`: Clippy mit `-D warnings` und
`cargo test --workspace` mit den Tauri-System-Paketen (WebKitGTK 4.1, GTK 3,
AppIndicator, librsvg). Der Linux-Job baut kein Paket; er hält den
`not(windows)`-Zweig des Rust-Kerns grün (GDI-Druck → `NichtUnterstuetzt`,
SSID → `iwgetid`), damit sich auf Linux entwickeln lässt (Roadmap
„Linux-Variante", Stufe 0). Release-Referenz bleibt der Windows-Job;
Windows-Minuten zählen bei GitHub doppelt, Ubuntu-Minuten einfach.

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

Der Check misst gegen `origin/main` (fehlt die Ref, gegen das lokale `main`)
und zählt entlang `--first-parent` mit `git diff <sha>^ <sha>` — so zählt auch
ein per Merge-Commit gelandeter Versionssprung mit, nicht nur die heute
üblichen Squash-Commits.

**Kein Auto-Tag, bewusst:** Der Tag löst ein Auto-Update auf allen laufenden
Turnier-PCs aus. Ob das gerade passt, weiß nur ein Mensch — deshalb setzt
kein Automat den Tag von sich aus: Den Zeitpunkt bestimmt ein Mensch, über den
Workflow [„Release auslösen"](#release-per-knopf-auslösen); der Check ist nur
das Netz.

**Grenze des Checks:** Er macht das Vergessen sichtbar, ersetzt aber keine
Zustellung. Wer das Repo zwei Tage nicht öffnet, sieht auch das rote Kreuz nicht.

## Hotfix an einer Geräte-Seite: Seiten-Paket (`seiten-paket.yml`)

> Spec: [features/hotfix-ohne-neustart.md](features/hotfix-ohne-neustart.md) ·
> ADR 0099. Seit v0.9.433 bietet die App passende Pakete im Banner an
> ([oberflaechen.md](oberflaechen.md)).

Ein Fehler in `tablet.html`, `tl.html` oder einer anderen Geräte-Seite
(`src-tauri/assets/*.html`) lässt sich als **Seiten-Paket** ausliefern: Der
Turnier-PC tauscht die Seiten im laufenden Betrieb, ohne Neustart. Ein Paket
gilt für **genau eine** veröffentlichte Version.

**Ablauf:**

1. Zweig `hotfix/vX.Y.Z` **am Tag** `vX.Y.Z` anlegen (nicht von `main`).
2. Den Fix dorthin übernehmen (Cherry-Pick) und pushen. Erlaubt sind
   Änderungen an den Seiten, an Doku, Tests und `src/io/*.mjs` — **nichts am
   Kern**.
3. Unter **Actions → Seiten-Paket → Run workflow** den Lauf **auf `main`**
   starten und den Tag eintragen (`vX.Y.Z`).
4. Der Job `pruefen` zeigt in seiner Zusammenfassung die geänderten Dateien
   und den **Unterschied der Seiten** gegenüber dem Tag. Ein Maintainer liest
   ihn und gibt im Environment `seiten-hotfix` frei. Erst dann signiert
   `signieren` das Paket und lädt es hoch.
5. **Den Fix auch nach `main` mergen.** Sonst fehlt er den Cloud-Geräten
   (die bekommen ihre Seiten vom Relay) und im nächsten Release.

**Warum von Hand und nicht bei jedem Push:** Ein Workflow, der bei einem Push
auf `hotfix/v*` startet, läuft mit der Workflow-Datei und den Skripten **aus
diesem Zweig**. Wer dort schreiben darf, könnte Prüfung und Zusammenfassung
umbiegen, und die Freigabe läse eine gefälschte Liste. Beim Start von `main`
kommen Workflow und Bau-Skript aus `main`; der Hotfix-Zweig wird nur als Daten
gelesen — über git aus einem nackten Klon, nie ausgecheckt, nie ausgeführt
(Security-Review 16.11.2026, weicht von der ersten Fassung der Spec ab).

**Wann der Job abbricht** (vor dem Signieren, mit Erklärung im Log):

- Der Zweig ändert gegenüber dem Tag einen **Kern-Pfad**: `src-tauri/**`
  außer `src-tauri/assets/*.html`, `relay-proto/**`, `Cargo.toml`,
  `Cargo.lock`. Auch eine Versionsnummer in `src-tauri/Cargo.toml` zählt —
  beim Cherry-Pick weglassen. Verschieben zählt als Löschen und Anlegen.
- Der Zweig stammt nicht vom Tag ab, oder der Tag trägt eine andere Version.
- Seit dem Tag ist keine Geräte-Seite geändert.
- Eine Seite ist keine gewöhnliche Datei (Verweis, Untermodul).
- Der Tag ist älter als die Version, die Seiten-Pakete kennt.
- Das Environment `seiten-hotfix` fehlt oder hat keine Pflicht-Freigabe.
- Unter dem Paketnamen liegt auf dem Server schon ein **anderer** Inhalt
  (Zweig neu geschrieben, gleiche Commit-Zahl). Ein veröffentlichtes Paket
  wird nie ersetzt; ein weiterer Commit ergibt die nächste Folge.

**Was entsteht** (unter `download/bts-light/` auf badhub.de):

| Datei | Inhalt |
|---|---|
| `seiten/seiten-<kern>-<folge>.btsp` + `.sig` | das Paket (gzip-JSON mit allen Seiten) und seine Signatur. |
| `seiten.json` | unsignierter Index: je Version die neueste Folge, höchstens die drei jüngsten Versionen. Wird bei jedem Lauf aus den Dateien neu gebildet, die wirklich unter `seiten/` liegen. |

Die **Folge** ist die Zahl der Commits auf dem Hotfix-Zweig seit dem Tag
(`git rev-list --count vX.Y.Z..HEAD`). Sie wächst mit jedem Commit; die App
nimmt nur eine höhere als die aktive. Derselbe Stand ergibt immer dieselben
Bytes (als Bauzeit steht die Zeit des Commits im Paket); signiert wird nur,
wenn der Bau nach der Freigabe dem Probebau davor gleicht. Hochgeladen wird
erst das Paket, dann der Index.

**Einmalig einzurichten (Admin) — vor dem ersten Lauf:**

- Environment **`seiten-hotfix`** mit Pflicht-Freigabe („Required
  reviewers", ohne Selbstfreigabe) und der Zweig-Regel **nur `main`**. Ohne
  die Zweig-Regel ließe sich der Workflow auch „von" einem anderen Zweig
  starten — dann liefe wieder dessen Fassung.
- Schreibrecht auf `hotfix/v*` für die, die Hotfixes vorbereiten.
- Die Secrets, die der Job liest: `TAURI_SIGNING_PRIVATE_KEY`,
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, `SSH_DEPLOY_KEY_V2`,
  `SSH_KNOWN_HOSTS`.

> **Zur Schranke:** Die Freigabe schützt nur, wenn der Signaturschlüssel
> **im Environment** liegt und nicht (auch) als Repo-Secret. Ein Repo-Secret
> kann jeder Workflow auf jedem Zweig lesen — auch einer, den jemand mit
> Schreibrecht neu anlegt. Der Release-Lauf liest den Schlüssel heute als
> Repo-Secret; das ist eine bestehende Lage, kein Teil dieses Jobs. Wer sie
> schließen will, legt den Schlüssel in Environments (eines für den
> Release-Lauf, `seiten-hotfix` für diesen Job) und löscht das Repo-Secret.

> **Noch nicht im Betrieb gelaufen** (Stand 16.11.2026): Der Workflow braucht
> das Environment und einen echten Hotfix-Zweig. Offen sind bis zum ersten
> Lauf: ob das Signierwerkzeug auf dem Linux-Runner so startet, ob der Job
> die Schutzregeln des Environments lesen darf, und die Rechte von
> `bts-deploy` für das Unterverzeichnis `seiten/`. Ein Lauf, der auf die
> Freigabe wartet, hält spätere Veröffentlichungen auf — ablehnen statt
> liegen lassen.

Die Regeln des Bau-Skripts (`scripts/build-seiten-paket.mjs`) prüft
`scripts/test-seiten-paket.mjs` in der CI; dass ein so gebautes Paket die
Prüfung der App besteht, hält der Rust-Test
`paket_aus_dem_bauskript_besteht_die_app_pruefung` fest.

## Kern-Artefakt und Datenformat (`release.yml`, Job `kern`)

> Spec: [features/hotfix-ohne-neustart.md](features/hotfix-ohne-neustart.md) ·
> ADR 0100/0101. Seit v0.9.439 nutzt die App das Artefakt: Sie lädt es nach
> einem gefundenen Update zusätzlich, prüft es und bietet dann „Jetzt
> wechseln" an ([erste-schritte.md](erste-schritte.md#updates)). Fehlt es,
> bleibt es beim Installer.

Nach `publish` legt jeder Tag-Release zusätzlich ab (unter
`download/bts-light/`):

| Datei | Inhalt |
|---|---|
| `kern/bts-light-<version>.exe` | die nackte Programmdatei, aus dem fertigen Installer entnommen (`7z x`), damit sie mit der installierten Datei identisch ist |
| `kern/kern-<version>.json` + `.sig` | der **Kern-Kopf**: Version, SHA-256 und Größe der Programmdatei, `rueckwaerts_lesbar_ab`. Signiert wird der Kopf, nicht die exe. |
| `kern.json` | unsignierter Index: nur die neueste Version |

Der Job läuft erst, wenn Installer und `latest.json` veröffentlicht sind, und
darf scheitern (`continue-on-error`) — das Auto-Update dieses Releases hält
er nicht auf. Ein Zeitlimit von 15 Minuten sorgt dafür, dass er auch den
**nächsten** Release nicht lange blockiert (alle Release-Läufe teilen sich
eine Sperre). **Ein Scheitern ist leise:** Der Lauf bleibt grün, es gibt nur
eine Anmerkung. Nach einem Release also einmal nachsehen, ob `kern/` die neue
Version enthält; fehlt sie, bietet die App für diese Version nur den
Installer-Weg an.

Bevor der Job etwas signiert, prüft er den heruntergeladenen Installer gegen
dessen Signatur (`scripts/verify-minisign.mjs`, Updater-Schlüssel aus
`tauri.conf.json`) und dass er zu dieser Version gehört. Der Installer liegt
als Release-Asset offen; ohne diese Prüfung stellte der Job für eine
ausgetauschte Datei eine echte Signatur aus. Hochgeladen wird erst die
Nutzlast, dann der Index.

### `release/datenformat.json` — darf man zurück?

Alte und neue Version teilen sich `config.json` und die Stores. Ob eine App
nach einem Update zur vorigen Version **zurück** darf, entscheidet
`rueckwaerts_lesbar_ab`: die älteste Version, die die Daten der aktuellen noch
lesen und damit fachlich richtig arbeiten kann. Dass sie neue Einstellungen
nicht kennt und beim Speichern verliert, ist erlaubt. Der Wert wandert in den
signierten Kern-Kopf; fehlt er, gilt „nicht lesbar".

Damit die Angabe nicht still veraltet, vergleicht der Test
`abdruck_weicht_ab_macht_rot` (`src-tauri/tests/datenformat_abdruck.rs`) einen
**Abdruck** aller gespeicherten Formate mit dem Feld `abdruck` in derselben
Datei. **Ändert ein PR ein gespeichertes Format, ist die CI rot**, und die
Fehlermeldung nennt den neuen Abdruck. Dann sind zwei Dinge einzutragen:

1. `abdruck` — der neue Wert aus der Meldung.
2. `rueckwaerts_lesbar_ab` — bleibt, wenn die vorige Version die Daten weiter
   lesen kann (ein neues Feld mit Standardwert ist der Normalfall). Auf die
   **neue** Version setzen bei: neuer Enum-Variante, die gespeichert wird;
   umbenanntem Feld oder geänderter Bedeutung; Einweg-Migration.

Dazu eine neue Zeile in `begruendung`. Im PR prüft die CI zusätzlich gegen den
Zielzweig (`scripts/check-datenformat.mjs`): Die Grenze **sinkt nie**, und ein
geänderter Abdruck ohne geänderte Begründung ist ein Fehler — wer nur die Zahl
aus der Meldung kopiert, hat nicht entschieden.

Der Abdruck geht über die Definitionen aller Typen mit `Deserialize` in den
Dateien der Liste `DATEIEN` im Test, dazu über eine Namensliste der
`relay-proto`-Typen, die in gespeicherten Dateien stecken (Punktverlauf,
Zettel-Ereignisse, Meetingpoint). Ein zweiter Test verlangt, dass **jede**
Quelldatei mit einem `Deserialize`-Typ entweder dort oder in
`NICHT_GESPEICHERT` steht — ein neuer Store kann also nicht unbemerkt
entstehen. Grenzen: Der Abdruck erkennt Struktur, nicht jede
Bedeutungsänderung (etwa einen anderen Standardwert hinter
`#[serde(default = "…")]`), und ein neuer `relay-proto`-Typ in einem Store
muss von Hand in die Namensliste.

## Benötigte GitHub-Secrets (einmalig eingerichtet)

| Secret | Zweck |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | privater Updater-Signaturschlüssel |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Passwort dieses Schlüssels |
| `SSH_DEPLOY_KEY` | SSH-Key für den Upload nach badhub.de |
| `SSH_KNOWN_HOSTS` | Host-Fingerprint des badhub.de-Servers |
| `ANDROID_KEYSTORE_B64` | Base64 des Signatur-Keystores der Tablet-Kiosk-App, Alias `bts-light-tablet` |
| `ANDROID_KEYSTORE_PASS` | Passwort dieses Keystores |
| `RELEASE_TAG_KEY` | privater Deploy-Key für den Workflow „Release auslösen" — **nicht** als Repo-Secret, sondern im Environment `release-tag` (nur `main`) |
| `TURNIERKERN_LESE_TOKEN` | Lese-Token für das private Repo `Assembler23/turnierkern` (Crate `turnierkern`, seit v0.9.449). Der Schritt „Zugang zum Turnierkern“ in `ci.yml` (beide Jobs), `release.yml` (`build`) und `relay-deploy.yml` biegt die Git-URL per `insteadOf` auf HTTPS mit Token um und setzt `CARGO_NET_GIT_FETCH_WITH_CLI=true` — vor **jedem** cargo-Aufruf, auch `-p bts-relay` löst den ganzen Workspace auf. **Dependabot-Läufe und PRs aus Forks** bekommen keine Secrets: Dort scheitert cargo am Kern |

Der **Android-Keystore** wird wie der Updater-Schlüssel **nie gewechselt** —
Sideload-Updates auf ein bereits eingerichtetes Tablet verlangen dieselbe
Signatur, siehe [tablet-android-app.md](tablet-android-app.md#update-der-app).
Erzeugt wird er einmalig mit

    keytool -genkeypair -v -keystore bts-light-tablet.keystore -alias bts-light-tablet -keyalg RSA -keysize 2048 -validity 10000

und für das Secret Base64-kodiert:

    base64 -w0 bts-light-tablet.keystore

Fehlt `ANDROID_KEYSTORE_B64`, baut der `android`-Job im Release-Workflow
statt einer signierten nur eine unsignierte Debug-APK — der Job ist
`continue-on-error`, ein Fehlschlag dort blockiert also nie den
Windows-Installer oder `latest.json`.

Das SDK richtet in beiden Workflows `android-actions/setup-android` ein —
**mit `packages: platform-tools`**. Ohne die Angabe installiert die Action
(auch in v4) zusätzlich das alte SDK-Paket `tools`, das Google aus dem
Repository entfernt hat; seit 15.09.2026 brach der Job damit vor dem ersten
Gradle-Aufruf ab („Failed to find package 'tools'"), in der CI auf main wie
in jedem PR. Build-Tools und Plattform braucht die Action nicht zu
installieren: AGP lädt sie beim Bau selbst nach, die Lizenzen nimmt die
Action an.

Die `versionCode`-Formel steht **zweimal** im Repo — `app/build.gradle.kts`
(`versionCodeAus`) und `kern/Version.kt` (`versionCode`, für den JVM-Test)
— ein CI-Job vergleicht sie nicht gegeneinander. Läuft eine der beiden
Stellen der anderen davon, entsteht ein falscher `versionCode`, den nichts
automatisch entdeckt: beim Ändern der Formel **immer beide** anfassen.

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
