# Mehr-Hallen-Turniere

bts-light unterstützt Turniere, die in **zwei oder mehr Hallen** parallel
gespielt werden — Treiber war der Köpi-Cup (BVBB) mit Halle 1 (4 Felder)
und Halle 2 (7 Felder). Drei eng verzahnte Bausteine machen das möglich:

1. **Stabile Feld-Identität (CourtID)** — Feldnamen wiederholen sich
   über Hallen hinweg („Halle 1 · Feld 1" und „Halle 2 · Feld 1"), die
   BTP-`CourtID` nicht.
2. **Hallen sichtbar machen** — Court-Monitor, Tablet-Übersicht,
   Liveticker-Hallen-Monitor zeigen je Halle einen Abschnitt mit
   Überschrift; QR-Code-Liste und Geräte-Zuweisung gruppieren nach Halle.
3. **LAN und Cloud gleichzeitig** — die Haupthalle bindet ihre
   Tablets/Monitore lokal per LAN an (schnell, offline), die zweite Halle
   übers Cloud-Relay (Internet), **für dieselbe Turnier-Instanz**.

Diese Datei ist der zentrale Einstieg; die Details liegen in den
detaillierten Feature-Docs ([tablet.md](tablet.md),
[court-monitor.md](court-monitor.md), [cloud-relay.md](cloud-relay.md)).

Eingeführt in v0.9.4–v0.9.13 (sieben Schritte, siehe
[roadmap.md](roadmap.md)).

## Feld-Identität: CourtID statt Feldname

**Problem.** Bis v0.9.5 hat bts-light Felder über ihren Namen identifiziert.
In einem Zwei-Hallen-Turnier mit zweimal „Feld 1" kollabierten dadurch
elf physische Felder im Tablet-Server auf sieben — Tablet-Zuweisungen,
Spielstände und Geräte-Zuordnung wurden zwischen den Hallen vertauscht.

**Lösung (v0.9.6).** Die Identität jedes Felds ist jetzt die stabile
BTP-`CourtID` (`i64`). Der Anzeigename („Feld 1") bleibt der **Label** —
nur für die UI, nie für Routing. Der Refactor war breit:

- `BtpCourt { id: i64, name: String, location_id: Option<i64>, sort_order: i64 }`
  und `BtpSnapshot.court_infos: Vec<BtpCourt>` ([`btp/model.rs`](../src-tauri/src/btp/model.rs)).
- `BtpMatch.court_id: Option<i64>` parallel zu `court: Option<String>`
  (Letzteres bleibt für Anzeige/Liveticker).
- `TabletState`: alle Maps `HashMap<i64, …>` statt `HashMap<String, …>`
  (`courts`, `active`, `court_state`).
- Wire-Protokoll: `TabletMsg::Identify` trägt `courtId: i64` und
  `courtLabel: String` ([`relay-proto`](../relay-proto/src/lib.rs));
  Relay-Routing per CourtID.
- Tablet-Server-Routen: `/court/<court_id>` und `/court/<court_id>/sock`.

**Migrationshinweis für bestehende Installationen.** Geräte-Zuweisungen
(Court-Monitor-Pis ↔ Feld) hängen seit dem Refactor an der CourtID — alte
Zuweisungen, die noch am Feldnamen klebten, müssen einmalig neu gemacht
werden. Mit v0.9.13 ist der Refactor stabil; ein etwaiger Namens-Fallback
ist als Restposten in der Roadmap markiert.

## Hallen sichtbar im UI

BTP liefert Hallen als `Locations` (`Location{ID, Name}`) und je Court
eine `LocationID`. Hat das Turnier **≥ 2 Hallen**, gilt:

- **Court-Monitor (`monitor.html`):** Header zeigt „Halle 2 · Feld 6"
  statt nur „Feld 6". Details: [court-monitor.md](court-monitor.md).
- **Tablet-Übersicht (`TabletPanel.tsx`):** Felder, QR-Code-Liste und
  Geräte-Zuweisung sind nach Halle gruppiert. Details:
  [tablet.md](tablet.md).
- **Liveticker-Hallen-Monitor (`/live?display=monitor` auf badhub.de):**
  Das Court-Grid ist nach Halle gruppiert — je Halle ein Abschnitt mit
  Überschrift. `&halle=<Name>` filtert auf eine Halle (fester TV je
  Halle). Quelle: bts-light pusht `event.courts[].hall` ab v0.9.8.
- **Vorbereitungs-Monitor (`/live?display=next&halle=<Name>`):** Ein
  Meeting-Point-TV je Halle zeigt nur die Vorbereitungs-Spiele *seiner*
  Halle. Quelle: bts-light setzt `upcoming_matches[].hall` je gerufenem
  Spiel. Details: [preparation.md](preparation.md).

Bei **Ein-Hallen-Turnieren** ist die Halle leer und nichts gruppiert —
alle Ansichten bleiben byte-für-byte wie vorher.

`BtpSnapshot::is_multi_hall()` (`locations.len() >= 2`) ist der
zentrale Test im UI-Code.

## LAN und Cloud gleichzeitig

**Problem.** Bei einem Zwei-Hallen-Turnier ist die Haupthalle (mit dem
bts-light-Laptop + BTP) lokal vernetzt — LAN ist dort schnell und offline-
fähig. Die **zweite Halle** hat ihren eigenen Internet-Zugang, aber kein
LAN zum Turnier-PC.

**Lösung (v0.9.13).** Die Verbindungsart ist nicht mehr ein Entweder-oder:

- `ConnectionMode` (`src-tauri/src/config.rs`) hat drei Varianten —
  `Lan`, `Cloud`, `LanAndCloud` (`#[serde(rename = "lan+cloud")]` nur auf
  der dritten; alte `config.json` lädt unverändert).
- Helper `lan_enabled()` (`Lan | LanAndCloud`) und `cloud_enabled()`
  (`Cloud | LanAndCloud`) als Rückgrat aller Call-Sites — kein
  `_ =>`-Catch-all in `match`-Blöcken, damit der Compiler die
  Vollständigkeit erzwingt.
- `start_sync` (`commands.rs`) spawnt LAN-Server + mDNS und Relay-Client
  in zwei **unabhängigen** `if`-Blöcken. `TabletState` ist als geteilter
  `Arc` ohnehin schon parallelfähig — der Aufwand lag im Config-Modell,
  der Wire-Form und der UI.
- `monitor_devices` vereint im Doppelbetrieb die LAN- und Cloud-Geräte-
  Liste über `merge_device_lists` (`relay-proto`): Dedup je `id`,
  Online-Flag wird ge-ODER-t (online sobald **eine** Quelle es meldet),
  stabile Sortierung.
- Frontend: SetupWizard zeigt LAN und Cloud als **zwei einzeln schaltbare
  Kacheln**; TabletPanel zeigt je Feld zwei QR-Codes (LAN und Cloud);
  CourtMonitorPanel zeigt beide Monitor-Adressen.

**Ein-Modus-Turniere** (nur LAN oder nur Cloud) verhalten sich exakt wie
vorher — Default bleibt `Lan`. Details im Cloud-Pfad:
[cloud-relay.md](cloud-relay.md), Details im LAN-Pfad: [tablet.md](tablet.md).

## Ansagen je Halle (Phase 1, v0.9.128)

Bis v0.9.127 liefen Feld-Ansagen **global** auf dem Haupt-PC (jede neue
Feldbelegung, egal welche Halle). In einem 2-Hallen-Setup soll aber jede Halle
**nur ihre eigenen** Ansagen hören.

- **Einstellung** `AnnounceConfig.announce_hall` (BTP-Location-Name; leer = alle
  Hallen). UI im SetupWizard-Abschnitt „Sprachansagen" (Auswahl erscheint ab
  ≥2 erkannten Hallen). `config.rs`, `src/types.ts`, `SetupWizard.tsx`.
- **Filter:** `MatchAnnouncer.tsx` sagt eine neue Feldbelegung nur an, wenn
  `court.location == announce_hall` (sonst alle). Einzelhallen-Turniere bleiben
  unverändert (Feld leer).
- **Infobox:** sobald `is_multi_hall()` greift, zeigt die Status-Seite
  (`Dashboard.tsx`) einen Hinweis mit den erkannten Hallen + Sprung zur
  Einstellung.

Damit löst sich **Turnier B** (zwei Hallen, je eigene Turnierleitung): zwei
eigenständige bts-light-Master, jeder auf seine Halle gescoped — sofern beide an
dieselben Turnierdaten kommen (BTP-Zugang). **Turnier A** (eine Turnierleitung,
Ansage-Gerät in der zweiten Halle) folgt in Phase 2/3 (Ansage-Slave als
Info-Display über LAN bzw. Cloud-Relay). Konzept: siehe Master-Slave-Plan.

## Disziplinen je Halle — Vergabe-Constraint (Phase 1b, v0.9.129)

Welche Disziplin/Klasse in welcher Halle gespielt wird, ist einstellbar und
**beschränkt die Feldvergabe** (manuell **und** automatisch).

- **Config:** `AppConfig.discipline_hall_rules: Vec<DisciplineHallRule>`
  (`{ discipline, draw_name, hall }`). `draw_name` leer = **Kategorie-Default**
  (gilt für alle Auslosungen der `discipline`, snake_case `Discipline::as_str()`);
  `draw_name` gesetzt = **Override** für genau diese Auslosung (z. B. „HE A"),
  schlägt den Default. `hall` = BTP-`Location`-Name. (`config.rs`)
- **Auflösung:** `AppConfig::allowed_hall_for(discipline, draw_name)` (Override →
  Default → `None` = keine Einschränkung) und `hall_allows_match(…, court_hall)`.
- **Erzwingung:** Auto-Vergabe filtert in `sync.rs` (Court-Pick-Closure) auf die
  erlaubte Halle; die manuelle Vergabe (`commands::assign_court`) liefert bei
  Verstoß einen **Hard-Block** (`Err`, Hinweistext). Beide nutzen dieselbe Regel.
- **UI:** SetupWizard-Abschnitt „Disziplinen je Halle" (Tabelle, Quelle der
  Auslosungen = `tournament_draws`-Command). In der Spielübersicht
  (`FieldOverviewPage`) werden nicht erlaubte Felder fürs gewählte Spiel
  ausgegraut; ein Drop dorthin wird abgewiesen (Backend erzwingt es zusätzlich).

Damit deckt **Turnier B** auch den Fall ab, dass Kategorie X fest in Halle A und
andere in Halle B laufen (Felder gehen nur in ihre Halle).

## Ansage-Slave-Modus (Phase 2, v0.9.130)

Statt Audio über die Leitung zu schicken, läuft in der zweiten Halle ein
**eigener bts-light-Rechner als Ansage-Slave**: er liest dieselbe BTP-Datei
(über das gemeinsame Netz) und sagt **seine** Halle selbst an — mit eigener
Azure-/Web-Speech-Stimme, ohne Audio-Übertragung.

- **Config:** `AppConfig.slave_mode: bool` (`config.rs`). Aktiv →
  `start_sync` (`commands.rs`) startet **keinen** Tablet-Server/mDNS/Relay; die
  Sync-Engine (`sync.rs::run_once`) **liest** BTP + `set_snapshot` (damit der
  `MatchAnnouncer` ansagt), **überspringt** aber Auto-Feldvergabe und
  Liveticker-Push (`SyncOutcome::SlaveActive`). Schreibt nie nach BTP.
- **Halle:** über die bestehende „Ansagen nur für Halle X"-Einstellung
  (`announce.announce_hall`, Phase 1) — der Slave sagt nur seine Halle an.
- **UI:** Schalter „Ansage-Slave-Modus" oben im SetupWizard.
- **Architektur:** genau **ein Master** (mit BTP-Steuerung: Vergabe + Push);
  beliebig viele Slaves (read-only). Voraussetzung: der Slave erreicht den
  BTP-Rechner im selben Netz (LAN/WLAN).

## Cloud-Ansage-Slave (B1a, v0.9.142)

Sind die Hallen **nicht im selben Netz** (km entfernt, getrennte LTE-Router),
erreicht der LAN-Slave den BTP-Rechner nicht. Dafür der **Cloud-Ansage-Slave**:
- **Master** (cloud-aktiv) pusht zusätzlich pro Feld die **Halle**
  (`HostFrame::MatchAssigned.hall`) und neue **Freitexte**
  (`HostFrame::Freetext`) an den Relay.
- **Relay** speichert je Namespace `court_hall` + `freetext` und liefert
  `GET /{ns}/info/announce/state?hall=&since=` (hallengefilterte Matches +
  neue Freitexte) — `relay/src/main.rs`, `relay-proto` (`AnnounceState`).
- **Slave**: `slave_mode` + **`master_namespace`** (Kopplungs-Code des Masters).
  Statt BTP zu lesen, pollt `CloudAnnounceSlave` (`cloud_announce_state`,
  `src-tauri/src/commands.rs` → `relay_client::fetch_announce_state`) und sagt
  Matches seiner Halle + Freitext lokal an (Stimme/Azure lokal).
- **Pairing-UI**: SetupWizard zeigt den eigenen Kopplungs-Code (`install_id`)
  und nimmt im Slave-Modus den Master-Code entgegen.
- **Einrichtung (Assistent, v0.9.143):** in den Einstellungen führt ein Schritt-
  für-Schritt-Block durch die Kopplung. Der Slave-Schalter ist **immer** sichtbar
  (eine ferne Halle hat kein BTP → kann Mehr-Hallen nicht erkennen). Master zeigt
  seinen Code (Kopieren), Slave trägt ihn ein + wählt seine Halle.
- **Slave-Online-Anzeige (v0.9.143):** der Slave meldet beim Relay-Poll seine
  Präsenz (`?slave=<install_id>`), der Relay hält `slaves` (id → Halle + last-seen)
  und liefert `GET /{ns}/slaves`; der Master pollt `cloud_slaves` und zeigt in der
  Kopfzeile, ob die ferne Halle verbunden ist (online < 12 s). Rein informativ.
- **Rollout:** Relay muss **vor** dem Client deployt sein (neuer `HostFrame` +
  `/slaves`-Route).

## Tablets & TVs in der fernen Halle — Direkt-Cloud (Weg A, v0.9.144)

Use-Case: **beide** Hallen haben Tablets **und** TVs, aber Turnierleitung und
Feldvergabe sitzen **nur** in Halle A. Die ferne Halle **steuert nichts**.

Der Ergebnis-Datenpfad dafür existiert bereits vollständig und kennt **keine
Hallentrennung**: Der Master pusht **alle** Felder (auch die der fernen Halle)
an den Relay (`push_all_courts`, kein Hallenfilter), ein Tablet an **jedem**
CourtID bekommt sein `MatchAssigned` und liefert sein Ergebnis über
`/{ns}/result` → Master → `process_result` → `SENDUPDATE` ins **Master-BTP**
(gegen das Court-Match validiert, R5). Deshalb hängen die Tablets/TVs der
fernen Halle **direkt** am Cloud-Relay des Masters — der Slave-PC bleibt
read-only (nur Ansage). Der schwere bidirektionale Rückkanal (Slave→Master,
alter „B2") entfällt damit für diesen Use-Case. Begründung + verworfene
Alternative: [ADR 0002](adr/0002-ferne-halle-direkt-cloud-geraete.md).

Neu gebaut wurde nur das **Onboarding** (die Crew in Halle B braucht die
Adressen ihrer Felder ohne Blick auf den Master):

- `CourtBrief.hall` (serde-default) — der Master füllt es beim `Courts`-Push
  (`relay_client.rs::push_courts`), der Relay liefert es unter `/{ns}/courts`
  mit aus (`relay/src/main.rs::courts_list`).
- Command `slave_devices` (`commands.rs`) → holt die Feldliste des
  Master-Namespace (`relay_client::fetch_courts`), liefert `relay_base`, die
  Hallen-Optionen (`relay_proto::distinct_halls`) und die auf die eigene Halle
  (`announce.announce_hall`) gefilterten `Vec<CourtBrief>`.
- Slave-Dashboard-Panel `SlaveDevicesPanel.tsx` — **zuerst** die Hallen-Auswahl
  (siehe unten), dann je Feld ein Tablet-QR (`<relay_base>/qr/<id>`) und der
  Monitor-Link (`<relay_base>/court/<id>/display`).

### Hallen-Auswahl auf dem Cloud-Slave — warum eigens

`announce_hall` ist die eine Einstellung, die **sowohl** die Ansage (welche
Halle spricht der Slave an) **als auch** den Geräte-Filter steuert. Der Match
im Relay ist ein **Byte-genauer** Vergleich `court_hall == announce_hall`.
Problem: alle Hallen-Dropdowns der App speisen sich aus dem **lokalen BTP**
(`tabletOverview`/`tournamentStats`) — der Cloud-Slave hat **kein** BTP, also
wäre die Liste leer und die Auswahl unsichtbar. Folgen ohne Fix: leeres
`announce_hall` = **alle** Hallen (der Slave sagt Halle A mit an), oder ein
manuell getippter, nicht exakt passender Name = **null** Ansagen/Felder.

Lösung: der Slave zieht die Hallennamen aus der **Relay-Feldliste**
(`slave_devices.all_halls`) und lässt sie im Panel wählen — dieselbe
`announce_hall`. Ohne gewählte Halle zeigt das Panel keine Felder, sondern
fordert die Auswahl ein. Der **Master** wiederum warnt auf dem Dashboard, wenn
bei ≥2 Hallen keine Ansage-Halle gewählt ist (sonst sagt er beide an).

**Voraussetzungen (betrieblich, keine Bugs):**

- Master läuft mit `Cloud`/`LanAndCloud` und ist durchgängig verbunden (Single
  Point of Failure: fällt der Master oder sein LTE aus, bekommt Halle B keine
  neuen Ansagen/Zuweisungen und Ergebnisse landen nicht im BTP).
- **Ansage-Halle gesetzt** — am Master (= Halle A) und am Slave (= Halle B).
- **Feldvergabe bei zwei gleichzeitig bespielten Hallen:** die „aktive Halle"
  (Tages-Halle) bleibt **leer** → die Auto-Vergabe belegt dann **nur** Matches,
  die pro Halle **„in Vorbereitung" gerufen** wurden (`sync.rs`, `require_call`).
  Die Disziplin-je-Halle-Regeln (Phase 1b) sind dabei ein **Constraint** (ein
  gerufenes Match landet nur in der erlaubten Halle), **kein** selbsttätiger
  Verteiler. Ablauf: TL ruft die nächsten Spiele je Halle in Vorbereitung, die
  Regel sortiert sie in die richtige Halle.

**Grenzen (bekannt):**

- **Kein lokaler Puffer** — jede Ergebnis-Übermittlung läuft synchron über die
  Cloud (20-s-Timeout); bei Aussetzern erneut senden.
- **Cloud-Tablet-PIN = `0000`** (der Relay kennt den Operator-PIN nicht,
  `relay/src/main.rs` `__TABLET_PIN__` leer → `tablet.html`-Fallback), und das
  Cloud-Feldwechsel-Menü listet **alle** Hallen. Ein Helfer in Halle B könnte
  ein Tablet auf ein Halle-A-Feld umstellen — abgemildert durch das
  Hallen-Präfix im Feld-Label („Halle 2 · 6").

- **Noch offen (B1b/B2):** Cloud-**Info-/Kombi-Monitore** und die
  **Slave-seitige TV-/Geräte-Zuweisung** der fernen Halle (Pi-Kiosk aus Halle B
  zuweisen/identify/reload — heute master-only; braucht ein hallen-begrenztes
  `MonitorControl`-Merge im Relay). Siehe `docs/features/multihall-cloud-plan.md`.

## Offene Punkte

- **Verbindungsweg je Gerät anzeigen.** Im Parallelbetrieb pro Gerät
  (Tablet, Court-Monitor) als Badge sichtbar machen, ob es bts-light über
  LAN oder Cloud erreicht — Wunsch der Turnierleitung, noch nicht
  eingeplant (siehe [roadmap.md](roadmap.md)).
- **Namens-Fallback (Schritt 7 des CourtID-Refactors).** Übergangs-Code,
  der noch über Feldnamen fallback-routet, sollte entfernt werden.

## Beteiligte Dateien (Querverweis)

| Thema | Datei(en) | Doku |
|---|---|---|
| CourtID-Modell | `src-tauri/src/btp/model.rs` (`BtpCourt`, `BtpSnapshot`) | [btp_protocol.md](btp_protocol.md) |
| Tablet-Server-Routing | `src-tauri/src/tablet/server.rs` (`/court/<court_id>`) | [tablet.md](tablet.md) |
| Wire-Routing per CourtID | `relay-proto/src/lib.rs` (`TabletMsg::Identify`) | [cloud-relay.md](cloud-relay.md) |
| Hallen-Gruppierung Tablet | `src/pages/TabletPanel.tsx` (`groupByHall`) | [tablet.md](tablet.md) |
| Hallen-Gruppierung Monitor | `src-tauri/src/tablet/monitor.rs`, `assets/monitor.html` | [court-monitor.md](court-monitor.md) |
| Liveticker-Hallen-Monitor | badhub: `public/assets/js/live.js` (`groupCourtsByHall`, `?halle=`) | badhub: `docs/features/liveticker_bts.md` |
| `LanAndCloud`-Modus | `src-tauri/src/config.rs`, `commands.rs` (`start_sync`, `monitor_devices`) | [cloud-relay.md](cloud-relay.md), [tablet.md](tablet.md) |
| Geräte-Liste vereinen | `relay-proto/src/lib.rs` (`merge_device_lists`) | [cloud-relay.md](cloud-relay.md) |
| Vorbereitungs-Aufrufe je Halle | `src-tauri/src/tablet/state.rs`, `src/pages/PreparationPanel.tsx` | [preparation.md](preparation.md) |
