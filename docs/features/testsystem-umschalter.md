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

**Voraussetzung für den Cloud-Modus:** Auf dem Testsystem muss ein
`bts-relay` hinter nginx unter `/bts-relay/` laufen. Fehlt er, finden die
Tablets im Cloud-Modus kein Ziel — dann für den Testlauf auf LAN stellen.

Fremde Hosts bleiben **immer** unangetastet: Wer eine eigene badhub-Instanz
betreibt, bekommt seine Adresse nicht umgeschrieben (Test in
`badhub_host::tests::fremde_hosts_bleiben_unangetastet`).

## Der Prozess-Schalter

Cloud-Relay und Log-Upload haben keinen Config-Zugriff. Für sie hält
`src-tauri/src/badhub_host.rs` einen `AtomicBool`, der an **genau zwei**
Stellen aus derselben Push-URL nachgezogen wird:

- `AppConfig::load_from` — jedes Laden der Konfiguration (App-Start, auch eine
  von Hand geänderte Datei),
- `commands::save_config` — jedes Speichern der Einstellungen.

Mehr Setzer darf es nicht geben, sonst ist die Ableitung wieder eine zweite
Wahrheit. Wie jeder Wechsel der Verbindungsart greift die Umstellung des
Relays erst beim nächsten **Stoppen/Starten** der Übertragung (R3).

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
- `src-tauri/src/tablet/relay_client.rs`, `slave_bridge.rs`,
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
