# <Feature-Titel> — Spezifikation

> Status: **Entwurf | abgestimmt <YYYY-MM-DD>** (via /idee: Brief → Grill → How-To → Review).
> Quelle: <Idee | Meeting-Transkript vom YYYY-MM-DD>. Betroffene Crates: <src-tauri / relay / relay-proto / src>.
> ADR: <docs/adr/NNNN-…md oder „keiner nötig">.

<!--
Vorlage für Specs aus dem `idee`-Skill (Phase 4). Beim Finalisieren: alle Platzhalter füllen oder
Abschnitt entfernen, keine <TBD>/<TODO> stehen lassen, personenbezogene Daten aus Transkripten
entfernen. Konzept: docs/spec-pipeline-konzept.md
-->

## Kontext / Problem

<!-- Welches echte Problem, für wen (Turnierleiter, Schiedsrichter, Zuschauer am Liveticker)?
     Was ist der Auslöser (Idee/Turnier-Erfahrung/Meeting)? Belege/Beispiele. -->

## Zielbild & Erfolgskriterien

<!-- Was soll nach Umsetzung möglich sein? Messbare Erfolgskriterien. Zielgruppe sind
     Turnierleiter ohne technischen Hintergrund — „funktioniert ohne Erklärung" ist ein Kriterium. -->

## Nicht-Ziele

<!-- Was wird bewusst NICHT gebaut (Scope-Grenze). -->

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:** <src-tauri/src/… · relay/ · relay-proto/ · src/pages|components — neu oder Erweiterung>
- **Architekturregeln (CLAUDE.md R1–R6):** <R1 nur Tauri-Commands? · R2 BTP ist die Wahrheit
  (SENDTOURNAMENTINFO/SENDUPDATE)? · R3 LAN vs. Cloud-Relay — beide Pfade bedacht? · R4 ein Host
  je Namespace, ein aktives Tablet je Court? · R5 Validierung in `process_result`? · R6 `install_id`>
- **Konfiguration & Abwärtskompatibilität:** <neue Felder in `config.rs`? Bestehende Installationen
  bekommen das per Auto-Update — Default/Migration definiert? `identifier` de.badhub.btslight und
  Updater-Pfad `download/bts-light/` bleiben unangetastet>
- **Datenschutz:** <kein Geburtsjahr speichern/anzeigen/loggen; Spielernamen nur zum Liveticker-Zweck;
  im Zweifel Feld weglassen. Embedded Secrets sind bewusst und kein Befund.>
- **Abhängigkeiten:** <BTP-Version/Protokoll? badhub-Endpunkt? Relay/nginx? neue Cargo-/npm-Dependency
  (Notwendigkeit, Pflege, Lizenz — globale Regel 5)?>

## Akzeptanzkriterien

<!-- Jedes als konkreter, testbarer Satz. Positiv- UND Negativ-/Fehlerfälle
     (Netz weg, BTP-Neustart, Tablet-Reconnect, doppelte Ergebnismeldung …). -->
- [ ] <…>
- [ ] <…>

## Tests

<!-- TDD ist Pflicht: welche Rust-Unit-Tests (Serde-Roundtrips, Broker-Routing, Parser-Regression,
     Validierung)? `cargo test` grün, `npm run build` fehlerfrei. Manueller Turnier-Testfall? -->

## Risiken & Rollback

<!-- Was kann schiefgehen — im laufenden Turnier? Ist die Änderung zurückrollbar
     (ältere Version installierbar, Config bleibt lesbar)? -->

## Offene Fragen / Annahmen

<!-- Verbleibende offene Punkte aus dem Grill; explizite Annahmen. -->

## Betroffene Doku-Dateien

<!-- Pflicht laut CLAUDE.md: welche docs/**/*.md werden im selben Commit gepflegt?
     Großes Feature → eigene docs/<feature>.md statt Sektion in fremder Datei.
     Jede veröffentlichte Version zusätzlich docs/changelog.md. -->

## Umsetzungs-Hinweise

<!-- Erst NACH Freigabe relevant. Ergebnis der How-To-Phase: Reihenfolge kleiner Schritte,
     Review-Bedarf (code-reviewer Pflicht; security-reviewer bei neuem User-Input/Auth/Datei-URL),
     Version gemeinsam bumpen in src-tauri/Cargo.toml + src-tauri/tauri.conf.json + package.json,
     Verweis auf ADR falls vorhanden. -->
