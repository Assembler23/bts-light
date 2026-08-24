# Felder sperren im TL-Web — Spezifikation

> Status: **abgestimmt 2026-08-23** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Nutzer-Anforderung vom 23.08.2026. Betroffene Crates: `src-tauri`, `relay-proto`.
> ADR: [0044](../adr/0044-sperrliste-turniergebunden.md); Nachtrag zu
> [0030](../adr/0030-halle-bindet-vergabe.md).

## Kontext / Problem

Die Turnierleitung steht in der Halle, nicht am Turnier-PC. Fällt ein Feld aus
— gerissenes Netz, defekte Beleuchtung, nasser Boden —, muss sie heute quer
durch die Halle zum PC laufen, um es aus der automatischen Vergabe zu nehmen.
Bis dahin legt die Automatik weiter Spiele auf ein unbespielbares Feld.

Die Feldsperre selbst **existiert vollständig**; nur ihr Bedienweg endet am
PC:

| Baustein | Stand |
|---|---|
| `config.locked_courts` | vorhanden, persistiert |
| `TabletState.locked_courts` + `set_court_locked` | vorhanden |
| Tauri-Command `set_court_locked` | vorhanden |
| Wirkung auf Auto-Vergabe (`sync.rs`) und Vorverteilung | vorhanden |
| **Anzeige** im TL-Web (`.card.locked`, „Dieses Feld ist gesperrt.") | vorhanden |
| **Setzen** im TL-Web | **fehlt — Gegenstand dieser Spec** |

Vier Anlässe, alle vom Nutzer bestätigt: Feld unbespielbar · Feld reserviert
(Einspielen, Siegerehrung) · Halle wird gegen Turnierende verkleinert · Feld
am Turniermorgen noch nicht aufgebaut.

Der Grill hat dabei einen **bestehenden Datenverlust-Bug** freigelegt
(`docs/roadmap.md`, offener Eintrag): `keep_host_managed_fields` schützt
`locked_courts` nicht, der Setup-Assistent schickt den beim Öffnen
aufgenommenen Stand zurück. Heute merkt das der PC-Bediener, weil er selbst
gesperrt hat. Sobald die TL aus der Halle sperrt, wäre der Ablauf: Feld
sperren → jemand am PC speichert irgendeine Einstellung → Sperre still weg →
Automatik legt ein Spiel auf das kaputte Feld. Der Fix gehört deshalb in diese
Spec.

## Zielbild & Erfolgskriterien

Die Turnierleitung tippt am Tablet auf das ⋯ einer Feld-Kachel und wählt „Feld
sperren". Die Kachel zeigt sich sofort als gesperrt, die Automatik vergibt
dorthin nichts mehr, und am Turnier-PC steht dieselbe Sperre. Ein Spiel, das
gerade darauf läuft, merkt nichts davon.

**Erfolgskriterien**

1. Ein Feld lässt sich am Tablet in **höchstens zwei Tipps** sperren (⋯ →
   Eintrag), ohne Erklärung und ohne den PC.
2. Nach dem Sperren erhält das Feld beim nächsten Vergabelauf **kein** Spiel
   mehr — im LAN- wie im Cloud-Betrieb.
3. Eine gesetzte Sperre überlebt jedes Speichern der Einstellungen am
   Turnier-PC (heute nicht der Fall).
4. Am Folgeturnier ist keine Sperre des Vortags mehr aktiv.

## Nicht-Ziele

- **Mehrere Felder auf einmal** sperren (Sperr-Modus, Mehrfachauswahl).
- Ein **eigenes TL-Panel** für Feldsperren.
- Ein **Sperr-Grund** oder eine Notiz am Feld.
- **Zeitgesteuerte** Sperren („ab 18 Uhr").
- Ein laufendes Spiel beim Sperren **vom Feld nehmen** — der Weg dafür
  existiert und bleibt getrennt.
- Die Sperre auf dem **Wandmonitor** anzeigen. `MonitorState` kennt kein
  `locked`; der TV am gesperrten Feld läuft weiter wie bisher.
- Die **Besetzung** (Zähltafelbediener, Schiedsrichter) am gesperrten Feld
  antasten — das Feld wird ja nicht geräumt.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:** `relay-proto` (`TlAction`) · `src-tauri/src/tablet/tl.rs`
  (Ausführung, `TlState`) · `src-tauri/src/tablet/state.rs` (Turnierbindung) ·
  `src-tauri/src/config.rs` · `src-tauri/src/commands.rs`
  (`keep_host_managed_fields`, Desktop-Command) · `src-tauri/assets/tl.html`.
  **`relay/` braucht keine Code-Änderung** — der Relay parst `TlAction` zwar
  typisiert (`relay/src/main.rs`), bekommt die neue Variante aber über die
  gemeinsame Crate `relay-proto` mit, und er wird zusammen mit `tl.html` aus
  demselben Merge deployt. Er ist damit nie älter als die Seite, die ihn
  anspricht.
- **Architekturregeln:**
  - **R1** — die Weboberfläche schreibt ausschließlich über die
    TL-Kommando-Strecke; kein direkter Zugriff.
  - **R2** — BTP kennt keine Feldsperre. Sie bleibt bts-light-seitig, **kein**
    `SENDUPDATE`. `touches_courts` (`tl.rs`) bleibt unverändert, sonst liefe
    die Aktion in die Feld-Beanspruchung und in `write_courts_to_btp`.
  - **R3** — LAN und Cloud nehmen denselben Weg (`tl::execute`), wie alle
    TL-Aktionen.
  - **R5-Geist** — der Host prüft: unbekannte CourtID wird abgelehnt.
- **Konfiguration & Abwärtskompatibilität:** neues Feld
  `locked_courts_tournament: String` mit `#[serde(default)]`. Alte Configs
  bleiben lesbar (leeres Feld = „Turnier unbekannt"), neue Configs sind für
  ältere Versionen lesbar (unbekanntes Feld wird ignoriert). `identifier` und
  Updater-Pfad unangetastet.
- **Datenschutz:** Die Sperrliste enthält ausschließlich CourtIDs — keine
  Personendaten.
- **Abhängigkeiten:** keine neuen Cargo-/npm-Abhängigkeiten. Keine Abhängigkeit
  zu BTP-Versionen, badhub oder dem Pi-Kiosk.

## Verhalten

### Sperren

- Die Sperre wirkt **sofort auf den Zustand**; die Auto-Vergabe berücksichtigt
  sie beim **nächsten Sync-Lauf** (≤ ein Poll-Zyklus).
- Ein **laufendes Spiel bleibt unangetastet** und zählt normal zu Ende. Der
  Menüeintrag heißt bei belegtem Feld deshalb ehrlich **„Feld nach diesem
  Spiel sperren"**.
- Eine bereits nach BTP geschriebene, noch unbestätigte Vergabe
  (`reserved_courts`-Fenster) **gewinnt** — die Sperre wirkt ab dem nächsten
  Spiel. Alles andere hieße, einen geschriebenen Zustand zurückzunehmen (R2).
- **Ohne Rückfrage.** Ein Fehlgriff richtet keinen Schaden an und ist mit
  einem zweiten Tipp zurückgenommen.

### Freigeben

- **Mit Rückfrage.** Das ist die gefährliche Richtung: Ein zweites TL-Gerät
  sieht nur „gesperrt" ohne Grund (Grund ist Nicht-Ziel) und gäbe ein kaputtes
  Feld frei — die Automatik legt sofort ein Spiel darauf.

### Halle ohne offenes Feld

Sperrt jemand das **letzte nicht gesperrte Feld einer Halle**, werden die
**automatisch** verteilten Hallen-Zuordnungen *dieser Halle* geräumt. Grund:
ADR 0030 bindet die Vergabe an die Halle; ohne das Räumen bekämen die dorthin
vorverteilten Spiele gar kein Feld mehr, obwohl in der Nachbarhalle welche
frei sind. Hand-, Regel- und Aufruf-Hallen bleiben unberührt (wie bei
`ClearAutoHalls`).

### Vor dem BTP-Import

Wie **alle** turnierbezogenen TL-Aktionen abgelehnt („Es ist noch kein
Turnier geladen"). Die Prüfung „existiert dieses Feld?" braucht den Snapshot
ohnehin, und das TL-Web zeigt vorher keine Felder an.

### Turnierwechsel

Die Sperren gelten für das **laufende Turnier**. Wechselt der Turniername im
Snapshot, werden sie verworfen. Siehe [ADR 0044](../adr/0044-sperrliste-turniergebunden.md).

### Sichtbarkeit des Menüeintrags

Der `TlState` trägt `can_lock_courts`; die Oberfläche zeigt den Eintrag nur,
wenn der Host ihn setzt. **Warum:** Das Relay bettet `tl.html` ein und wird bei
jedem main-Merge deployt, die App kommt erst über einen Release-Tag. Ein
älterer Host verwirft eine unbekannte `TlAction` **still** — das Gerät bekäme
nicht einmal eine Fehlermeldung. Das Merkmal entkoppelt den Merge vom Tag.

## Akzeptanzkriterien

- [ ] **E1** Im ⋯-Menü einer Feld-Kachel steht „Feld sperren" (freies Feld),
      „Feld nach diesem Spiel sperren" (belegtes Feld) bzw. „Feld freigeben"
      (gesperrtes Feld).
- [ ] **E2** „Feld sperren" wirkt ohne Rückfrage; „Feld freigeben" fragt nach.
- [ ] **E3** Nach dem Sperren vergibt `auto_assign` kein Spiel mehr auf dieses
      Feld — geprüft im selben Testlauf, nicht erst nach einem Neustart.
- [ ] **E4** Nach dem Sperren steht die Sperre in `config.locked_courts` **und**
      in `TabletState.locked_courts`. Schlägt das Schreiben der Config fehl,
      bleibt **beides** unverändert und das Gerät bekommt einen Fehler.
- [ ] **E5** Ein `save_config` aus dem Setup-Assistenten lässt `locked_courts`
      und `locked_courts_tournament` unverändert.
- [ ] **E6** Eine CourtID, die der aktuelle Snapshot nicht kennt, wird mit
      `TlErrorCode::NotAllowed` abgelehnt und verändert nichts.
- [ ] **E7** Ohne geladenes Turnier wird die Aktion abgelehnt.
- [ ] **E8** Dieselbe `opId` zweimal gesendet führt die Aktion **einmal** aus
      (Idempotenz über den bestehenden `remember_result`-Weg).
- [ ] **E9** Zwei TL-Geräte, die gleichzeitig schalten, führen zu genau einem
      konsistenten Endzustand („der letzte gewinnt", nie ein verlorener
      Schreibvorgang) — der Config-Zyklus läuft unter einem Guard.
- [ ] **E10** Wechselt der Turniername im Snapshot, ist die Sperrliste leer.
- [ ] **E11** Wird das letzte nicht gesperrte Feld einer Halle gesperrt, sind
      die Auto-Hallen-Zuordnungen dieser Halle geräumt; die anderer Hallen und
      die nicht-automatischen bleiben.
- [ ] **E12** Ein laufendes Spiel auf dem gesperrten Feld bleibt zugewiesen und
      lässt sich normal zu Ende werten.
- [ ] **E13** Ein Host **ohne** dieses Feature liefert `can_lock_courts = false`;
      die Oberfläche zeigt den Eintrag dann nicht.
- [ ] **E14** Bricht die Verbindung während des Kommandos ab, ist der Zustand
      entweder vollständig gesetzt oder unverändert — nie halb.

## Tests

Rust-Unit-Tests (TDD, vor der Umsetzung rot):

| Test | Ort | Sichert |
|---|---|---|
| Serde-Roundtrip `LockCourt` | `relay-proto` | Wire-Form |
| `every_tl_action()` ergänzt | `relay-proto` | Vollständigkeits-Wächter über alle Varianten |
| Sperre wirkt sofort auf `auto_assign` | `sync.rs` (Vorbild `auto_assign_skips_locked_court`) | E3, E4 |
| unbekannte CourtID ⇒ Ablehnung | `tl.rs` | E6 |
| ohne Snapshot ⇒ Ablehnung | `tl.rs` | E7 |
| `keep_host_managed_fields` erhält `locked_courts` | `commands.rs` | E5 |
| gleiche `opId` zweimal | `tl.rs` | E8 |
| Turnierwechsel leert die Sperren | `state.rs` | E10 |
| letztes offenes Feld ⇒ Auto-Hallen dieser Halle geräumt | `tl.rs` | E11 |

`cargo test --workspace`, `cargo clippy --workspace --all-targets -D warnings`
und `npm run build` müssen grün sein. **Manueller Turnier-Testfall:** Feld am
Tablet sperren, am PC eine Einstellung speichern, prüfen dass die Sperre steht
(das ist E5 im echten Ablauf).

## Risiken & Rollback

- **Im laufenden Turnier:** Die gefährliche Richtung ist das versehentliche
  **Ent**sperren — dagegen steht die Rückfrage. Das Sperren selbst kann
  höchstens ein Feld ungenutzt lassen, was sofort auffällt.
- **Das automatische Räumen der Auto-Hallen (E11)** ist der eingreifendste
  Teil: Es verschiebt Spiele in eine andere Halle. Es greift ausschließlich,
  wenn die Halle **kein einziges** offenes Feld mehr hat — dann ist die
  Alternative Stillstand.
- **Rollback:** Eine ältere Version liest die Config weiter (unbekanntes Feld
  wird ignoriert); die Sperren bleiben wirksam, nur der TL-Web-Weg fehlt
  wieder. Der Fix an `keep_host_managed_fields` fällt beim Rollback weg — der
  Datenverlust-Bug wäre dann wieder da, aber nicht schlimmer als heute.
- **Versions-Schere:** durch `can_lock_courts` entschärft (E13).

## Offene Fragen / Annahmen

- **A1 (geprüft, bestätigt):** Jedes gekoppelte TL-Gerät darf sperren, ohne
  zusätzliche Berechtigung. Dieselben Geräte dürfen heute schon werten und
  kampflos setzen; Sperren ist weniger eingreifend und umkehrbar.
  `slave_mode` ist ohnehin ausgeschlossen.
- **A2 (im Grill korrigiert):** Ursprünglich „keine Rückfrage" für beide
  Richtungen. Gilt nur fürs Sperren; das Entsperren bekommt eine.
- **A3 (im Grill präzisiert):** Der Menüeintrag benennt bei belegtem Feld, dass
  die Sperre erst nach dem Spiel greift.
- **Keine offenen Fragen.** Alle zehn Grill-Blocker sind entschieden
  (vier vom Nutzer, sechs begründet vom Umsetzenden — Herleitung in
  `_intake/tl-web-felder-sperren/2-grill.md`).

## Doku-Pflicht (CLAUDE.md-Tabelle)

`docs/turnierleitung-web.md` (Bedienung) · `docs/features/turnierleitung-web.md`
(Spec-Ergänzung) · `docs/cloud-relay.md` (neue `TlAction` auf dem Draht) ·
`docs/changelog.md` · Versions-Trippel.

**Zusätzliche Korrektur im selben Commit:**
`docs/features/hallen-vorverteilung.md` behauptet, `locked_courts` sei
„RAM-only: nach Neustart rechnet das Verhältnis mit allen Feldern" — das ist
falsch, seit die Sperren beim Start aus der Config geladen werden.
`docs/roadmap.md`: der Eintrag zum `keep_host_managed_fields`-Bug wird
geschlossen.
