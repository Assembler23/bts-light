# TP-Datei selbst finden — Spezifikation

> Status: **Entwurf 15.11.2026, wartet auf Freigabe** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Idee vom 15.11.2026. Betroffene Crates: `src-tauri`, `src`. Nur bts-light, kein Relay, kein badhub.
> Baut auf [tp-datei-upload.md](tp-datei-upload.md) und [tp-takt-upload.md](tp-takt-upload.md) auf.
> ADR: [0096](../adr/0096-tp-pfad-aus-den-handles-von-btp.md) (Handles),
> [0097](../adr/0097-tp-automatik-kennung-nur-bei-gleichheit-oder-leer.md) (Kennung),
> [0098](../adr/0098-tp-automatik-schreibpfad-mit-schreibzaehler.md) (Schreibpfad).

## Kontext / Problem

Für den TP-Upload muss der Turnierleiter die in BTP geöffnete `.TP`-Datei im
Dateidialog suchen („meist unter *Dokumente* oder dort, wo du das Turnier
angelegt hast"). Er muss also wissen, wo BTP die Datei abgelegt hat — bei
OneDrive-Ordnern, Sicherungskopien und Vorjahresdateien eine echte
Fehlerquelle für eine Zielgruppe ohne technischen Hintergrund.

BTP selbst weiß den Pfad. Gemessen am 15.11.2026 mit
`src-tauri/tests/btp_tp_pfad_probe.rs` (Ergebnisse in
[btp_protocol.md](../btp_protocol.md), Punkt „Pfad aus dem Prozess"):

- Die offenen Datei-Handles von `btp.exe` liefern genau eine `.tp` mit vollem
  Pfad, in rund 70 ms, ohne Hänger — gleich ob per Doppelklick oder über
  „Datei → Öffnen" geöffnet.
- Läuft BTP als Administrator, verweigert Windows den Zugriff sofort und
  eindeutig (Fehler 5).
- BTP lässt sich nur einmal starten: höchstens eine offene `.tp`.
- Der Fenstertitel nennt den Turniernamen, die Kommandozeile ist nach
  „Datei → Öffnen" leer — beide taugen nicht.

## Zielbild & Erfolgskriterien

bts-light trägt die TP-Datei im Normalfall ohne Zutun ein und hält sie
aktuell. Der Dateidialog bleibt für die Fälle, in denen die Suche nichts
liefern kann.

- **E1** Frische Installation, BTP lokal mit geöffnetem Turnier: Nach dem
  Öffnen des Setups stehen Datei **und** Turnier-Kennung da, ohne Dateidialog
  und ohne Abtippen.
- **E2** Neues Turnier bei alter Kennung: höchstens **ein** Bestätigungsklick.
- **E3** Im laufenden Turnier ändert sich die Kennung nie ohne Bestätigung.
- **E4** Wo die Suche scheitert, sagt das Setup in einem Satz warum.

## Nicht-Ziele

- Suche, wenn BTP auf einem anderen PC läuft oder bts-light Slave ist.
- Umgehen der Zugriffssperre, wenn BTP als Administrator läuft.
- Suche unter Linux oder macOS.
- Suche nach `.TP`-Dateien auf der Festplatte („Zuletzt verwendet",
  Verzeichnis-Scan).
- Änderungen am Upload selbst, am Schreib-Gate oder an badhub.
- Erkennen eines ausgetauschten Inhalts unter gleichem Pfad — das fängt
  weiter der Halt `GuidFremd` des Uploads ab.

## Verhalten

### Wann gesucht wird

Nur wenn **alle** drei gelten: Schalter „TP-Datei automatisch finden" ist an,
die BTP-Adresse zeigt auf diesen PC (`127.0.0.0/8`, `localhost`, `::1`),
bts-light ist nicht Slave.

| Zeitpunkt | PR |
|---|---|
| Öffnen des Setups | 1 |
| Knopf „In BTP geöffnete Datei suchen" (sucht auch bei ausgeschaltetem Schalter) | 1 |
| Start von bts-light | 2 |
| Start der Übertragung und danach alle 60 s | 2 |

Es läuft höchstens eine Suche gleichzeitig. Der 60-s-Takt setzt aus, solange
ein TP-Upload läuft; der Knopf sucht trotzdem (die Suche nimmt das
Schreib-Gate nie).

### Was mit einem Fund geschieht

Ein Fund zählt nur, wenn die Datei die bestehende Prüfung besteht (lesbar,
Jet-Kopf, höchstens 32 MB). Die volle Lesung für die Kennung läuft nur, wenn
der gefundene Pfad vom eingetragenen abweicht oder die Bindung fehlt — nicht
in jedem Takt.

| Kennung der Datei gegenüber der eingetragenen | Aktion |
|---|---|
| gleich | Pfad still übernehmen und binden |
| keine eingetragen | Pfad **und** Kennung still übernehmen |
| weicht ab | Rückfrage |
| Datei ohne lesbare Kennung | Rückfrage; Ja bindet nur den Pfad, die eingetragene Kennung bleibt |

Eine Kennung wird nie geleert.

### Rückfrage

- Text sinngemäß wie heute: „‚<Dateiname>' gehört zu Turnier <kurz>, oben ist
  <kurz> eingetragen. Kennung aus der Datei übernehmen?" Läuft die
  Übertragung, mit dem Zusatz, dass sie dabei kurz neu startet.
- **Ja:** Datei und Kennung werden übernommen. Läuft die Übertragung, startet
  bts-light sie selbst neu (vorhandener Weg Stop → Start).
- **Nein:** Nichts ändert sich, der Schalter „TP-Datei automatisch finden"
  geht aus.
- Die Frage wird je Fund (Pfad + Kennung) einmal gestellt. Solange sie offen
  ist, ändert sich nichts; der Upload hält bei fremder Datei wie bisher an.
- Öffnet BTP wieder eine Datei mit der eingetragenen Kennung, wird eine
  offene Frage zurückgezogen.
- Nach einem Neustart von bts-light wird eine unbeantwortete Frage erneut
  gestellt.
- Im Setup erscheint die Frage an der heutigen Stelle (`tpFrage`), außerhalb
  als Banner über allen Seiten (PR 2).

### Übersteuern

- „TP-Datei wählen" (Dateidialog) schaltet den Schalter aus. Die Hand-Datei
  gilt, bis der Schalter wieder eingeschaltet wird — dauerhaft, nicht je
  Turnier.
- Der Schalter ist zugleich der Notschalter.

### Meldungen im Setup, wenn die Suche nichts liefert

| Lage | Meldung (sinngemäß) |
|---|---|
| BTP läuft nicht | „BTP läuft nicht auf diesem PC." |
| BTP ohne geöffnetes Turnier | „In BTP ist kein Turnier geöffnet." |
| BTP als Administrator | „BTP läuft als Administrator — bitte die Datei von Hand wählen." |
| Abfrage blockiert (Frist) | „BTP hat nicht geantwortet — bitte die Datei von Hand wählen." |
| mehr als eine `.tp` offen | „Mehrere Turnierdateien offen — bitte die Datei von Hand wählen." |
| Fund fällt in der Prüfung durch | das bestehende Urteil („Das ist keine TP-Datei", „Größer als 32 MB", nicht lesbar) mit Dateiname |
| BTP auf anderem PC / Slave | „BTP läuft auf einem anderen PC — bitte die Datei von Hand wählen." |
| kein Windows | Schalter und Knopf sind ausgeblendet |

Außerhalb des Setups (Takt, PR 2) sind diese Lagen still; sie stehen nur im
Log.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - neu `src-tauri/src/tp_finden/mod.rs` (reine Logik) und
    `tp_finden/windows.rs` (Handle-Durchlauf), `not(windows)`-Zweig nach dem
    Muster `print/mod.rs`
  - `config.rs` `TpUploadConfig`, `commands.rs` (neue Commands, `AppState`,
    `keep_host_managed_fields`, `identity_bundle`/`apply_imported_identity`,
    `start_sync`), `lib.rs`
  - `src/pages/SetupWizard.tsx`, neu `src/io/tpWahl.mjs`, in PR 2
    `components/TpFundBanner.tsx`, `App.tsx`, `pages/Dashboard.tsx`
- **Architekturregeln:** R1 — die Suche ist ein Tauri-Command, React greift
  nicht auf Prozesse zu. R2–R6 unberührt (kein BTP-Protokoll, kein Relay,
  keine Ergebnisse).
- **Konfiguration & Abwärtskompatibilität:** `tp_upload.finden_automatisch`
  (`#[serde(default)]`, Standard **an**) und in PR 2 `tp_upload.stand`
  (Schreibzähler, Standard 0). Eine Bestands-Config ohne die Felder bleibt
  lesbar; Bestandsinstallationen haben die Automatik nach dem Auto-Update an,
  ein vorhandener Pfad wird nur bei gleicher Kennung still nachgezogen.
  `identifier` und Updater-Pfad unangetastet.
- **Datenschutz:** Der Pfad enthält den Windows-Benutzernamen. Im Log steht
  nur der Dateiname, nie der volle Pfad (Logs werden hochgeladen). Das
  Identitäts-Bündel trägt den Pfad nicht. Aus der Datei wird nichts gelesen,
  was nicht schon heute gelesen wird (Kopf, Kennung).
- **Abhängigkeiten:** keine neue Dependency; das vorhandene `windows`-Crate
  0.62 bekommt die Features `Win32_System_Threading`,
  `Win32_System_Diagnostics_ToolHelp`, `Win32_Storage_FileSystem`. Abhängig
  von der BTP-Eigenheit „hält die Datei offen, Prozess heißt `btp.exe`".

## Akzeptanzkriterien

### PR 1 — Suche im Setup

- [ ] **AK1** BTP lokal mit offenem Turnier, Kennung eingetragen und gleich:
  Beim Öffnen des Setups steht der Pfad mit „Turnierdatei erkannt" und
  „Turnier-Kennung passt" da, ohne Dateidialog.
- [ ] **AK2** Wie AK1, aber keine Kennung eingetragen: Pfad und Kennung sind
  gefüllt, mit „Turnier-Kennung aus der Datei übernommen".
- [ ] **AK3** Kennung weicht ab: Es erscheint die Rückfrage; Ja übernimmt
  Datei und Kennung, Nein lässt beides unverändert und schaltet den Schalter
  aus.
- [ ] **AK4** Datei ohne lesbare Kennung: Rückfrage; Ja bindet den Pfad, die
  eingetragene Kennung bleibt.
- [ ] **AK5** Jede Lage der Tabelle „Meldungen im Setup" zeigt ihre Meldung;
  nichts wird eingetragen.
- [ ] **AK6** Gespeichert wird erst mit „Speichern"; Abbrechen des Setups
  verwirft den Fund.
- [ ] **AK7** „TP-Datei wählen" schaltet den Schalter aus; danach sucht das
  Setup beim Öffnen nicht mehr, der Knopf sucht weiterhin.
- [ ] **AK8** Bestands-Config ohne das neue Feld lädt mit Automatik an.
- [ ] **AK9** Der Handle-Durchlauf verwendet nie `DUPLICATE_CLOSE_SOURCE`; ein
  blockierendes Handle wird nach 3 s übersprungen, nach zwei Überspringern
  endet die Suche mit „nicht geantwortet"; ab drei noch blockierten Threads
  startet keine weitere Suche.
- [ ] **AK10** Im Log erscheint zur Suche nur der Dateiname.
- [ ] **AK11** `cargo build` und `cargo test` laufen unter Linux; dort
  liefert die Suche „nicht verfügbar".

### PR 2 — Automatik außerhalb des Setups

- [ ] **AK12** Start von bts-light mit lokalem BTP und leerer Kennung: Pfad
  und Kennung sind eingetragen, bevor der Turnierleiter „Starten" drückt.
- [ ] **AK13** Übertragung läuft, Datei wird in BTP unter anderem Pfad mit
  gleicher Kennung geöffnet: Innerhalb von 60 s ist der neue Pfad gebunden,
  **ohne** Neustart der Übertragung; ein angehaltener Upload läuft wieder an.
- [ ] **AK14** Übertragung läuft, in BTP wird ein anderes Turnier geöffnet:
  Innerhalb von 60 s erscheint das Banner; bis zur Antwort bleiben Pfad und
  Kennung unverändert und der Upload sendet die fremde Datei nicht.
- [ ] **AK15** Ja im Banner bei laufender Übertragung: Pfad und Kennung sind
  übernommen und die Übertragung ist neu gestartet; Liveticker und Upload
  laufen unter derselben Kennung.
- [ ] **AK16** Nein im Banner: nichts geändert, Schalter aus, kein weiteres
  Banner.
- [ ] **AK17** Dieselbe Frage erscheint nicht in jedem Takt erneut; öffnet
  BTP wieder die passende Datei, verschwindet das Banner.
- [ ] **AK18** Setup ist offen, während der Takt einen Pfad übernimmt:
  „Speichern" im Setup überschreibt Pfad, Bindung, Schalter und Kennung
  nicht mit dem alten Stand.
- [ ] **AK19** Das Identitäts-Bündel enthält keinen TP-Pfad; ein Import
  behält Pfad, Bindung und Schalter des Ziel-PCs.
- [ ] **AK20** BTP-Adresse nicht lokal oder Slave: Es wird nie gesucht.

## Tests

**Rust-Unit-Tests, ohne Windows lauffähig** (`tp_finden/mod.rs`, `config.rs`,
`commands.rs`):

- PR 1: `lesbarer_pfad_entfernt_praefix_und_unc`,
  `waehle_tp_ignoriert_ldb_und_gross_klein`, `waehle_tp_meldet_mehrere`,
  `btp_ist_lokal_nur_loopback`, `haenger_deckel_sperrt_ab_drei`,
  `tp_upload_ohne_feld_ist_automatik_an`,
  `finden_automatisch_ueberlebt_roundtrip`, je ein Test pro Lage auf
  `befund_zu_suche` (mit injizierter Dateiprüfung).
- PR 2: `gleiche_kennung_zieht_pfad_still_nach`,
  `leere_kennung_wird_gefuellt`, `fremde_kennung_fragt_einmal`,
  `ohne_kennung_fragt_und_bindet_nur_pfad`, `automatik_aus_tut_nichts`,
  `offene_frage_aendert_nichts`, `frage_kuendigt_neustart_an_wenn_laeuft`,
  `passende_datei_zieht_frage_zurueck`,
  `keep_host_managed_fields_schuetzt_tp_bei_veraltetem_stand`,
  `keep_host_managed_fields_laesst_setup_aenderung_bei_gleichem_stand`,
  `identity_bundle_leert_tp_pfad`,
  `apply_imported_identity_behaelt_tp_pfad_dieses_pcs`,
  `pfadwechsel_loest_takt_wiederanlauf`.

**Frontend:** `scripts/test-tp-wahl.mjs` für `src/io/tpWahl.mjs`
(Entscheidung nach Fund/Kennung, Meldungstexte); `npm run build` fehlerfrei.

**Nur am echten BTP prüfbar:** der Windows-Teil. Die Sonde
`tests/btp_tp_pfad_probe.rs` bleibt als Messwerkzeug; vor dem Merge von PR 1
ist sie einmal gegen den neuen Code gegenzuprüfen (gleicher Pfad).

**Manueller Testfall am Windows-PC:** AK1–AK3, AK13–AK15 mit dem
Liveticker-Testturnier.

## Risiken & Rollback

- **Falsches Turnier im laufenden Betrieb:** abgefangen durch Rückfrage
  (ADR 0097) und den unveränderten Halt `GuidFremd`.
- **Neustart mitten im Turnier:** nur nach ausdrücklichem Ja, die Frage
  kündigt ihn an.
- **Hänger an einem Handle:** nie gemessen, aber durch Frist und Deckel
  begrenzt; schlimmstenfalls drei blockierte Threads bis zum Beenden.
- **Virenscanner:** Kopieren fremder Handles kann auffallen (ungemessen).
- **Rollback:** Schalter aus stellt das heutige Verhalten her. Eine ältere
  Version liest die Config weiter (unbekannte Felder werden ignoriert).

## Offene Fragen / Annahmen

- **Ungemessen:** Datei auf Netzlaufwerk oder USB-Stick, Defender, zwei
  Windows-Benutzer am selben PC. Annahme: Die Dateiprüfung fängt unlesbare
  Funde ab; sonst bleibt die Handwahl.
- **Annahme:** Eine LAN-IP oder der Rechnername des eigenen PCs als
  BTP-Adresse gilt als „nicht lokal" (nur Loopback zählt).
- **In PR 1 nachzumessen:** ob für `GetFileType` und
  `GetFinalPathNameByHandleW` eine Handle-Kopie mit minimalem Zugriff genügt
  statt `DUPLICATE_SAME_ACCESS` (das kopiert kurzzeitig BTPs Schreibrecht).
- **In PR 1 zu bestätigen:** ob die drei `windows`-Features ohne
  `Win32_Security` reichen (zeigt der Windows-CI-Build).
- **Gesetzt, vom Nutzer nicht ausdrücklich bestätigt:** Takt 60 s; Handwahl
  gilt dauerhaft statt je Turnier; offene Frage wird zurückgezogen, wenn BTP
  wieder passt; Datei ohne Kennung führt zur Rückfrage.

## Betroffene Doku-Dateien

- **PR 1:** `docs/tp-datei-upload.md` (Schalter, Knopf, Meldungen, mit
  `pruef`-Ankern auf die neuen Beschriftungen), `docs/einstellungen.md`,
  `docs/erste-schritte.md`, `docs/btp_protocol.md` („Messung offen" →
  umgesetzt), `docs/logging.md`, neue Zeile „TP-Datei finden" in der
  Doku-Tabelle von `CLAUDE.md`, `docs/changelog.md`, `docs/roadmap.md`,
  ADR 0096 auf accepted.
- **PR 2:** `docs/tp-datei-upload.md` (der Satz „übernimmt sie nie still"),
  `docs/oberflaechen.md` (Banner, Dashboard-Zeile), `docs/einstellungen.md`
  (Nein schaltet ab), `docs/logging.md`, `CLAUDE.md`-Zeile ergänzen,
  `docs/changelog.md`, `docs/roadmap.md`, ADR 0097/0098 auf accepted.

## Umsetzungs-Hinweise

Zwei PRs, je mit eigenem Versionssprung (gemeinsam in `src-tauri/Cargo.toml`,
`src-tauri/tauri.conf.json`, `package.json`; Nummer erst beim Merge festlegen).

**PR 1 — Kern, Command, Setup**

1. Reine Logik in `tp_finden/mod.rs` testgetrieben: `lesbarer_pfad`,
   `waehle_tp` → `Auswahl { Keine, Eine, Mehrere }`, `btp_ist_lokal`,
   `HaengerDeckel`.
2. Config-Feld `finden_automatisch` mit den beiden Config-Tests.
3. Ergebnis-Typ `TpSuche` (serde-Tag `art`: `gefunden`, `btp_nicht_gestartet`,
   `kein_turnier_offen`, `kein_zugriff`, `haenger`, `mehrere`, `vorpruefung`,
   `nicht_lokal`, `nicht_verfuegbar`, `laeuft_schon`) und die reine Abbildung
   `befund_zu_suche(roh, pruefe)`.
4. `tp_finden/windows.rs`: Arbeiter mit Generation und Frist aus der Sonde,
   über das `windows`-Crate; liefert nur `RohBefund`.
5. Command `tp_datei_finden` (async, `spawn_blocking`, prozessweite Sperre
   gegen Doppellauf, prüft Slave und Loopback, loggt nur den Dateinamen);
   Registrierung in `lib.rs`, Anbindung in `src/api.ts`/`src/types.ts`.
6. Setup: Entscheidung aus `pickTpDatei` nach `src/io/tpWahl.mjs` ziehen,
   gemeinsamer Weg `uebernehmePfad(path, quelle)` für Dialog und Suche,
   Schalter, Knopf, Suche beim Öffnen.

**PR 2 — Automatik außerhalb des Setups**

1. `entscheide(Lage) -> Aktion` testgetrieben.
2. Schreibpfad: `tp_upload.stand`, Schutz in `keep_host_managed_fields`,
   Bündel/Import, Takt-Wiederanlauf aus `save_config` als Helfer herausziehen
   und auch nach `mutate_config` rufen.
3. `tp_finden_schritt` (rein über Merker und Config), `tp_finden_schleife`
   unter `TaktWaechter` neben `tp_takt_schleife` (60 s, erster Tick sofort),
   einmaliger Lauf beim App-Start; Merker in `AppState.tp_finden`.
4. Commands `tp_finden_status` und `tp_finden_antwort`.
5. `TpFundBanner.tsx` (Polling alle 10 s nach dem Muster
   `PrintWarningBanner.tsx`), Hinweiszeile im Dashboard,
   `neustartUebertragung()` aus dem Setup herausziehen, Nachziehen der
   TP-Felder im offenen Setup.

**Reviews:** `code-reviewer` nach jeder Änderung. `security-reviewer` für
PR 1 (Handles fremder Prozesse, Prozess nur über den Namen erkannt,
`unsafe`-Blöcke, Pfad nie in Log oder Bündel) und für PR 2 (Config-Schreiben
ohne Nutzerklick).
