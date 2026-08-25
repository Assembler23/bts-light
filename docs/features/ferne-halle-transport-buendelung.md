# Transport-Bündelung in der fernen Halle (Multiplexer) — Spezifikation

> Status: **Entwurf** (via /idee: Brief → Grill → How-To → Review), 2026-08-25.
> Quelle: Betreiber-Beobachtung 2026-08-24 — „ein Tablet in der Slave-Halle ist
> sofort auf das badhub-relay umgestiegen, obwohl wir eigentlich eine LAN-Adresse
> angegeben haben".
> Betroffene Crates: `src-tauri`, `relay`, `relay-proto`.
> ADR: `docs/adr/0048-substrom-adressierung-traeger.md` (zu erstellen) —
> schreibt [ADR 0002](../adr/0002-ferne-halle-direkt-cloud-geraete.md) fort.
> **Vorbedingung:** [`lan-tls-verschluesselt.md`](lan-tls-verschluesselt.md) (freigegeben).

## Kontext / Problem

In der fernen Halle läuft heute **kein** LAN-Tablet-Server. Stattdessen läuft die
**Slave-Brücke** (`slave_bridge.rs`), und die tut genau eins: umleiten. `/court/{id}`
und `/monitor` antworten mit einer Weiterleitung auf `https://badhub.de/bts-relay/…`,
und selbst die lokal gerenderte Feldauswahl `/felder` verlinkt in die Cloud.

Die im Feld eingetippte LAN-Adresse ist damit ein Türsteher, keine LAN-Verbindung.
Jedes Gerät der Halle baut **seine eigene** Verbindung ins Internet auf. Das ist
kein Fehler, sondern [ADR 0002](../adr/0002-ferne-halle-direkt-cloud-geraete.md)
(„Weg A"), bewusst so gebaut, weil der Ergebnisweg ins Master-BTP nur über den
Master führt.

ADR 0002 hat den Gegenentwurf („Weg B") ausdrücklich **aufgeschoben, nicht
verworfen**. Diese Spec holt dessen **Transport-Hälfte** nach — ohne die dort
ebenfalls beschriebene Offline-Pufferung.

**Wer hat den Schmerz?** Die Crew in der fernen Halle, die Geräte an einer
badhub-Adresse einrichtet statt an einer lokalen; und der Betreiber, der jedes
Gerät einzeln durch den Hallen-Uplink telefonieren lässt.

## Zielbild & Erfolgskriterien

In der Partnerhalle sieht keine Crew und kein Gerät mehr eine badhub-Adresse.
Tablets, Court-Monitore und Vorbereitungs-/Übersichtsanzeigen sprechen
ausschließlich mit dem Slave-PC unter `bts-light.local` (verschlüsselt, siehe
Vorbedingung). Ihr WebSocket-Verkehr läuft gebündelt über **eine**
Trägerverbindung zum Master-Relay. Ergebnisse landen unverändert im Master-BTP.

**Messbare Erfolgskriterien**

1. Aus der fernen Halle besteht **genau eine** WebSocket-Verbindung zum Relay,
   unabhängig von der Zahl der Geräte (ablesbar am Relay).
2. Ein Tablet der fernen Halle zählt einen Punkt → der Stand erscheint auf dem
   Court-Monitor derselben Halle, und das Ergebnis landet im Master-BTP.
3. Kein Gerät der Halle zeigt in der Adresszeile oder in einem Netzwerk-Abruf eine
   `badhub.de`-Adresse.
4. Fällt **ein** Tablet aus (WLAN weg), gibt der Relay dessen Court-Slot frei,
   ohne die übrigen Geräte der Halle zu stören.
5. Fällt der **Träger** aus, räumt der Relay genau dessen Substrom-Einträge und die
   Halle kann über die weiterhin gültigen Direkt-Cloud-Adressen weiterarbeiten.
6. Kein Tablet der fernen Halle meldet dauerhaft „veraltete Fassung".

## Nicht-Ziele

- **Kein schreibender Rückkanal Slave→Master.** Steuerung (Spiele in Vorbereitung
  aufrufen, Zweit-/Drittaufruf) läuft weiter über die TL-Web-Oberfläche. Die
  Roadmap-Punkte `roadmap.md:63` und `:76` bleiben damit ausdrücklich unberührt.
- **TL-Web-Geräte bleiben Direkt-Cloud.** So reisen keine Geräte-Token und keine
  Schreibrechte durch den Träger.
- **Kein Zustandsspiegel, keine lokale Ergebnis-Pufferung.** Fällt das Internet
  aus, steht die Halle wie heute. Offline-Fähigkeit ist **nicht** Ziel — das bleibt
  die zweite Hälfte von „Weg B" und ein eigenes Vorhaben.
- **Kein automatischer Rückfall** auf Direkt-Cloud bei Slave-Ausfall. Der Notnagel
  ist dokumentiert, nicht automatisiert.
- **Keine HTTP-Nutzlast im Träger** — Bilder, Logos und Zustandsabrufe laufen
  weiterhin als eigene HTTPS-Anfragen (siehe Architektur-Entscheidung).
- Gilt **nicht** für den klassischen LAN-Slave (leerer `master_namespace`) — der
  liest BTP selbst und hat gar keinen Relay.

## Architektur-Entscheidung (Kern der Spec)

**1. WebSocket-Verkehr geht durch den Träger, HTTP-Verkehr nicht.**

Über die Tablet-WebSocket läuft heute nichts Großes: `MAX_STATE_LEN` = **64 KB**
(`relay/main.rs:169`), Punktverlauf 8 KB. Die dicken Brocken — Werbebilder **12 MB**
(`:87`), Logo **2 MB** (`:93`) — sind HTTP-Uploads Host→Relay und HTTP-Downloads
Relay→Anzeige. Head-of-Line-Blocking entstünde also **erst dadurch, dass der Mux die
HTTP-Strecken mit in den Träger zöge**. Tut er nicht.

**2. Der Slave terminiert lokal und stempelt selbst.**

Die Seiten-Marke ist ein Hash über den Seiteninhalt (`relay-proto:1380-1385`), den
**jeder Server aus seiner eigenen Binärdatei** berechnet (`relay/main.rs:52`,
`assets.rs:19`). Lieferte der Slave die Seite aus, während der Relay den `Pong`
stempelt, wichen die Marken fast immer ab — jedes Tablet der Halle meldete dauerhaft
„veraltet", und ein `ReloadTablets` löste eine **Reload-Schleife mitten im Turnier**
aus. Die Doku am Enum hält das ausdrücklich fest (`relay-proto:1292-1310`).

Deshalb: Der Slave liefert die Seiten aus **eigenem** Asset-Bestand, terminiert die
Tablet-WebSocket lokal und stempelt den `Pong` mit **seiner** Marke — die für die von
ihm ausgelieferte Seite die richtige ist. Der Träger transportiert nur Fachframes.

Das löst zugleich: die `__BASE__`-Injektion für `monitor.html` (das im Gegensatz zu
`tablet.html` **nicht** origin-relativ ist, `monitor.html:406`) und die
Vereinslogo-Weiche (`tablet.html:1051`, Zweig `/info/club-logo` bei leerem Präfix).

**3. Substrom-Adressierung: synthetischer Kanal je Substrom.**

Der Relay unterscheidet Geräte heute über `Tx::same_channel()` — an **acht** Stellen
(`:2541, 2547, 2708, 2945, 2998, 3427, 3489, 4342`), darunter `is_holder` (`:2945`),
das R4 („ein aktives Tablet je Court") durchsetzt, und `release_host_slot` (`:3489`).

Statt diese acht Stellen auf eine Stream-Identität umzubauen, bekommt **jeder
Substrom im Relay seinen eigenen `mpsc`-Kanal**; ein Fan-in-Task bündelt zurück auf
den Träger. Damit bleiben `type Tx` und alle acht Stellen **unverändert** — und mit
ihnen R4, [ADR 0017](../adr/0017-reconnect-ownership.md) und
[ADR 0020](../adr/0020-tote-verbindung-read-idle-tablet-stale.md) strukturell gültig,
statt neu bewiesen werden zu müssen.

Der Preis ist ein lokales Refactoring: `tablet_conn` (`:2232`) und `monitor_conn`
(`:2416`) lesen heute direkt vom Socket; ihre Sitzungslogik wird in eine Funktion
gezogen, die Frames aus einem Kanal konsumiert.

## Betroffene Komponenten / Architekturregeln / Daten

- **`relay/`** — Sitzungslogik von der Transportschicht trennen (`tablet_conn`
  `:2232`, `monitor_conn` `:2416`); neue Route `/{ns}/carrier-ws` mit
  `carrier_conn`, eigenem Ping/Stale und eigenem Deckel.
- **`relay-proto/`** — `CarrierFrame`, `StreamOpen`, `StreamClose`, `CarrierHello`
  mit Protokollversion; Deckel `MAX_STREAMS_PER_CARRIER`, `MAX_CARRIER_FRAME_LEN`.
- **`src-tauri/src/tablet/slave_bridge.rs`** — vom 302-Weiterleiter zum lokalen
  Terminator (Seiten, WS-Endpunkte, HTTP-Endpunkte, Ping/Stale je Gerät).
- **`src-tauri/src/tablet/relay_client.rs`** — Vorlage für den Träger (Backoff
  1→30 s, Read-Idle 15 s, Pong-Echo `:189`).
- **`src-tauri/src/config.rs`** — `SlaveMuxConfig`.
- **`src-tauri/src/commands.rs`** — Start/Abbau des Trägers (`:1119-1140`, `:1199`).
- **Architekturregeln:**
  - **R1** gewahrt — kein neuer Frontend-Pfad am Kern vorbei.
  - **R2/R5** unberührt: `process_result` validiert jedes Ergebnis unverändert am
    Master. Der Träger ist reiner Transport.
  - **R3** — der Mux ist eine Ausprägung des Cloud-Pfads, keine neue Verbindungsart.
  - **R4 gewahrt**: Die Träger-Rolle steht **neben** dem `host`-Slot, nie darin.
    `try_claim_host` (`:3332`) und `release_host_slot` (`:3489`) werden nicht
    angefasst. „Ein aktives Tablet je Court" bleibt durch `is_holder` durchgesetzt,
    weil jeder Substrom seinen eigenen Kanal hat.
  - **R6** unberührt — Namespace bleibt die `install_id` des Masters.
- **Konfiguration:** `SlaveMuxConfig { enabled: bool }` nach dem Muster
  `PrintConfig` (`config.rs:335-342`), eingehängt mit `#[serde(default)]`.
  **Default `false`** — bewusst Opt-in, anders als beim TLS-Baustein, weil hier
  echtes Turnierrisiko im Spiel ist. Bestehende `config.json` bleibt lesbar; eine
  ältere App-Version ignoriert das Feld.
- **Datenschutz:** Durch den Slave reisen Spielernamen und Lizenznummern. **Keine
  Frame-Nutzlast darf ins Slave-Log** — dieses wird per `log_upload.rs` hochgeladen.
  Kein Geburtsjahr, keine neuen Felder.
- **Abhängigkeiten:** keine neue Cargo- oder npm-Abhängigkeit. Relay hinter nginx
  `/bts-relay/` unverändert. Kein BTP-, kein badhub-Endpunkt betroffen.

## Akzeptanzkriterien

**Positivfälle**

- [ ] Bei `mux.enabled = true`, aktivem `slave_mode` und gültigem
      `master_namespace` baut der Slave **genau eine** Trägerverbindung auf.
- [ ] Ein Tablet unter `https://bts-light.local:8443/court/<id>` erhält seine
      Match-Zuweisung und liefert sein Ergebnis ins Master-BTP.
- [ ] Ein Court-Monitor derselben Halle zeigt den Live-Stand.
- [ ] Die `device_id` kommt je Substrom **unverfälscht** am Relay an — der Slave
      setzt oder ersetzt sie nie.
- [ ] Der Slave beantwortet `/info/club-logo` selbst; `tablet.html` ruft kein
      `badhub.de` direkt.
- [ ] `monitor.html` wird mit dem **lokalen** `__BASE__` ausgeliefert.
- [ ] Der `Pong` trägt die Marke **des Slaves**, und kein Tablet meldet „veraltet".

**Negativ- und Fehlerfälle**

- [ ] Bricht **ein** lokales Gerät weg, sendet der Slave `StreamClose`; der Relay
      gibt genau dessen Court-Slot frei. Die übrigen Substrome laufen weiter.
- [ ] Bricht der **Träger** weg, räumt der Relay **alle** seine Substrome
      (`detach_tablet` je Feld, `unsubscribe_monitor` je Abo) und ruft
      `namespace_aufraeumen`.
- [ ] Der Relay kennt `/{ns}/carrier-ws` nicht oder lehnt die Protokollversion ab
      → der Slave fällt auf die **heutigen Weiterleitungen** zurück; die Halle
      arbeitet weiter.
- [ ] `mux.enabled = false` → der Slave verhält sich **exakt** wie heute
      (302-Weiterleitungen); nichts anderes ändert sich.
- [ ] Eine `config.json` **ohne** `mux`-Abschnitt lädt fehlerfrei.
- [ ] Ein Substrom kann **kein** fremdes Feld autorisieren: ein Frame von Substrom A
      wird für den Court von Substrom B abgewiesen (`is_holder`).
- [ ] Deckel greifen: mehr als `MAX_STREAMS_PER_CARRIER` Substrome werden abgewiesen,
      ohne den Träger zu beenden.
- [ ] Der `host`-Slot des Masters bleibt unberührt, während ein Träger verbunden ist.
- [ ] Ein Tablet ohne `localStorage` (leere `device_id`) verbindet sich, verliert
      aber wie heute die Reconnect-Ownership — dokumentiert, kein Absturz.

**Betrieb**

- [ ] Die Umstellung der Halle auf die lokalen Adressen erfolgt **zwischen**
      Turnieren, nie während eines laufenden. Grund: `https://bts-light.local` ist
      eine andere Origin als `https://badhub.de`; `tablet.html` hält `pendingResult`
      origin-gebunden im `localStorage` — ein Wechsel im Betrieb verlöre
      unbestätigte Ergebnisse. Steht so in der Bediendoku.

## Tests

Auf dem **ADR-0019-Harness** (`relay/main.rs:9432-10131` — in-process, keine echten
Sockets). Zwei Regeln des Harness gelten zwingend: alle `rx` bleiben im
Test-Hauptscope (sonst siebt `retain(|t| t.send(...).is_ok())` die Sender aus), und
Assertions laufen erst **nach** `join_all` und nur über reihenfolge-unabhängige
Invarianten.

**Neues Szenario `run_mux_isolation`** (`LoadParams` um `streams_per_carrier`
erweitern, `:9441`):

1. `tablets.len() == F` bei **einem** Träger.
2. `is_holder` liefert für Feld X **nur** für dessen Substrom `true`.
   *Dieser Test fällt gegen den heutigen Stand garantiert rot — das ist sein Wert.*
3. Detach **eines** Substroms lässt die übrigen Einträge stehen.
4. Träger-Abriss räumt genau seine N Einträge, danach ist der Namespace leer.
5. `device_id` kommt je Substrom unverfälscht an.

**Weitere Rust-Unit-Tests**

- `relay-proto`: Serde-Roundtrips für `CarrierFrame`, `StreamOpen`, `StreamClose`,
  `CarrierHello` (Projektstandard).
- Protokollversion: unbekannte Version → definierte Ablehnung, kein Panic.
- **Marken-Wächter:** Die Marke, die der Slave im `Pong` stempelt, ist die der von
  ihm ausgelieferten Seite.
- Deckel-Grenzfälle nach Muster `run_cap_boundary` (`:9966`).

**Regression:** `mod load` (`light_*`, `:10020-10070`) muss nach dem Refactoring in
Etappe 1 **unverändert** grün bleiben — das ist die Absicherung, dass die Trennung
von Sitzungs- und Transportschicht verhaltensneutral war.

**Gates:** `cargo test` grün, `cargo clippy --workspace --all-targets -- -D warnings`
grün, `npm run build` fehlerfrei.

**Manueller Turnier-Testfall (nicht automatisierbar):** zwei Hallen, Master mit
`LanAndCloud`, ferne Halle mit `mux.enabled`. Punkt zählen, Ergebnis absenden, ein
Tablet-WLAN gezielt abschalten, den Slave-PC gezielt neu starten.

## Risiken & Rollback

| Risiko | Gegenmaßnahme |
|---|---|
| **`is_holder` autorisiert fremde Felder**, falls Substrome sich doch einen Kanal teilen → ein Ergebnis liefe in ein fremdes Feld, **ohne dass ein heutiger Test rot wird** | Weg A (eigener Kanal je Substrom) + Testinvariante 2 als Wächter |
| **Tote Substrom-Slots**: ein pingender Träger hält Slots endlos, weil der Relay nur den Träger misst | Slot-Freigabe wandert in den Slave, der die echte Geräte-WS hält und den Abriss sofort sieht — schneller als die heutigen 15 s |
| **Reload-Schleife im Turnier** durch abweichende Seiten-Marken | Der Slave liefert **und** stempelt; Marken-Wächter-Test |
| **Slave wird Single Point of Failure der Halle** — heute liegt er nicht im Datenpfad | Bewusstes Restrisiko. Notnagel: Direkt-Cloud-Adressen bleiben gültig, mit dem Hinweis, dass ein unbestätigtes Ergebnis neu zu erfassen ist |
| **Neuer Slave gegen alten Relay** macht die Halle blind | Protokollaushandlung beim Verbinden; bei Ablehnung Rückfall auf die heutigen Weiterleitungen |
| **Head-of-Line** bei mehreren gleichzeitigen 64-KB-`StateSync` | Klein gehalten durch den WS/HTTP-Schnitt; `RESULT_TIMEOUT` beträgt 8 s. Restrisiko, ungemessen |
| **Origin-Wechsel** verliert `pendingResult` | Umstellung nur zwischen Turnieren (Akzeptanzkriterium) |

**Rollback:** dreistufig. `mux.enabled = false` schaltet zur Laufzeit auf das heutige
Verhalten zurück. Die Protokollaushandlung fängt einen Relay-Rollback ab. Eine ältere
App-Version ignoriert das Config-Feld dank `serde(default)`.

**Deploy-Reihenfolge:** Etappen 1–3 (Relay) sind rückwärtskompatibel und für sich
wirkungslos — sie fügen eine Route hinzu, die niemand ruft. Sie müssen **vor** der
Slave-Seite gemergt sein; der Relay deployt automatisch bei jedem main-Merge.

## Offene Fragen / Annahmen

**Annahmen**

- Der Fan-in-Task erzeugt keine nennenswerte Latenz gegenüber der heutigen
  Direktverbindung. **Ungemessen** — beim Feldtest gegen die bekannte Zahl
  p50 15 ms Punkt→Anzeige zu prüfen.
- Die Zahl gleichzeitiger Cloud-Verbindungen ist als Problem **nicht belegt**; der
  Betreiber hat den Mux nach Vorlage dieser Tatsache bewusst gewählt (25.08.2026).
  Der greifbare Gewinn ist die lokale Adressierung, nicht die Verbindungszahl.
- Getrennte Broadcast-Domains je Halle bleiben Voraussetzung, sonst kollidieren zwei
  `bts-light.local`.

**Offene Fragen (blockieren die Umsetzung nicht)**

- Soll der Träger bei dauerhaftem Relay-Ausfall selbsttätig auf Weiterleitungen
  zurückfallen, oder bleibt das eine Bedienhandlung? *Empfehlung: nur bei
  abgelehnter Protokollversion automatisch, sonst Bedienhandlung — sonst pendelt
  die Halle bei flatterndem Uplink zwischen zwei Origins und verliert `pendingResult`.*
- Wird `MAX_STREAMS_PER_CARRIER` an `MAX_TABLETS_PER_NS` (64) angelehnt oder
  niedriger gesetzt?

## Betroffene Doku-Dateien

- `docs/multi-hall.md` — übergreifende Erzählung; der Abschnitt „Tablets & TVs in der
  fernen Halle — Direkt-Cloud (Weg A)" bekommt die Mux-Variante daneben.
- `docs/cloud-relay.md` — Wire-Ebene: Träger-Rolle, `CarrierFrame`, Deckel.
- `docs/tablet.md` — Tablet-Anbindung in der fernen Halle.
- `docs/court-monitor.md` — Monitor-Anbindung über den Slave.
- `docs/changelog.md`, `docs/roadmap.md`.
- `docs/adr/0002-…md` — auf „Transport-Hälfte umgesetzt durch ADR 0048"
  fortschreiben.
- **Neue Zeile in `CLAUDE.md`** für den Mux-Pfad.

## Umsetzungs-Hinweise

*Erst nach Freigabe relevant.* Vollständiger Plan:
`docs/features/_intake/ferne-halle-transport-buendelung/3-how-to.md`. Sieben Etappen:

1. **Sitzungslogik von der Transportschicht trennen** (Relay) — reines Refactoring,
   `mod load` ist die Absicherung.
2. **Substrom-Rahmen** in `relay-proto` mit Serde-Roundtrip-Tests.
3. **Träger-Rolle im Relay** — Route, Ping/Stale, Deckel, Fan-in; `host`-Slot
   unangetastet.
4. **Slave als lokaler Terminator** — Seiten mit eigener Marke und eigenem
   `__BASE__`, WS-Endpunkte als Substrome, HTTP selbst beantwortet, Ping/Stale je
   Gerät; Weiterleitungen bleiben als Rückfall.
5. **Config + Verdrahtung** — `SlaveMuxConfig`, Default `false`.
6. **Tests** auf dem ADR-0019-Harness.
7. **Doku, ADR 0048, Reviews.**

**Reviews:** `security-reviewer` (der Slave trägt fremde Geräteverbindungen, neue
Relay-Rolle), `code-reviewer` (Pflicht nach jeder Änderung).

**Version** gemeinsam bumpen in `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`
und `package.json` — Nummer **erst beim Merge** festlegen.
