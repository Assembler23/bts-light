# Turnierquelle: BTP oder badhub-Turnierkern

> **Status (20.11.2026):** Paket L1 umgesetzt (v0.9.448): BTP liegt hinter der Turnierquelle, ohne Verhaltensänderung.
> Paket L2 umgesetzt (v0.9.449): Quelle `badhub` lesend — Kopplung, Turnierpaket, Kern-Faltung, Wizard-Schritt.
> Paket L3 umgesetzt (v0.9.450): Schreibpfad über Kernbefehle, lokales Protokoll, Lease, Outbox, Relay-Aktion
> `KernBefehl` für Eingaben aus badhub, LAN-Slave liest vom Master, Stores an der Turnier-UUID.
> v0.9.451: Rückgabe der Steuerung auch aus dem TL-Web (`TlAction::BadhubZurueckgeben`); Turnierpaket in Teilen
> (badhub kappt auf 2000 Ereignisse, bts-light lädt seitenweise nach).
> v0.9.454: Rückzug während des Turniers (Kern 0.13.0) als Kernbefehl aus badhub und als Aktion im TL-Web; `entfaellt`
> in der Abbildung (Kern 0.14.0 und Zählweise je Spiel kamen mit v0.9.452).
> v0.9.455: Standby-Übernahme ohne Netz (Paket S2): Spiegeln der Posten im LAN, Notfall-Sicherung, Übernehmen, Outbox mit `standby: true`.
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
| Abgleich | alle 5 s `GET turnier/ereignisse?ab=<n>&max=500` (seitenweise bis zum Kopf, siehe „Paket in Teilen“), alle 60 s `GET turnier/paket` mit `If-None-Match` (ETag-Header, sonst `stammdaten_etag`). Offline bleibt der letzte Stand; 401/403 hält den Abgleich bis zum nächsten Start an |
| API-Basis | aus der Push-Adresse (`badhub.url`, folgt damit dem Testsystem-Schalter) + `/api/torg/v1`, beim Koppeln in `badhub_turnier.basis` festgehalten — das Token geht nie an das andere System. Nur `https://` (Ausnahme `localhost`/`127.0.0.1` für Tests) |
| Kern | Crate `turnierkern` (privates Repo `Assembler23/turnierkern`, Tag `v0.17.0`, seit v0.9.458), `KERN_VERSION` gleich dem Tag (Test). Weicht `paket.kern_version` ab, zeigt der Assistent einen Hinweis; Lesen geht, solange das Protokoll keine Ereignisarten enthält, die diese Version nicht kennt (neue Befehle eines neueren Kerns, z. B. `zaehlweise_festlegen` ab 0.14.0, lassen das Falten scheitern — deshalb bts-light zusammen mit dem Kern in badhub aktualisieren), Schreiben (L3) verlangt Gleichstand |

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
- **Löschen (L3):** Der Personenauszug (`personen.bin`) wird nach der Rückgabe der Lease (nur mit leerer Outbox
  möglich) gelöscht, sonst spätestens 14 Tage nach Turnierende (`turnier.ende`, sonst `beginn`; `aufraeumen` beim Start
  der Übertragung). Ordner alter Kopplungen ohne wartende Ereignisse werden dabei ganz gelöscht.

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
| `scoring` | Zählweise **je Spiel** (`spiele[].zaehlweise`, Kern ab 0.14.0: Spiel → Konkurrenz/KO-Phase → Turnier → 3×21); `best_of` = 2 × Gewinnsätze − 1, Pause bei 11 (bis 21) bzw. 8 (bis 15), bei 11er-Sätzen (5×11, Bundesliga) keine | ohne Deckel `cap_score` 999 |
| Status `entfaellt` (Kern 0.13.0) | Rückzug: Gruppe mit `streichen` oder beide Seiten zurückgezogen | Spiel erscheint **nicht** im Modell — nicht aufrufbar, nicht in der Auto-Vergabe; Gruppenspiele sind nie Vorspiel |
| `durch_rueckzug` | kampfloses Ergebnis aus einem Rückzug | Finished + Walkover wie ein gemeldetes kampflos |
| `gestrichen`, `Zeile.zurueckgezogen` | Tabellenwirkung | bts-light rechnet keine Tabellen; der Rückzugs-Stand reist als `badhub_meldungen[].zurueckgezogen` ins TL-Web |
| `rest_minutes` | `rahmen.mindestpause_min`, sonst `planung.rahmen.pause` | |
| `events`, `entries` | `konkurrenzen`, Personenauszug | |
| `highlight`, `shuttles` | 0 | nur BTP; bei Kern-Turnieren gilt der lokale Aufrufzustand |
| `member_id` | `None` | das Paket trägt keine Lizenznummer |
| `tournament_name` | `turnier.name` | nur Anzeige; die lokalen Stores binden seit L3 an `badhub:<uuid>` (`TabletState::turnier_schluessel`) |

### Schreiben und BTP-Berührpunkte bei Quelle badhub

| Berührpunkt | Bei badhub |
|---|---|
| `write_result_to_btp`, `write_result_settled`, `write_courts_to_btp`, `write_officials_to_btp`, `write_highlight_to_btp` (Tablet, TL-Web, Desktop, Auto-Vergabe, Farben, Schiedsrichter, Nachschub-Flush) | Weichen in `quelle/mod.rs`: seit L3 Kernbefehle (siehe unten), kein BTP-Kontakt, kein Schreib-Gate, keine `eigene_writes`-Merker; Zeilenfarbe nur lokal (`Ok`) |
| Nachschub-Queue (`btp-retry.json`) | eingereiht wird nur, wenn `quelle::nachschub_moeglich` (Tablet, TL-Web, Desktop-Ergebnis/Walkover/Disqualifikation); bei badhub geht die Ablehnung des Kerns zurück, das Tablet behält sein Ergebnis |
| `quelle::stand` (Sync-Takt) | badhub-Zweig, kein BTP-Abruf; Fehler als `SyncOutcome::QuelleFehler` („badhub-Turnier: …“, nie „BTP nicht erreichbar“) |
| LAN-/Ansage-Slave (liest sonst BTP direkt) | seit L3 `badhub::slave`: Ereignisse vom Master (`GET :8088/quelle/ereignisse?ab=<n>`, höchstens 500 je Antwort), gefaltet mit dem eigenen Paket; kein BTP, keine Lease. Zugang über den **Slave-Schlüssel** (16 Zufallsbytes, vom Master beim Start erzeugt und im Dashboard gezeigt, am Slave in den Einstellungen eingetragen, beide verschlüsselt in `slave.bin`, Vergleich in konstanter Zeit, Kopf `X-Bts-Slave`) — die Turnier-UUID allein ist öffentlich |
| Auto-Vergabe, Schiedsrichter-Abgleich | schreiben nur, wenn `quelle::schreiben_moeglich` (BTP: immer; badhub: Lease) — keine Warnung je Takt ohne Lease |
| TL-Antwort bei Schreibfehler | `quelle::schreib_ablehnung`: BTP wortgleich „BTP hat abgelehnt: …“ (`btp_error`), badhub die Kern-Ablehnung (`kern_abgelehnt`) |
| TP-Takt, TP-Datei-Suche (`tp_takt.rs`, `tp_finden`) | starten nicht (`QuellenStart.btp_dienste`) |
| TP-Upload per Knopf (`tp_upload_now`) | `pruefe_upload_bedingungen` lehnt ab |
| Schreib-Gate (`btp::write_gate`) | unberührt — nur BTP-Writes und TP-Upload nutzen es, beide laufen nicht |
| `test_btp` im Assistenten | bleibt; der BTP-Abschnitt ist bei badhub ausgeblendet |
| Ansage-Freitext des LAN-Slaves (`config.btp.host` als Master-Adresse) | unverändert, kein BTP |


### Datenformat (ADR 0101)

Neue gespeicherte Variante `quelle = "badhub"`, neuer Block `badhub_turnier`, neue Dateien unter `turniere/`. Die Grenze
`rueckwaerts_lesbar_ab` bleibt: v0.9.448 liest `"badhub"` als BTP, ältere übergehen das Feld, `turniere/` liest keine
ältere Version. Ein Rückweg während eines badhub-Turniers schaltet die Halle auf BTP — bewusst in Kauf genommen, weil L2
nur liest und nichts verloren geht.

**L3 (v0.9.450), neu entschieden:** neue Dateien `protokoll.jsonl`, `lease.json`, `relay.bin`, `slave.bin`, `verworfen-*.jsonl`;
`paket.json` optional mit `turnier.ende` und `schiedsrichter[].person`. Die Grenze **bleibt**: `config.json` ist
unverändert, BTP-Installationen haben keine dieser Dateien, für BTP-Turniere bleibt der Rückweg fachlich richtig — und
„BTP unverändert“ verbietet, ihnen den Rückweg zu nehmen. In Kauf genommen: Ein Rückweg auf v0.9.449 **während** eines
badhub-Turniers unter der Lease zeigt nur den von badhub bestätigten Stand und nimmt keine Eingaben an; Protokoll und
Lease bleiben liegen, nach dem erneuten Update lädt die Outbox mit derselben Epoche weiter. badhub-Turniere laufen bis
zur Generalprobe E1 nicht produktiv.

### Abstimmung mit badhub (B1)

- Gebaut gegen das Spec-Format; getestet gegen eine Attrappe (`quelle/badhub/tests.rs`).
- `GET turnier/ereignisse`: gelesen wird eine Liste, `{ "ereignisse": [...] }` oder (seit dem Paket in Teilen)
  `{ "ab", "n", "ereignisse" }` mit dem Kopf `n`.
- `runden`: Schlüssel = Spielnummer (als Text).
- Feldnamen in `rahmen.hallen[].felder`: mit oder ohne Hallenpräfix; maßgeblich ist das Kern-Feld aus `spiel_aufrufen`.
- `POST koppeln`: 400/401/403/404/409/410/422 gelten als „Code ungültig“, 429 als Drossel.
- Liveticker-GUID (geklärt v0.9.453, 21.11.2026): Bei Quelle badhub mit gekoppeltem Turnier gilt die GUID aus der
  Kopplung — `turnier.guid` aus dem Turnierpaket (turnier.de-GUID des Turniers, sonst seine `public_guid`, wie
  `tournaments.tournament_uuid` der Turnierseite), gespeichert als `BadhubTurnierConfig.guid`; fehlt sie (Kopplung
  vor v0.9.453, älteres badhub), die `uuid`. `AppConfig::tournament_uuid_kanonisch` (Liveticker, Aushang, Logo) und
  `checkin_guid`/`checkin_bereit` (Meldeliste, Check-In-Stand, Punktverlauf, Zettel) lesen sie; das Feld
  `tournament_uuid` bleibt unangetastet und gilt bei BTP unverändert. Der Assistent zeigt die GUID statt des Felds
  und verlangt es nicht. Anlass: Köpi-Cup 2027 auf Test lief unter der GUID eines älteren Testturniers, die noch im
  Feld stand — der Stand landete auf dessen Turnierseite. Ändert sich die turnier.de-GUID in badhub nach der
  Kopplung, gilt sie erst nach erneutem Koppeln. Die Oberfläche liest dieselbe GUID über `wirksameTurnierGuid()`
  (`src/tournamentGuid.ts`). Bei Quelle badhub gibt es keine TP-Datei: `tp_datei_fuer_turnier` liefert `None`, die
  TP-Automatik (`tp_finden::entscheide`) bleibt still.
- Spielerlinks (v0.9.456, 21.11.2026): Der Personenauszug trägt je Person die DBV-Lizenz (`PaketPerson.lizenz`, badhub ab
  21.11.2026); `abbildung::spieler` setzt sie als `member_id` (nur in der Form `LV-Nummer`). Daran hängen wie bei BTP die
  Spielerlinks der TL-Sicht (`spielerLink`) und `p0/p1_member_ids` im Liveticker. Ohne Lizenz bleibt der Name unverlinkt.

## Paket L3 (umgesetzt, schreibend)

### Weiche → Kernbefehl (`quelle/badhub/schreiben.rs`)

| Weiche | Kernbefehl |
|---|---|
| Feld zuweisen (`write_courts_to_btp`, Court mit Spiel) | `spiel_aufrufen {konkurrenz, spiel, feld, jetzt}` — Feld = Kern-Schreibweise aus dem Paket-Rahmen (`abbildung::feld_kern_name`) |
| Umhängen (Court ohne Spiel + Court mit demselben Spiel) | nur `spiel_aufrufen` auf das neue Feld (der Kern behält den Beginn) |
| Feld räumen ohne Ergebnis | `aufruf_zuruecknehmen` für das dort aufgerufene Spiel |
| Ergebnis / Walkover / Aufgabe / Disqualifikation (`write_result_*`, ScoreStatus 0/1/2/3) | `ergebnis_melden {saetze, ende, jetzt, beginn, bis, netto_min}`; kampflos = `ende: kampflos{sieger}` ohne Sätze, Aufgabe = `aufgabe{aufgebend: Verlierer}`, Disqualifikation (kennt der Kern nicht) mit Sätzen als Aufgabe, ohne als kampflos; `bis` = Spielende, `beginn` = Ende − gemessene Dauer; `netto_min` (seit v0.9.458, Kern 0.17.0) = erster Punkt bis Spielende aus dem Zeiten-Store, auf die Bruttozeit geklemmt, nur Tablet-Weg, nie kampflos |
| Schiedsrichter (`write_officials_to_btp`) | je Spiel `offizielle_setzen {schiedsrichter, aufschlagrichter}` mit IDs aus `paket.schiedsrichter` und `person` von dort (`0` = kein Dienst) |
| Zeilenfarbe (`write_highlight_to_btp`) | keiner — nur lokal |

Zeiten: lokale Wandzeit des Hallen-PCs `JJJJ-MM-TTTHH:MM:SS` (`schreiben::wandzeit`). Ein Befehl, der nichts ändern würde
(dasselbe Ergebnis zweimal vom Tablet, derselbe Aufruf, dieselbe Besetzung), entsteht nicht. Team 1 = oben.

### Ausführen (`KernQuelle::ausfuehren`)

1. Schreibrecht: aktive Lease und `paket.kern_version` = `KERN_VERSION` (sonst `SchreibFehler::Gesperrt`).
2. Jeder Befehl bekommt eine Kennung `bts-<Lauf>-<ms hex>-<Zähler>` (nur `[A-Za-z0-9-]`, ≤ 64 — was badhub annimmt).
3. `pruefen_turnier` der Reihe nach auf den fortgeschriebenen Stand — **alle oder keiner** (wie ein BTP-Schreibvorgang).
   Ablehnung → `SchreibFehler::Kern {text, grund}` (Text ohne Namen, `grund` im Wortlaut des Kerns) an Tablet/TL.
   Bekannte Kennung → wirkungslos (Idempotenz des Kerns).
4. Posten (Befehl + Ereignisse + lokal Akteur/Zeit) per `write_all` + `sync_data` an `turniere/<uuid>/protokoll.jsonl`;
   beim Laden wird eine angefangene letzte Zeile verworfen und abgeschnitten.
5. Stand = Paket-Ereignisse (von badhub bestätigt) + eigene darüber hinaus, neu gefaltet.

### Lease (`lease.rs`, Abgleich-Task)

- `POST turnier/lease {n, relay?, uebernehmen: false}` mit `n` = lokalem Stand; ohne gespeicherte Lease sofort, mit
  aktiver Lease einmal je Lauf und **nur bei leerer Outbox** (sonst stimmte `n` nicht — die Outbox läuft dann mit der
  gespeicherten Epoche). Vor der Lease holt der Abgleich Ereignisse nach (`n_veraltet` heilt so im nächsten Takt);
  unter der Lease holt er keine Ereignisse, nur alle 60 s die Stammdaten.
- `relay: {ns: install_id, tl_token}` nur mit Cloud-Weg. Der Kern-Zugang bleibt über Neustarts gleich.
- Offline beim Start: gespeicherte Lease (Epoche) gilt weiter, kein Ablauf. Ohne gespeicherte Lease: Eingaben gesperrt.
- `lease_belegt`: Hinweis „ein anderes Gerät steuert das Turnier“, Eingaben gesperrt.
- `409 uebernahme_nicht_freigegeben` (badhub B2: Übernahme ohne Freigabe im Web): Hinweis „Übernahme in badhub im
  Bereich Turniertag freigeben (gilt 10 Minuten)“. bts-light sendet heute stets `uebernehmen: false`; die Meldung ist
  für den Übernahme-Weg vorbereitet (`lease_ablehnung_text`).
- Weiterleitung aus badhub B2 (`{"opId", "viewRev": 0, "action": {"action": "kern_befehl", "befehl": {"id", "art", …}}}`
  ohne `jetzt`) wird genau so angenommen (Test `weiterleitung_von_badhub_b2_genau_so_wie_gesendet`).
- Rückgabe (Knopf **Turnier beenden / Steuerung an badhub zurückgeben**, Tauri `badhub_turnier_beenden`; seit v0.9.451
  auch **Turnier beenden** im TL-Web, siehe unten): nur mit leerer Outbox, `DELETE turnier/lease {epoche, n}`, danach
  Zustand `zurueckgegeben`, Kern-Zugang weg, Personenauszug gelöscht. Kein automatisches Neunehmen nach Neustart. Bei
  offener Outbox lautet die Ablehnung „N Ereignisse warten noch auf den Abgleich mit badhub …“ (`text_outbox_offen`).
- **Steuerung wieder übernehmen** (`badhub_steuerung_uebernehmen`, nach Rückgabe oder Rückholung): eigene, nicht
  bestätigte Posten wandern nach `verworfen-<ms>.jsonl`, Paket frisch von badhub, Lease weg — der Abgleich nimmt sie neu.

### Outbox (`outbox_schleife`, eigener Task)

- Lädt die Posten nach `basis_n` (= bestätigtes `n`) hoch: höchstens 200 Posten und 1000 Ereignisse je Aufruf.
- `200 {bis_n}` → bestätigt; eigene Ereignisse bis dort wandern ins Paket (`paket.json`), `lease.json` trägt `bis_n`.
- `409 luecke {erwartet_n}` → ab dort fortsetzen (liegt `erwartet_n` außerhalb des eigenen Bereichs: angehalten).
- `409 epoche_veraltet` → `entzogen`: Outbox steht, Eingaben gesperrt, rotes Band im TL-Web und im Hauptfenster.
- `401/403` (Widerruf — der in badhub die Lease beendet — oder eine vorgeschaltete Sperre) → nur **angehalten**
  (rot, „badhub lehnt den Zugang dieses PCs ab“): Eingaben bleiben möglich, nichts wird verworfen; erst ein späteres
  `epoche_veraltet` oder die Turnierleitung entscheidet über die Rückholung.
- Protokoll: Scheitert das Anhängen mittendrin (Platte voll), wird die Datei auf die alte Länge zurückgeschnitten.
- `422 abweichung` → angehalten (rot, „Abgleich mit badhub angehalten“); sollte bei gleicher Kern-Version nie vorkommen.
- Netz, 429, 5xx → Neuversuch nach 5, 10, 20, 40, 60 s, danach alle 60 s.
- Lage (`Abgleich`): „badhub: aktuell“ / „N Ereignisse warten, letzter Kontakt HH:MM“ — Tauri `badhub_abgleich`
  (Dashboard `BadhubAbgleichPanel`) und `TlState.badhub_abgleich` (TL-Web, nur bei badhub gesetzt).

### Eingaben aus badhub über das Relay

`TlAction::KernBefehl { befehl }` (höchstens 4096 Byte, 5 Sätze, 64 Zeichen je Angabe) vom Kern-Zugang `badhub-kern` (Wire-Ebene: [cloud-relay.md](../cloud-relay.md),
„Kernbefehle aus badhub“). `KernQuelle::befehl_von_badhub`: nur die sechs Durchführungs-Befehle, Kennung wie oben
geprüft, `jetzt` setzt der Knoten; danach derselbe Weg wie eine Eingabe in der Halle (Akteur `badhub` im Protokoll).
Antwort `ok`, `kern_abgelehnt` (+ `ablehnung`), `not_allowed` (keine Lease) oder `unsupported`.

### Abstimmung mit badhub (B2)

- **`paket.schiedsrichter[].person` fehlt in B1.** badhub prüft beim Hochladen `person` gegen die Schiedsrichterliste;
  bts-light schickt sie nur, wenn das Paket sie trägt. B2 sollte `person` ins Paket aufnehmen (sonst
  `offizieller_unbekannt` für Offizielle, die auch spielen).
- B2 schickt Kernbefehle mit `op_id` = Befehls-ID; `jetzt` darf fehlen. Erwartete Antworten wie oben; `host_offline`
  bzw. Zeitüberschreitung → Hinweis „Halle nicht erreichbar“.
- Befehle aus bts-light tragen kein Feld `geraet`: Wer in der Halle gehandelt hat, steht nur lokal im Protokoll.

## Standby-Übernahme ohne Netz (v0.9.455, Paket S2)

Spec: badhub `bts_light_steuerung.md` („Standby-Übernahme ohne Netz → bts-light (Paket S2)“), ADR-0084 Nachtrag.
Gegenseite badhub S1 (PR #1401).

| Was | Wo |
|---|---|
| Rolle | `paket.turnier.lease` (`PaketLease {epoche, geraet, standby, du_bist}`) beim Koppeln und im 60-s-Paket, am Slave zusätzlich `GET /turnier/lease` höchstens einmal je Minute im Hintergrund (`slave::rolle_auffrischen`); abgelegt in `rolle.json`. Standby = `du_bist: "standby"` |
| Spiegeln | Standby läuft als LAN-Slave: `GET :8088/quelle/ereignisse?ab=<spiegel_n>&posten=1` → zusätzlich `posten` (nicht bestätigte Posten des Masters, Befehl + Ereignisse), `bestaetigt_n`, `epoche`. Ohne `posten=1` bleibt die Antwort Byte für Byte wie bisher (älterer Slave), ein älterer Master liefert keine Posten → Hinweis, Rückfall auf die Ansage-Slave-Anzeige. Bestätigtes (≤ `bestaetigt_n`) wandert lückenlos ins eigene Paket, der Rest nach `spiegel.jsonl`; Stand in `spiegel.json`. Anzeige `StandbyLage` („Standby für <Hauptrechner>, gespiegelt bis n = …, vor x s“), Dashboard `StandbyPanel` |
| Notfall-Sicherung | `config.badhub_notfall_ordner` (Vorgabe leer = aus, Assistent). Task `notfall_schleife` am Hauptrechner (5-s-Prüfung, höchstens alle 30 s nach Änderung), beim Anhalten `stop_sync_kern` → `notfall_sichern(…, true)`. Datei `notfall-<uuid>.btsk`: `{format: "bts-light-notfall", version: 1, uuid, epoche, n, geschrieben_ms, posten: [{befehl, ereignisse}]}` — keine Namen, keine Tokens, kein Akteur; atomar über `schreiben_privat` (temp + rename). Einlesen `sicherung_lesen` (Größe ≤ 64 MiB, Format, Version, UUID, lückenlos, `n`) + `KernQuelle::sicherung_einlesen` (gleiches Turnier, Anschluss an das eigene Paket, neuer als der Spiegel; alter Spiegel nach `verworfen-<ms>.jsonl`) |
| Übernehmen | Tauri `badhub_standby_uebernehmen` (Desktop, Rückfrage mit Warnung) → `KernQuelle::als_standby_uebernehmen` (nur mit bestätigter Standby-Rolle und bekannter Epoche): Spiegel → `protokoll.jsonl`, `lease.json {epoche: bisherige, bis_n: Paket-n, standby: true}`; dann `slave_mode = false` und Neustart der Übertragung — Tablet-Server, TL-Web, Auto-Vergabe wie am Hauptrechner |
| Outbox | Erster Upload mit `standby: true` (`OutboxAuftrag.standby`); bis dahin kein Ereignis-/Paket-Abruf und kein `POST turnier/lease` (badhub trägt noch die Ereignisse des Hauptrechners). Antworten: `200 {bis_n, epoche, abgewiesen}` → neue Epoche; passt der Stand (nichts abgewiesen, `bis_n` = eigener Stand bzw. darüber bei vollständig gesendetem Protokoll → Rest holt der Abgleich), normal weiter, sonst `lease.neu_laden` (Eingaben gesperrt, `neu_laden_in`: Paket frisch, Nicht-Bestätigtes nach `verworfen-<ms>.jsonl`). Danach Relay-Angaben per `POST turnier/lease`. `403 kein_standby` → angehalten (rot). `409 inhaber_aktiv {zuletzt_gesehen_vor_s}` → Posten behalten, Backoff, Hinweis „Hauptrechner war vor x s noch online …“. `409 epoche_veraltet` → `GET turnier/lease`: `du_bist: inhaber` (Antwort ging verloren) → neue Epoche + neu laden, sonst entzogen. `Abgleich.warnung` zeigt `abgewiesen > 0` im Hauptfenster und im TL-Web |
| Tests | `tests_standby.rs`: Spiegeln (Befehl + Ereignisse, Bestätigtes ins Paket, älterer Master), Antwort ohne `posten=1` unverändert, Sicherung schreiben/einlesen (keine Namen/Tokens, abgeschnitten, falsches Turnier, Lücke, nichts Neues, fremde Datei), Übernahme + erster Upload (Relay nachmelden, alter Hauptrechner → `epoche_veraltet`), ohne Rolle kein Übernehmen, übersprungen, teilweise abgewiesen + Neuladen, `kein_standby`, `inhaber_aktiv`, `epoche_veraltet` (verlorene Antwort / wirklich veraltet), Rolle aus `GET /turnier/lease` |

**Grenzen:** Ohne LAN-Spiegel und ohne Sicherung fehlen die Eingaben seit dem letzten Upload des Hauptrechners (Papierzettel).
Tablets und Monitore koppeln nach der Übernahme neu. Mehr als 200 Posten bei der Übernahme: Wird im ersten Teil etwas
abgewiesen, wandern auch die noch nicht gesendeten Posten in die Sicherungsdatei (nicht in badhubs Ablage).

**Datenformat (ADR 0101), neu entschieden:** siehe `release/datenformat.json` (v0.9.455) — Grenze bleibt.

## Rückzug während des Turniers (v0.9.452, Kern 0.13.0)
## Rückzug während des Turniers (v0.9.454, Kern 0.13.0)

| Was | Wo |
|---|---|
| Kernbefehl aus badhub | `schreiben::ERLAUBTE_ARTEN` um `rueckzug`, `rueckzug_zuruecknehmen` erweitert (= badhub `Lease::BEFEHLE`); `jetzt` setzt der Knoten (`ARTEN_MIT_JETZT`); Größen-/Feldprüfung wie bisher (`befehl_begrenzt`, serde lehnt unbekannte `ergebnisse` und Meldung als Text ab) |
| TL-Web | `relay-proto` `TlAction::BadhubRueckzug {entry_id, ergebnisse}` / `BadhubRueckzugZuruecknehmen {entry_id}`, `TlRueckzugErgebnisse`; Host `tablet/tl.rs` `execute_rueckzug_action` (nach dem Kern-Zugang-Riegel, mit Wiederholungs-Speicher) → `KernQuelle::rueckzug` (Konkurrenz aus `stand.meldung`, Kennung + `jetzt` vom Knoten, Akteur `bts-light`); Zustand `TlState.badhub_meldungen` aus `KernQuelle::rueckzug_meldungen` (nur unter Lease); Dialog `#rz-dlg` in `assets/tl.html` |
| Ablehnungen | `schreiben::ablehnung_text`: `schon_zurueckgezogen`, `nicht_zurueckgezogen`, `nicht_zuruecknehmbar {spiel}`, `meldung_zurueckgezogen {spiel}`, `unbekannte_meldung`, `meldung_nicht_in_konkurrenz`, `nicht_ausgelost` |
| Rechte | Kern-Zugang darf nur `kern_befehl` (Relay `kern_zugang_passt` + Host); jedes TL-Gerät darf zurückziehen (gleiche Vertrauensstufe wie Ergebnisse) |
| Tests | `tests_rueckzug.rs` (Abbildung `entfaellt`/`durch_rueckzug`, Zählweise je Spiel, Kernbefehl, TL-Weg, Texte), `server.rs` `rueckzug_aus_dem_tl_web_nicht_vom_kern_zugang_und_nicht_bei_btp`, Relay `der_rueckzug_aus_dem_tl_web_kommt_nur_von_tl_geraeten_durch`, `relay-proto` `badhub_rueckzug_roundtrips_and_demands_the_choice` |

**Datenformat:** Protokoll und Paket können neue Befehle/Ereignisse (`rueckzug`, `zurueckgezogen`,
`zaehlweise_festlegen`) tragen, die v0.9.451 (Kern 0.12.0) nicht lesen kann. Das ist der schon in L3 entschiedene Fall
„Kern-Version muss gleich sein“: Schreiben verlangt Gleichstand mit badhub, ein Rückweg während eines badhub-Turniers
nimmt keine Eingaben an. `config.json` und BTP-Dateien sind unverändert — die Grenze bleibt.

## Rückgabe aus dem TL-Web (v0.9.451)

| Was | Wo |
|---|---|
| Aktion | `relay-proto` `TlAction::BadhubZurueckgeben` (`{"action": "badhub_zurueckgeben"}`, ohne Felder, Serde-Roundtrip) |
| Relay | reicht durch; `kern_zugang_passt` lässt den Kern-Zugang `badhub-kern` nur `KernBefehl` senden → `403` |
| Host | `tablet/tl.rs` `execute`: Kern-Zugang abgelehnt (vor allem anderen), dann `badhub_rueckgabe_erlaubt` (Quelle badhub, kein Slave) und `quelle::badhub::turnier_beenden` — derselbe Weg wie der Desktop-Knopf; Antwort `badhub_rueckgabe_antwort` (`not_allowed` + Klartext) |
| Merkmal | `TlState.can_return_badhub`; Knopf `#bh-beenden` in `assets/tl.html` nur mit `badhub_abgleich.zustand` `aktuell`/`wartet`, Rückfrage, bei `wartend > 0` sofort „N Ereignisse warten noch …“ |
| Rechte | Das TL-Web hat kein abgestuftes Rechtemodell: Profile blenden nur aus (kein Zugriffsschutz), `TlDevice.hall` wird nicht erzwungen. Deshalb darf **jedes gekoppelte TL-Gerät** zurückgeben — dieselbe Vertrauensstufe, die Ergebnisse eintragen und überschreiben darf. Nur der Kern-Zugang darf es nicht: badhub holt die Steuerung über seine eigene Oberfläche zurück |

## Paket in Teilen (v0.9.451)

badhub kappt `GET turnier/paket` auf höchstens 2000 Ereignisse und liefert neben dem Kopf `n` das letzte enthaltene
`ereignisse_bis_n`. Der Rest kommt über `GET turnier/ereignisse?ab=<n>&max=500` (`{ab, n, ereignisse}`).

- **Lesen:** `api::paket` liest `n`/`ereignisse_bis_n` aus derselben Antwort (`PaketKopf`, nicht Teil von `Paket` —
  `paket.json` behält sein Format) und meldet `PaketAntwort::Neu.nachladen_bis`. Fehlt `ereignisse_bis_n` (altes
  badhub) oder ist `ereignisse_bis_n >= n`, gilt das Paket als vollständig.
- **Seitenweise:** `seiten_holen` fragt ab dem lokalen `n`, bis der Kopf erreicht ist; der Kopf jeder Antwort geht dem
  des Pakets vor (badhub kann inzwischen weiter sein). Maßgeblich ist der Kopf, nicht die Seitenlänge — eine kurze Seite
  heißt nur bei einem badhub ohne Kopf „Ende“. Jede Seite wird sofort abgelegt; leere Seite oder kein Fortschritt
  beendet den Durchlauf, höchstens 400 Seiten am Stück.
- **Koppeln** (`koppeln`) und **Steuerung wieder übernehmen** (`uebernehmen_in`) laden nach (`paket_nachladen`). Bricht
  das Netz mittendrin ab, bleibt ein lückenloser Anfang liegen: Die Kopplung gilt trotzdem (der Code ist eingelöst), der
  Assistent zeigt `Kopplung.nachlade_hinweis`, der Abgleich holt den Rest. 401/403 beim Nachladen bricht ab.
- **Vor `POST turnier/lease`:** Der Abgleich (`abgleich_schritt_in`) nimmt die Lease nur mit vollständigem Stand; sonst
  bleibt der Hinweis „Stand wird von badhub nachgeladen“ bzw. der Netzfehler stehen. So stimmt `n` beim ersten Versuch
  (früher heilte `n_veraltet` erst über mehrere Takte).
- **Stammdaten-Abruf alle 60 s:** braucht kein Nachladen — die Ereignisse werden mit den gespeicherten vereinigt, das
  Paket ist immer ein Anfang ohne Lücke, und was fehlt, holt der nächste Takt ab `n`.
- **Jeder Pfad, der das Paket ersetzt oder lädt,** endet bei „seitenweise bis `n`, dann erst Lease“: Koppeln und
  Übernehmen laden selbst nach, der Start (`KernQuelle::laden`) und der 60-s-Abruf überlassen den Rest dem Abgleich,
  der ohne vollständigen Stand keine Lease nimmt; die Outbox läuft ohnehin nur unter einer Lease.
- **Befund aus v0.9.450 (behoben):** „Steuerung wieder übernehmen“ ersetzte das Paket, ließ aber bestätigte Posten im
  Protokoll. Trug badhubs gekapptes Paket weniger, als der PC bestätigt hatte, zählten sie als „wartend“ — kein
  Nachladen, keine Lease, keine Outbox, Neu-Koppeln abgewiesen. Jetzt leert `uebernehmen_in` das Protokoll nach dem
  Verwerfen des Nicht-Angekommenen (der Rest ist bestätigt und kommt von badhub zurück), und `KernQuelle::laden` räumt
  einen so hängenden Stand ohne Lease (`protokoll_ohne_lease_raeumen`: was über dem Paket liegt, gesichert nach
  `verworfen-<ms>.jsonl`).
- **Rückgabe nur einmal zugleich:** `zurueckgeben_in` sperrt über `rueckgabe_laeuft` (auch bei Abbruch freigegeben).
  Ein zweiter, gleichzeitiger `DELETE` bekäme sonst `epoche_veraltet`, würde als Rückholung gewertet und holte mit dem
  nächsten Paket die Namen zurück.
- Tests: `gekapptes_paket_wird_beim_koppeln_seitenweise_nachgeladen`,
  `abbruch_beim_nachladen_laesst_einen_lueckenlosen_anfang_den_der_abgleich_ergaenzt` (`tests.rs`),
  `lease_erst_wenn_der_stand_bis_zum_kopf_nachgeladen_ist`,
  `uebernehmen_mit_mehr_bestaetigten_als_das_gekappte_paket_traegt_laedt_nach_und_nimmt_die_lease`,
  `haengender_stand_aus_v0_9_450_heilt_beim_laden`, `gleichzeitige_rueckgabe_bleibt_zurueckgegeben_und_holt_keine_namen_zurueck`
  (`tests_l3.rs`).
