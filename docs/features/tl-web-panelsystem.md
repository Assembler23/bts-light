# TL-Web Panelsystem — Spezifikation

> Status: **abgestimmt 2026-08-15** (via /idee: Brief → Grill → How-To → Review).
> Quelle: UX-Evaluation der Turnierleitungs-Oberfläche vom 14.08.2026 (additiv
> gewachsene Unübersichtlichkeit nach mehreren Feature-Sessions). Betroffene
> Crates: src-tauri (config, tablet), relay, relay-proto.
> ADR: [0024](../adr/0024-tl-panel-profile-verwaltung-im-web.md) (Profil-Verwaltung
> als zweite Web-Ausnahme), [0025](../adr/0025-tl-panel-profile-transport-persistenz.md)
> (Transport & Persistenz).

## Kontext / Problem

`src-tauri/assets/tl.html` (Turnierleitungs-Oberfläche, LAN + Cloud-Relay) ist
über viele Feature-Sessions additiv gewachsen: 4291 Zeilen, 9 permanent im DOM
verankerte Abschnitte (Felder, Walkover-Vorschläge, Zähltafel-Warteschlange,
Schiedsrichter, 4× Queue-Untergruppen, Beendet), 5 unkoordinierte
Badge-Farbfamilien, eine fix im HTML verdrahtete Panel-Reihenfolge und nur ein
Layout-Breakpoint (1100px). Turnierleiter berichten (und eine strukturierte
Bestandsaufnahme vom 14.08.2026 bestätigt), dass die Seite unübersichtlich
geworden ist: kein Abschnitt lässt sich dauerhaft ausblenden, keine
Priorisierung nach Dringlichkeit über Abschnittsgrenzen hinweg, Queue-Zeilen
mit bis zu 10 Elementen, drei unabhängige Drag&Drop-Mechanismen mit
uneinheitlichem visuellem Vokabular.

Betroffen sind Turnierleiter an allen Gerätetypen — Tablet im Handbetrieb
genauso wie große Wandmonitore/Beamer im Dauerbetrieb — mit gegensätzlichen
Bedürfnissen (Tablet: kompakt, Touch-tauglich; Wandmonitor: möglichst viel
Information gleichzeitig sichtbar).

## Zielbild & Erfolgskriterien

Die additiv gewachsene, starre Abschnitts-Liste wird durch ein einheitliches
**Panel-System** ersetzt: jeder der 9 Abschnitte ist einzeln dauerhaft
ein-/ausblendbar, umsortierbar und in der Höhe frei verteilbar. Benannte,
server-seitige **Profile** bündeln diese Einstellungen an einem einzigen Ort
(statt der heute auf drei Bedienstellen verstreuten Konfigurierbarkeit) und
lösen den Tablet-vs-Großbildschirm-Zielkonflikt über Konfigurierbarkeit statt
eine einzige "richtige" Darstellung.

**Messbare Erfolgskriterien (am nächsten Testturnier/Einsatz prüfbar):**
- Ein Turnierleiter kann alle 9 Panels einzeln aus- und einblenden, ohne
  Erklärung zu benötigen (Zielgruppe: technisch unversierte Turnierleiter).
- Ein für ein Gerät gewähltes Profil übersteht Seiten-Reload zuverlässig
  (Geräte-Bindung, nicht nur Browser-Session).
- Queue-Zeilen zeigen ohne aufgeklapptes Kontextmenü höchstens 6 sichtbare
  Elemente (statt heute bis zu 10) — Sekundäres liegt hinter einem Kebab.
- Alle Badges lassen sich eindeutig einer von drei Dringlichkeitsstufen
  zuordnen; niemand im Testturnier verwechselt eine Info-Badge (z. B.
  Matchball) mit einer Alarm-Anzeige (überfällig).
- Die Testturnier-Checkliste (siehe „Risiken & Rollback") besteht vollständig
  auf iPad Safari, Android Chrome und einem Wandmonitor, bevor das Feature
  bei einem echten Turnier eingesetzt wird.

## Nicht-Ziele

- Andere Anzeige-Seiten (`assets/tablet.html`, `assets/overview.html`,
  `assets/monitor.html`, `assets/preparation.html`) bleiben unangetastet —
  andere Zielgruppen, anderes Problem.
- Kein Mehrspalten-Grid für die Listen-Panels — eine Spalte bleibt, mit frei
  verteilbarer Höhe je Panel statt Breiten-Aufteilung. Bewusste Abwägung,
  keine Verwässerung: Feldkacheln wachsen bereits selbst in der Breite
  (auto-fill Grid), nur die Listenspalte bleibt schmal, das wird nicht durch
  ein zweites, komplexeres Layoutsystem gelöst.
- Keine automatische Migration bestehender `localStorage`-Anzeige-Werte —
  die neue Version startet mit einem eingebauten Standardprofil (heutige
  Default-Werte), TL richtet Profile einmalig neu ein.
- Kein Parallelbetrieb alt/neu, kein Umschalt-Flag — direkter Ersatz,
  abgesichert durch ein dediziertes Testturnier vor dem ersten echten
  Einsatz.
- Bearbeitung des Hallen-Raster-Layouts (`state.layouts`) bleibt
  ausschließlich im React-Setup-Wizard (`FieldOverviewPage.tsx`).
- Keine geräte-lokale Override-Ebene über dem Profil — Profil ist
  verbindlich. Wer abweichende Einstellungen will, wechselt/erstellt ein
  eigenes Profil.
- Keine neuen fachlichen Turnier-Workflows — reine Struktur-/
  Darstellungs-Überarbeitung bestehender Abschnitte.
- **Zweite, bewusste Ausnahme von „kein Setup aus dem Web"** (siehe
  [ADR 0024](../adr/0024-tl-panel-profile-verwaltung-im-web.md)): Profil-
  Verwaltung (Anlegen/Bearbeiten/Löschen/Wählen) läuft direkt in `tl.html`,
  nicht im Setup-Wizard — begründete Ausnahme, kein Rückfall in alte Debatte.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - `src-tauri/src/config.rs` — neue Typen `TlPanelProfile`, `TlPanelSetting`,
    `TlDisplaySettings`, `TlListPosition`; `TlWebConfig.profiles`/
    `default_profile_id`; `TlDevice.profile_id`.
  - `src-tauri/src/commands.rs` — `keep_host_managed_fields`,
    `identity_bundle`, `apply_imported_identity` erweitert.
  - `relay-proto/src/lib.rs` — `TlAction::ProfileSave/ProfileDelete/
    ProfileSelect/ProfileSetDefault`, `TlAuthDevice.profile_id`,
    `MAX_TL_PROFILES`.
  - `src-tauri/src/tablet/tl.rs` — `profiles_view`, neue `execute()`-Arme,
    `touches_courts`/`action_fingerprint`/`action_label` erweitert.
  - `src-tauri/src/tablet/server.rs` — `X-Tl-Active-Profile`-Header auf dem
    LAN-Pfad.
  - `src-tauri/src/tablet/relay_client.rs` — `push_tl_auth` mirrort
    `profile_id`.
  - `relay/src/main.rs` — `tl_token_profile`-Map, Header auf dem Cloud-Pfad.
  - `src-tauri/assets/tl.html` — Panel-System, Profil-Verwaltungs-UI,
    Badge-Vereinheitlichung, Queue-Zeile entschlackt.
  - **Nicht betroffen:** `src/pages/TlWebPanel.tsx` (keine neue
    Tauri-Command-Oberfläche für Profile).

- **Architekturregeln (CLAUDE.md R1–R6):**
  - R1: greift nicht für Profil-CRUD — läuft über `TlAction`/
    `tl.rs::execute`, keine neue Tauri-Command-Grenze.
  - R2: unberührt, Profile enthalten keine Turnierdaten.
  - R3: zentral — Katalog eingebettet in `TlState` (geteilt, LAN+Cloud
    identisch), individuelle Zuordnung über `TlAuth`-Spiegel +
    `X-Tl-Active-Profile`-Header auf beiden Pfaden (siehe
    [ADR 0025](../adr/0025-tl-panel-profile-transport-persistenz.md)).
  - R4: `MAX_TL_PROFILES` begrenzt den Katalog, damit `TlState` unter dem
    64-KB-Limit bleibt.
  - R5: jede Profil-Mutation läuft einmal in `tl.rs::execute`, von
    LAN-Server und Relay-Client gleichermaßen aufgerufen.
  - R6: kein Namespace-Bezug nötig, Profile hängen an `AppConfig`/`TlDevice`.

- **Konfiguration & Abwärtskompatibilität:** neue Felder in `config.rs`, alle
  `#[serde(default)]` — bestehende `config.json` ohne diese Felder lädt
  unverändert mit leeren Defaults. Ohne Profile im Katalog liefert `tl.html`
  ein eingebautes Standardprofil (heutige Default-Werte: Spielnummer an,
  Nationen/Vereine aus, Disziplin/Runde/Gruppe an, Liste rechts, alle 9
  Panels sichtbar in heutiger Reihenfolge/Höhe). Kein Breaking Change.
  `identifier` `de.badhub.btslight` und Updater-Pfad `download/bts-light/`
  unangetastet.
- **Datenschutz:** Profile speichern ausschließlich Layout-/
  Sichtbarkeits-Einstellungen, keine personenbezogenen Daten. Der
  bestehende Datensparsamkeits-Wächter-Test in `tablet/tl.rs` wird um
  Profile erweitert (keine Personendaten-Felder in `TlPanelProfile`).
- **Abhängigkeiten:** keine neue Cargo-/npm-Dependency — Fortführung des
  bestehenden Vanilla-JS/Pointer-Events-Musters in `tl.html`.

## Akzeptanzkriterien

- [ ] Alle 9 Panels (Felder, Walkover, Zähltafel, Schiedsrichter, 4×
      Queue-Untergruppen, Beendet) haben eine einheitliche Kopfzeile mit
      Titel, Zähler und Ein-/Ausblend-Schalter.
- [ ] Ein ausgeblendetes Panel bleibt nach Seiten-Reload ausgeblendet
      (persistiert im Profil, nicht nur im DOM-Zustand).
- [ ] Panels lassen sich per Drag umsortieren; das Griffsymbol unterscheidet
      sich visuell klar vom bestehenden ⠿-Symbol der Item-Drag-Listen
      (Queue, Schiedsrichter, Feldkacheln).
- [ ] Jede Grenze zwischen zwei sichtbaren Panels ist ziehbar (Höhe frei
      verteilbar); ist das direkt benachbarte Panel ausgeblendet, bindet
      sich der Rand automatisch an das nächste sichtbare Panel.
- [ ] Panel-Höhen sind gegen ein Mindestmaß geklammert (kein Panel wird auf
      0/negative Höhe gezogen); Doppeltipp auf einen Rand setzt die
      automatische Verteilung zurück.
- [ ] Ein Turnierleiter kann in `tl.html` ein Profil anlegen, benennen,
      bearbeiten, löschen und als Standard markieren.
- [ ] Jedes Gerät wählt ein Profil; die Wahl ist an die Geräte-Identität
      gebunden und übersteht Seiten-Reload, sowohl über LAN als auch über
      Cloud-Relay.
- [ ] Wird das einem Gerät zugewiesene Profil gelöscht, fällt das Gerät beim
      nächsten Poll automatisch auf das Standardprofil zurück, ohne
      Fehlermeldung.
- [ ] Bearbeiten zwei Geräte gleichzeitig dasselbe Profil, gewinnt die
      zuletzt gespeicherte Änderung (Last-Write-Wins), ohne Datenkorruption
      und ohne dass eine Fehlermeldung erscheint.
- [ ] Alle Badges (inkl. des bisher unbenannten „gesperrt"-Badges) sind einer
      von drei Stufen zugeordnet: Neutral/Info (u. a. Matchball, Satzball,
      „Manuell einsortiert"), Warnung (u. a. Feldvergabe-Ausnahme,
      Schiedsrichter-Konflikt), Alarm (u. a. überfällig, gesperrt,
      Verletzung, TL gerufen). Die rote, pulsierende Matchball-Kennzeichnung
      bleibt optisch von Alarm-Rot („überfällig" am Feld-Streifen)
      unterscheidbar.
- [ ] Eine Queue-Zeile zeigt ohne geöffnetes Kontextmenü höchstens 6
      sichtbare Elemente; Nachruf-Buttons und Auto-Ausnahme-Umschalter
      liegen hinter einem Kebab-Menü.
- [ ] Bestehende Touch-Ergonomie bleibt erhalten: alle interaktiven Ziele
      ≥ 40–44 px, Drag-Griffe nutzen `touch-action: none` exakt am Griff,
      Pfeiltasten-Fallback funktioniert weiterhin für alle Drag-Listen
      (Item- und Panel-Ebene).
- [ ] Eine `config.json` aus einer älteren Version (ohne die neuen Felder)
      lädt unverändert; `tl.html` zeigt in dem Fall das eingebaute
      Standardprofil.
- [ ] Ein Identitäts-Export (PC-Umzug, ADR 0006) nimmt den Profil-Katalog
      mit, aber keine Geräte-Profil-Zuordnung (konsistent mit dem
      bestehenden Verwerfen von Token/Label bei Geräten).
- [ ] `security-reviewer` hat die Profil-Verwaltungs-UI und das
      Header-basierte Zuordnungs-Routing im Relay geprüft, insbesondere:
      kein Leak von Profil-Zuordnungen über Namespace-Grenzen hinweg.

**Negativ-/Fehlerfälle:**
- [ ] Netz weg während offenem Profil-Editor: keine Datenkorruption, die
      zuletzt lokal sichtbare Eingabe geht beim nächsten erfolgreichen
      Speichern normal durch (kein Sonderverhalten nötig, bestehendes
      TlAction-Verhalten).
- [ ] Unbekanntes/abgelaufenes Token: kein `X-Tl-Active-Profile`-Header,
      kein Fallback auf ein falsches Profil.
- [ ] BTP-Neustart während laufender Panel-Konfiguration: Panel-/Profil-
      Zustand bleibt erhalten (unabhängig vom BTP-Zustand).

## Tests

TDD-Pflicht, siehe detaillierte Liste in
`docs/features/_intake/tl-web-panelsystem/3-how-to.md` Abschnitt 6:

- `config.rs`: Serde-Roundtrip `TlPanelProfile`, Default-Verhalten bei
  fehlenden Feldern (alte `config.json`), `TlDevice.profile_id`-Default.
- `commands.rs`: `identity_bundle` strippt `profile_id`, behält den
  Profil-Katalog; `apply_imported_identity`/`keep_host_managed_fields`
  schützen live editierte Profile.
- `relay-proto`: Serde-Roundtrip der vier neuen `TlAction`-Varianten,
  `TlAuthDevice.profile_id` mit/ohne Wert.
- `relay/src/main.rs`: Header wird korrekt gesetzt (200 und 304), fehlt bei
  unbekanntem Token, wird bei Token-Widerruf aus der Map entfernt.
- `tablet/tl.rs`: `profiles_view`, `execute()`-Arme (Upsert, ID-Generierung,
  Fallback auf Standard bei Löschung, Last-Write-Wins, Kappung bei
  `MAX_TL_PROFILES`, Sicherheitstest „Gerät kann nur sich selbst binden"),
  Datensparsamkeits-Wächter-Test erweitert.
- `server.rs`: LAN-Pfad setzt denselben Header, kein Leak zwischen zwei
  Geräten in derselben Testreihe.

`cargo test --workspace` grün, `npm run build` fehlerfrei. Manueller
Testturnier-Testfall: siehe „Risiken & Rollback".

## Risiken & Rollback

**Risiko:** Komplettersatz einer 4291-Zeilen-Live-Produktionsdatei ohne
Parallelbetrieb/Flag. Mitigation: dediziertes Testturnier VOR dem ersten
echten Einsatz, mit geräte-/szenariobasierter Checkliste (analog zur
ursprünglichen TL-Web-Spec):
- iPad Safari, Android Chrome und ein Wandmonitor: je einmal Profil
  anlegen/bearbeiten/löschen/wählen.
- Profilwechsel übersteht Reload.
- Gelöschtes, aktuell zugewiesenes Profil fällt sichtbar auf Standard
  zurück.
- WLAN aus/an während offenem Profil-Editor.
- BTP-Neustart — Panel-Konfiguration bleibt erhalten.
- Zwei Geräte bearbeiten gleichzeitig dasselbe Profil (Last-Write-Wins ohne
  Fehlermeldung).
- Drag-Rand bindet sich korrekt an nächsten sichtbaren Nachbarn bei
  ausgeblendetem Zwischen-Panel.

**Rollback:** Ältere App-Version ignoriert die neuen Config-Felder
(`#[serde(default)]`) schlicht — eine `config.json`, die mit der neuen
Version geschrieben wurde, bleibt mit einer älteren Version lesbar
(zusätzliche Felder werden von Serde beim Deserialisieren ignoriert, sofern
kein `deny_unknown_fields` gesetzt ist — im How-To zu bestätigen). Release-
Tags nur durch Admin (bekannte Einschränkung), ein spontaner Rollback
mitten im Turnier ist praktisch nicht durchführbar — genau deshalb ist das
Testturnier vor dem ersten echten Einsatz Pflicht, nicht optional.

## Offene Fragen / Annahmen

- Ob Serde beim Laden einer neuen `config.json` mit einer älteren
  App-Version die unbekannten neuen Felder tatsächlich stillschweigend
  ignoriert (kein `deny_unknown_fields` im relevanten Struct-Pfad), ist im
  Implementierungsschritt zu verifizieren, nicht nur angenommen.
- Panel-Höhen als relative Einheiten (`height_fr`) + Viewport-Klammerung:
  konkretes Mindestmaß (Analogie `--liste-min`) wird beim Umsetzen der
  `tl.html`-Panel-Grenzen festgelegt, nicht in dieser Spec vorab beziffert.
- Ein verlorenes/neu gekoppeltes Gerät bekommt eine neue Geräte-Identität —
  die Profilzuweisung geht damit bei jedem Geräteaustausch verloren
  (akzeptierte Konsequenz, kein Blocker: Gerät fällt automatisch auf
  Standardprofil zurück, TL wählt bei Bedarf neu).

## Betroffene Doku-Dateien

- **Neu:** diese Datei (`docs/features/tl-web-panelsystem.md`).
- `docs/turnierleitung-web.md` — „Anzeige"-Abschnitt durch „Profile"
  ersetzen, Panel-System-Bedienung neu beschreiben, Badge-Tabelle auf 3
  Stufen umstellen.
- `docs/features/turnierleitung-web.md` — Querverweis auf diese Spec,
  veraltete `localStorage`-basierte Akzeptanzkriterien als abgelöst
  markieren, Nicht-Ziel-Klausel um die zweite Ausnahme ergänzen.
- `docs/cloud-relay.md` — neuer Abschnitt „Panel-Profile" (`TlAuthDevice
  .profile_id`, `X-Tl-Active-Profile`-Header, neue `TlAction`-Varianten in
  der bestehenden Tabelle).
- `CLAUDE.md` — Doku-Tabellenzeile TL-Web um die neuen Code-Pfade erweitern.
- `docs/changelog.md` — bei Release.
- `docs/roadmap.md` — Verweis auf diese Spec.

## Umsetzungs-Hinweise

Vollständiger Plan in
`docs/features/_intake/tl-web-panelsystem/3-how-to.md`. Kurzfassung,
Reihenfolge:

1. Profil-Datenmodell + Serde-Tests (`config.rs`).
2. Persistenz-Layer (`commands.rs`).
3. `relay-proto`-Typen + Roundtrip-Tests.
4. Broker-Routing (`relay/src/main.rs`, `X-Tl-Active-Profile`-Header).
5. `tablet/tl.rs` (`profiles_view`, `execute()`-Arme).
6. `server.rs` (LAN-Header).
7. `relay_client.rs` (`push_tl_auth`).
8. `tl.html` — Profil-Verwaltungs-UI (**security-reviewer Pflicht**).
9. `tl.html` — Panel-System (Kopfzeile, Sichtbarkeit, Reorder mit eigenem
   Griffsymbol, generalisierter Steg mit Nachbar-Bindung).
10. `tl.html` — Badge-Vereinheitlichung (3 Stufen).
11. `tl.html` — Queue-Zeile entschlacken (Kebab für Sekundäres).
12. Doku (siehe oben).
13. Testturnier-Checkliste vor erstem Produktiveinsatz.

Schritte 1–7 werden einzeln grün, bevor die große `tl.html`-Überarbeitung
beginnt. `code-reviewer` nach jedem Schritt Pflicht; `security-reviewer`
verbindlich für Schritt 8 (neue Web-Schreib-Oberfläche) und Schritt 4
(Header-basierte Personalisierung im Relay, kein Leak über
Namespace-Grenzen).

Version-Bump (`src-tauri/Cargo.toml` + `tauri.conf.json` + `package.json`
gemeinsam) erst im letzten Schritt, nicht pro Zwischenschritt.
