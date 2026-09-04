# Mehrere Liveticker je Verband — Spezifikation

> Status: **freigegeben 04.09.2026** (Brainstorming im Dialog: Problem →
> Kennungs-Frage → drei Ansätze → Entwurf in sechs Abschnitten → Freigabe).
> Quelle: Nutzer-Frage vom 04.09.2026 („zwei BVBB-Turniere am selben
> Wochenende"). Betroffene Repos: **badhub** (Hauptteil: Schreibpfad,
> Lesepfad, Admin, Migration) und **bts-light** (`src-tauri` Config + Payload,
> `src/` Setup, Aushang). ADR: [0054](../adr/0054-liveticker-kind-turnier-je-guid.md).

## Kontext / Problem

Das Preset „BVBB" in bts-light trägt ein **verbandsweites** Liveticker-Passwort.
badhub leitet daraus den Schlüssel `bvbb` ab, und die Tabelle `liveticker_state`
hält genau **eine Zeile je Schlüssel** (`tournament_key` ist Primärschlüssel).
Laufen zwei Turniere desselben Verbands parallel, überschreiben sich ihre
Installationen gegenseitig: Jeder volle Stand (`tset`, alle 60 s) ersetzt den
des anderen Turniers, jedes Punkt-Update (`tupdate_match`) wird in den Stand
gemischt, der gerade drin liegt. Auf `badhub.de/live?t=bvbb` flackert dann mal
das eine, mal das andere Turnier.

Am selben Schlüssel hängen außerdem der Spieler-Index (Spielerseiten
`/spieler/<Nr>/live`), der Zeitplan (`sched`), der Check-In-Bezug der
Sponsor-Leiste (trifft immer das *zuletzt bepushte* Check-In-Turnier des
Schlüssels) und der Turnierleitungs-PIN des Hallen-Check-Ins.

**Nicht betroffen** und bereits je Installation eindeutig: Tablets, TL-Web,
Cloud-Monitore und Diagnose-Logs (`install_id`, R4/R6) sowie die
Check-In-Meldeliste selbst (adressiert über die turnier.de-GUID, ADR 0009).

badhub ist seit April 2026 mandantenfähig (Migration 105): Ein Admin kann je
Turnier einen eigenen Zugang mit eigenem Passwort anlegen, und die Live-Seite
zeigt bei mehreren aktiven Schlüsseln eine Auswahl. Das löst das Problem heute
— aber nur mit Handarbeit vor **jedem** Turnierwochenende. Parallele Turniere
sind im Verbandsalltag der Normalfall, nicht die Ausnahme.

## Zielbild & Erfolgskriterien

Zwei (oder mehr) bts-light-Installationen desselben Verbands laufen mit dem
**unveränderten Preset** parallel, ohne dass ein Admin vorher etwas anlegt.
badhub erkennt das Turnier an seiner **turnier.de-GUID**, die in bts-light zum
Pflichtfeld wird, und führt jedes Turnier unter dem Verbandszugang als eigenes
**Kind-Turnier**. Die Live-Seite zeigt bei einem laufenden Turnier direkt
dieses, bei mehreren die vorhandene Auswahl; Aushang und Dashboard verlinken
direkt auf das eigene Turnier.

**Erfolgskriterien**

- Zwei Installationen mit dem Preset „BVBB" und verschiedenen GUIDs pushen
  eine Stunde parallel; beide Stände sind auf `badhub.de/live` getrennt und
  vollständig sichtbar, keine Spielzeile des einen Turniers erscheint im anderen.
- `badhub.de/live?t=bvbb` zeigt die Auswahl, `…?t=bvbb&g=<GUID>` direkt das
  Turnier. Der QR-Code des Aushangs führt ohne Zwischenschritt zum eigenen Turnier.
- Ein Push **ohne** GUID (alter Client) verhält sich exakt wie heute.
- Kein Admin-Eingriff nötig; die Admin-Liste zeigt die Kinder unter ihrem Verband.
- Kein Lesepfad in badhub muss die GUID neu durchreichen.

## Nicht-Ziele

- **Kein eigener Check-In-PIN je Kind.** Der Turnierleitungs-PIN
  (`liveticker_tournaments.tl_pin_hash`) bleibt am Elternzugang; parallele
  Turniere eines Verbands teilen ihn. Bewusst offen gelassen als Folgeschritt.
- **Keine Änderung der Auth.** Das Verbandspasswort bleibt der einzige
  Berechtigungsnachweis; Kinder haben kein eigenes Passwort. Kein
  Bereitstellungs-Endpunkt, keine Geheimnisse in der App (Ansatz C, verworfen).
- **Kein Umbau der Zustandstabellen** auf zusammengesetzte Schlüssel (Ansatz A,
  verworfen — siehe ADR 0054).
- **Keine Zusammenführung alter Sender.** letilo/bts und bts-light-Versionen
  vor dieser Spec pushen ohne GUID auf den Elternschlüssel und kollidieren
  untereinander weiter wie heute.
- **Kein Verbands-Check der GUID.** badhub prüft nicht, ob die GUID zu einem
  Turnier des Verbands gehört; jede wohlgeformte GUID erzeugt ein Kind unter
  dem Zugang, dessen Passwort der Push trägt.

## Betroffene Komponenten / Architekturregeln / Daten

### badhub

| Baustein | Änderung |
|---|---|
| `db/migrations/198_liveticker_kind_turniere.sql` | `liveticker_tournaments`: `parent_key VARCHAR(64) NULL`, `tournament_uuid CHAR(36) NULL`, `UNIQUE KEY uq_parent_uuid (parent_key, tournament_uuid)`, Index auf `parent_key`. |
| `lib/live_update_lib.php` | Neu `liveUpdateKindAufloesen(PDO, string $elternKey, array $msg): string` — liefert den Kindschlüssel (legt das Kind bei Bedarf an) oder den Elternschlüssel, wenn keine wohlgeformte GUID in der Nachricht steht. Neu `liveUpdateKinderAufraeumen(PDO, string $elternKey)`. |
| `app/Http/Controllers/Api/LiveUpdateController.php` | Ruft nach `liveUpdateAuth()` die Auflösung auf und übergibt den Kindschlüssel an `liveUpdatePersist()`. |
| `app/Http/Controllers/Api/LivetickerController.php` | `liveQjson`: `?t=` mit frischen Kindern → eines: dessen Stand; mehrere: `multiple_active`. Neuer Parameter `?g=<GUID>` → Kind direkt. `liveTournaments`: optional `?parent=<key>` filtert auf Kinder eines Verbands; Antwortzeile bekommt `parent_key` und `tournament_uuid`. |
| `public/assets/js/live.js` | `showPicker()` übergibt bei gesetztem `?t=` den Schlüssel als `parent`; `renderPicker()` verlinkt Kinder mit `?t=<kindschlüssel>`. `g` aus der URL wird an `live_qjson.php` durchgereicht. |
| `app/Http/Controllers/Admin/LivetickerController.php` + View | Kinder eingerückt unter dem Verband (GUID, Name, letzter Push). Aktionen am Kind: Name pinnen, löschen. Passwort/Aktiv/Gültigkeit nur am Elternzugang. |
| `lib/checkin_tournament_lookup.php` | unverändert — arbeitet je Schlüssel; mit Kindschlüssel eindeutig. |
| `checkin_pin_authorize()` | Kind hat keinen `tl_pin_hash` → Rückfall auf den des `parent_key` (ein `COALESCE` über einen Self-Join). |

### bts-light

| Baustein | Änderung |
|---|---|
| `src-tauri/src/config.rs` | Neu `Config.tournament_uuid: String` (Pflicht, GUID-Form). `CheckinConfig.tournament_uuid` bleibt als Migrationsquelle: `load_from` übernimmt einen bestehenden Wert einmalig, wenn das neue Feld leer ist. `CheckinConfig::is_ready()` liest den neuen Wert. |
| `src-tauri/src/badhub/payload.rs` | `tournament_uuid` in `TsetMessage`, `TupdateMessage`, `SchedMessage`, `CheckinBrandingMessage` (bei `CheckinRosterMessage` vorhanden). |
| `src-tauri/src/sync.rs` | Startprüfung: ohne gültige GUID kein Sync-Start (Fehlertext wie bei fehlendem Badhub-Passwort). GUID in alle Builder injizieren. |
| `src-tauri/src/commands.rs` | `start_sync` verweigert ohne GUID; `spawn_branding_push` trägt die GUID; Dashboard-Live-Link und `match_live_url`-artige Ableitungen hängen `&g=<GUID>` an. |
| `src-tauri/src/aushang.rs` | `daten_aus` bekommt die GUID; beide QR-Codes zeigen auf `<live_url>&g=<GUID>`. |
| `src/pages/SetupWizard.tsx` | GUID-Feld wandert aus dem Check-In-Abschnitt in den Turnier-Abschnitt, Pflicht; Einfügen der ganzen turnier.de-Adresse via `extractTournamentGuid` wie heute. „Verbindung starten" gesperrt ohne gültige GUID. |
| `src/tournamentGuid.ts` | unverändert. |
| Doku | `docs/aushang.md` (Grenze „zeigt auf den Verband" entfällt), `docs/spieler-check-in.md` (GUID-Ablage), `docs/btp_protocol.md`/Liveticker-Abschnitt (neues Feld), `docs/changelog.md`, badhub `docs/features/liveticker_bts.md`. |

### Architekturregeln

- **R2** unberührt: BTP bleibt die Wahrheit; die GUID ist eine Konfigurationsangabe.
- **R6** unberührt: `install_id` bleibt Relay-Namespace und Log-Kennung. Sie
  kommt **nicht** in den Push und **nicht** in URLs (sie ist zugleich das
  Cloud-Token, ADR 0006).
- **ADR 0009** wird bestätigt: Die turnier.de-GUID ist die Turnier-Identität
  gegenüber badhub, nun nicht mehr nur für den Check-In.

## Verhalten im Detail

### Kindschlüssel

`kind = eltern + "-" + lower(hex(sha256(upper(GUID))))[0..8]`, z. B.
`bvbb-3f9a2c1d`. Deterministisch, damit zwei gleichzeitige erste Pushs
dieselbe Zeile treffen (`INSERT IGNORE` auf den Unique-Index) und damit der
Schlüssel notfalls clientseitig ableitbar wäre. 8 Hex-Zeichen (32 Bit) genügen
für die Größenordnung „Turniere je Verband und Jahrzehnt"; eine Kollision
träfe zwei Turniere desselben Verbands mit derselben Kurzform und würde
über den Unique-Index auf (`parent_key`, `tournament_uuid`) als Fehler
sichtbar, nicht still.

GUID-Normalisierung wie in bts-light (`is_tournament_uuid`): Leerzeichen und
`{…}` entfernen, Großschreibung.

### Schreibpfad

1. `liveUpdateAuth()` liefert wie heute den Schlüssel des Zugangs, dessen
   Passwort passt (`eltern`).
2. Trägt `$msg['tournament_uuid']` (bei `tset`/`sched`: auch
   `$msg['event']['tournament_uuid']` akzeptieren) eine wohlgeformte GUID:
   Kind suchen, sonst anlegen mit `tournament_key = kind`, `parent_key =
   eltern`, `tournament_uuid`, `display_name` = Turniername aus der Nachricht
   (Rückfall: Elternname), `password_hash NULL`, `is_legacy 0`,
   `is_active 1`, `valid_from/valid_until NULL`, `created_by_admin_id NULL`.
   Ergebnis: `kind`.
3. Sonst: Ergebnis `eltern` (Verhalten wie heute).
4. `liveUpdatePersist()` läuft **unverändert** mit dem Ergebnis als
   `$matchedTournament`. Die bestehende Namensübernahme aus dem `tset`
   (`name_locked` respektierend) aktualisiert damit den Kindnamen.
5. Nach einem erfolgreichen `tset` einmal je Anfrage:
   `liveUpdateKinderAufraeumen(eltern)` löscht Kinder dieses Verbands, deren
   `liveticker_state.updated_at` älter als **30 Tage** ist (oder die keinen
   Stand haben und älter als 30 Tage sind), samt `liveticker_state`,
   `liveticker_player_index`, `liveticker_schedule` und der Zuordnung in
   `checkin_tournaments` (`tournament_key` zurück auf `eltern`, damit die
   Check-In-Seite des vergangenen Turniers erreichbar bleibt). Muster: die
   48-h-Bereinigung des Spieler-Index; kein Cron.

Die Sperre eines Elternzugangs (`is_active 0`, abgelaufenes Fenster) wirkt
über die Auth: kein Push kommt durch, das Kind wird nach
`LIVETICKER_HIDE_AFTER_SECONDS` unsichtbar. Deshalb brauchen Lesepfade keinen
Blick auf den Elterndatensatz.

### Lesepfad

- `live_qjson.php?t=<key>`: Ist `<key>` ein Elternzugang mit **frischen**
  Kindern (Push jünger als `LIVETICKER_HIDE_AFTER_SECONDS`): genau eines →
  dessen Stand liefern (Antwort trägt zusätzlich `tournament_key` des Kinds);
  mehrere → `multiple_active` wie heute. Keine frischen Kinder → Elternstand
  wie heute (alte Sender).
- `live_qjson.php?t=<key>&g=<GUID>`: Kind zu (`key`, GUID) auflösen; unbekannt
  → `tournament_not_found`.
- `live_qjson.php?t=<kindschlüssel>`: funktioniert wie jeder Schlüssel heute.
- `live_tournaments.php?parent=<key>`: nur Kinder dieses Verbands (frisch,
  wie heute gefiltert). Ohne `parent`: wie heute alle frischen Schlüssel —
  Kinder **und** Eltern mit frischem Stand, damit alte Sender sichtbar bleiben.
- Live-Seite: bei `multiple_active` ruft `showPicker()` die Liste mit
  `parent=<t>` ab, wenn `?t=` gesetzt ist; die Auswahl verlinkt
  `?t=<kindschlüssel>`. Ohne `?t=`: wie heute.
- Spielerseiten, Teilnehmerlisten (`/live/{key}/teilnehmer`), Badge,
  Check-In-Bezug der Sponsor-Leiste: unverändert, weil je Schlüssel.

### Admin-Liste

Kinder erscheinen eingerückt unter ihrem Elternzugang mit GUID, Name, letztem
Push und Link auf `?t=<kindschlüssel>`. Am Kind erlaubt: `edit` (Name pinnen)
und `delete` (löscht wie oben samt Zuständen). `create` ignoriert
Kind-Felder; `rotate_password`, `toggle_active`, `send_password` lehnen
Kindschlüssel mit 400 ab. Zählung „aktive Turniere" nimmt Kinder mit.

### bts-light

- **Pflicht:** `start_sync` verweigert den Start ohne gültige GUID mit einem
  klaren Text („Turnier-GUID von turnier.de fehlt — im Setup unter *Turnier*
  eintragen"). Das Setup sperrt den Start-Knopf analog.
- **Ablage:** `Config.tournament_uuid` (oberste Ebene). Beim Laden einer
  Config ohne dieses Feld, aber mit `checkin.tournament_uuid`, wird der Wert
  übernommen und beim nächsten Speichern persistiert. `checkin.tournament_uuid`
  bleibt im Schema (Abwärtskompatibilität älterer Versionen beim Rückrollen),
  wird aber nicht mehr angezeigt.
- **Payload:** `tournament_uuid` (Großschreibung, ohne Klammern) auf oberster
  Ebene in `tset`, `tupdate_match`, `sched`, `centry_list` (vorhanden) und
  `checkin-branding`.
- **Links:** Dashboard-Live-Link und Aushang-QR = `live_url` + `&g=<GUID>`
  (bzw. `?g=` wenn `live_url` noch keinen Query-String hat). Die bestehende
  `&display=`-Ergänzung des Dashboards bleibt und wird hinter `g` angehängt.
- **Aushang:** Kürzel-Ableitung (`kuerzel_aus_live_url`) unverändert; das
  Blatt zeigt weiterhin das Verbandskürzel, der Code führt zum Turnier.

## Akzeptanzkriterien

1. Push mit GUID `A` unter Passwort `bvbb` → Zeile `bvbb-<h(A)>` mit
   `parent_key = bvbb`, Stand liegt unter `bvbb-<h(A)>`, **nicht** unter `bvbb`.
2. Zwei Pushs mit GUIDs `A` und `B` im Wechsel → zwei Stände, beide
   vollständig; `live_qjson.php?t=bvbb` liefert `multiple_active`;
   `…&g=A` liefert `A`.
3. Nur `A` frisch (B älter als 30 min) → `?t=bvbb` liefert `A` direkt.
4. Push ohne GUID unter `bvbb` → Stand unter `bvbb`, wie heute; `?t=bvbb`
   ohne frische Kinder liefert ihn.
5. Zwei gleichzeitige erste Pushs mit derselben GUID → genau eine Kindzeile,
   beide Pushs 200.
6. GUID in Klein-/Klammer-Schreibweise → dieselbe Kindzeile wie in Großschreibung.
7. Elternzugang deaktiviert → Push 401, Kind nach 30 min nicht mehr in
   `live_tournaments.php`.
8. Kind mit Stand älter als 30 Tage → nach dem nächsten `tset` desselben
   Verbands gelöscht (Turnier, Stand, Index, Zeitplan); Check-In-Zuordnung
   zeigt wieder auf `bvbb`.
9. `checkin_pin_authorize` für ein Kind akzeptiert den PIN des Elternzugangs.
10. Admin: Kind sichtbar unter Eltern; `rotate_password` auf Kind → 400.
11. bts-light: Config mit nur `checkin.tournament_uuid` lädt mit gefülltem
    `tournament_uuid`; Start ohne GUID → Fehlertext, Sync läuft nicht an.
12. bts-light: `tset`/`tupdate_match`/`sched`/`checkin-branding` enthalten
    `"tournament_uuid":"<GUID>"`; Aushang-SVG-URL endet auf `&g=<GUID>`.

## Tests

**badhub** (Integration über Docker, MariaDB 11.4, Muster
`tests/integration/liveticker_player_index_db_test.php`):
`tests/integration/liveticker_kind_turniere_db_test.php` deckt 1–9 ab;
Idempotenz (5) über zwei Aufrufe von `liveUpdateKindAufloesen` in
verschiedenen Verbindungen. Unit-Test der Schlüsselableitung mit festem
Erwartungswert. Admin (10) über einen HTTP-Test der bestehenden Art.

**bts-light** (`cargo test`, `npm run build`, `scripts/test-*.mjs`):
Serde-Tests in `payload.rs` je Nachricht (12), Lader-Migration in `config.rs`
(11), `aushang.rs`-Test für `&g=`/`?g=`, `sync.rs`-Test „kein Start ohne
GUID". Setup-Gate ist Frontend-Logik ohne Harness → manuell prüfen.

**Feldtest:** zwei Installationen mit Preset „BVBB" und zwei Test-GUIDs auf
`test.badhub.de` (Basic-Auth-Hürde beachten, siehe
[testsystem-umschalter.md](testsystem-umschalter.md)) oder produktiv am
nächsten Doppel-Wochenende.

## Risiken & Rollback

- **Ausrollreihenfolge:** erst badhub, dann bts-light. Ein neuer Client gegen
  altes badhub schickt ein unbekanntes Feld, das ignoriert wird — Verhalten
  wie heute. Ein altes badhub gegen alte Clients: unverändert.
- **Rückrollen badhub:** Kinder bleiben als normale Zeilen ohne Passwort
  stehen; Pushs landen wieder auf dem Elternschlüssel. Kinder von Hand
  löschen oder liegen lassen (unsichtbar nach 30 min).
- **Rückrollen bts-light:** Ältere Version liest `checkin.tournament_uuid`
  weiter (Feld bleibt im Schema), pusht ohne GUID → Elternschlüssel.
- **Falsche GUID** (Tippfehler = GUID eines vergangenen Turniers): dessen
  Ticker wird still überschrieben. Akzeptiert — das Turnier ist vorbei, und
  das Setup zieht die GUID ohnehin aus der eingefügten turnier.de-Adresse.
- **Pflichtfeld für alle:** auch Turniere außerhalb turnier.de (Preset
  „Eigenes Turnier") müssen eine GUID eintragen. Annahme: praktisch alle
  Verbandsturniere laufen über turnier.de. Wer keine hat, kann jede
  wohlgeformte GUID verwenden — badhub prüft die Herkunft nicht.
- **Admin-Liste wächst** um ein Kind je Turnier; die 30-Tage-Bereinigung hält
  sie klein.

## Offene Fragen / Annahmen

- Check-In-PIN je Kind: Folgeschritt, nicht Teil dieser Spec.
- Ob `live_tournaments.php` ohne `parent` Eltern mit frischem Stand **und**
  Kinder zeigt (angenommen: ja), entscheidet sich am Verhalten alter Sender —
  bei Zweifel im Feld nachmessen.
- Deploy von badhub erfolgt nicht von diesem Rechner
  (siehe Memory „badhub deployen").

## Betroffene Doku-Dateien

bts-light: `docs/aushang.md`, `docs/spieler-check-in.md`, `docs/btp_protocol.md`
(Liveticker-Nachrichten), `docs/changelog.md`, `docs/roadmap.md`, `CLAUDE.md`
(Tabellenzeile). badhub: `docs/features/liveticker_bts.md` (Abschnitt
Multi-Tenant + Admin-Workflow), `features/hallen_checkin.md` (PIN-Erbe).

## Umsetzungs-Hinweise

Reihenfolge: (1) badhub-Migration + Auflösung + Tests, (2) badhub-Lesepfad +
Live-Seite, (3) badhub-Admin, (4) badhub-Deploy, (5) bts-light Config +
Payload + Tests, (6) bts-light Setup + Links + Aushang, (7) Doku + Version.
Schritte 1–3 und 5–6 sind je Repo ein PR; bts-light erst nach dem
badhub-Deploy taggen.
