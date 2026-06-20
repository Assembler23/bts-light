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

## Offene Punkte

- **Ansage-Slave (Phase 2/3).** Ein reines Ansage-Gerät (Browser-Seite, nur Ton)
  je Halle, das die Ansagen seiner Halle abspielt — über LAN (lokaler Server)
  bzw. Cloud-Relay. Cloud-Info-Displays sind heute LAN-only (Relay trägt nur
  Court-Monitore), das ist der größere Teil von Phase 3.

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
