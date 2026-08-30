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
**Panel-System** ersetzt: jeder Abschnitt ist einzeln dauerhaft
ein-/ausblendbar, zuklappbar, umsortierbar, einer Spalte zuordenbar und in
der Höhe frei verteilbar. Benannte, server-seitige **Profile** bündeln
diese Einstellungen an einem einzigen Ort (statt der heute auf drei
Bedienstellen verstreuten Konfigurierbarkeit) und lösen den
Tablet-vs-Großbildschirm-Zielkonflikt über Konfigurierbarkeit statt eine
einzige "richtige" Darstellung.

**Nachtrag 15.08.2026:** Im selben Umsetzungslauf kam ein zweiter,
vom Nutzer angestoßener Feinschliff dazu (siehe „Nachtrag" am Ende dieser
Spec und [ADR 0026](../adr/0026-spielliste-eine-globale-reihenfolge-eine-liste.md)):
die vier Warteliste-Unterabschnitte wurden zu **einem** Panel „Spiele"
zusammengeführt (Status jetzt ein Abzeichen an der Zeile statt eigener
Abschnitt), die manuelle Reihenfolge wurde von „je Halle" auf **eine
globale Reihenfolge** umgestellt, Panels wurden zusätzlich **zuklappbar**
(nicht nur aus-/einblendbar), und das zunächst verworfene
**Mehrspalten-Layout** wurde doch umgesetzt. Es sind jetzt **6 Panels**:
Felder, Walkover-Vorschläge, Zähltafel-Warteschlange, Schiedsrichter,
Spiele, Beendete Spiele.

**Messbare Erfolgskriterien (am nächsten Testturnier/Einsatz prüfbar):**
- Ein Turnierleiter kann alle Panels einzeln aus- und einblenden sowie
  auf-/zuklappen, ohne Erklärung zu benötigen (Zielgruppe: technisch
  unversierte Turnierleiter).
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
- ~~Kein Mehrspalten-Grid für die Listen-Panels~~ — **zurückgenommen am
  15.08.2026.** Dieses Nicht-Ziel beruhte auf einem Lesefehler: In der
  `/idee`-Phase waren *feste Spalten-Presets mit ziehbaren Breiten*
  gewählt worden; die Antwort wurde als „eine Spalte reicht" verstanden.
  Umgesetzt ist seither das **Mehrspalten-Layout** (1–3 Spalten je Profil,
  Zuordnung je Panel, ziehbare Spaltenbreiten) — siehe Abschnitt F des
  Umbauplans zur Spielliste. Kein freies Dashboard: feste Presets, kein
  automatisches Umbrechen. Mit ihm entfällt auch der Sonderstatus von
  „Felder" als fixierte Ankerposition, und die Einstellung „Spielliste
  rechts/darunter" geht in den Presets auf (`list_position` bleibt in
  Config und Wire erhalten und wird für Bestandsprofile einmalig in eine
  Spaltenaufteilung übersetzt).
- Keine automatische Migration bestehender `localStorage`-Anzeige-Werte —
  die neue Version startet mit einem eingebauten Standardprofil (heutige
  Default-Werte), TL richtet Profile einmalig neu ein.
  (Nachtrag 17.08.2026: Der **Schriftgrößen-Zoom** (`tlZoom`) ist bewusst
  wieder geräte-lokal in `localStorage` — er beschreibt nicht, WAS ein
  Bildschirm zeigt, sondern wie groß genau dieses Display es braucht.
  Nicht ins Profil migrieren.)
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

- [ ] Alle 6 Panels (Felder, Walkover, Zähltafel, Schiedsrichter, Spiele,
      Beendete Spiele) haben eine einheitliche Kopfzeile mit Titel, Zähler,
      Auf-/Zuklapp-Schalter und Ein-/Ausblend-Schalter.
- [x] **(Nachtrag)** Ein zugeklapptes Panel zeigt in der Kopfzeile eine
      Vorschau statt nur den Zähler: „Schiedsrichter" den nächsten
      Schiedsrichter, „Spiele" das nächste Spiel. Ein zugeklapptes Panel
      beansprucht keine Höhe im Steg-Verteilungsmodell.
- [x] **(Nachtrag)** Die manuelle Reihenfolge gilt **hallenübergreifend**
      (ADR 0026) — kein Präfix je Halle mehr. Die Spielliste zeigt keine
      Hallen-Gruppierung; stattdessen trägt jede Zeile ein Hallen-Kürzel
      (nur Anzeige). Gerufene Spiele stehen oben, alles Übrige folgt der
      manuellen bzw. BTP-Reihenfolge — Spielbereitschaft ist kein
      Sortierkriterium, nur noch ein Abzeichen an der Zeile.
- [x] **(Nachtrag)** Das Panel „Spiele" zeigt alle angesetzten Spiele
      (nichts wird weggelassen/gekürzt); die Länge wird durch
      Nachladen beim Scrollen beherrscht.
- [x] **(Nachtrag)** Das ⋮-Menü einer Spielzeile bietet zusätzlich „Nach
      oben schieben" und „Ergebnis eintragen"; die Feldkachel hat ein
      eigenes ⋯-Menü (einteilen, ansagen, Aufruf wiederholen, 2. Aufruf
      Partei A/B, Ergebnis eintragen; seit 17.08.2026 auch „📈
      Punktverlauf"). Ein Aufruf je Partei zählt die
      Aufruf-Stufe nicht doppelt, wenn beide Parteien nacheinander gerufen
      werden.
- [x] **(Nachtrag)** 1 bis 3 Spalten je Profil, Spalten-Zuordnung je
      Panel, Spaltenbreiten ziehbar. Unter dem Tablet-Breakpoint werden
      Spalten immer gestapelt, unabhängig vom Profil. „Felder" ist ein
      Panel wie jedes andere (keine fixierte Ankerposition mehr);
      „Spielliste rechts/darunter" wird für Bestandsprofile einmalig in
      eine Spaltenaufteilung übersetzt.
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
- [x] **Inhalts-Schalter gehören ins Profil, nicht ins Gerät** (bekräftigt
      18.08.2026): Die Achse des Panels „Spielzeiten"
      (`display.time_stats_axis`, Spec `tl-sicht-feinschliff`) beschreibt,
      WAS gezeigt wird, und liegt deshalb im Profil — anders als der
      Schriftgrößen-Zoom, der eine reine Display-Eigenschaft ist. Ein Profil
      ohne das Feld liest sich als „Konkurrenz", also als die bisherige
      Ansicht.
- [x] **Die Höhenverteilung hängt allein an `height_fr`, nicht am Inhalt**
      (nachgeschärft 18.08.2026 nach einem Fehlerbericht): Wächst der Inhalt
      eines Panels — die Spielliste lädt beim Scrollen nach —, ändert das
      seine Höhe nicht. Umsetzung: `.panel { flex-basis: 0 }` in `tl.html`;
      mit dem Vorgabewert `auto` ist die Flex-Basis die Inhaltshöhe, und
      dann verdrängt ein wachsendes Panel seine Nachbarn bis auf
      `--panel-min`. Zugleich Voraussetzung für das Kriterium darüber:
      `flex-grow` — das, was der Steg schreibt — verteilt **nur freien
      Raum**; übersteigt die Summe der Inhaltshöhen die Spalte, gibt es
      keinen, und das Ziehen bleibt wirkungslos. Zugeklappte Panels bleiben
      ausgenommen (`.panel.panel-collapsed { flex: none }`) und behalten
      exakt ihre Kopfzeilenhöhe.
- [x] **Ein Bedarfs-Deckel darf den Steg nicht blockieren** (18.08.2026,
      aus demselben Bericht): `fitCourts()` deckelt „Felder" per
      Inline-`max-height` auf seinen Bedarf. Zwei Dinge müssen dafür
      stimmen, und beide stimmten nicht: Der Deckel muss die **Kachelbox**
      messen, nicht `scrollHeight` des Panel-Körpers (der ist nach unten
      durch `clientHeight` geklammert, der Deckel wäre also stets die
      aktuelle Höhe und gäbe nie etwas ab) — und `wireStegs()` muss den
      Deckel beim `pointerdown` räumen, weil `max-height` jedes
      `flex-grow` schlägt und das Panel sonst unter dem Finger stehen
      bleibt. Prüfvorgehen: `<style>`-Block aus `tl.html` extrahieren, eine
      `.tl-column` mit zwei Panels und `.panel-steg` aufbauen, Höhen vor
      und nach der Änderung messen. Für die Inline-Skripte/CSS der Assets
      gibt es keinen automatisierten Harness — diese Messung ist der
      Ersatz und bei Änderungen an `fitCourts()`/`wireStegs()` zu
      wiederholen.
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

## Nachtrag 15.08.2026 — Spielliste vereinfacht, Mehrspalten-Layout

Im selben Umsetzungslauf, direkt im Anschluss an die erste Fassung, kamen
auf Wunsch des Nutzers fünf weitere Änderungen dazu. Vollständiger Plan:
`docs/features/_intake/tl-liste-vereinfachen/plan.md`. Architektur-
Entscheidung: [ADR 0026](../adr/0026-spielliste-eine-globale-reihenfolge-eine-liste.md)
(löst [ADR 0023](../adr/0023-manuelle-spielreihenfolge-praefix-je-halle.md) ab).

- **A — Sortierung hallenunabhängig.** `QueueOrderStore` führt eine
  einzige globale `Vec<i64>` statt einer Map je Halle. Der Wire-Vertrag
  (`TlAction::QueueReorder`/`QueueOrderReset`) änderte sich nicht — er trug
  nie eine Halle. Der bestehende Schutz gegen die Hallengrenze
  (`sync.rs::auto_assign::require_call`) hängt **nicht** am Sortier-Präfix,
  sondern an der Aufruf-Pflicht im Mehr-Hallen-Betrieb — die Umstellung ist
  dadurch gefahrlos. Eine bestehende `queue-order.json` im alten
  Map-Format wird nicht migriert (neuer Feldname `queue` statt `order`);
  das Turnier startet mit leerem Präfix, Turnierbindung bleibt erhalten.
- **B — keine Hallen-Gruppierung in der Spielliste.** Ersetzt durch ein
  Hallen-Kürzel je Zeile (kürzestes eindeutiges Präfix über alle Hallen).
  Die Gruppierung der Feldkacheln nach Halle bleibt unverändert — sie ist
  Voraussetzung des Feld-Rasters.
- **C — eine Spielliste statt vier Unterabschnitten.** „In Vorbereitung
  gerufen"/„Spielbereit"/„Noch nicht bereit"/„Ohne Hallenzuordnung" sind
  jetzt ein Panel „Spiele" mit Status als Zeilen-Abzeichen. Alle
  angesetzten Spiele werden gezeigt, mit Nachladen beim Scrollen.
- **D — Panels zuklappbar.** Neues Profil-Feld `TlPanelSetting.collapsed`,
  zweite Dimension neben `visible`.
- **E — Feldkachel entschlackt** (⋯-Menü) **+ Aufruf je Partei am Feld.**
  `TlAction::AnnounceCourtCall` bekam ein optionales `side` (Roadmap-Punkt
  „Gezielter zweiter/dritter Aufruf — auch je Partei" damit erledigt). Die
  Aufruf-Stufe zählt weiterhin einmal je Feld (Zusage: alle Geräte zeigen
  dieselbe Zahl); eine Parteien-Maske der laufenden Runde verhindert
  Doppelzählung, wenn nacheinander beide Parteien gerufen werden.
  (Nachtrag 17.08.2026: Mit der Anzeige-Option „Aufrufe unbegrenzt" —
  `TlDisplaySettings.unlimited_court_calls` — zählt der Host über 3
  hinaus; die Zusage „alle Geräte, eine Zahl" gilt unverändert, siehe
  `docs/announcements.md`.)
  (Nachtrag 30.08.2026: Dazu kam die Anzeige-Option **„Spiele mit noch
  offener Paarung zeigen"** — Spec
  [`tl-offene-paarungen`](tl-offene-paarungen.md). Gespeichert wird sie
  **invertiert** als `TlDisplaySettings.hide_open_matches` /
  `hideOpenMatches`: Beide Typen sind `#[serde(default)]` mit reinen `bool`s,
  ein fehlendes Feld kommt also als `false` zurück. Hieße es
  `show_open_matches`, stünde jedes Profil, das es vor diesem Update schon
  gab, nach dem Auto-Update auf „aus" — und niemand sähe die neue Anzeige,
  bis er den Schalter fände. So herum gilt für alle Profile der gewollte
  Standard „anzeigen". Das Häkchen im Editor fragt trotzdem positiv; die
  Umkehrung passiert beim Übernehmen. Es erscheint nur, wenn der Turnier-PC
  überhaupt eine `open_queue` schickt — sonst stünde an einem älteren Host
  ein wirkungsloses Häkchen.)
- **F — Mehrspalten-Layout**, siehe Akzeptanzkriterien oben und ADR 0025
  (Nachtrag dort). `TlPanelProfile.columns`/`column_widths`,
  `TlPanelSetting.column` — reisen auf demselben Weg wie der übrige
  Profil-Inhalt, kein neuer Datenpfad.

**Betroffene zusätzliche Komponenten:** `tablet/queue_order.rs`,
`tablet/assign.rs`, `sync.rs`, `tablet/state.rs`, `badhub/payload.rs`,
`tests/queue_order_consistency.rs`, `src/pages/PreparationPanel.tsx`,
`src/components/AnnounceJobPlayer.tsx`, `src/io/announceCourt.ts`.

**Zusätzliche Doku-Pflicht:** `docs/features/spielliste-manuelle-reihenfolge.md`
(Zielbild/Akzeptanzkriterien auf global umstellen), `docs/btp_protocol.md`
(„je Halle" streichen), `docs/preparation.md` (Präfix global + Liveticker-
Hinweis: ein langer Präfix aus einer Halle kann alle 15
`display=next`-Plätze belegen), `docs/announcements.md` (Aufruf je Partei
am Feld), `docs/adr/0023-…md` (Status auf „superseded by ADR-0026").
