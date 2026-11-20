# Turnierquelle: BTP oder badhub-Turnierkern

> **Status (20.11.2026):** Paket L1 umgesetzt (v0.9.448): BTP liegt hinter der Turnierquelle, ohne Verhaltensänderung.
> Paket L2 umgesetzt (v0.9.449): Quelle `badhub` lesend — Kopplung, Turnierpaket, Kern-Faltung, Wizard-Schritt.
> Ziel und Gesamtplan: badhub `docs/features/bts_light_steuerung.md`, Entscheidung badhub ADR-0084 (bts-light hält am
> Turniertag die Lease eines badhub-Turniers).

## Ziel

bts-light soll Turniere, die in badhub organisiert werden (eigener Turnierkern in Rust, Repo `Assembler23/turnierkern`),
genauso steuern wie BTP-Turniere, auch ganz ohne Internet. Dafür wird BTP eine von zwei **Turnierquellen**.

## Harte Bedingung: BTP-Turniere laufen unverändert

Nach jedem Paket verhält sich ein BTP-Turnier genauso wie vorher: gleiche Requests an BTP, gleiche Zeiten, gleiche
Oberflächen.

1. `config.json` bekommt `quelle` mit Vorgabe `btp`. Eine Konfiguration ohne das Feld (jede vor v0.9.448) gilt als BTP.
   Ein unbekannter Wert wird ebenfalls als BTP gelesen, damit ein Downgrade die Konfiguration nicht unlesbar macht.
2. Der BTP-Code wird nur verschoben: keine Logik- und keine Timing-Änderung (5-s-Takt, Nachschub-Queue, Schreib-Gate
   ADR 0060, `btp::eigene_writes`).
3. Kern, Outbox und Lease starten nur bei `quelle = badhub`, an einer Stelle beim Start, nicht verstreut über die
   Konsumenten.
4. Stores und `relay-proto` bleiben abwärtskompatibel. Kern-Turniere legen ihre Dateien getrennt ab.
5. Die bestehende Testsuite bleibt unverändert grün. Vor dem ersten echten Turnier mit L1 gibt es einen BTP-Probelauf mit
   Tablets, Monitoren und Auto-Feldvergabe.

## Paket L1 (umgesetzt)

| Was | Wo |
|---|---|
| Quelle in der Konfiguration | `config.rs` `QuelleArt` (nur `Btp`, `#[serde(other)]`), `AppConfig.quelle` (`#[serde(default)]`) |
| Lesen des Turnierstands | `quelle::stand(config)`; bei BTP genau der bisherige `client::fetch_snapshot`-Abruf samt Fehlertext; Aufrufer `SyncEngine::run_once` (`sync.rs`) |
| Schreibwege nach BTP | `quelle/btp.rs`: `write_result_to_btp`, `write_result_settled`, `write_courts_to_btp`, `write_officials_to_btp`, `write_highlight_to_btp` samt Hilfen, wortgleich aus `tablet/server.rs` verschoben; `tablet::server` exportiert sie unter den alten Namen weiter |
| Datenformat | `release/datenformat.json`: neuer Abdruck, Grenze bleibt (ältere Versionen übergehen `quelle`) |

Nicht in L1: `test_btp` im Einrichtungsassistenten prüft weiter gezielt eine BTP-Verbindung mit den eingegebenen Daten,
nicht die eingestellte Quelle.

## Paket L2 (umgesetzt, nur lesend)

### Bausteine

| Was | Wo |
|---|---|
| Quelle in der Konfiguration | `QuelleArt::Badhub` (`"badhub"`; `Btp` bleibt `#[default]` + `#[serde(other)]` und steht deshalb zuletzt), `AppConfig.badhub_turnier` (`BadhubTurnierConfig`: UUID, ID, Name, API-Basis, Gerätename; nur der Kern setzt ihn, `keep_host_managed_fields`) |
| Der eine Abzweig beim Start | `quelle::starten(config, daten)` → `QuellenStart { kern_abgleich, btp_dienste }` in `start_sync_kern`. Bei BTP startet nichts Neues; bei badhub wird das Turnier von der Platte geladen und `quelle::badhub::abgleich_schleife` gestartet, TP-Takt und TP-Datei-Suche nicht |
| Lesen | `quelle::stand` → `quelle::badhub::stand` → `KernQuelle::stand` = `turnierkern::falten_turnier` + `abbildung::abbilden` (zwischengespeichert, bis neue Ereignisse oder Stammdaten kommen) |
| Kopplung | Tauri-Command `badhub_koppeln(code, geraet_name)` → `POST koppeln {code, install_id, name}` → `GET turnier/paket` → Ablage → `config.badhub_turnier`. `badhub_turnier_info` für den Assistenten |
| Abgleich | alle 5 s `GET turnier/ereignisse?ab=<n>&max=500`, alle 60 s `GET turnier/paket` mit `If-None-Match` (ETag-Header, sonst `stammdaten_etag`). Offline bleibt der letzte Stand; 401/403 hält den Abgleich bis zum nächsten Start an |
| API-Basis | aus der Push-Adresse (`badhub.url`, folgt damit dem Testsystem-Schalter) + `/api/torg/v1`, beim Koppeln in `badhub_turnier.basis` festgehalten — das Token geht nie an das andere System. Nur `https://` (Ausnahme `localhost`/`127.0.0.1` für Tests) |
| Kern | Crate `turnierkern` (privates Repo `Assembler23/turnierkern`, Tag `v0.12.0`), `KERN_VERSION` gleich dem Tag (Test). Weicht `paket.kern_version` ab, zeigt der Assistent einen Hinweis; Lesen geht trotzdem (das Ereignisschema wächst nur additiv), Schreiben (L3) verlangt Gleichstand |

### Ablage und Verschlüsselung

```text
<App-Daten>/turniere/schluessel.bin      AES-256-Schlüssel, unter Windows per DPAPI an das Benutzerkonto gebunden
<App-Daten>/turniere/<uuid>/paket.json   Paket ohne Personen + ETag
<App-Daten>/turniere/<uuid>/personen.bin meldungen + schiedsrichter, AES-256-GCM
<App-Daten>/turniere/<uuid>/geraet.bin   Gerätetoken, AES-256-GCM
```

- bts-light hatte bisher keinen Schlüsselspeicher (Passwörter liegen im Klartext in `config.json`). Für den
  Personenauszug verlangt die Spec den Schlüsselspeicher des Betriebssystems: **DPAPI** (`CryptProtectData`, ohne
  Oberfläche) über das ohnehin eingebundene `windows`-Crate, Verschlüsselung mit `ring` (ohnehin im Baum). Eine
  kopierte Datei ist auf einem anderen PC oder Konto unlesbar. Unter Linux (nur Entwicklung/CI) liegt der Schlüssel roh
  mit 0600, gekennzeichnet als `ROH1`.
- Das **Token** liegt bewusst nicht in `config.json`: Es reist so nicht im Umzugs-Bündel mit (`identity_bundle` leert
  zusätzlich `badhub_turnier`) und geht nicht verloren, wenn eine ältere Version die Konfiguration zurückschreibt.
- Die UUID kommt aus dem Netz und wird vor jedem Pfadbau geprüft (nur Hex und `-`, 8–64 Zeichen).
- Keine Namen in Logs: Meldungen nennen Turnier-ID, Zahlen und Statuscodes; Formatfehler nur Zeile/Spalte.
- **Offen (L3):** Löschen des Personenauszugs nach Lease-Rückgabe und leerer Outbox bzw. spätestens 14 Tage nach
  Turnierende (Spec „Personendaten auf dem Hallen-PC“). In L2 bleibt er liegen, bis neu gekoppelt wird.

### Abbildung Kern-Stand → `BtpSnapshot` (`quelle/badhub/abbildung.rs`)

| BTP-Feld | Quelle | Anmerkung |
|---|---|---|
| `id` | `konkurrenz.nr * 100000 + spiel` | stabil, alle Stores hängen daran |
| `draw_id`, `planning_id` | Konkurrenz-Nr., Spielnummer | ein „Draw“ je Konkurrenz (Gruppen und KO zusammen) |
| `from1`/`from2` | Gerüst: `Sieger`/`Verlierer {spiel}` → Spielnummer; Losplatz und Gruppenrang → `None` | dieselbe Kante wie in BTP: `vorspiel_fuer`/`correction_blocker` finden das Vorspiel über `planning_id` im selben Draw |
| `entry1_id`/`entry2_id`, `team1`/`team2` | Meldungsnummer (turnierweit eindeutig), Personen aus dem Auszug | 0 / leer = Platz offen |
| `team*_seed` | `setzlisten` + `setzungen` | „1, 2, 3/4, 5/8 …“ bzw. einzeln |
| `status`, `winner`, `sets`, `result` | Kern-Spiel: `beendet` → Finished; `aufgerufen` **mit** auflösbarem Feld → OnCourt, sonst Scheduled; `kampflos` → Walkover, `aufgabe` → Retired | der Kern kennt keine Disqualifikation |
| `court_id`, `court` | Kern-Feld → Index in `rahmen.hallen[].felder` (1-basiert, über alle Hallen) | bei mehreren Hallen Kern-Schreibweise „<Halle-ID> n“ (wie badhub `Spielansicht::felder`), Felder dürfen im Paket mit oder ohne Präfix stehen; ohne Hallen im Paket die Feldzahl aus dem Kern-Rahmen |
| `locations`, `court_infos`, `courts` | `rahmen.hallen` | Halle = Location (1-basiert) |
| `planned_time`, `display_order`, `location_id` | `planung.zeiten` (Beginn, Position in der Liste, Halle) | |
| `round_name` | `runden[konkurrenz][spielnummer]` aus dem Paket | Rückfall: „Gruppe A“, „Finale“, „Halbfinale“, „Viertelfinale“, „Achtelfinale“, „Runde n“, „Spiel um Platz 3“ |
| `draw_name`, `class_label`, `discipline` | Konkurrenz-Schlüssel, Klasse aus Name/Schlüssel, Disziplin-Kürzel | |
| `match_num` | Spielnummer der Konkurrenz | „aus Spiel 3“ gilt innerhalb der Konkurrenz |
| `official1_id`/`official2_id`, `officials` | Kern-Offizielle, Namen aus `schiedsrichter` | |
| `scoring` | Zählweise der Konkurrenz (Vorgabe Badminton) | ohne Deckel `cap_score` 999 |
| `rest_minutes` | `rahmen.mindestpause_min`, sonst `planung.rahmen.pause` | |
| `events`, `entries` | `konkurrenzen`, Personenauszug | |
| `highlight`, `shuttles` | 0 | nur BTP; bei Kern-Turnieren gilt der lokale Aufrufzustand |
| `member_id` | `None` | das Paket trägt keine Lizenznummer |
| `tournament_name` | `turnier.name` | schützt wie bei BTP die lokalen Stores; Umstellung auf die UUID mit L3 |

### Schreiben und BTP-Berührpunkte bei Quelle badhub

| Berührpunkt | Bei badhub |
|---|---|
| `write_result_to_btp`, `write_result_settled`, `write_courts_to_btp`, `write_officials_to_btp`, `write_highlight_to_btp` (Tablet, TL-Web, Desktop, Auto-Vergabe, Farben, Schiedsrichter, Nachschub-Flush) | Weichen in `quelle/mod.rs` statt Re-Export: `Err(SCHREIBEN_FOLGT)`, kein BTP-Kontakt, kein Schreib-Gate, keine `eigene_writes`-Merker |
| Nachschub-Queue (`btp-retry.json`) | `process_result` reiht nur ein, wenn `quelle::nachschub_moeglich`; bei badhub wiederholbare Ablehnung an das Tablet (es behält das Ergebnis). Der Flush erreicht badhub nie, weil kein Eintrag entsteht |
| `quelle::stand` (Sync-Takt) | badhub-Zweig, kein BTP-Abruf |
| LAN-/Ansage-Slave (liest sonst BTP direkt) | Fehlertext „folgt“, kein BTP-Abruf, kein Abgleich (Stand vom Master kommt mit L3) |
| TP-Takt, TP-Datei-Suche (`tp_takt.rs`, `tp_finden`) | starten nicht (`QuellenStart.btp_dienste`) |
| TP-Upload per Knopf (`tp_upload_now`) | `pruefe_upload_bedingungen` lehnt ab |
| Schreib-Gate (`btp::write_gate`) | unberührt — nur BTP-Writes und TP-Upload nutzen es, beide laufen nicht |
| `test_btp` im Assistenten | bleibt; der BTP-Abschnitt ist bei badhub ausgeblendet |
| Ansage-Freitext des LAN-Slaves (`config.btp.host` als Master-Adresse) | unverändert, kein BTP |

Laufen Auto-Vergabe, Schiedsrichter-Rotation oder Farben bei einem badhub-Turnier, scheitert ihr Write mit der
Meldung im Log (Warnung je Takt); die Turnierleitung schaltet sie bis L3 am besten aus.

### Datenformat (ADR 0101)

Neue gespeicherte Variante `quelle = "badhub"`, neuer Block `badhub_turnier`, neue Dateien unter `turniere/`. Die Grenze
`rueckwaerts_lesbar_ab` bleibt: v0.9.448 liest `"badhub"` als BTP, ältere übergehen das Feld, `turniere/` liest keine
ältere Version. Ein Rückweg während eines badhub-Turniers schaltet die Halle auf BTP — bewusst in Kauf genommen, weil L2
nur liest und nichts verloren geht. Mit L3 (lokales Protokoll und Outbox) ist die Grenze neu zu entscheiden.

### Abstimmung mit badhub (B1)

- Gebaut gegen das Spec-Format; getestet gegen eine Attrappe (`quelle/badhub/tests.rs`).
- `GET turnier/ereignisse`: Hülle nicht festgelegt — gelesen wird eine Liste oder `{ "ereignisse": [...] }`.
- `runden`: Schlüssel = Spielnummer (als Text).
- Feldnamen in `rahmen.hallen[].felder`: mit oder ohne Hallenpräfix; maßgeblich ist das Kern-Feld aus `spiel_aufrufen`.
- `POST koppeln`: 400/401/403/404/409/410/422 gelten als „Code ungültig“, 429 als Drossel.
- Liveticker-GUID: bleibt Pflicht im Assistenten (`tournament_uuid`); ob badhub dort die Turnier-UUID annimmt
  (GUID-Brücke ADR-0080), ist mit B1 zu klären.

## Nächste Pakete (Plan, badhub-Spec)

- **L3:** Schreibpfad über neutrale Befehle hinter der Quelle, eigenes Protokoll auf der Platte, Outbox an badhub,
  Relay-Aktion `KernBefehl` für Eingaben aus badhub.
