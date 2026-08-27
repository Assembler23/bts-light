# Testsystem-Umschalter (`test.badhub.de`)

**Stand:** 25.08.2026 · umgesetzt

Ein Turnier probeweise fahren, ohne die Produktiv-Datenbank von badhub zu
berühren: ein Schalter im Setup-Assistenten stellt die ganze Installation auf
`test.badhub.de` um — Liveticker-Push, Hallen-Check-In, Cloud-Relay und
Diagnose-Logs.

## Warum kein eigenes Konfigurationsfeld

Der Modus wird **aus der Push-URL abgeleitet** (`badhub.url`), nicht als
`test_mode`-Flag gespeichert. Ein Flag neben der URL wären zwei Wahrheiten, die
auseinanderdriften können — und ausgerechnet die stille Kombination „Flag an,
URL zeigt auf Produktiv" schriebe Testdaten in den echten Liveticker. Der
Fehler fiele erst auf, wenn die falschen Spiele öffentlich stehen.

Die Ableitung hat eine zweite Wirkung: Wer die Test-Adresse bisher von Hand
unter „Anderes Turnier (manuell)" eingetragen hat, bekommt Relay und Logs jetzt
automatisch mit umgestellt.

## Was mitwandert

| Strecke | Weg |
|---|---|
| Liveticker-Push (`live_update.php`) | `badhub.url` direkt |
| Live-Seite, Aushang-QR | `badhub.live_url` |
| Hallen-Check-In, TL-Panel, Aussprache-Wörterbuch, Vereinslogos, Logo-/Sponsoren-Push | `commands::badhub_origin` aus `badhub.url` |
| Cloud-Relay (Tablets, TL-Web, Monitore, Slave-Brücke) | Prozess-Schalter → `badhub_host::relay_https`/`relay_wss` |
| Diagnose-Logs (`bts_log.php`, `tablet_log.php`, `pi_log.php`) | Prozess-Schalter → `badhub_host::api_url` |
| Vereinslogos an Tablet/TL-Web im Cloud-Modus | `location.origin` der ausgelieferten Seite |

Zum Relay gehört auch der **Träger** der fernen Halle (`tablet/carrier.rs`,
ADR 0048): Er wählt denselben Host, sonst wird er nie bereit und jedes lokale
Gerät fällt auf einen Relay zurück, auf dem kein Host sitzt.

**Voraussetzung für den Cloud-Modus:** Auf dem Testsystem muss ein
`bts-relay` hinter nginx unter `/bts-relay/` laufen — freigegeben von der
Basic-Auth-Wand (siehe unten) und mit
`PUBLIC_BASE=https://test.badhub.de/bts-relay`. Ohne die Variable baut der
Relay seine QR-Codes mit dem Produktiv-Default, und ein Tablet landet in einem
Namespace ohne Host. Fehlt der Test-Relay ganz, gehört der Testlauf auf LAN.

Fremde Hosts bleiben **immer** unangetastet: Wer eine eigene badhub-Instanz
betreibt, bekommt seine Adresse nicht umgeschrieben (Test in
`badhub_host::tests::fremde_hosts_bleiben_unangetastet`).

## Die Basic-Auth-Wand von test.badhub.de (Stand 26.08.2026)

**Der Umschalter allein genügt nicht.** `test.badhub.de` liegt komplett hinter
einer nginx-htpasswd-Wand (`/etc/nginx/htpasswd_test_badhub`, badhub-Repo
`docs/ops/deployment.md`) — und zwar **auch vor den Maschinen-Endpunkten**.
Gemessen am 26.08.2026:

```
GET  https://test.badhub.de/                     → 401  WWW-Authenticate: Basic realm="test.badhub.de — Staging"
HEAD https://test.badhub.de/api/live_update.php  → 401  WWW-Authenticate: Basic
POST https://test.badhub.de/api/live_update.php  → 401  WWW-Authenticate: Basic   (mit korrektem Bearer-Token!)
GET  https://test.badhub.de/bts-relay/health     → 401  (Produktiv: 200 {"ok":true,…})
```

### Warum bts-light das nicht selbst lösen kann

HTTP kennt genau **einen** `Authorization`-Header, und den beansprucht der
Bearer-Token des Livetickers. Schickt bts-light stattdessen Basic-Credentials
für nginx, kommt die Anfrage zwar durch die Wand — aber PHP findet keinen
Bearer und lehnt selbst mit 401 ab. Ein Client-seitiger Zugang zum Testsystem
ist damit **nicht baubar**; die Freigabe muss serverseitig passieren.

### Freigegeben am 26.08.2026 — so umgesetzt

Auf dem Server erledigt. Zwei Teile, bewusst getrennt:

1. **`/etc/nginx/conf.d/bts-test-freigabe.conf`** (neu) enthält den
   `map`-Block, der `$test_realm` je nach Pfad auf den Staging-Realm oder auf
   `off` setzt.
2. Im vHost wurde aus `auth_basic "test.badhub.de — Staging";` genau
   `auth_basic $test_realm;`.

**Warum der `map` in `conf.d` liegt:** `/etc/nginx/sites-enabled/test.badhub.de`
ist **kein Symlink**, sondern eine aus `badhub.de` *abgeleitete* Datei („nicht
von Hand gepflegt", siehe ihren Dateikopf). Eine Änderung dort überlebt das
nächste Ableiten nicht — `conf.d` wird von `nginx.conf` **vor** `sites-enabled`
eingebunden und bleibt unberührt. Die eine unvermeidbare Zeile im vHost ist im
Dateikopf unter Punkt 3 der Ableitungs-Änderungen vermerkt.

**Falle beim Nachmachen:** `sites-available/test.badhub.de` (5 KB) und
`sites-enabled/test.badhub.de` (42 KB) sind zwei völlig verschiedene Dateien.
Ein Patch in `sites-available` ist wirkungslos, und `nginx -t` merkt davon
nichts — die Kontrolle ist `nginx -T | grep …` auf der *aktiven* Konfiguration.

Verifiziert nach dem Reload:

```
GET  /api/live_update.php   → 405   (PHP antwortet, will POST)
GET  /api/v1/pronunciations → 200
POST /api/live_update.php   → 400 "Malformed JSON"   mit Verbands-Token  ← Auth durch
POST /api/live_update.php   → 401                    mit falschem Token
GET  /  ·  /live  ·  /checkin/…  → weiterhin 401 Basic
```

### Was freigegeben ist

Nur Maschinen-Endpunkte mit eigener Bearer-Auth plus Rate-Limit — die Wand
schützt dort nichts, was nicht schon geschützt wäre. Alle HTML-Seiten bleiben
dahinter, denn die Test-DB ist eine tägliche Kopie der Produktivdaten:

| Pfad | Wofür |
|---|---|
| `/api/live_update.php` | Liveticker-Push **und** Check-In-Meldeliste |
| `/api/checkin-branding` | Logo- und Sponsoren-Push |
| `/checkin/<uuid>/tl/*` | Check-In-Panel der Turnierleitung |
| `/api/v1/pronunciations`, `/api/v1/club-logo` | Aussprache, Vereinslogos (öffentliche GETs) |
| `/api/bts_log.php`, `/api/tablet_log.php`, `/api/pi_log.php` | Diagnose-Logs |
| ~~`/bts-relay/`~~ | **Nicht freigegeben** — auf `test.badhub.de` gibt es gar keinen Relay-Block; der Cloud-Modus fällt im Testbetrieb aus, Tablets gehören dort auf LAN |

### Was bts-light stattdessen tut

Ein 401 **mit** `WWW-Authenticate: Basic` ist kein falsches Liveticker-Passwort,
sondern eine Wand davor. `badhub::push::abgelehnt` trennt beide Fälle und meldet
den zweiten als [`PushError::BasicAuthWall`] mit eigenem Text. Ohne diese
Unterscheidung meldete bts-light „Badhub lehnte die Anmeldung ab – Passwort
prüfen" — und man sucht stundenlang am völlig richtigen Token.

Die Erkennung liest die **Bytes** des Headers, nicht `to_str()`: Der echte
Realm enthält einen Gedankenstrich außerhalb von ASCII, für den `to_str()`
`Err` liefert — die Prüfung finge sonst ausgerechnet am Ernstfall ins Leere.

### Das Token ist dasselbe wie auf Produktiv

Die Verbands-Tokens liegen als bcrypt-Hash in der DB-Tabelle
`liveticker_tournaments` (badhub `lib/live_update_lib.php`, `liveUpdateAuth`);
der `.env`-Wert `BTS_TICKER_PASSWORD` greift nur noch für Legacy-Zeilen. Der
Sync `ops/badhub-prod-to-test.sh` spiegelt täglich 05:00 UTC die komplette
Prod-DB nach `badhub_test` und nimmt nur `admin_users` und `widget_api_keys`
aus. **Das Testsystem hat also dieselben Verbands-Tokens** — das Feld
„Liveticker-Passwort des Testsystems" bleibt für den Fall, dass dort einmal
eine eigene Zeile angelegt wird.

## Der Prozess-Schalter

Cloud-Relay und Log-Upload haben keinen Config-Zugriff. Für sie hält
`src-tauri/src/badhub_host.rs` einen `AtomicBool`, der an **genau drei**
Stellen aus derselben Push-URL nachgezogen wird:

- `AppConfig::load_from` — jedes Laden der Konfiguration (App-Start, auch eine
  von Hand geänderte Datei),
- `commands::save_config` — jedes Speichern der Einstellungen,
- `commands::import_identity` — das Identitäts-Bündel (ADR 0006) bringt eine
  eigene Push-URL mit; ohne diese Stelle liefe der Liveticker auf dem einen
  und die Tablets liefen auf dem anderen System, bis die App neu startet.

Mehr Setzer darf es nicht geben, sonst ist die Ableitung wieder eine zweite
Wahrheit. Umgekehrt darf **kein Test** ihn setzen: Die Unit-Tests der
Bibliothek teilen sich einen Prozess, und ein Fixture auf `test.badhub.de`
kippte die festen Erwartungen in `badhub::push` und `tablet::slave_bridge`.

Wie jeder Wechsel der Verbindungsart greift die Umstellung des Relays erst
beim nächsten **Stoppen/Starten** der Übertragung (R3).

## Bedienung

Setup/Einstellungen → **1 · Liveticker-Ziel** → Schalter **„Testsystem
(test.badhub.de)"**. Solange er an ist:

- zeigen die Verbands-Kacheln die Test-Adressen (auch der Kopier-Knopf),
- erscheint ein Feld **„Liveticker-Passwort des Testsystems"** — vorbelegt mit
  dem Verbands-Token. Hat das Testsystem eigene Tokens, kommt sonst
  „Badhub lehnte die Anmeldung ab",
- trägt die Kopfzeile der App auf jeder Seite **„… · TESTSYSTEM"** in
  Bernstein.

Im manuellen Modus ist das URL-Feld die Wahrheit: Wer dort `test.badhub.de`
eintippt, sieht den Schalter umspringen; das Speichern biegt die Adresse nicht
mehr zurück.

## Beteiligte Stellen

- `src-tauri/src/badhub_host.rs` — Host-Logik + Prozess-Schalter (Rust)
- `src/io/badhubZiel.mjs` (+ `.d.mts`) — dieselbe Logik fürs Frontend,
  getestet in `scripts/test-badhub-ziel.mjs`
- `src/presets.ts` — `findPresetFor` (systemunabhängige Preset-Erkennung),
  `tenantShortLabel` (Marke „TESTSYSTEM")
- `src/pages/SetupWizard.tsx` — Schalter, Warnhinweis, Testpasswort
- `src/components/AppShell.tsx` — Einfärbung der Kopfzeile
- `src-tauri/src/tablet/relay_client.rs`, `slave_bridge.rs`, `carrier.rs`,
  `log_upload.rs`, `tablet/server.rs`, `commands.rs` — vormals hart
  verdrahtete Adressen
- `src-tauri/assets/tablet.html`, `tl.html` — Vereinslogo-Basis über
  `location.origin`

## Nicht umgestellt

- Der eingebaute Log-Token und die Verbands-Tokens (bewusst eingebettet, siehe
  CLAUDE.md „Embedded Secrets").
- Der Auto-Update-Endpunkt (`badhub.de/download/bts-light/`) — eine
  Testinstallation soll dieselben Updates bekommen wie eine echte.
- Beispiel-URLs in Fehlertexten.
