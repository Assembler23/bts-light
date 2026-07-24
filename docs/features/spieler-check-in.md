# Hallen-Check-In — Spezifikation

> Status: **Entwurf** (via /idee: Brief → Grill → How-To → Review), erarbeitet 2026-07-24.
> Quelle: Idee + Chat-Abstimmung vom 24.07.2026.
> Betroffene Crates: `src-tauri/` (btp, badhub, config, commands, sync) · `src/` (Seite, Ansage-Texte).
> Zweites Repo: **badhub** (öffentliche Seite, Persistenz, Admin-Verwaltung).
> ADR: [docs/adr/0009-hallen-checkin-persistenz-und-identitaet.md](../adr/0009-hallen-checkin-persistenz-und-identitaet.md)

## Kontext / Problem

Beim Köpi-Cup 2025 entstand ein großer Andrang am Anmeldetisch: Bei frühem
Turnierstart trafen praktisch alle Teilnehmer gleichzeitig ein, und zusätzlich
wurde am selben Tresen der Zahlungsstatus geprüft. Die Anwesenheitsprüfung ist
heute ein manueller, sequenzieller Vorgang an einem einzigen Ort — obwohl die
Turnierleitung die Information „wer ist da?" **vor der Auslosung** braucht.

BTP hat zwar eigene Check-in-Felder (`Player.CheckedIn`, `FirstCheckIn`), aber
sie hängen **am Spieler und gelten turnierweit**. Der Fall „in Herrendoppel B
anwesend, in Herreneinzel A noch nicht" ist damit nicht abbildbar. BTPs
Check-in zielt auf einzelne Spiele, nicht auf die Anwesenheit vor der
Auslosung.

Den Schmerz hat die **Turnierleitung** (Warteschlange, kein Überblick vor der
Auslosung) und der **Spieler** (Anstehen bei früher Anfangszeit).

## Zielbild & Erfolgskriterien

Spieler bestätigen über eine öffentlich erreichbare Webseite selbst, dass sie in
der Halle und spielbereit sind. Die Turnierleitung sieht den Stand je Klasse und
kann Fehlende gezielt ausrufen lassen.

Erfolgskriterien, messbar beim nächsten Turnier:

1. Mindestens die Hälfte der Teilnehmer einer Klasse checkt selbst ein, ohne
   dass jemand am Tresen dafür angesprochen wurde.
2. Die Turnierleitung kann zum Anmeldeschluss einer Klasse ohne Rückfrage am
   Tresen benennen, wer fehlt.
3. Ein Turnierleiter richtet das Feature ein, ohne dass jemand es ihm erklärt:
   Turnier-GUID eintragen, Anfangszeiten pflegen, QR-Aushang drucken.
4. Fällt das Internet aus, läuft das Turnier unverändert weiter — der
   Check-In-Bereich verschwindet mit einem verständlichen Hinweis.

## Nicht-Ziele

- **Kein Rückschreiben nach BTP.** Check-In-Stände bleiben außerhalb von BTP.
- **Keine Kopplung an die Feldvergabe.** Der Check-In-Zustand beeinflusst
  `sync.rs`/`AutoAssignConfig` nicht. „Nicht eingecheckte Spieler nicht aufs
  Feld setzen" wird **bewusst ausgeschlossen** — sonst hinge die Feldvergabe an
  einer ungeprüften Selbstauskunft von einem Handy.
- **Keine Spieler-Selbstansicht und keine Push-Benachrichtigung** in dieser
  Stufe (eigene Seite mit „deine nächsten Spiele", „du bist in N Spielen dran").
  Ausdrücklich gewünscht, aber Ausbaustufe 2.
- **Kein Terminal-Produkt.** Die Seite ist touch-tauglich (siehe
  Akzeptanzkriterien), aber es entsteht keine Kiosk-Anwendung.
- **Keine Identitätsprüfung.** Der Zugang ist offen; wer einen fremden Namen
  anklickt, wird technisch nicht gehindert (siehe Risiken).
- **Kein Status `abgemeldet`** in dieser Stufe.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  `src-tauri/src/btp/model.rs` (Snapshot um Events/Entries erweitern) ·
  `src-tauri/src/badhub/{payload,diff,push}.rs` (neuer Nachrichtentyp) ·
  `src-tauri/src/config.rs` (neuer Block) · `src-tauri/src/commands.rs`
  (Abruf + Schreib-Commands) · `src-tauri/src/sync.rs` (Push einhängen) ·
  `src/pages/CheckinPanel.tsx` (neu) · `src/io/announcer.ts` (Textbau) ·
  `src/{App.tsx,types.ts,api.ts}` + `src/components/SideNav.tsx`.
  **Nicht betroffen:** `relay/`, `relay-proto/` — der Relay bedient Tablets und
  Monitore, nicht öffentliche Web-Clients.
- **Architekturregeln (CLAUDE.md R1–R6):**
  - **R1** gewahrt: Der Abruf der Check-In-Stände läuft über einen
    Tauri-Command, **nicht** per `fetch()` aus React gegen badhub.
  - **R2** gewahrt: BTP bleibt die Wahrheit für Klassen, Meldungen und Spieler.
    Der Check-In-Zustand ist ein **zusätzlicher**, BTP-fremder Zustand und
    fließt nicht zurück.
  - **R3**: Das Feature braucht Internet und ist damit unabhängig vom
    LAN-/Cloud-Modus der Tablets. Im reinen LAN-Betrieb ohne Internet ist es
    nicht verfügbar (siehe AK-A6).
  - **R4/R5** unberührt: keine Court→Match-Zuordnung, keine Ergebnisse,
    `process_result` nicht betroffen.
  - **R6** unberührt: `install_id` bleibt Relay-Namespace und Log-Zuordnung.
    Der Check-In nutzt sie **nicht** als Turnier-Identität.
  - **Mehr-Hallen (D7):** Genau ein Master schreibt. Slaves (`slave_mode`)
    und Zweit-Master sind read-only. Die Fehlt-Ansage läuft **ungefiltert**,
    also ohne `announce_hall`-Filter — eine Klasse startet in einer Halle,
    der Check-In gilt turnierweit.
- **Konfiguration & Abwärtskompatibilität:**
  Neuer Block `CheckinConfig { enabled, tournament_uuid, missing_names_max }`
  in `config.rs`, eingehängt in `AppConfig` mit `#[serde(default)]` nach dem
  Muster `ScorekeeperConfig`. `AppConfig` hat **keinen**
  Migrationsmechanismus — Abwärtskompatibilität entsteht ausschließlich über
  `serde(default)`, deshalb ist der Migrationstest Pflicht. Anfangszeiten und
  Check-In-Stände liegen **nicht** in der Config, sondern in badhub unter der
  Turnier-GUID; eine Installation läuft über Jahre über viele Turniere.
  Tauri-`identifier` `de.badhub.btslight` und Updater-Pfad
  `download/bts-light/` bleiben unangetastet.
- **Datenschutz:**
  - Kein Geburtsjahr — weder speichern, anzeigen noch loggen.
  - Die öffentliche Seite zeigt **Vorname, Nachname** und, sofern BTP sie
    liefert, **Verein** und **Nationalität** zur Unterscheidung Gleichnamiger.
    Beide Felder sind in BTP optional (`Player.ClubID`, `Player.Country`) und
    im Testmitschnitt leer — die Seite muss ohne sie funktionieren.
  - Der Status „Rückfrage an Turnierleitung" wird auf der öffentlichen Seite
    **nie als Zustand ausgeliefert**. Betroffene sehen in der Liste aus wie
    jeder Nicht-Eingecheckte; erst beim Klick auf den eigenen Namen erscheint
    „bitte zur Turnierleitung kommen". Grund: der Status hat typischerweise
    einen finanziellen Hintergrund (offene Zahlung) und darf nicht öffentlich
    neben einem Klarnamen stehen.
  - Namen werden beim Schreiben **und** beim Lesen durch badhubs
    Anonymisierungs-Gate (`src/Anonymization.php`, Art. 17) geführt, damit
    eine anonymisierte Person nicht über den BTS-Push namentlich wieder
    auftaucht.
- **Abhängigkeiten:**
  - BTP-Protokoll: `Entry.EventID` (siehe „Umsetzungs-Hinweise").
  - badhub-Endpunkt (bestehender Liveticker-Kanal + neue Check-In-Endpunkte).
  - **Keine neue Cargo-Dependency** — QR-Codes rendert badhub mit dem dort
    bereits vorhandenen `vendor/qrcode.js`.
  - **Keine neue npm-Dependency.**

## Fachliche Festlegungen

| Thema | Festlegung |
|---|---|
| Turnier-Identität | **turnier.de-Turnier-GUID** (36 Zeichen, aus der URL `turnier.de/tournament/<GUID>/matches`), einmalig in bts-light eingetragen. Authentifiziert wird über den **bestehenden** Liveticker-Kanal. Siehe ADR 0009. |
| Spieler-Identität | BTP-`PlayerID` innerhalb des Turniers (immer vorhanden). `MemberID` (Lizenznummer) wird mitgeschickt **wenn vorhanden**, nie Pflicht. |
| Klassen-Schlüssel | BTP-`EventID` plus Event-Name als Anzeigetext. |
| Anfangszeit | Manuell gepflegt, je Klasse. |
| Anmeldeschluss | Eigener Zeitpunkt je Klasse, **Default = Anfangszeit**. |
| Check-In-Fenster | Öffnet **1 h vor** der Anfangszeit, schließt zum Anmeldeschluss. |
| Zugang | Offen, keine technische Hürde. Verteilung per QR-Code/Aushang in der Halle. |
| Status | `offen` · `eingecheckt` · `Rückfrage an Turnierleitung`. |
| Granularität | Immer je Klasse. Ein Spieler in drei Klassen checkt dreimal ein. |
| Doppel-Modus | Turnierweit einstellbar: **pro Spieler** oder **pro Meldung**. Später änderbar. Roh gespeichert wird **immer je Spieler** samt Herkunft (`selbst` · `durch Partner` · `Turnierleitung`); die Einstellung ändert nur Eingabe und Anzeige. Dadurch ist Umschalten verlustfrei und braucht keine Migration. |
| Konflikt TL ↔ Spieler | Serverseitiger Zeitstempel, letzter Schreibvorgang gewinnt — **außer**: ein manuelles Zurücksetzen durch die TL sperrt den Selbst-Check-In für diesen Spieler in dieser Klasse, bis die TL ihn wieder öffnet. |
| Ansagen | Immer **per Klick**. Kein selbsttätiges Sprechen. |
| Persistenz | badhub, unter der Turnier-GUID. |
| Zustandsberechnung | **Serverseitig in badhub**, Zeitzone `Europe/Berlin`. Nie die Uhr des Spieler-Handys. |
| Wer pflegt in badhub | Die Turnierleitung selbst, über die badhub-Rolle `liveticker`, beschränkt auf **eigene** Turniere (Ownership über `created_by_admin_id`). Siehe „Zugang der Turnierleitung". |

### Zugang der Turnierleitung zur badhub-Verwaltung

Die Rolle `liveticker` existiert in badhub und ist genau für diesen Zweck
angelegt — eigene Turniere verwalten, mit Ownership-Prüfung, eigenem
Login-Redirect, 2FA-Pfad und Pfad-Whitelist. Sie ist derzeit jedoch
**funktionslos**: die einzige für sie freigegebene Seite weist sie mit HTTP 403
ab. Das ist **kein Fehler, sondern eine bewusste Rücknahme** (badhub-Commit
`c6bcf71`, 16.04.2026: „Zugangsbeschränkung von superadmin+liveticker auf
nur-superadmin geändert — BTS-Nutzer brauchen kein Admin-Panel mehr"). Grund
war der im selben Commit eingeführte Passwort-Versand per E-Mail: Zugangsdaten
kommen seither per Mail, das Panel wurde überflüssig.

Der Hallen-Check-In schafft erstmals einen Bedarf, den diese Begründung nicht
abdeckt — Anfangszeiten, Anmeldeschlüsse und den Rückfrage-Status kann niemand
außer der Turnierleitung pflegen.

**Festlegung:** Die neue Check-In-Verwaltungsseite wird für die Rolle
`liveticker` freigeschaltet (Aufnahme in `LIVETICKER_ADMIN_ALLOWED_PATHS`,
Ownership über `created_by_admin_id`). Das **Zugangsverwaltungs-Panel**
(Turniere anlegen, Passwörter rotieren/versenden) bleibt unverändert
Superadmin-only — die Entscheidung von 04/2026 wird damit nicht angetastet,
sondern nur um einen Gegenstand ergänzt, den es damals nicht gab.

## Umsetzung in drei Schnitten

Nacheinander lieferbar, jeder einzeln testbar und commitbar.

- **Schnitt A** — Meldelisten-Push (bts-light) + Persistenz und
  Admin-Verwaltung (badhub). Nichts ist öffentlich sichtbar.
- **Schnitt B** — öffentliche Check-In-Seite + QR-Aushang (badhub).
- **Schnitt C** — Turnierleitungs-Sicht + Ansage-Texte (bts-light).

## Akzeptanzkriterien

### Schnitt A — Meldeliste und Persistenz

- [ ] **A1** Aus einem BTP-Snapshot mit `Entries` erzeugt bts-light je Klasse
      die Liste der gemeldeten Spieler — **auch wenn für die Klasse noch keine
      Auslosung existiert** (Zuordnung über `Entry.EventID`).
- [ ] **A2** Enthält der Snapshot keine `Entries`- oder `Events`-Gruppe, ist die
      Meldeliste leer; die App läuft normal weiter und stürzt nicht ab.
- [ ] **A3** Ein Spieler ohne `MemberID` und ohne `ClubID` erscheint vollständig
      in der Meldeliste (nur Vor- und Nachname).
- [ ] **A4** Die Meldeliste wird an badhub gesendet, **wenn sie sich geändert
      hat**. Zwei aufeinanderfolgende Sync-Zyklen mit identischer Meldeliste
      erzeugen genau **einen** Push.
- [ ] **A5** Ist `slave_mode` aktiv, sendet die Instanz **keine** Meldeliste.
- [ ] **A6** Ist keine Turnier-GUID konfiguriert oder `enabled = false`, sendet
      die App keine Meldeliste. (Das Ausblenden des Bereichs in der Oberfläche
      gehört zu Schnitt C, siehe C3/C4.)
- [ ] **A7** Eine Turnier-GUID in falschem Format wird im Setup abgelehnt,
      bevor sie gespeichert wird.
- [ ] **A8** Eine `config.json` einer älteren Version (ohne `checkin`-Block)
      lädt fehlerfrei; alle neuen Felder stehen auf ihren Defaults.
- [ ] **A9** badhub nimmt die Meldeliste nur mit gültigem Bearer-Passwort an;
      ein falsches Passwort führt zu HTTP 401 und keiner Speicherung.
- [ ] **A10** Ein erneuter Push aktualisiert Stammdaten und legt neue Meldungen
      an, überschreibt aber **keinen** bestehenden Check-In-Zustand und keinen
      von der TL gesetzten Status.
- [ ] **A11** Spieler, die in badhub als anonymisiert markiert sind, werden
      weder gespeichert noch ausgeliefert.
- [ ] **A12** Antwortet badhub mit 5xx, bleibt die App funktionsfähig; der
      nächste Zyklus sendet die vollständige Meldeliste erneut.
- [ ] **A13** Ein Benutzer mit der badhub-Rolle `liveticker` erreicht nach dem
      Login die Check-In-Verwaltung und kann dort Anfangszeit, Anmeldeschluss,
      Doppel-Modus und den Rückfrage-Status pflegen — **nur für Turniere, die
      ihm gehören**. Fremde Turniere sind weder sichtbar noch änderbar.
- [ ] **A14** Dieselbe Rolle erhält auf dem Zugangsverwaltungs-Panel
      (Turniere anlegen, Passwörter rotieren/versenden) weiterhin HTTP 403.

### Schnitt B — Öffentliche Check-In-Seite

- [ ] **B1** Die Übersichtsseite zeigt alle Klassen des Turniers mit Anfangszeit
      und einem der vier Zustände: `Check-In ab HH:MM möglich` ·
      `Check-In läuft` · `abgeschlossen` · `live`.
- [ ] **B2** Alle Zustände werden **serverseitig** in `Europe/Berlin` berechnet.
      Eine falsch gestellte Uhr auf dem Endgerät ändert nichts.
- [ ] **B3** Der Zustand wechselt zu `Check-In läuft` genau 1 h vor der
      Anfangszeit und zu `abgeschlossen` zum Anmeldeschluss.
- [ ] **B4** Über die Sommerzeit-Umstellung hinweg bleiben die Fenster korrekt.
- [ ] **B5** Ein Klick auf eine Klasse zeigt die Namensliste dieser Klasse mit
      dem jeweiligen Zustand (`offen` / `eingecheckt`).
- [ ] **B6** Die Spielersuche durchsucht **ausschließlich die Meldeliste dieses
      Turniers**, über dessen Klassen hinweg, und zeigt zu einem Namen alle
      seine Meldungen. Es findet **keine** Suche im badhub-Spielerbestand statt.
- [ ] **B7** Ein Klick auf den eigenen Namen bei offenem Fenster setzt den
      Zustand auf `eingecheckt`; die Änderung ist nach dem nächsten Poll für
      alle sichtbar.
- [ ] **B8** Ein Check-In-Versuch **vor** Fensteröffnung oder **nach**
      Anmeldeschluss wird abgelehnt und begründet.
- [ ] **B9** Ein Check-In-Versuch für einen Spieler oder eine Klasse, die nicht
      zur zuletzt gepushten Meldeliste des Turniers gehören, wird abgelehnt.
- [ ] **B10** Ein Spieler mit Status „Rückfrage an Turnierleitung" erscheint in
      der Liste **wie ein Nicht-Eingecheckter**; erst der Klick auf seinen Namen
      zeigt „bitte zur Turnierleitung kommen". Er wird **nicht** eingecheckt.
- [ ] **B11** Ein Spieler, dessen Check-In die TL zurückgesetzt hat, kann sich
      **nicht** selbst wieder einchecken, solange die Sperre besteht.
- [ ] **B12** Der öffentliche Pfad kann einen Check-In **nicht** zurücknehmen.
- [ ] **B13** Doppelter Check-In (Doppelklick, zwei Geräte) führt zum selben
      Endzustand und erzeugt keinen Fehler.
- [ ] **B14** Im Modus *pro Meldung* checkt ein Klick beide Partner ein — den
      Klickenden mit Herkunft `selbst`, den Partner mit `durch Partner`.
- [ ] **B15** Ein Umschalten des Doppel-Modus verändert **keine** gespeicherten
      Check-Ins, nur deren Darstellung.
- [ ] **B16** Übermäßig viele Anfragen von derselben Adresse werden gedrosselt.
- [ ] **B17** Die Seite ist auf einem Touch-Display in Hoch- und Querformat
      bedienbar: Ziel-Flächen ≥ 44 px, **keine** Funktion nur per Hover
      erreichbar.
- [ ] **B18** Die Seite ist als `noindex` markiert.
- [ ] **B19** Es existiert eine druckbare Aushang-Seite je Turnier mit QR-Code,
      Kurz-URL und Turniername.

### Schnitt C — Turnierleitungs-Sicht und Ansagen

- [ ] **C1** bts-light zeigt je Klasse, wie viele Spieler eingecheckt sind und
      wer fehlt.
- [ ] **C2** Die TL kann einen Spieler manuell auf `eingecheckt` setzen und
      zurücksetzen; das Zurücksetzen sperrt den Selbst-Check-In (siehe B11).
- [ ] **C3** Ist badhub nicht erreichbar, zeigt die Seite einen verständlichen
      Hinweis („Check-In braucht Internet") statt einer Fehlermeldung, und das
      übrige Programm bleibt uneingeschränkt bedienbar.
- [ ] **C4** Antwortet badhub auf den Check-In-Endpunkt mit 404 oder 400 (altes
      badhub, Feature dort noch nicht ausgerollt), verhält sich die App wie in
      C3 und blendet den Bereich aus.
- [ ] **C5** Check-Ins, die entstanden sind, während bts-light nicht lief,
      erscheinen beim nächsten Abruf vollständig.
- [ ] **C6** Ein Klick erzeugt die Ansage „Noch N Minuten bis Anmeldeschluss
      <Klasse>" und spielt sie ab.
- [ ] **C7** Ein Klick erzeugt die Ansage der fehlenden Spieler. Bis
      `missing_names_max` Namen werden sie vorgelesen; **darüber** wird nur die
      Anzahl angesagt („In Herrendoppel B fehlen noch 23 Anmeldungen").
- [ ] **C8** Fehlt niemand, wird keine Fehlt-Ansage angeboten.
- [ ] **C9** Die Fehlt-Ansage läuft unabhängig von `announce_hall`.
- [ ] **C10** Gesprochen wird ausschließlich nach einem Klick der
      Turnierleitung — die App sagt nie selbsttätig etwas an.
- [ ] **C11** Wechselt das Turnier (neue GUID), zeigt die Sicht keine Stände des
      Vorturniers.

## Tests

**bts-light (Rust, `cargo test` grün vor jedem Commit):**

- `btp/model.rs` — `Entry.EventID` → Klasse gegen das echte Fixture
  `tests/fixtures/btp-tournament-2halls.bin` (A1); Snapshot ohne
  `Entries`/`Events` liefert leer statt Panik (A2); Spieler ohne
  `MemberID`/`ClubID` (A3).
- `badhub/payload.rs` — Key-Test des neuen Nachrichtentyps (Muster:
  `serializes_to_expected_json_keys`).
- `badhub/diff.rs` — identische Meldeliste → kein Push, ein neuer Spieler →
  Push (A4).
- `config.rs` — alte `config.json` ohne `checkin`-Block lädt mit Defaults (A8),
  Muster `config_without_announce_key_loads_with_defaults`.
- `sync.rs` — im `slave_mode` wird keine Meldeliste gepusht (A5); ohne
  konfigurierte GUID wird nichts gepusht (A6).
- Fenster- und Textlogik als **reine** Funktionen: Ansage unter/über
  `missing_names_max`, leere Liste (C7, C8).

**Frontend:** `npm run build` (= `tsc && vite build`) fehlerfrei.

**badhub (`php tests/unit/*_test.php`, kein PHPUnit):**

- Auth über den Liveticker-Kanal; falsches Passwort → 401 (A9).
- Rollen-Zugriff: `liveticker` erreicht die Check-In-Verwaltung und sieht nur
  eigene Turniere (A13); dieselbe Rolle bleibt auf dem Zugangsverwaltungs-Panel
  gesperrt (A14).
- Push überschreibt keinen Zustand (A10); anonymisierte Spieler gefiltert (A11).
- Zustandsberechnung `Europe/Berlin` inkl. Sommerzeitgrenze (B2–B4).
- Ablehnung turnierfremder Spieler/Klassen (B9) und außerhalb des Fensters (B8).
- Status „Rückfrage" erscheint nicht in der öffentlichen Liste (B10).
- TL-Sperre hat Vorrang (B11); öffentlicher Pfad kann nicht zurücknehmen (B12).
- Rate-Limit greift (B16).
- Plus ein `@smoke`-Playwright-Test der öffentlichen Seite und ein
  Regression-Eintrag (badhub-TDD-Pflicht).

**Manueller Turnier-Testfall:** An einem echten Turnierexport prüfen, ob
`MemberID` und `ClubID` gefüllt sind und ob `EventID` einen BTP-Neuimport
überlebt (siehe „Offene Fragen").

## Risiken & Rollback

| Risiko | Bewertung / Mitigation |
|---|---|
| Fremder checkt einen Abwesenden ein | **Bewusst akzeptiert.** Die Verteilung per QR in der Halle senkt die Hürde für Unbeteiligte; die TL kann jederzeit zurücksetzen und sperren (C2/B11). Technische Validierung (B9) verhindert *technischen* Missbrauch, nicht *sozialen*. |
| Öffentlicher Schreibendpunkt | Serverseitige Validierung jeder Anfrage gegen die zuletzt gepushte Meldeliste und das Zeitfenster, Rate-Limit, kein Zurücknehmen (B8/B9/B12/B16). Dies ist die sicherheitsrelevanteste Einzelentscheidung; `security-reviewer` ist für Schnitt B Pflicht. |
| Lastspitze zur Fensteröffnung | badhub läuft auf Shared Hosting. Poll-Intervall und Cache-Header werden bewusst länger gewählt als beim Liveticker; der Poller nutzt Backoff. |
| Versionsschiefstand badhub ↔ bts-light | badhub wird **zuerst** ausgerollt und toleriert unbekannte Felder; bts-light behandelt 404/400 als „Feature nicht verfügbar" (C4) — derselbe Codepfad wie der Offline-Fall (C3), also mitgetestet. |
| Reste eines Vorturniers | Alle Daten hängen an der Turnier-GUID; ein neues Turnier hat eine neue GUID (C11). |
| Ausfall im laufenden Turnier | Der Check-In ist **additiv**: fällt er aus, läuft das Turnier wie bisher weiter (C3). Keine Kopplung an Feldvergabe oder Ergebnisse. |
| Rollback | Ältere bts-light-Version bleibt installierbar; `#[serde(default)]` hält die Config lesbar. badhub-Tabellen können stehen bleiben, ohne etwas zu stören. |

## Offene Fragen / Annahmen

1. **Stabilität der `EventID` über einen BTP-Neuimport.** Bestimmt, wie stark
   der Klassenname als Absicherung gebraucht wird. Am echten Turnier zu prüfen.
2. **Verfügbarkeit von `MemberID` und `ClubID` in echten Turnieren.** Im
   Testmitschnitt sind beide leer. Sie bestimmen, wie gut das
   Anonymisierungs-Gate greift und ob der Verein bei Namensgleichheit als
   Unterscheidungsmerkmal taugt. **Annahme:** Das Feature funktioniert
   vollständig ohne beide Felder.
3. **Annahme:** Ein Turnier hat genau eine Turnier-GUID, die vor Turnierbeginn
   bekannt ist. Turniere ohne turnier.de-Eintrag können den Check-In nicht
   nutzen.

## Betroffene Doku-Dateien

**bts-light** (im selben Commit zu pflegen):
- `docs/spieler-check-in.md` — **neu**, die Feature-Doku (großes Feature →
  eigene Datei).
- `CLAUDE.md` — neue Zeile in der Doku-Pflicht-Tabelle.
- `docs/btp_protocol.md` — **Korrektur**: die `Entry`-Zeile ist unvollständig,
  richtig ist `Entry: ID, EventID, Player1ID[, Player2ID]`.
- `docs/changelog.md` — je veröffentlichter Version.
- `docs/roadmap.md` — Verweis auf diese Spec.
- `docs/adr/0009-hallen-checkin-persistenz-und-identitaet.md` — **neu**.

**badhub:**
- `docs/features/hallen_checkin.md` — **neu**.
- `docs/schema_evolution.md` — Pflicht bei jeder Migration.
- `CLAUDE.md` — Eintrag der R3-Ausnahme (Check-In-Tabellen ohne
  `federation_id`, weil verbandsübergreifend wie der Liveticker).
- Rollen-Doku: die Wiederbelebung der Rolle `liveticker` für die
  Check-In-Verwaltung festhalten — sie war seit 04/2026 faktisch stillgelegt.
  Die Abgrenzung zum weiterhin gesperrten Zugangsverwaltungs-Panel gehört
  ausdrücklich dazu.

## Umsetzungs-Hinweise

*Erst nach Freigabe relevant. Verdichtet aus der How-To-Phase — die
ausführliche Fassung mit allen Datei-Ankern steht in
`_intake/spieler-check-in/3-how-to.md`.*

**Reihenfolge:** Schnitt A → B → C. **badhub wird jeweils zuerst ausgerollt.**

**Vorhandene Muster, denen zu folgen ist (bts-light):**

| Aufgabe | Vorlage |
|---|---|
| Meldeliste senden | `badhub/payload.rs` (Serialize-Struct + `build_*`) + `badhub/push.rs::push_update` (Bearer, 15 s) |
| Nur bei Änderung senden | `badhub/diff.rs::preparation_calls` — `BTreeMap`-Fingerabdruck |
| Check-In-Stände abrufen | `commands.rs::fetch_pronunciations` — `badhub_origin()`, eigener Timeout-Client, Datei-Cache, **liefert nie `Err`**. Genau dieses Verhalten erfüllt C3 **und** C4. |
| Neuer Config-Block | `ScorekeeperConfig` + `#[serde(default)]`-Feld in `AppConfig` + Migrationstest; Spiegel in `src/App.tsx` und `src/types.ts` |
| Meldeliste aus BTP | `btp/model.rs::entry_map` liest heute nur `EntryID → PlayerIDs` und **verwirft die `EventID`** — hier ansetzen |
| TL-Sicht | `commands.rs::preparation_candidates` (View-Struct) + `pages/PreparationPanel.tsx` (Poll, Auswahl, `busy`-Flag) |
| Neue Seite anmelden | Es gibt **keinen Router**: `NavView`-Variante + `SideNav`-Eintrag + `switch`-Case in `App.tsx` (dort erzwingt `const _exhaustive: never` Vollständigkeit) |
| Ansage abspielen | `publishFreetext` → `FreetextAnnouncer` → `playFreeText`; Textbau als **reine** Funktion nach Muster `buildPreparationSegments` |
| Push-Cut für Slaves | `sync.rs` — der Meldelisten-Push gehört **hinter** den `slave_mode`-Return |

**Vorhandene Muster (badhub):** Write-Endpunkt nach `api/live_update.php`
(Bearer → Tenant), Rate-Limit für öffentliche Endpunkte nach
`api/push_subscribe.php` (`bh_client_ip()`), Read-Endpunkt mit
serverseitigem `age_seconds` nach `Api/LivetickerController`, öffentliche
Seite nach `public/live.php` + `assets/js/live.js` (Poller mit Backoff),
Admin-Seite nach `Admin/LivetickerController`, sicherheitskritische Aktionen
nach `Admin/PlayerPrivacyController` (CSRF + Re-Verify + Audit-Log),
Anonymisierung über `src/Anonymization.php`. Migration als nächste freie
Nummer unter `db/migrations/`; **keine Semikolons in SQL-Kommentaren** (der
Migrationsrunner splittet darauf).

**Reviews:** `code-reviewer` nach **jeder** Code-Änderung in beiden Repos ·
`security-reviewer` **zwingend** für Schnitt B (neuer öffentlicher
Schreibpfad) · badhubs `database-reviewer` für die Migration.

**Version** gemeinsam bumpen in `src-tauri/Cargo.toml`,
`src-tauri/tauri.conf.json` und `package.json` — je Schnitt eine eigene
Version.
