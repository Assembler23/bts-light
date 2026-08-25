# Verschlüsselte LAN-Strecke (https + wss) — Spezifikation

> Status: **Entwurf** (via /idee: Brief → Grill → How-To → Review), 2026-08-25.
> Quelle: Betreiber-Gespräch 2026-08-24 („können wir denn auch mal lösen, dass wir
> lokales Netz auch über https kommunizieren … bzw. verschlüsselt (wss / https)").
> Betroffene Crates: `src-tauri` (Kern + UI).
> ADR: `docs/adr/0047-lan-tls-selbstsigniert-konkret.md` (zu erstellen) —
> konkretisiert [ADR 0005](../adr/0005-lan-https-selbstsigniert.md).

## Kontext / Problem

Der eingebettete Tablet-Server spricht im Hallennetz ausschließlich **Klartext-HTTP**
(`server.rs:44`, `TABLET_PORT = 8088`). Zwei Folgen:

1. **Spielstände, Spielernamen und Lizenznummern reisen unverschlüsselt** durch ein
   Netz, in dem regelmäßig fremde Geräte hängen (Verleih-Sets, Hallen-WLAN).
2. **Keine Akku-Anzeige im LAN-Betrieb.** Die Battery-API ist nur im *Secure Context*
   verfügbar. Cloud-Tablets (`https://badhub.de`) melden ihren Akkustand, LAN-Tablets
   prinzipbedingt nicht — die Turnierleitung sieht im LAN-Betrieb keine Akkus
   (Turnier-Feedback 19.07.2026). Im Frontend bleibt das Badge
   (`TabletPanel.tsx:476`) im LAN schlicht leer.

[ADR 0005](../adr/0005-lan-https-selbstsigniert.md) hat am 19.07.2026 bereits
entschieden, HTTPS mit selbstsigniertem Zertifikat anzubieten — **implementiert wurde
es nie**: `8443` kommt im Code nicht vor, Server-TLS existiert nirgends, `rustls` liegt
nur client-seitig im Baum (`src-tauri/Cargo.toml:32,35`). Diese Spec löst die
Entscheidung ein und konkretisiert sie um die Punkte, die 2026-07 noch nicht bedacht
waren.

**Zusätzlicher Treiber:** Für das Nachfolge-Vorhaben *Transport-Bündelung in der
fernen Halle* sollen Geräte der Partnerhalle lokal statt über die Cloud angebunden
werden. Ohne TLS würden sie dabei den Secure Context **verlieren**, den sie über
`https://badhub.de` heute haben. Der TLS-Baustein ist damit dessen Voraussetzung —
nützt aber unabhängig davon sofort dem Master-LAN.

Tilos Original-BTS betreibt in der Praxis HTTPS mit selbstsigniertem Zertifikat; der
Weg ist im Feld erprobt.

## Zielbild & Erfolgskriterien

Der Tablet-Server bietet seine Seiten **zusätzlich** über `https` an, die zugehörigen
WebSockets über `wss`. Port `8088` bleibt unverändert offen, damit kein Bestandsgerät
bricht. Der Turnierleiter bekommt in der Feld-Übersicht je Feld einen zweiten QR-Code
(„verschlüsselt"), muss aber nichts umstellen, damit alles weiterläuft.

**Messbare Erfolgskriterien**

1. Auf einem handbedienten Tablet ist nach **einmaliger** Bestätigung der
   Zertifikatswarnung der **Akkustand in der Turnierleitung sichtbar**
   (`TabletPanel.tsx:476`, Badge erscheint) — im reinen LAN-Betrieb, ohne Internet.
2. Ein Bestands-Pi, der per Subnetz-Scan `http://…:8088/health` sucht, findet den
   Server **unverändert** (`pi/shared-startbrowser.sh:84-100`).
3. Nach Neustart von App **und** Tablet ist **keine erneute** Zertifikatsbestätigung
   nötig.
4. Ein Gerät mit falsch gestellter Uhr (RTC-los, ohne NTP) lädt die https-Seite
   **ohne stillen Fehlschlag**.
5. Tablet-, Monitor- und TL-Web-WebSockets laufen über `wss` durch (`/ws`,
   `/monitor-ws`, `/tl-ws`).

## Nicht-Ziele

- **Kein Zwangs-Rollout auf die Pi-Flotte.** Court-Monitore dürfen auf
  `http://…:8088` bleiben. `pi/shared-startbrowser.sh` und `pi/setup-monitor.sh`
  werden hier **nicht** umgestellt.
- **Kein Anspruch „durchgängig verschlüsselt".** Es gilt *verschlüsselt, wo das Gerät
  es kann*. Diese Formulierung ist bewusst gewählt und gehört so in die Doku.
- **Kein Secure Context für Kiosk-Geräte.** `--ignore-certificate-errors` lädt die
  Seite, macht sie aber **nicht** zum Secure Context. Der Akku-Nutzen gilt
  ausdrücklich nur für handbediente Tablets, an denen jemand die Ausnahme einmal
  bestätigt.
- **Keine lokale CA, kein Trust-Store-Rollout** (ADR 0005 verwarf das als Option C).
- **Kein HTTP→HTTPS-Zwangs-Redirect** — er würde die Bestands-Pis brechen.
- **Kein automatisches Umschalten bestehender Geräte** auf die neue Adresse.
- Der Cloud-Pfad (Relay auf badhub.de) bleibt unberührt; dort terminiert nginx TLS.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - `src-tauri/src/tablet/tls.rs` — **neu** (Zertifikat erzeugen/laden, SAN-Sammlung).
  - `src-tauri/src/tablet/server.rs` — Router-Extraktion aus `run()` (`:520-604`),
    zweiter Listener `run_tls`, schema-bewusste URL-Bildung (`lan_host()` `:657`,
    QR-Route `:953`).
  - `src-tauri/src/commands.rs` — Handle `tls_server` (neben `:84`), Start (`:1088`),
    Abbau (`:1199`), `tablet_overview()` (`:1333`) liefert die HTTPS-Basis mit.
  - `src-tauri/src/config.rs` — `TlsConfig`.
  - `src-tauri/src/tablet/mdns.rs` — optionaler TXT-Record (siehe Annahmen).
  - `src-tauri/installer/firewall-hooks.nsh` — zweite netsh-Regel (TCP 8443).
  - `src/pages/TabletPanel.tsx`, `src/pages/CourtMonitorPanel.tsx`,
    `src/pages/Dashboard.tsx` — zweite Adresse anzeigen.
- **Architekturregeln:**
  - **R1** bleibt gewahrt — das Frontend erfährt die HTTPS-Basis über
    `tablet_overview()`, keinen Direktzugriff.
  - **R2/R5** unberührt — der Ergebnispfad und `process_result` werden **nicht**
    angefasst; TLS ist reine Transportschicht.
  - **R3** bleibt zweiwertig: `Lan`/`Cloud`/`LanAndCloud` unverändert. TLS ist
    **keine** neue Verbindungsart, sondern ein zweiter Zugang zum bestehenden
    LAN-Server.
  - **R4/R6** unberührt.
- **Konfiguration & Abwärtskompatibilität:** neues `TlsConfig { enabled: bool,
  port: u16 }` nach dem Muster `PrintConfig` (`config.rs:335-342`), eingehängt als
  `#[serde(default)] pub tls: TlsConfig`. `#[serde(default)]` am Container deckt jede
  Tiefe ab (`config.rs:801-803`) — **jede bestehende `config.json` bleibt unverändert
  lesbar**. Default `enabled: true`, `port: 8443`. Tauri-`identifier`
  `de.badhub.btslight` und Updater-Pfad `download/bts-light/` bleiben unangetastet.
- **Datenschutz:** Das Vorhaben **verbessert** den Datenschutz (Spielernamen und
  Lizenznummern reisen künftig verschlüsselt). Keine neuen Felder, keine neuen
  Speicherungen, kein Geburtsjahr. Das Schlüsselmaterial ist kein personenbezogenes
  Datum, aber schützenswert (siehe Risiken).
- **Abhängigkeiten:** zwei neue Cargo-Crates, beide MIT OR Apache-2.0 und aus dem
  `rustls`-Ökosystem:
  - `rcgen` 0.14.9 — Zertifikatserzeugung, 24,9 Mio Downloads/Monat, zuletzt
    veröffentlicht 10.08.2026, Repo `rustls/rcgen`.
  - `tokio-rustls` 0.26.4 — TLS-Serving, 155,6 Mio Downloads/Monat, Repo
    `rustls/tokio-rustls`. Liegt bereits transitiv im `Cargo.lock` und wird nur zur
    **Direkt**-Abhängigkeit erhoben.
  - Die rustls-Version ist an `Cargo.lock:3527` (**0.23.40**) anzugleichen, sonst
    entstehen zwei rustls-Kopien im Baum.
  - Kein badhub-Endpunkt, keine BTP-Berührung, keine npm-Abhängigkeit.

## Akzeptanzkriterien

**Positivfälle**

- [ ] Bei aktivem LAN-Modus lauscht der Server auf `8088` (HTTP) **und** `8443`
      (HTTPS); beide liefern dieselben Routen.
- [ ] `https://bts-light.local:8443/court/<id>` lädt die Tablet-Seite, und die
      WebSocket-Verbindung kommt über `wss` zustande (`tablet.html:1372` wählt das
      Protokoll bereits selbst aus `location.protocol`).
- [ ] `wss` funktioniert ebenso für `/monitor-ws` und `/tl-ws`.
- [ ] Nach einmaliger Zertifikatsbestätigung meldet ein Tablet seinen Akkustand; das
      Badge erscheint in `TabletPanel.tsx:476`.
- [ ] Beim zweiten App-Start wird das **bestehende** Zertifikat geladen, nicht neu
      erzeugt (Byte-Gleichheit der Dateien).
- [ ] Das erzeugte Zertifikat trägt `bts-light.local`, `localhost`, `127.0.0.1` und
      die lokalen IPv4-Adressen als SAN.
- [ ] `notBefore` liegt fest auf **2020-01-01**, `notAfter` zehn Jahre nach Erzeugung.
- [ ] Die Feld-Übersicht zeigt je Feld **beide** Adressen (HTTP und HTTPS), HTTP
      bleibt die Vorgabe.

**Negativ- und Fehlerfälle**

- [ ] Ist Port 8443 belegt, protokolliert die App den Fehler und der **HTTP-Server
      läuft unbeeinträchtigt weiter** — der Turnierbetrieb bricht nicht.
- [ ] Sind Zertifikatsdateien beschädigt oder leer, liefert das Modul einen sauberen
      Fehler (**kein Panic**); der HTTP-Server läuft weiter.
- [ ] Ein fehlgeschlagener TLS-Handshake beendet die Accept-Schleife **nicht** —
      der nächste Client wird normal bedient.
- [ ] `tls.enabled = false` in der `config.json` startet **keinen** TLS-Listener;
      alles verhält sich exakt wie vor dieser Version.
- [ ] Eine `config.json` **ohne** `tls`-Abschnitt (jede Bestandsinstallation) lädt
      fehlerfrei und bekommt die Defaults.
- [ ] Eine ältere App-Version liest eine `config.json` **mit** `tls`-Abschnitt
      fehlerfrei (Rollback-Fähigkeit).
- [ ] Der Subnetz-Scan eines Bestands-Pi auf `http://…:8088/health` liefert
      unverändert `200`.
- [ ] `tl_push_takt` läuft bei aktivem HTTP **und** HTTPS **genau einmal** — nicht
      doppelt.
- [ ] ALPN bietet ausschließlich `http/1.1` an (nicht `h2`), damit die
      WebSocket-Upgrades nicht brechen.

## Tests

**Rust-Unit-Tests (TDD, vor der Implementierung zu schreiben)** — in
`src-tauri/src/tablet/tls.rs`:

1. `erzeugt_zertifikat_in_leerem_verzeichnis` — beide Dateien entstehen.
2. `zweiter_aufruf_laedt_statt_neu_zu_erzeugen` — Byte-Gleichheit; sichert die
   Zusage „Ausnahme überlebt Neustarts" (Erfolgskriterium 3).
3. `not_before_liegt_in_der_vergangenheit` — **Regressionswächter** für die
   RTC-lose Pi-Uhr; dieser Test darf nie „repariert" werden.
4. `san_enthaelt_mdns_namen_und_localhost`.
5. `beschaedigte_dateien_ergeben_fehler_ohne_panic`.
6. `alpn_bietet_nur_http11` — Wächter gegen die stillste Falle des Vorhabens.

**Regression:** Die rund 120 bestehenden Handler-Tests (`server.rs:3909 ff.`) müssen
nach der Router-Extraktion **unverändert grün** bleiben — sie sind die Absicherung
dafür, dass das Refactoring verhaltensneutral war.

**Optionaler Listener-Test:** „beide Ports lauschen" wäre der **erste**
HTTP-Listener-Test im Repo. Machbar nach dem Muster `spawn_mock_btp`
(`server.rs:3930`, Port 0 + `local_addr().port()`); dafür nimmt `run_tls` den Port als
Parameter. ADR 0019 grenzt ausdrücklich ab, dass HTTP-Layer und Socket-Realität von
den bestehenden Tests **nicht** bewiesen werden.

**Gates:** `cargo test` grün, `cargo clippy --workspace --all-targets -- -D warnings`
grün (der CI-Schalter), `npm run build` fehlerfrei.

**Manueller Turnier-Testfall (Feldtest-Pflicht, nicht automatisierbar):**
1. Tablet öffnet die HTTPS-Adresse, Warnung einmal bestätigen → Akku-Badge erscheint.
2. Tablet neu starten → **keine** erneute Warnung.
3. Punkt zählen → Stand erscheint am Monitor (wss durchgängig).
4. Bestands-Pi einschalten → findet den Server unverändert über HTTP.
5. Pi ohne Internet und ohne NTP booten (falsche Uhr) → HTTPS-Seite lädt.

## Risiken & Rollback

| Risiko | Gegenmaßnahme |
|---|---|
| **ALPN bietet `h2`** → `/ws`, `/monitor-ws`, `/tl-ws` brechen | ALPN fest auf `http/1.1`; Unit-Test + gezielter Feldtest |
| **Doppelter `tl_push_takt`** — zwei Sekundentakte schreiben beide `notify_tl`; der Fehler ist in `server.rs:594-601` als **schon einmal aufgetreten** dokumentiert | Takt bleibt allein in `run()`; `run_tls()` startet ihn nie |
| **Windows-Firewall blockt 8443** — im Feld nicht von „TLS geht nicht" unterscheidbar | zweite netsh-Regel in `firewall-hooks.nsh` |
| **DHCP-Wechsel entwertet Ausnahmen** — Zertifikatsausnahmen hängen an Host **und** Port | `bts-light.local` als SAN ist Pflicht, nicht Kür; Betriebsempfehlung: Namen nutzen, nicht die IP |
| **Origin-Wechsel im laufenden Turnier** — `https://…:8443` ist eine andere Origin als `http://…:8088`; `tablet.html` hält `pendingResult` und Kartenereignisse origin-gebunden im `localStorage` → **unbestätigte Ergebnisse gingen verloren** | Beide Adressen laufen parallel, **kein** Zwangsumschalten; Doku schreibt fest: Umstellung nur **zwischen** Turnieren |
| **Zwei rustls-Kopien im Baum** | Version an `Cargo.lock:3527` (0.23.40) angleichen; `dependency-auditor` |
| **Privater Schlüssel auf Platte** | Ablage im `app_config_dir()` neben `config.json`; `0600` unter `#[cfg(unix)]` (Windows-Nutzerprofil trägt den Schutz selbst) |
| **TLS-Start scheitert** | Fehler wird protokolliert, HTTP läuft unberührt weiter |

**Rollback:** TLS ist rein additiv. `tls.enabled = false` schaltet es ab; eine ältere
App-Version ignoriert das Feld dank `serde(default)` und läuft unverändert auf 8088.
Es gibt keinen Migrationsschritt, der sich nicht rückgängig machen ließe.

## Offene Fragen / Annahmen

**Annahmen**

- **Der mDNS-Hostname genügt.** `bts-light.local` löst bereits auf die IP auf; ein
  Browser, der `https://bts-light.local:8443` öffnet, braucht **keinen** zweiten
  Service-Eintrag — der Port steht in der URL. Ein zweiter `ServiceInfo` wäre nur für
  Service-Discovery nötig, und die liest heute **niemand** (einzige Konsumenten sind
  die Pi-Skripte, und die nutzen mDNS erst als dritte Wahl hinter IP-Cache und
  Subnetz-Scan). Ein TXT-Record `https=8443` bleibt als Ein-Zeilen-Wegweiser optional.
- **Die 398-Tage-Grenze greift nicht.** Sie gilt für öffentlich vertrauenswürdige CAs,
  nicht für ein selbstsigniertes Zertifikat mit manuell bestätigter Ausnahme. Sollte
  ein Browser sie dennoch erzwingen, fällt Erfolgskriterium 3 — dann ist die Laufzeit
  zu kürzen und die Ausnahme regelmäßig neu zu bestätigen.
- Der Turnierleiter kann die einmalige Browser-Warnung bestätigen („Erweitert →
  trotzdem fortfahren"). ADR 0005 nennt bereits als bekannte Grenze, dass manche
  Kiosk-Browser das nicht erlauben.

**Offene Fragen (bewusst offen, blockieren die Umsetzung nicht)**

- Soll der Knopf „Zertifikat neu erzeugen" (Notfall bei IP-Wechsel) sofort mitkommen
  oder erst, wenn der Bedarf im Feld auftritt? *Empfehlung: sofort, aber gut versteckt
  unter Wartung — er entwertet alle bestehenden Ausnahmen.*
- Bleiben `pi/shared-startbrowser.sh` und `pi/setup-monitor.sh` dauerhaft auf HTTP,
  oder gibt es später ein Kriterium für die Umstellung? *Hier bewusst Nicht-Ziel.*
- `commands.rs:2647` und `:2716` bauen im **klassischen LAN-Slave** hart
  `http://{btp.host}:8088/…`. Das bleibt funktionsfähig, solange 8088 offen ist —
  eine spätere Abschaltung von HTTP müsste diese Stellen mitnehmen.

## Betroffene Doku-Dateien

- `docs/tablet.md` — Tablet-Server, Adressen, die einmalige Zertifikatswarnung.
- `docs/court-monitor.md` — Monitor-Adressen; warum die Pis auf HTTP bleiben.
- `docs/pi-setup.md` und `docs/pi-dual-image.md` — ausdrücklicher Vermerk, dass die
  Pi-Skripte unverändert HTTP sprechen.
- `docs/changelog.md` — veröffentlichte Version.
- `docs/roadmap.md` — Verweis auf diese Spec.
- `docs/adr/0005-lan-https-selbstsigniert.md` — auf „konkretisiert durch ADR 0047"
  fortschreiben.
- **Neue Zeile in `CLAUDE.md`** (Doku-Tabelle) für `src-tauri/src/tablet/tls.rs`.

## Umsetzungs-Hinweise

*Erst nach Freigabe relevant.* Ergebnis der How-To-Phase
(`docs/features/_intake/lan-tls-verschluesselt/3-how-to.md`), sieben Etappen:

1. **Router extrahieren** — `fn router(ctx) -> Router` aus `server.rs:522-586`
   herausziehen; reines Refactoring, bestehende Tests sind die Absicherung.
2. **Zertifikatsmodul** `tablet/tls.rs` — reine Logik, voll testbar, TDD zuerst.
3. **TLS-Listener** — `TlsConfig` in `config.rs`, `run_tls` in `server.rs`
   (**ohne** `tl_push_takt`), Handle-Verdrahtung in `commands.rs`.
4. **Firewall** — zweite netsh-Regel in `installer/firewall-hooks.nsh`.
5. **Adressen im UI** — beide anbieten statt umschalten; das Muster „zwei QR-Codes je
   Feld" existiert bereits für LAN/Cloud in `TabletPanel.tsx`.
6. **Doku + ADR 0047.**
7. **Reviews.**

**Gewählter Weg (ADR-Kern):** `tokio-rustls` direkt mit einem eigenen
`axum::serve::Listener`-Adapter (rund 40 Zeilen) statt der Crate `axum-server`.
Begründung: `tokio-rustls` liegt bereits im `Cargo.lock`, steht unter demselben
`rustls`-Dach wie `rcgen` und bringt keine Einzelmaintainer-Abhängigkeit in den
kritischen Pfad. Verworfene Alternative `axum-server` 0.8.0 (MIT, 8,9 Mio
Downloads/Monat, seit 06.12.2025 unverändert) spart den Adapter, muss aber sowohl der
`axum`- als auch der `rustls`-Version folgen.

**Reviews:** `dependency-auditor` **vor** dem Merge (zwei neue Direkt-Abhängigkeiten),
`security-reviewer` (TLS-Terminierung, privater Schlüssel auf Platte),
`code-reviewer` (Pflicht nach jeder Änderung).

**Version** gemeinsam bumpen in `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`
und `package.json` — die Nummer **erst beim Merge** festlegen, weil parallele PRs
sonst kollidieren.
