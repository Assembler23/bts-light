# TP-Datei während des Turniertags an badhub — Spezifikation

> Status: **Freigegeben 26.10.2026** (via Spec-Pipeline: Brief →
> Grill → How-To → Review; die Skills fehlten im Checkout, die Phasen liefen
> von Hand nach [spec-pipeline-konzept.md](../spec-pipeline-konzept.md)).
> **Umsetzung:** badhub B1–B4 auf Prod (26.10.2026, PRs #1045, #1046, #1055,
> #1057); bts-light **L1 umgesetzt in v0.9.300** (Knopfdruck, Schreib-Gate,
> Handbuch [tp-datei-upload.md](../tp-datei-upload.md)); L2 (Automatik) hat
> eine eigene Spec: [tp-takt-upload.md](tp-takt-upload.md) (freigegeben
> 28.10.2026, ADR 0061).
> Vor dem ersten Einsatz bei einem echten Turnier bleiben die Messungen A2 und
> Jet-Nachlauf (unten, „Offene Fragen“) Pflicht.
> Quelle: Idee vom 26.10.2026 („bts-light läuft meist auf dem BTP-Rechner —
> die TP-Datei kann schon am Turniertag nach badhub").
> Betroffene Repos: **bts-light** (`src-tauri`, Frontend, Handbuch) und
> **badhub** (Endpunkt, Spool-Worker, Importpfad, Turnierseite).
> ADR: [0060](../adr/0060-upload-sperrt-btp-writes-schreib-gate.md) (bts-light);
> badhub-ADRs 0051 (vorläufige Turniere im Schema) und 0052 (Spool, Worker,
> Löschfrist) entstehen in den badhub-PRs B2/B3.
> Messung: [btp_protocol.md](../btp_protocol.md) („Match-ID und Turnier-GUID
> gegen die TP-Datei", PR #390).

## Kontext / Problem

Die badhub-Turnierseite zeigt Meldungen, Auslosungen, KO-Bäume, Gruppen- und
Kreuztabellen und die Spielliste eines Turniers — heute aber erst **nach** dem
Turnier, wenn der nächtliche Exporter die Excel-Exporte und die `.TPBACKUP`
geliefert hat. Am Turniertag selbst gibt es nur die Liveticker-Seite mit Feldern
und Warteliste. Wer wissen will, wie ein Baum steht oder wann die eigene Gruppe
weitergeht, findet nichts.

bts-light läuft in aller Regel auf demselben Rechner wie BTP. Die TP-Datei
(die Access/Jet-Datenbank, mit der BTP arbeitet) liegt also in Reichweite.
badhub kann solche Datenbanken bereits lesen: Der Python-Extraktor
`ops/tp_extract/tp_extract.py` liest die `.TPBACKUP` im Speicher und liefert
Setzplätze und Auslosungstypen (badhub-ADR 0039/0047).

Die Messung vom 26.10.2026 an einer **offenen** TP-Datei (BTP lief) ergab:

- BTP hält die Datei mit einer `.ldb`-Sperrdatei. Exklusives Öffnen scheitert,
  lesendes Öffnen mit Freigabe und Kopieren gelingen. Zwei Kopien im Abstand
  von 2 s waren byteidentisch.
- `Match.ID` im BTP-Protokoll ist `PlayerMatch.id` in der Datei (959
  Match-Zeilen, davon 361 echte Spiele, Felder der Stichprobe identisch). Die
  Verknüpfung mit dem Liveticker-Snapshot braucht keine Abbildung.
- Die Turnier-GUID reist **nicht** im Protokoll-Snapshot. In der Datei steht
  sie in `Settings.LinkCode`; `Settings.WebID` ist die turnier.de-Kurz-ID. Der
  Verband steht nirgends in der Datei.
- gzip drückt 3,6 MB auf 0,44 MB (12 %). Der Parse der sechs Kerntabellen
  dauert 0,5 s. Platzierungen (`PlayerMatch.ranking`) sind in der Datei leer.
- Die Tabelle `Player` enthält Geburtsdatum, Adresse, Telefon und E-Mail;
  `Club` Kontaktdaten; dazu `Payment`, `Log`, `Message`, `usagelog`.

## Zielbild & Erfolgskriterien

Der Turnierleiter wählt im Setup einmalig die TP-Datei. Im Dashboard drückt er
**„TP-Datei an badhub senden"**. bts-light wartet eine Lücke in den eigenen
BTP-Schreibvorgängen ab, liest die Datei zweimal, vergleicht, komprimiert und
sendet sie an badhub. badhub legt sie außerhalb des Webroots ab, extrahiert im
Hintergrund die Turnierdaten und baut daraus ein **vorläufiges Turnier** im
bestehenden Schema. Spätestens zwei Minuten später zeigt die Turnierseite
Auslosungen, Bäume, Tabellen und Spielliste — und darin die Zwischenstände
und Ergebnisse aus dem Liveticker. Die Meldeliste der Turnierseite entsteht
wie heute aus den Spielzeilen; eine eigene `entries`-Ausgabe ist nicht Teil
dieser Spec. Der endgültige
Exporter-Import nach dem Turnier ersetzt den vorläufigen Stand.

Erfolgskriterien:

1. Nach einem Upload der Messdatei (`WebID` 529398) zeigt die badhub-Turnierseite
   alle 13 Konkurrenzen mit ihren Auslosungen, die Spielliste nach Tagen und
   die Gruppentabellen; die Zeilen tragen Verein und Nation aus der Datei.
2. Ein Spiel, das der Liveticker als beendet kennt und das in der TP-Kopie
   noch offen ist, steht auf der Turnierseite mit dem Liveticker-Ergebnis.
   Ein laufendes Spiel zeigt den Live-Satzstand als Zwischenstand.
3. Ein zweiter Klick ohne Änderung meldet „unverändert" und löst keine
   Verarbeitung aus.
4. Ein Upload mit falscher Datei (anderer `LinkCode`) oder mit einer Datei
   ohne `LinkCode` führt zu einer Meldung im Dashboard, die beide GUIDs
   (gekürzt auf die ersten 8 Zeichen) und den Dateinamen nennt. In badhub
   ändert sich nichts.
5. Während des Uploads zählen die Tablets weiter. Ein BTP-Write, der
   eintrifft, während die Kopie läuft, wartet die Kopie ab (typisch 5 s, mit
   Wiederholung bis etwa 9 s). Ein Write, der eintrifft, während die Kopie
   noch auf einen langen Write **wartet**, reiht sich hinter ihr ein und
   wartet Rest-Wartephase (höchstens 20 s) plus Kopie. Nichts wird verworfen.
6. Nach dem Endimport des Exporters ist der vorläufige Stand ersetzt, ein
   weiterer Upload wird mit 409 abgelehnt, und die Spool-Datei ist gelöscht.

## Nicht-Ziele

- **Kein Takt in Stufe 1.** Nur Knopfdruck. Stufe 2 (optionaler Takt, nur
  bei geänderter Datei, nur solange der Liveticker läuft) ist seit 28.10.2026
  eigens spezifiziert: [tp-takt-upload.md](tp-takt-upload.md).
- **Kein Ersatz des Exporters.** Der Endstand mit Platzierungen kommt weiterhin
  aus dem nächtlichen Export. Der Upload liefert Vorläufiges.
- **Keine Platzierungen, keine Medaillen, keine Statistik** aus dem Upload.
  Spielerprofil, HeadsUp, Heimstatistik, Data-Page, Sitemap und `/api/v1/
  tournaments` sehen vorläufige Turniere nicht.
- **Kein Jet-Leser in bts-light.** Die Datei wird nicht clientseitig gefiltert
  oder geprüft (außer Kopf und Größe). `LinkCode` prüft der Server.
- **Kein Upload aus Slave-Instanzen** (`docs/multi-hall.md`), kein Knopf im
  TL-Web oder am Tablet, nur das Desktop-Dashboard.
- **Kein Liga-Modus** (Mannschaftsspiele, Port 9911), **kein Rückweg** badhub →
  BTP oder badhub → bts-light, **kein Weg über das Cloud-Relay** (direkte
  HTTPS-Verbindung wie der Sponsor-Push), **keine Änderung am
  Liveticker-Payload**.
- **Kein automatisches Finden der Datei** über den BTP-Prozess. Der
  Fenstertitel von `btp.exe` nennt den Dateinamen; das bleibt eine Option für
  später.
- **Kein Anlegen eines Liveticker-Kind-Turniers** durch den Upload. Das
  vorläufige Turnier hängt an der GUID, nicht am Liveticker-Key.

## Entscheidungen (Grill 26.10.2026)

| Frage | Entscheidung |
|---|---|
| Quelle | Die **offene** TP-Datei, gewählt per Dateiwähler. Keine BTP-Sicherung. |
| Rohdatei auf badhub | **Spool** außerhalb des Webroots, eine Datei je GUID, Worker extrahiert, Löschung beim Endimport oder nach 14 Tagen, kein Body-Logging. |
| Vorrang je Spiel | **Ergebnis schlägt kein Ergebnis.** Bei zwei Ergebnissen gewinnt die jüngere Quelle. Ein Upload löscht nie ein Liveticker-Ergebnis. |
| Sichtbarkeit | **Alles** aus der Datei; der Knopf ist die Freigabe. Das Handbuch warnt vor unveröffentlichten Auslosungen. |
| Frische der Turnierseite | Seiten-Cache **60 s** für vorläufige Turniere statt 900 s. |
| Verband | **bts-light sendet das Verbandskürzel** aus der Liveticker-URL mit; der Server prüft es gegen den Zugang. |
| Upload nach Endimport | **409** „Turnier bereits endgültig importiert", der Knopf zeigt die Meldung. |
| Platzierungen | **Keine** vorläufig; Standard-Tab der Turnierseite ist „Spiele". |
| `LinkCode` leer oder abweichend | **Beides ablehnen**; Meldung nennt beide GUIDs gekürzt und den Dateinamen. |

## Betroffene Komponenten / Architekturregeln / Daten

**Architekturregeln.** R1: Dateizugriff, Schreib-Gate und Upload liegen im
Rust-Kern; das Frontend löst nur per Tauri-Command aus. R2: BTP bleibt die
Wahrheit; bts-light schreibt nie in die TP-Datei, und `tournament_matches` in
badhub bleibt reine TP-Wahrheit (die Live-Verschmelzung geschieht beim Rendern).
R6: Diagnose-Logs nennen Pfad und Größe, nie Inhalt.

**bts-light** (`src-tauri/src/`, `src/`, `docs/`):

| Datei | Änderung | Muster |
|---|---|---|
| `config.rs` | Block `TpUploadConfig { path: String, tournament_uuid: String }` mit `#[serde(default)]` an `AppConfig.tp_upload`; `AppConfig::tp_datei_fuer_turnier() -> Option<PathBuf>` liefert den Pfad nur, wenn die gespeicherte GUID zur kanonischen `tournament_uuid` passt. | `PrintConfig`, `spiegele_turnier_guid` |
| **neu** `btp/write_gate.rs` | `WriteGate` auf `tokio::sync::RwLock<()>` + `AtomicU64 last_end_ms`; `schreiben()` (Lesesperre), `fuer_kopie(ruhe, max_wartezeit)` (Schreibsperre), `in_flight()` (Testsonde: Zahl der gehaltenen Lesesperren); eine prozessweite Instanz `GATE`, Tests mit eigenen Instanzen. | Prozess-Schalter in `badhub_host.rs` |
| `tablet/server.rs` `write_result_to_btp`, `write_courts_to_btp`, `write_officials_to_btp`, `write_highlight_to_btp` | Je eine Zeile `let _schreibt = write_gate::GATE.schreiben().await;` am Anfang. Keine Signaturänderung; die 16 Aufrufer bleiben unberührt. | — |
| **neu** `badhub/tp_upload.rs` | `tp_upload_url(live_url)` (Status-URL = `tp_upload_url` + `/status`), `lese_stabil(pfad, abstand) -> Result<Vec<u8>, TpUploadFehler>` (`Unruhig`, `KeineTpDatei`, `ZuGross`, `NichtLesbar`), `sende(...)` mit eigenem Client (Timeout 60 s), `UploadAntwort { Angenommen { job }, Unveraendert { received_at }, Abgelehnt { result, message }, BereitsEndgueltig, ZuSchnell { retry_after_s }, VerbandFalsch { message }, NochNichtVerfuegbar, Unauthorized, Status(u16) }`. | `checkin_branding_url`/`push_checkin_branding` in `badhub/push.rs`, `log_upload.rs` (roher Body mit Bearer) |
| `btp/wire.rs` | `gzip_compress` wird `pub(crate)`. | — |
| `commands.rs` | `tp_upload_now`, `tp_upload_status`, `tp_file_check`, reine Funktion `pruefe_upload_bedingungen(&AppConfig)`; `AppState.tp_upload: Mutex<TpUploadStatus>`. | `pruefe_startbedingungen`, `spawn_branding_push`, `read_tournament_logo`, `print_warning` |
| `lib.rs` | Commands registrieren (`tauri-plugin-dialog` ist eingebunden, `dialog:allow-open` erlaubt). | — |
| `src/types.ts`, `src/api.ts` | `TpUploadConfig`, `TpUploadStatus`, `TpFileInfo`; Wrapper `tpUploadNow`, `tpUploadStatus`, `tpFileCheck`. | `PrintConfig`, `aushangHtml` |
| `src/pages/SetupWizard.tsx` | Abschnitt **„TP-Datei (Turnierdatenbank)"** hinter „2 · BTP-Verbindung"; Dateiwähler, Pfadanzeige, Ergebnis von `tp_file_check`, Knopf „Entfernen", Hinweis bei entferntem BTP-Host; im Slave-Modus ausgeblendet. | Abschnitt „Schiedsrichterzettel", `pickLogo` |
| `src/pages/Dashboard.tsx` | Abschnitt **„Turnierdaten an badhub"** vor „Aushang für die Halle": Knopf **„TP-Datei an badhub senden"**, Statuszeile, Hinweistext zur Sichtbarkeit; ohne Pfad Knopf aus + Link ins Setup. | Aushang-Abschnitt, `PrintWarningBanner.tsx` (Poll) |
| `docs/tp-datei-upload.md` (**neu**, Handbuch), `docs/einstellungen.md`, `docs/erste-schritte.md`, `docs/oberflaechen.md`, `docs/multi-hall.md`, `docs/handbuch.json` | Bedienung, Einstellungstabelle, Status-Seite, „Nur der Master lädt hoch"; `<!-- pruef: -->`-Anker für Knopf, Abschnittstitel und Meldungen. | `docs/aushang.md` |
| `docs/btp_protocol.md` | Absatz „Schreib-Gate": eine neue Stelle, an der ein BTP-Write warten kann. | — |
| `CLAUDE.md`, `docs/changelog.md`, `docs/roadmap.md`, Versionsdateien | Doku-Zeile, Changelog-Eintrag, Roadmap „Spezifiziert" → „Umgesetzt"; Version gemeinsam bumpen, Nummer erst beim Merge. | — |

Keine neue Abhängigkeit: `flate2`, `reqwest`, `tokio` (Feature `sync`) und
`tauri-plugin-dialog` sind vorhanden. Kein Hash-Crate (Byte-Vergleich im
Client, SHA-256 nur am Server).

**badhub** (Schwester-Repo, Pfade relativ zu dessen Wurzel):

| Datei | Änderung | Muster |
|---|---|---|
| `ops/tp_extract/tp_extract.py` (+ `test_tp_extract.py`, `README.md`) | **Rohmodus:** Positionsargument darf eine Datei sein; beginnt sie mit `JET_KOPF`, entfällt `entpacken()`. Option `--json <pfad>`. Neue Ausgaben (Schema +1): `tournament` aus `Settings` (`Tournament`, `LinkCode`, `WebID`, `Location`, `LocationCity`), `days` aus `TournamentDay`, `matches` aus `PlayerMatch`, aufgelöst über `Entry` → `Player` (**nur** `id`, `memberid`, `name`, `firstname`, `club`, `country`) → `Club` (`id`, `name`, `clubid`) → `Country` (`id`, `code`), dazu `Draw`-/`Event`-/`Court`-Namen. Wächter-Grep um `payment|log|message` erweitert. | `setzplaetze()`/`draws()`/`zeilen()` |
| `lib/tp_extract.php` | `tpExtractJsonRoh`, `tpMatchesAusJson` (Zeilen in der Form von `parseMatchRow`, danach `turnierVereinIdsSetzen`), `tpTournamentAusJson`. | `tpSeedsAusJson`/`tpDrawsAusJson` |
| `db/migrations/246_tournaments_source.sql` | `tournaments.source ENUM('export','tp_live') NOT NULL DEFAULT 'export'`, `final_imported_at DATETIME NULL`, `live_uploaded_at DATETIME NULL`, Index auf `source`; Backfill `final_imported_at = imported_at`. | Migrationen 059/154 |
| `scripts/import_tournaments_bulk.php`, beide Admin-Wizards | Setzen `source='export'`, `final_imported_at=NOW()`; Endimport über eine `tp_live`-Zeile ist der normale UPDATE in place; danach Spool-Datei der GUID löschen. | ADR-0008-Regel |
| **neu** `lib/tp_live_lib.php` | Reine Funktionen: Spool-Pfade (nur normalisierte GUID im Namen), Meta lesen/schreiben, `tpLiveAnnahmeEntscheiden`, `tpLivePlausibel`, `tpLiveImport` (Transaktion, drei Kindtabellen, **kein** `enrich_tournament`-Spawn), `tpLiveAufraeumen`. | `lib/checkin_branding_lib.php` |
| **neu** `app/Http/Controllers/Api/TpUploadController.php`, `routes/api.php` | `POST /api/tp-upload` und `GET /api/tp-upload/status`. Rahmen aus `CheckinBrandingController` (405, Rate-Limit-Dateien, `liveUpdateAuth`). | `CheckinBrandingController` |
| **neu** `cron/import_tp_live.php` + systemd-Timer (minütlich) + `tools/cron_registry.php` | Worker je `<GUID>.tp` mit `flock`; `--once --guid=` für Sofort-Anstoß und Handbetrieb. | `cron/import_tp_ligen.php` |
| `app/Http/Controllers/TurnierController.php`, **neu** `src/Turnier/LiveVerschmelzung.php`, Blade-Partials, `turnier.js` (Smoke) | Cache 60 s, Standard-Tab „Spiele", Verschmelzung vor Gruppentabelle/KO-Baum/Spielliste, Banner und Marken. | Cache-Override für 404 im selben Controller |
| Konsumenten von `tournaments` (`TournamentsController`, `TurniereController`, `PlayerProfileService`, `HeadsupController`, `lib/data_page.php`, `scripts/refresh_home_stats.php`, `TurnierPlatzierungen`, `lib/sitemap_data.php`, `lib/turnier_ordner.php`; beim Bauen alle Treffer von `grep -rln "FROM tournaments" app src lib cron scripts public` durchsehen, Stand 26.10.2026: 24 Dateien) | Filter `source='export'` überall, wo aggregiert oder in Profile geschrieben wird; Turnierliste markiert „vorläufig". | — |
| Doku | `docs/features/tournament_importer.md`, `docs/features/liveticker_bts.md` (Endpunkt-Vertrag), `docs/ops/hetzner_setup.md`, `docs/features/cron_jobs.md`, ADR 0051/0052, `changelog.json`, Datenschutz-Absatz in badhub-`CLAUDE.md`. | — |

## Verhalten im Detail

### Datei wählen und an die GUID binden (bts-light)

- Der Abschnitt „TP-Datei (Turnierdatenbank)" im Setup öffnet den
  Systemdialog mit Filter `*.tp`. Beim Wählen läuft `tp_file_check`: Endung
  `.tp` (Groß-/Kleinschreibung egal, BTP schreibt `.TP`), Datei vorhanden,
  lesbar, Kopf `\x00\x01\x00\x00Standard Jet DB`, Größe ≤ 32 MB. Das
  Ergebnis steht neben dem Pfad („Turnierdatei erkannt, 3,6 MB", „keine
  TP-Datei", „zu groß").
- **Seit v0.9.302** liest `tp_file_check` die Datei ganz und liefert die
  Turnier-GUID aus `Settings.LinkCode` mit (`TpFileInfo::Erkannt.guid`,
  Byte-Muster in `badhub/tp_guid.rs`, Messung in `btp_protocol.md`). Ohne
  Kennung im Setup wird sie übernommen; eine **andere** Kennung lehnt die
  Datei ab — keine stille Umlenkung des Livetickers. Vor jedem Upload
  derselbe Abgleich (`UploadAusgang::GuidFremd`, kein Netzzugriff, Halt der
  Automatik). Details: Takt-Spec, „Ergänzung 28.10.2026".
- Die Bindung an die GUID entsteht **beim Wählen der Datei**: Der Wizard
  übernimmt die zu diesem Zeitpunkt eingetragene Turnier-GUID in
  `tp_upload.tournament_uuid`; `save_config` schreibt den Block unverändert
  durch. Ändert der Nutzer die GUID — im selben Durchlauf oder später —,
  bleibt der Pfad sichtbar, aber `tp_datei_fuer_turnier()` liefert `None`;
  Setup und Dashboard sagen „Datei neu wählen", bis die Datei erneut gewählt
  wird. Es wird nichts still gelöscht.
- Ist `btp.host` weder `127.0.0.1` noch `localhost`, zeigt der Abschnitt den
  Hinweis „Die Datei liegt auf dem BTP-Rechner — Upload nur von dort
  empfohlen". Kein Verbot.
- Im Slave-Modus sind Abschnitt und Knopf ausgeblendet.

### Upload auf Knopfdruck (bts-light)

`tp_upload_now` prüft die Vorbedingungen synchron (Verletzung → `Err` mit
Text, kein Status), startet den Upload in einem eigenen Rust-Task und gibt
sofort „läuft" zurück. Ein zweiter Klick während `laeuft` ist ein No-op. Das
Dashboard fragt nur `tp_upload_status` alle 5 s ab, solange `laeuft`; **alle**
Netzzugriffe (Upload und Status-Poll an badhub) laufen im Rust-Task (R1). Der
Knopf ist unabhängig davon, ob die Liveticker-Übertragung läuft.

1. **Vorbedingungen** (`pruefe_upload_bedingungen`, reine Funktion):
   kein Slave-Modus; Badhub-Passwort gesetzt; Turnier-GUID gesetzt; Pfad
   konfiguriert und GUID passt; `live_url` enthält ein Verbandskürzel
   (`kuerzel_aus_live_url`). Jede Verletzung hat einen eigenen Text mit
   Verweis auf den Setup-Abschnitt.
2. **Schreib-Gate** (ADR 0060): `GATE.fuer_kopie(ruhe = 3 s, max_wartezeit =
   20 s)`. Laufende BTP-Writes werden höchstens 20 s abgewartet; danach
   „BTP ist gerade beschäftigt, bitte gleich noch einmal". Mit gehaltener
   Sperre vergehen 3 s Ruhe, in denen kein neuer Write beginnt.
3. **Stabil lesen** (`lese_stabil`): Datei mit Freigabe lesen, 2 s warten,
   erneut lesen, Bytes vergleichen. Bei Abweichung einmal wiederholen, dann
   „Datei ändert sich gerade, bitte gleich noch einmal". Kopf- und
   Größenprüfung wie beim Wählen. Datei fehlt oder ist nicht lesbar
   (verschoben, Laufwerk oder UNC-Pfad getrennt) → „TP-Datei nicht gefunden:
   ‹Basisname› — im Setup neu wählen". Erst danach wird die Sperre
   freigegeben.
4. **Senden** (`sende`): `POST <badhub.url ohne letztes Segment>/tp-upload`,
   Header `Authorization: Bearer <Passwort>`, `Content-Type:
   application/octet-stream`, `Content-Encoding: gzip`, `X-Tournament-Uuid:
   <kanonische GUID>`, `X-Federation: <Kürzel>`, `X-File-Name: <Basisname,
   percent-kodiert nach RFC 3986 in UTF-8>` (Header-Werte sind ASCII; ein
   Name mit Umlaut darf den Upload nie verhindern, der Server dekodiert).
   Body: `gzip_compress(bytes)`. Timeout 60 s. Die URL folgt `badhub.url`,
   also auch dem Testsystem-Umschalter.
5. **Antwort → Status** (`TpUploadStatus { zeitpunkt_ms, groesse, ergebnis,
   laeuft }`):

   | Server | Dashboard-Text |
   |---|---|
   | 202 `{"status":"accepted","job":…}` | „angenommen, wird verarbeitet" → der Rust-Task fragt alle 5 s `GET <tp_upload_url>/status?uuid=` ab, bis `result` vorliegt, höchstens 2 Minuten; dann `result`-Text (`ok` → „übernommen, N Spiele"; `guid_mismatch`/`parse_error`/`implausible`/`final_imported` → `message` des Servers). Nach 2 Minuten ohne `result`: „angenommen, Verarbeitung dauert noch — Stand beim nächsten Klick"; `laeuft` endet. |
   | 200 `{"status":"unchanged","received_at":…}` | „unverändert seit hh:mm" (Zeit aus `received_at`, damit es auch nach App-Neustart stimmt) |
   | 200 `{"status":"rejected","result":…,"message":…}` (gleiche Bytes wie eine zuvor abgelehnte Datei) | `message` des Servers, z. B. die Mismatch-Meldung erneut |
   | 409 | „Turnier bereits endgültig importiert" |
   | 429 + `Retry-After` | „zu schnell hintereinander, in N s wieder möglich" |
   | 403 | „Verbandskürzel ‹x› passt nicht zum badhub-Zugang — Liveticker-Adresse in „1 · Liveticker-Ziel" prüfen" (eigene Variante, **nicht** mit 401 zusammengelegt wie in `push.rs::abgelehnt`) |
   | 401 | Text wie beim Liveticker-Passwortfehler |
   | 400 / 404 / Basic-Auth-Wand | „badhub kennt den Upload noch nicht" — Statuszeile, kein Banner, kein Warn-Log. 400 kann vom neuen Server nur bei kaputter GUID kommen; der Client sendet nur kanonische GUIDs, deshalb ist die Zusammenlegung hinnehmbar. |
   | 413 / 415 | `message` des Servers |
   | andere (5xx, Deploy-Fenster) | bestehender Text von `PushError::Status` („badhub antwortete mit HTTP-Status N") |
   | Netzwerkfehler | „badhub nicht erreichbar" |

   Alle Fehlerantworten des Controllers tragen `{"status":"error","message":"…"}`;
   `Retry-After` zusätzlich als Header.

   Der Merker lebt in `AppState`, nicht in der `SyncEngine` (die wird bei jedem
   Speichern neu gebaut) und nicht in der Config. Er geht beim App-Neustart
   verloren; der Server kennt den Stand.
6. **Log:** eine Zeile mit Dateiname, Rohgröße, komprimierter Größe, Dauer und
   Statuscode. Nie Inhalt, nie Header-Werte.

### Schreib-Gate (bts-light, ADR 0060)

Alle BTP-Writes laufen durch vier Funktionen in `tablet/server.rs`, die nur
`&AppConfig` kennen. Deshalb ist das Gate prozessweit: `WriteGate` auf
`tokio::sync::RwLock<()>`. Jeder Write hält die **Lese**sperre, die Kopie
nimmt die **Schreib**sperre. `tokio::sync::RwLock` ist schreiber-bevorzugt:
Sobald die Kopie wartet, reihen sich neue Writes hinter ihr ein — genau das
liefert die garantierte Lücke. Folge für die Wartezeiten: Ein Write, der
während der Kopie eintrifft, wartet die Kopie ab (3 s Ruhe + 2 × Lesen mit 2 s
Abstand, typisch 5 s; mit der einmaligen Wiederholung bis etwa 9 s). Ein Write,
der eintrifft, während die Kopie noch auf einen langen Write wartet, wartet
Rest-Wartephase (höchstens 20 s) plus Kopie. Im Normalfall ohne langen Write
also nur die Kopie. Nichts geht verloren; der Nachschub (`flush_btp_retries`)
läuft ohnehin verzögert. Die Konstanten
`RUHE = 3 s`, `MAX_WARTEZEIT = 20 s`, `LESEABSTAND = 2 s` stehen kommentiert im
Modul und werden nach der Nachlauf-Messung (siehe Offene Fragen) angepasst.

Was das Gate **nicht** kann, steht im Handbuch: Eingaben des Turnierleiters
direkt in BTP sieht niemand. Dagegen stehen der Byte-Vergleich im Client und
die Plausibilitätsprüfung im Server.

### Annahme am Server (badhub, `POST /api/tp-upload`)

1. Methode ≠ POST → 405. Rate-Limit-Dateien je IP für Auth-Fehler wie beim
   Sponsor-Push. `liveUpdateAuth()` → Eltern-Key, sonst 401.
2. `X-Tournament-Uuid` fehlt oder ist keine GUID → 400. `X-Federation` muss
   ein aktiver `federations.slug` sein **und** gleich dem Eltern-Key — außer
   der Eltern-Key ist `default` (Legacy-Zeile), dann gilt das Kürzel. Sonst 403.
3. Body: bei `Content-Encoding: gzip` `gzdecode` mit Deckel 64 MB; entpackt
   ≤ 32 MB, sonst 413. Präfix `JET_KOPF`, sonst 415 „keine TP-Datei".
4. SHA-256; Entscheidung `tpLiveAnnahmeEntscheiden(meta, tournamentRow, sha, now)`:
   - `tournaments` mit dieser GUID hat `source='export'` oder
     `final_imported_at` gesetzt → **409** `{"message":"Turnier bereits
     endgültig importiert"}`.
   - gleicher SHA-256 wie die letzte Annahme **und** deren `result = ok` →
     **200** `{"status":"unchanged","received_at":…}`; gleicher SHA-256 und
     `result ≠ ok` → **200** `{"status":"rejected","result":…,"message":…}`
     (kein neuer Auftrag, kein Cooldown).
   - letzte Annahme vor weniger als 60 s → **429**, `Retry-After: 60`.
   - sonst → Datei atomar als `{BADHUB_DATA_DIR}/tp-live/<GUID>.tp` (`.tmp` +
     `rename`, Verzeichnis `0750 badhub:badhub`, außerhalb des Webroots),
     Meta `<GUID>.json` (`received_at` ISO 8601 UTC, `sha256`, `size`,
     `key`, `federation`, `file_name`, `processed_at = null`,
     `processed_sha = null`, `result = null`, `message = null`, `matches =
     null`; `last_ok_matches` bleibt aus der vorigen Meta erhalten),
     Sofort-Anstoß `php cron/import_tp_live.php --once --guid=<GUID>` im
     Hintergrund, **202** `{"status":"accepted","job":"<sha-Präfix>"}`.
     Endpunkt und Worker schreiben die Meta nur unter dem `flock` der GUID.
5. **Kein Body-Logging.** Die Protokollfunktion des Liveticker-Controllers
   wird nicht übernommen. Fehlertexte enthalten nie Dateiinhalt.

`GET /api/tp-upload/status?uuid=` (Bearer) liefert `received_at`,
`processed_at`, `result` (`ok` | `guid_mismatch` | `parse_error` |
`implausible` | `final_imported`), `matches`, `message`. Ohne Meta-Datei 404.
Eine Datei gilt nur als verarbeitet, wenn `processed_sha == sha256` — so kann
ein Worker, der gerade die alte Datei liest, eine inzwischen neu angenommene
nicht als erledigt stempeln.

### Worker und Import (badhub, `cron/import_tp_live.php`)

Minütlicher systemd-Timer plus Sofort-Anstoß aus dem Endpunkt; beide rufen
dieselbe Funktion. Je `<GUID>.tp`, deren `sha256 ≠ processed_sha`:

0. `tournaments` der GUID hat inzwischen `source='export'` (Endimport kam
   zwischen Annahme und Verarbeitung) → `result = final_imported`, Spool-Datei
   löschen, keine Datenbankänderung.
1. `flock` auf `<GUID>.lock`; läuft schon einer, Ende. Der Worker merkt sich
   den SHA-256 der Datei, die er liest, und schreibt ihn am Ende als
   `processed_sha`.
2. Extraktor im Rohmodus (`tpExtractJsonRoh`: `proc_open` ohne Shell, `-I`,
   `timeout 120`, `ulimit -v`), Ziel-JSON im Spool.
3. `tournament.LinkCode` aus der JSON gegen die GUID der Meta prüfen (beide
   normalisiert: ohne `{}`, Großschreibung). Leer oder abweichend →
   `result = guid_mismatch`, `message` mit beiden gekürzten GUIDs und
   `file_name`, Spool-Datei löschen, keine Datenbankänderung.
4. `tpLivePlausibel`: ablehnen nur bei Parserfehler (`parse_error`) oder wenn
   die Zahl der Match-Zeilen unter 50 % von `last_ok_matches` aus der Meta
   liegt (`implausible`, zerrissene Datei). Ohne Vorstand (erster Upload)
   wird jede parsebare Datei angenommen, auch mit 0 Spielen (vor der
   Auslosung). Nach `implausible` bleibt die Spool-Datei liegen (der nächste
   Upload überschreibt sie), die Datenbank bleibt unverändert. Verschwundene
   Ergebnisse werden angenommen — eine Korrektur in BTP ist legitim.
5. `tpLiveImport` in einer Transaktion: `tournaments` upsert über die GUID mit
   `source='tp_live'`, `federation_slug` aus der Meta, `name` aus
   `Settings.Tournament`, `external_id` aus `WebID` (oder NULL), `location`
   aus `Location`/`LocationCity`, `date_start`/`date_end` aus `TournamentDay`,
   `live_uploaded_at = received_at`; `tournament_matches`, `tournament_seeds`,
   `tournament_draws` leeren und neu schreiben (Zeilenform wie
   `parseMatchRow`, danach `turnierVereinIdsSetzen`); **nicht**
   `tournament_results`. Kein `enrich_tournament.php`: Verein, Vereinscode
   und Nation stehen in der Datei.
6. Meta `processed_at`, `processed_sha`, `result = ok`, `matches = N`,
   `last_ok_matches = N`. Danach `tpLiveAufraeumen`: `<GUID>.tp`, `.json`
   und `.lock` löschen, wenn `received_at` älter als 14 Tage ist.

Der Endimport des Exporters trifft die `tp_live`-Zeile über die GUID, macht
den normalen UPDATE in place (gleiche `id`), setzt `source='export'` und
`final_imported_at=NOW()`, schreibt die Kindtabellen neu und löscht
`<GUID>.tp`, **nicht** die Meta (der Status-Endpunkt antwortet weiter, bis
`tpLiveAufraeumen` nach 14 Tagen räumt). Danach lehnt der Endpunkt mit 409 ab.

### Live-Verschmelzung beim Rendern (badhub)

`TurnierController` lädt wie heute `tournament_matches` und zusätzlich den
`liveticker_state.snapshot_json` des Kind-Turniers, gefunden über
`liveticker_tournaments.tournament_uuid = <GUID>`. Gibt es kein Kind oder
keinen Snapshot, bleibt die Eingabe unverändert.

`LiveVerschmelzung::anwenden(array $tpRows, ?array $snapshot, ?int $uploadTs)`
ist eine reine Klasse mit Unit-Tests. Schlüssel: `btp_<id>` im Snapshot ↔
`match_id` in der Tabelle. Regel je Spiel:

1. Ergebnis schlägt kein Ergebnis — egal aus welcher Quelle.
2. Beide haben ein Ergebnis und es unterscheidet sich → die jüngere Quelle
   gewinnt (`end_ts` des Liveticker-Spiels gegen `live_uploaded_at`; liefert
   der Extraktor `PlayerMatch.lastupdated` je Zeile mit, ist das der
   präzisere Zeitstempel der TP-Seite — Entscheidung in PR B4, F2 bleibt).
3. Liveticker kennt das Spiel als laufend, Tabelle hat kein Ergebnis → die
   Live-Sätze erscheinen als Zwischenstand mit `live = true`.
4. Aus (1) folgt: Ein Upload ohne Ergebnis lässt ein Liveticker-Ergebnis
   stehen.

Die Ausgabe hat dieselben Spalten wie `tournament_matches` plus `live` und
`live_ts`. Gruppentabelle, KO-Baum und Spielliste lesen nur `winner`, `set*`
und `score_status` und brauchen keine Änderung. Die Anonymisierung
(`TurnierAnonymisierung`) bleibt beim Rendern; die Klasse fasst keine Namen an.

### Turnierseite und übrige Leser (badhub)

- `source='tp_live'` (ein laufendes Turnier ist nie `export`, deshalb deckt
  das „vorläufig/laufend" aus dem Grill ab): Seiten-Cache 60 s
  (`page_cache_ttl`), Standard-Tab
  „Spiele", Banner „Vorläufiger Stand vom hh:mm (TP-Datei) · Live-Stand hh:mm",
  Marke „live" an Zeilen mit `live = true`, Sieger-Tab zeigt „Platzierungen
  nach dem Turnier".
- Turnierliste zeigt das Turnier mit Badge „vorläufig".
- Alles, was aggregiert oder in Profile schreibt, filtert `source='export'`.
  `/api/v1/tournaments` antwortet für `tp_live` mit `found:false,
  reason:'provisional'`, damit der DBV-Downloader es weiter als „nicht
  importiert" behandelt.

### Rückwärtskompatibilität und Testsystem

- Altes badhub (ohne Endpunkt) antwortet 404 oder mit der Basic-Auth-Wand →
  „badhub kennt den Upload noch nicht", kein Fehlerbanner.
- Altes bts-light gegen neues badhub: unberührt.
- Ältere bts-light-Version nach einem Rückrollen lädt die `config.json` weiter
  und **verwirft den Block beim nächsten Speichern** (serde ignoriert
  unbekannte Felder). Der Pfad ist dann neu zu wählen. Hinnehmbar.
- Testsystem: Die Upload-URL folgt `badhub.url`. `test.badhub.de` blockt
  Bearer-Routen per Basic-Auth; ein Probelauf dort setzt eine Ausnahme für
  `/api/tp-upload` voraus (Kollege, nginx).

## Akzeptanzkriterien

**bts-light**

1. Setup: Wählen einer `.tp` oder `.TP` zeigt Pfad und „Turnierdatei erkannt,
   3,6 MB"; eine Datei ohne Jet-Kopf zeigt „keine TP-Datei", eine über 32 MB
   „zu groß"; beide werden nicht gespeichert.
2. Nach Änderung der Turnier-GUID zeigen Setup und Dashboard „Datei neu
   wählen"; der Knopf ist aus; der alte Pfad steht noch da.
3. Knopf ohne konfigurierten Pfad ist aus und verlinkt ins Setup. Im
   Slave-Modus fehlen Abschnitt und Knopf.
4. Klick → Status „läuft" → nach Antwort 202 „angenommen, wird verarbeitet" →
   nach spätestens 2 Minuten „übernommen, N Spiele", die Server-Meldung oder
   „angenommen, Verarbeitung dauert noch — Stand beim nächsten Klick".
   Verletzte Vorbedingungen kommen synchron als Fehlertext, ohne Status.
5. Zweiter Klick auf die unveränderte Datei → „unverändert seit hh:mm".
6. Während eines laufenden BTP-Writes (Mock, der 10 s hängt) startet die Kopie
   erst nach dessen Ende; ein Write, der während dieser Wartephase oder in
   der Ruhezeit ankommt, beginnt erst nach der Kopie (nicht vor ihr) und geht
   nicht verloren. Ein Write, der länger als 20 s hängt, führt zu „BTP ist
   gerade beschäftigt".
7. Ändert sich die Datei zwischen den beiden Lesevorgängen zweimal
   hintereinander → „Datei ändert sich gerade". Ist die Datei verschoben oder
   das Laufwerk getrennt → „TP-Datei nicht gefunden: ‹Basisname› — im Setup
   neu wählen".
8. Server 404 oder Basic-Auth-Wand → „badhub kennt den Upload noch nicht",
   kein Banner, kein Warn-Log. Server 409 → „Turnier bereits endgültig
   importiert". Server 429 → Text mit Restsekunden. Server 403 → Text mit
   dem Verbandskürzel und Verweis auf „1 · Liveticker-Ziel", **nicht** der
   Passwort-Text. Server 500 → „badhub antwortete mit HTTP-Status 500".
9. Gesendeter Body beginnt mit `1f 8b`; Header `X-Tournament-Uuid`,
   `X-Federation`, `X-File-Name` vorhanden; `X-File-Name` enthält nur den
   Basisnamen, percent-kodiert; eine Datei „Köpi-Cup 2026.TP" wird gesendet,
   nicht abgewiesen. Das Log enthält Dateiname und Größen, nie Inhalt.
10. `live_url` ohne Verbandskürzel → Fehler mit Verweis auf „1 ·
    Liveticker-Ziel", kein Upload.
11. `node scripts/test-handbuch-fakten.mjs` ist grün; die Anker für den Knopf
    „TP-Datei an badhub senden", den Abschnittstitel „TP-Datei
    (Turnierdatenbank)" und die Meldungen „BTP ist gerade beschäftigt",
    „badhub kennt den Upload noch nicht" und „Turnier bereits endgültig
    importiert" existieren.

**badhub**

12. Gültiger Bearer + Attrappe mit Jet-Kopf → 202 und Datei `<GUID>.tp` im
    Spool mit Meta; falscher Kopf → 415; ohne `X-Tournament-Uuid` → 400;
    Kürzel ≠ Eltern-Key → 403 mit `{"status":"error","message":…}`; gleiche
    Bytes erneut nach `result=ok` → 200 `unchanged` mit `received_at`;
    gleiche Bytes erneut nach `result=guid_mismatch` → 200 `rejected` mit der
    alten `message`; innerhalb 60 s neue Bytes → 429 mit `Retry-After`;
    Turnier mit `source='export'` → 409; 2 MB Zufallsbytes → 415 vom
    Controller, nicht 413 von nginx. Header-Wert `K%C3%B6pi-Cup%202026.TP`
    landet dekodiert als `file_name` in der Meta.
13. Worker auf eine Spool-Datei → `tournaments` mit `source='tp_live'`,
    `federation_slug` aus der Meta, drei Kindtabellen gefüllt,
    `tournament_results` leer, Meta `processed_at`, `processed_sha`,
    `result=ok`, `last_ok_matches`; zweiter Lauf ohne neue Datei tut nichts;
    wird während eines Laufs eine neue Datei angenommen, bleibt sie
    unverarbeitet markiert (`processed_sha ≠ sha256`) und der nächste Lauf
    nimmt sie; zwei gleichzeitige Läufe → einer wartet auf `flock`; Turnier
    inzwischen `source='export'` → `result=final_imported`, keine
    Datenbankänderung.
14. `LinkCode` leer oder ≠ Meta-GUID → `result=guid_mismatch`, Meldung mit
    beiden gekürzten GUIDs und Dateiname, keine Datenbankänderung, Datei
    gelöscht. Datei mit < 50 % von `last_ok_matches` → `implausible`,
    Datenbank unverändert, Spool-Datei bleibt liegen; erster Upload mit 0
    Spielen → `ok`.
15. Endimport über eine `tp_live`-Zeile: gleiche `id`, `source='export'`,
    `final_imported_at` gesetzt, Spool-Datei weg; danach Upload → 409.
16. Turnierseite bei `tp_live`: `Cache-Control` mit `max-age=60`, Standard-Tab
    „Spiele", Banner vorhanden, Sieger-Tab mit Hinweis; bei `export`
    unverändert (Golden-Test grün).
17. Verschmelzung: (a) Live beendet, TP offen → Live-Ergebnis; (b) TP beendet,
    Live unbekannt → TP-Ergebnis; (c) beide beendet, verschieden → jüngere
    Quelle; (d) laufend → Zwischenstand mit `live=true`; (e) Snapshot `null`
    → Eingabe unverändert; (f) Namen unverändert.
18. Spielerprofil, HeadsUp, HomeStats, Data-Page, Sitemap und
    `/api/v1/tournaments` zeigen das vorläufige Turnier nicht;
    `/api/v1/tournaments` antwortet `reason:'provisional'`.
19. Der Extraktor gibt aus `Player` nur `id`, `memberid`, `name`,
    `firstname`, `club`, `country` aus; der Wächter-Test schlägt bei jeder
    weiteren Spalte und bei `payment|log|message` an. Kein Logeintrag
    enthält Body oder Dateiinhalt.
20. Aufräumen: `<GUID>.tp`, `.json` und `.lock` älter als 14 Tage werden
    gelöscht; nach dem Endimport ist `.tp` sofort weg, die Meta antwortet am
    Status-Endpunkt noch bis zum Aufräumen.

## Tests

**bts-light** (Rust-Unit-Tests, TDD):

- `config.rs`: Roundtrip `TpUploadConfig`; `tp_datei_fuer_turnier` nur bei
  gleicher GUID; alte `config.json` ohne Block lädt.
- `btp/write_gate.rs` (`tokio::test(start_paused = true)`, eigene Instanz):
  Kopie wartet auf laufenden Write; Write in der Ruhezeit startet erst nach
  der Kopie; `max_wartezeit` überschritten → `GateFehler::Beschaeftigt`;
  Zeitstempel nach Drop.
- `tablet/server.rs`: die vier Write-Funktionen halten die Sperre (blockierender
  Mock-BTP nach dem Muster in `btp/client.rs`).
- `badhub/tp_upload.rs`: `lese_stabil` (gleich → Ok; Änderung dazwischen per
  `tempfile` → `Unruhig`; falscher Kopf → `KeineTpDatei`; > 32 MB → `ZuGross`);
  `tp_upload_url` für Prod und Testsystem; Mock-Server (Muster
  `spawn_http_mock`): 202/200 `unchanged`/200 `rejected`/409/429/403/401/
  404/400/500/Basic-Auth-Wand → je eigene `UploadAntwort`; Body-Präfix
  `1f 8b`; Header-Satz inkl. percent-kodiertem `X-File-Name`.
- `commands.rs`: `pruefe_upload_bedingungen` für alle sechs Verletzungen.
- `npm run build`; `node scripts/test-handbuch-fakten.mjs`.
- Manuell (Feldtest): Upload gegen Prod mit der Messdatei; Doppelklick;
  Upload während laufender Auto-Vergabe; Mock 404.

**badhub:**

- `ops/tp_extract/test_tp_extract.py`: Rohdatei erkannt; Container weiter
  erkannt; `matches` für Einzel, Doppel, w.o., aufgegeben, offen; fehlende
  `Country`-Zeile → `null`; `--json`; kaputte Pflichtspalte → Exit 2.
- `tests/unit/tp_extract_test.php` (Wächter-Grep erweitert),
  `tests/unit/tp_live_lib_test.php` (Entscheidungsmatrix, Pfade, Meta,
  Plausibilität, Aufräumen), `tests/unit/turnier_live_verschmelzung_test.php`
  (Fälle a–f).
- Feature-Test durch den Kernel für `/api/tp-upload` (Kriterium 12).
- Integrationstests: Worker auf Spool-Attrappe (13, 14), Endimport über
  `tp_live` (15), Golden-Test der Turnierseite um einen `tp_live`-Fall (16),
  Konsumenten-Filter (18).
- Playwright-Smoke `turnier-spiele.spec.ts` um Banner und Marke.
- `curl`-Probelauf gegen Prod mit der Messdatei vor dem bts-light-PR.

## Risiken & Rollback

- **Zerrissene Kopie trotz Gate** (Eingaben in BTP sind unsichtbar): Byte-Vergleich
  im Client, Jet-Kopf, Parser und 50-%-Regel im Server; schlimmstenfalls bleibt
  der letzte gute Stand stehen und der Turnierleiter drückt erneut. Messung A2
  steht aus (siehe unten).
- **Jet-Schreibcache:** Die Datei auf Platte kann hinter dem Stand in BTP
  liegen. Folge: ein Ergebnis fehlt in der Kopie, bleibt aber über
  Verschmelzungsregel 1 aus dem Liveticker sichtbar.
- **Datenschutz:** Die ganze Datenbank reist. Mitigation: TLS, Bearer, Spool
  außerhalb des Webroots, Löschung nach Endimport oder 14 Tagen, Extraktor mit
  Spaltenpositivliste und Wächter, kein Body-Logging, Security-Review. Die
  Lizenzfrage aus badhub-ADR 0047 (Weitergabe von TP-Daten) wird mit dem
  Rollout an jeden Turnierleiter real und ist **vor dem Feldtest** zu klären;
  das ist keine Technikfrage. **Geprüft 28.10.2026** (badhub
  `docs/drafts/tp-datei-weitergabe-einschaetzung.md`, Nachtrag ADR-0047): Der
  vorliegende EULA schränkt die Datei nicht ein; offen bleiben die Bestätigung
  durch Visual Reality, ein Freigabe-Satz des Veranstalters und die Ergänzung
  von AVV § 13.2 und Datenschutzerklärung. Für BVBB-Turniere vertretbar, für
  andere Verbände erst nach der AVV-Ergänzung.
- **Unveröffentlichte Auslosungen** werden sichtbar: Hinweistext am Knopf und
  im Handbuch; bewusst so entschieden.
- **Doppelter Master** (Turnier B in `multi-hall.md`): beide dürfen hochladen;
  zwei Kopien sind praktisch nie byteidentisch, also greifen der 60-s-Cooldown
  (429) und `flock` je GUID, nicht die Dedupe.
- **Andere Datei in BTP geöffnet:** Zeigt der Pfad auf eine Datei, die BTP
  nicht mehr bearbeitet, liefert der Upload denselben Stand wie zuvor und
  badhub meldet „unverändert" — was stimmt. Die falsche Datei (andere GUID)
  fängt der Server. Satz dazu im Handbuch.
- **Rollback badhub:** Route raus, Timer aus, Spool leeren, `DELETE FROM
  tournaments WHERE source='tp_live'` (CASCADE räumt Kinder). Migration bleibt
  (additiv). **Rollback bts-light:** Revert; ältere Version verwirft den
  Config-Block beim Speichern (Pfad neu wählen).

## Offene Fragen / Annahmen

- **Messung A2 (Pflicht vor Freigabe des Feldtests):** Kopie während
  laufender BTP-Eingaben (Ergebnis, Feldvergabe, Nachschub-Flush) zehnmal
  `lese_stabil`, Kopien mit `access-parser` parsen, Zeilenzahlen vergleichen.
- **Jet-Nachlauf** nach bestätigtem `SENDUPDATE`: Datei sekündlich hashen bis
  zur Ruhe. Bestimmt `RUHE`; bis dahin 3 s.
- **Datei ↔ xlsx** (badhub PR B1, Schritt 4): an einem Lieferordner mit
  `.TPBACKUP` und `exportMatchData` die Extraktor-JSON gegen `readXlsx()`
  vergleichen (`PlayerMatch.id` ↔ `MatchID`, `roundnr` ↔ `round`, Namen,
  Sätze, w.o., Club-Code, Nation); Spaltenabbildung danach festziehen.
- **Elternzugänge = Verbandsslug?** Vor PR B3 auf Prod prüfen (`SELECT
  tournament_key FROM liveticker_tournaments WHERE parent_key IS NULL AND
  is_active = 1`). Weicht einer ab, bekommt die Kürzel-Regel eine
  Admin-Zuordnung; erst dann eine Nutzerfrage.
- **Wann setzt BTP `LinkCode`?** Ungeprüft (Probe an einer Datei vor der
  turnier.de-Verknüpfung). Für das Verhalten unerheblich: leer wird abgelehnt.
- **nginx-Body-Limit** am Präfix-Block der Laravel-Routen auf Prod und Test:
  Prüfung per `curl` mit 2 MB Zufallsbytes (Kriterium 12). Eintrag in
  `verify_vhost_parity.php` nur, falls ein eigener `location` nötig wird.
- **Basic-Auth-Ausnahme** für `/api/tp-upload` auf `test.badhub.de`: Kollege.
- **UNC-Pfad** (Datei auf einem Netzlaufwerk): SMB-Konsistenz ungeprüft, nur
  Hinweis im Handbuch, kein Verbot.
- ~~Setzplätze ohne `stageentry`~~ **erledigt 26.10.2026:** Die „leeren"
  Tabellen waren ein Artefakt der ungereparierten `parse_table()` (Null-Bitmap,
  badhub-README des Extraktors); mit dem Extraktor-Leser liefert dieselbe Datei
  79 gesetzte Meldungen. Kein Handlungsbedarf.

## Betroffene Doku-Dateien

bts-light: `docs/tp-datei-upload.md` (neu, Handbuch, mit `pruef`-Ankern und
Eintrag in `docs/handbuch.json`) · `docs/einstellungen.md` ·
`docs/erste-schritte.md` · `docs/oberflaechen.md` · `docs/multi-hall.md` („Nur
der Master lädt hoch") · `docs/btp_protocol.md` (Schreib-Gate) ·
`docs/features/testsystem-umschalter.md` (Upload-URL folgt `badhub.url`) ·
`docs/changelog.md` · `docs/roadmap.md` · `CLAUDE.md` (Zeile unten) · ADR 0060.

badhub: `docs/features/tournament_importer.md` (Abschnitt „Vorläufige Turniere
aus der TP-Datei", Live-Adapter-Absatz anpassen) · `docs/features/liveticker_bts.md`
(Endpunkt-Vertrag) · `docs/ops/hetzner_setup.md` (Spool-Verzeichnis, Timer,
Rechte) · `docs/features/cron_jobs.md` · `ops/tp_extract/README.md` · ADR 0039
(Nachtrag) · ADR 0051 · ADR 0052 · `resources/changelog/changelog.json` ·
Datenschutz-Absatz in `CLAUDE.md`.

CLAUDE.md-Zeile (bts-light):

| Code-Pfad | Doku-Datei |
|---|---|
| **TP-Datei an badhub** (`config.rs` `TpUploadConfig`/`tp_datei_fuer_turnier`, `btp/write_gate.rs` `WriteGate`/`GATE` + Sperre in den vier `write_*_to_btp` in `tablet/server.rs`, `badhub/tp_upload.rs` `lese_stabil`/`tp_upload_url`/`sende`, `commands.rs` `tp_upload_now`/`tp_upload_status`/`tp_file_check`/`pruefe_upload_bedingungen` + `AppState.tp_upload`, Abschnitt „TP-Datei" im `SetupWizard`, Abschnitt „Turnierdaten an badhub" in `pages/Dashboard.tsx`; badhub: `/api/tp-upload`, `cron/import_tp_live.php`, `LiveVerschmelzung`) | `docs/tp-datei-upload.md` (Bedienung) · `docs/features/tp-datei-upload.md` (Spec) · `docs/einstellungen.md` · `docs/multi-hall.md` · `docs/btp_protocol.md` (Messung, Schreib-Gate) · ADR 0060 |

## Umsetzungs-Hinweise

**Reihenfolge:** badhub zuerst, denn ohne Endpunkt bekommt bts-light nur 404
und das Testsystem ist für Bearer-Routen gesperrt. Jeder badhub-Schritt ist
für sich deploybar und ändert nichts, solange keine Datei hochgeladen wird.
Deploy durch den Kollegen; Migration 246 vor B3.

1. **badhub PR B1 — Extraktor:** Tests zuerst (`test_tp_extract.py`), Rohmodus,
   `--json`, Ausgaben `tournament`/`days`/`matches`, `PLAYER_SPALTEN += club,
   country`, Schema +1; `lib/tp_extract.php` mit Fixture-JSON; Prüfpunkt
   Datei ↔ xlsx an einem Lieferordner; README und ADR-0039-Nachtrag. Kein
   Produktiveffekt.
2. **badhub PR B2 — Schema:** Migration 246 (Testlauf auf der Prod-Kopie wie
   bei 154); Importer setzen `source='export'`/`final_imported_at`;
   Integrationstest „Endimport ersetzt `tp_live` in place"; Konsumenten-Filter
   (noch wirkungslos); ADR 0051.
3. **badhub PR B3 — Endpunkt, Spool, Worker:** Tests zuerst für
   `tp_live_lib.php`; Controller; Worker mit `--once --guid=`; Timer-Unit und
   Registry; Doku; ADR 0052; **Abnahme** per `curl` gegen Prod mit der
   Messdatei (gzip, Bearer, Header), danach Status-Abruf und Turnierseite.
4. **badhub PR B4 — Turnierseite:** Tests zuerst für `LiveVerschmelzung`;
   Cache 60 s, Tab, Verschmelzung, Banner und Marken; Golden-Test und
   Playwright-Smoke; ADR-0051-Nachtrag „Verschmelzung beim Rendern".
5. **bts-light PR L1** in dieser Reihenfolge, jeweils Test zuerst: `config.rs`
   → `btp/write_gate.rs` → Sperre in den vier Write-Funktionen →
   `badhub/tp_upload.rs` → `pruefe_upload_bedingungen` → Commands und
   `AppState` → Frontend (`types.ts`, `api.ts`, Setup-Abschnitt,
   Dashboard-Abschnitt) → Handbuch und Anker → Spec-Status, ADR 0060,
   CLAUDE.md-Zeile, Changelog, Roadmap, Version. Subagents: code-reviewer
   nach jeder Code-Änderung; **security-reviewer** für `tp_file_check`/
   `lese_stabil` (Pfad aus dem Dialog, Größendeckel, kein Inhalt im Log) und
   für den Log-Satz.
6. **bts-light PR L2:** eigene Spec [tp-takt-upload.md](tp-takt-upload.md)
   (28.10.2026). Abweichend von der ersten Skizze hier: `auto_enabled` +
   `auto_minutes` (Minimum 5, Standard 15) statt `Option<u16>`, Änderungs-
   erkennung per ungegatetem SHA-256 statt `mtime` + Größe, Takt nur solange
   der Liveticker läuft, Anhalte-Regel mit Wiederanlauf (ADR 0061).

**Security-Review in badhub** zwingend für `TpUploadController` (untrusted
Body, `gzdecode`-Deckel, Header-Parsing, Pfad nur aus normalisierter GUID,
Cooldown, 409/429), `tp_extract.py` im Rohmodus (untrusted Jet-Datei:
`timeout 120`, `-I`, `ulimit -v`, Wächter) und die Konsumenten-Filter
(Datenleck „Zwischenstände in Profilen").

**Nicht vergessen:** Upload-URL aus `badhub.url` ableiten, nicht fest über
`badhub_host::api_url`; der Merker „letzter Upload" gehört in `AppState`, nicht
in die `SyncEngine`; Anonymisierung bleibt beim Rendern; `gzip_compress` in
`btp/wire.rs` wird `pub(crate)` statt dupliziert.
