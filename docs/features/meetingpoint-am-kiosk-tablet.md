# Meetingpoint am Kiosk-Tablet — Spezifikation

> Status: **abgestimmt 2026-11-09** (via /idee: Brief → Grill → How-To → Review, 09.11.2026).
> Quelle: Idee vom 09.11.2026. Betroffene Crates: src-tauri (Assets + `lobby_page`), `src/io` (reine Logik).
> ADR: keiner nötig (keine neue Schnittstelle, `/courts` bleibt unverändert).

## Kontext / Problem

Die Turnierleitung möchte ein oder mehrere der vorhandenen Kiosk-Tablets am Meetingpoint auslegen,
damit Spieler sich dort selbst einchecken. Die Meetingpoint-Seite und das Layout `meetingpoint` der
Anzeige-Hülle gibt es schon ([meetingpoint.md](../meetingpoint.md)). Der Weg dorthin ist aber für
ein Kiosk-Tablet umständlich und hat Nebenwirkungen:

- Die Kiosk-App startet immer mit der Feldwahl `http://<Turnier-PC>:8088/felder`.
- Von dort geht es nur über **Feld antippen** (belegt das Feld zum Zählen!) → Zahnrad → PIN →
  „Anzeige (nur Spielstand)" → Zahnrad → Anzeige wählen → Meetingpoint.
- Aus der Hülle gibt es bei einem feldlosen Layout keinen Rückweg zur Feldwahl („Zum Zählen
  wechseln" ist dort ausgeblendet).
- Nach jedem WLAN-Aussetzer oder Ladefehler lädt die Kiosk-App wieder `/felder`. Ein
  unbeaufsichtigtes Meetingpoint-Tablet steht dann auf der Zähl-Lobby, wo Spieler ein Feld antippen
  und es zum Zählen belegen können.

## Zielbild & Erfolgskriterien

- In der Feldwahl gibt es einen Eintrag **Meetingpoint**, der nach PIN-Eingabe direkt die
  Meetingpoint-Anzeige öffnet, ohne ein Feld zu belegen.
- Fällt ein Meetingpoint-Tablet auf die Feldwahl zurück, bringt es ein großer Knopf **Zurück zum
  Meetingpoint** ohne PIN wieder dorthin — bedienbar von jedem Spieler, ohne Erklärung.
- Aus der Meetingpoint-Anzeige kommt man hinter der PIN mit **Zur Feldwahl** zurück.
- Erfolg beim nächsten Turnier: ein Tablet wird in unter einer Minute vom Zähl- zum
  Meetingpoint-Tablet und zurück, ohne dass ein Feld belegt wird.

## Nicht-Ziele

- **Keine automatische Rückkehr** zum Meetingpoint nach Neustart/WLAN-Verlust (Entscheidung
  09.11.2026); nur der Hinweis-Knopf.
- **Keine ferne Halle:** Die Feldwahl eines Slave-PCs (`slave_bridge.rs` `felder_page_html`) bekommt
  keinen Eintrag; die Slave-Brücke hat keine `/anzeige`-Route. Dort bleibt
  `http://<Slave>:8088/meetingpoint` im Browser.
- **Keine Cloud:** `/felder` gibt es nur im LAN. „Zur Feldwahl" erscheint in der Cloud-Hülle nicht.
- Keine Änderung an der Android-Kiosk-App, an `/courts`, am Relay, an der Config.
- Einchecken per Tipp auf der Meetingpoint-Seite bleibt unverändert (ADR 0074).
- Die PIN ist Bedienschutz, keine Sicherheitsgrenze (steht im HTML, Standard „0000").

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:** `src-tauri/assets/lobby.html` (Eintrag, PIN-Pad, Hinweis-Knopf),
  `src-tauri/assets/anzeige.html` (Menüeintrag „Zur Feldwahl"), `src-tauri/src/tablet/server.rs`
  (`lobby_page` mit `State` + `render_lobby_html`), neu `src/io/lobbyMeetingpoint.mjs` +
  `scripts/test-lobby-meetingpoint.mjs`, `scripts/test-inline-kopien.mjs`, `.github/workflows/ci.yml`.
- **Architekturregeln:** R1 unberührt (keine Tauri-Commands, nur Seiten-Assets). R2 unberührt (keine
  Zuordnung, nur Navigation). R3: nur LAN; die Cloud-Hülle blendet den neuen Eintrag aus. R4: ein
  Meetingpoint-Tablet belegt **kein** Feld. R5/R6 unberührt.
- **Konfiguration & Abwärtskompatibilität:** keine neuen Felder. Sichtbarkeit hängt am bestehenden
  `meetingpoint.enabled`. Bestehender localStorage-Merker `badhub.anzeige.zuletzt` wird gelesen und
  nur im Feld `layout` umgeschrieben; alte Werte bleiben lesbar.
- **Datenschutz:** unverändert. Die Lobby liest von `/meetingpoint/state` nur den HTTP-Status, keine
  Namen. Die Meetingpoint-Seite zeigt wie bisher nur Namen.
- **Abhängigkeiten:** keine neue Dependency, kein badhub-/BTP-/Relay-Bezug.

## Verhalten im Detail

1. **Sichtbarkeit (live):** `lobby.html` fragt alle 15 s `GET /meetingpoint/state` (ohne `halle`)
   ab, mit der vorhandenen Frist-Logik `abrufMitFrist`. 200 → an, 404 → aus, jeder andere Ausgang
   (Netzfehler, Frist, 5xx) → letzter Stand bleibt. Startzustand: aus (Eintrag versteckt).
2. **Eintrag „Meetingpoint":** eigene, farblich abgesetzte Kachel nach den Feldkacheln mit
   Untertitel „Anzeige für Spieler · belegt kein Feld" und Zustand „PIN — antippen". Nur sichtbar,
   wenn an.
3. **PIN:** Tipp auf den Eintrag öffnet ein PIN-Pad (Ziffern wie in der Hülle, Abbrechen-Knopf).
   PIN = `bedien_pin(tablet_settings_pin)` als Platzhalter `__TABLET_PIN__`. Ist die geteilte
   5-Minuten-Freigabe `badhub.tablet.pinFreigabeBis` gültig, entfällt das Pad. Richtige PIN schreibt
   die Freigabe neu. Falsche PIN → Anzeige leert sich, Pad bleibt offen.
4. **Ziel:** `/anzeige?layout=meetingpoint` (ohne `halle`, damit der Hallen-Merker der Hülle greift;
   unbekannte Halle → bestehende Hallenwahl).
5. **Hinweis-Knopf:** Steht in `badhub.anzeige.zuletzt` `layout: "meetingpoint"` **und** ist der
   Meetingpoint an, zeigt die Lobby über dem Feldraster einen großen Knopf „Zurück zum Meetingpoint"
   → Ziel aus Nr. 4, **ohne PIN**. Er leitet nie selbst um.
6. **Marke zurücksetzen:** Tippt jemand in der Lobby ein Feld an, oder wählt in der Hülle „Zur
   Feldwahl", wird im Merker `layout` auf `"tafel"` gesetzt (`court`/`halle` bleiben). Damit erscheint
   der Hinweis-Knopf auf einem Zähl-Tablet nicht dauerhaft.
7. **„Zur Feldwahl" in der Hülle:** neuer Eintrag in `#aktionen`, nur im Hauptmenü hinter der PIN,
   nur wenn die Hüllen-Basis `/` ist (LAN, auch TLS-Port 8443); nie in der PIN-freien Feldwahl
   (`oeffneMenue(true)`), nie in der Cloud. Klick → Nr. 6, dann `location.href = "/felder"`.
   Er erscheint bei **allen** Layouts, nicht nur beim Meetingpoint.
8. **Meetingpoint wird ausgeschaltet:** Eintrag und Hinweis-Knopf verschwinden spätestens nach 15 s.
   Ein bereits offenes Meetingpoint-Tablet zeigt wie bisher „Meetingpoint ist am Turnier-PC
   ausgeschaltet"; der Weg zurück führt über Zahnrad → PIN → „Zur Feldwahl".

## Akzeptanzkriterien

- [ ] Meetingpoint an: `/felder` zeigt nach ≤ 15 s die Kachel „Meetingpoint"; aus: keine Kachel.
- [ ] Umschalten an↔aus im Betrieb wird ohne Neuladen der Lobby sichtbar (≤ 15 s).
- [ ] Netzfehler/Timeout bei `/meetingpoint/state` ändert den angezeigten Stand nicht.
- [ ] Tipp auf die Kachel ohne gültige Freigabe → PIN-Pad; falsche PIN → kein Seitenwechsel;
      richtige PIN → `/anzeige?layout=meetingpoint`, Freigabe für 5 min gesetzt.
- [ ] Mit gültiger Freigabe (z. B. kurz vorher am Zahnrad eingegeben) öffnet die Kachel ohne Pad.
- [ ] Auf dem Weg wird kein Feld belegt: `/courts` meldet danach kein zusätzliches `occupied`.
- [ ] Nach WLAN-Aussetzer (Kiosk-App lädt `/felder`) zeigt ein Meetingpoint-Tablet den Knopf
      „Zurück zum Meetingpoint"; Tipp führt ohne PIN zurück, Halle wie zuvor.
- [ ] Der Knopf erscheint nicht, wenn der Merker fehlt, kaputt ist oder ein anderes Layout nennt,
      und nicht, wenn der Meetingpoint aus ist.
- [ ] Nach Tipp auf ein Feld in der Lobby oder „Zur Feldwahl" erscheint der Knopf beim nächsten
      Laden der Lobby nicht mehr; die gemerkte Halle bleibt erhalten.
- [ ] Hülle im LAN (Basis `/`): „Zur Feldwahl" nur nach PIN sichtbar, führt auf `/felder`.
- [ ] Hülle über die Cloud (`/bts-relay/<ns>/anzeige`): kein „Zur Feldwahl".
- [ ] PIN-freie Feldwahl der Hülle (Start ohne gültiges Feld): kein „Zur Feldwahl".
- [ ] Feldwahl am Slave-PC unverändert (kein Eintrag).

## Tests

- **Rust (`tablet/server.rs`):** `render_lobby_html` ersetzt `__TABLET_PIN__` vollständig
  (kein Platzhalter übrig), leere PIN → „0000", Nicht-Ziffern gefiltert (über `bedien_pin`).
  Router-Test: `/meetingpoint/state` ohne `halle` → 404 bei `enabled=false`, 200 bei `true`
  (sichert den Vertrag, auf dem die Lobby aufsetzt).
- **JS (`scripts/test-lobby-meetingpoint.mjs`, eigener CI-Schritt):** `mpAusStatus` (200/404/sonst),
  `warZuletztMeetingpoint` (null, kaputtes JSON, anderes Layout, meetingpoint),
  `ohneMeetingpointMarke` (Halle/Feld bleiben, nur Layout wechselt, null wenn nichts zu tun),
  `meetingpointZiel`.
- **Drift (`scripts/test-inline-kopien.mjs`):** Inline-Kopien von `lobbyMeetingpoint.mjs` in
  `lobby.html` (und `ohneMeetingpointMarke` in `anzeige.html`) verhaltensgleich zum Modul; dazu die
  `pinFreigabe`-Kopien in `tablet.html`, `anzeige.html` und `lobby.html`.
- `cargo test`, `npm run build`, `node scripts/test-handbuch-fakten.mjs` grün.
- **Manuell:** Kiosk-Tablet im Hallennetz: Lobby → Meetingpoint (PIN) → WLAN aus/an → Hinweis-Knopf →
  zurück; Zahnrad → „Zur Feldwahl" → Feld antippen → Knopf weg.

## Risiken & Rollback

- Fehlbedienung: Ein Zählender landet trotz PIN am Meetingpoint → Rückweg über Zahnrad/PIN.
- Hinweis-Knopf auf einem Zähl-Tablet, wenn die Marke nicht zurückgesetzt wurde (z. B. Kiosk-App
  während der Meetingpoint-Anzeige neu gestartet, dann Feld über die Hülle statt Lobby gewählt): Der
  Knopf führt nur zur Anzeige, belegt nichts; ein Tipp auf ein Feld in der Lobby beseitigt ihn.
- Zusätzlicher Abruf alle 15 s je Lobby-Tablet: `/meetingpoint/state` ohne Halle ist billig; im
  Ein-Hallen-Turnier liefert er die volle (kleine) Sicht — vertretbar.
- Rollback: reine Asset-/Seitenänderung, ältere Version installierbar, keine Config-Wirkung.

## Offene Fragen / Annahmen

- Annahme: Der Turnier-PC ist der Master; Kiosk-Tablets in der fernen Halle sind nicht Ziel.
- Annahme: Die gemeinsame PIN (`tablet_settings_pin`) genügt; keine eigene Meetingpoint-PIN.

## Betroffene Doku-Dateien

- `docs/meetingpoint.md` — Abschnitt „Am Kiosk-Tablet" (Kachel, PIN, Hinweis-Knopf) mit
  `pruef`-Ankern auf „Meetingpoint" und „Zurück zum Meetingpoint" in `lobby.html`.
- `docs/tablet-bedienen.md`, `docs/tablet.md` — Feldwahl-Eintrag, Hüllen-Menü „Zur Feldwahl"
  (Anker `"Zur Feldwahl"` in `anzeige.html`).
- `docs/court-monitor.md`, `docs/oberflaechen.md` — laut CLAUDE.md-Tabelle für `lobby.html`/`anzeige.html`.
- `docs/tablet-android-app.md` — Rückfall auf `/felder` und Hinweis-Knopf.
- `docs/changelog.md`, `docs/roadmap.md`; CLAUDE.md-Zuordnungstabelle um eine Zeile ergänzen.

## Umsetzungs-Hinweise

Ein PR. Reihenfolge:
1. `src/io/lobbyMeetingpoint.mjs` + Test (TDD), CI-Schritt.
2. `render_lobby_html` + `lobby_page` mit `State` + Rust-Tests; Router-Test `/meetingpoint/state`.
3. `lobby.html`: Takt-Abfrage, Kachel, PIN-Pad (Vorlage `anzeige.html` `pinKey`/`renderPin`,
   Inline-Kopie `pinFreigabe`), Hinweis-Knopf, Marke beim Feld-Tipp zurücksetzen.
4. `anzeige.html`: `#feldwahl` in `#aktionen`, Sichtbarkeit in `zeigeHauptmenue`/`oeffneMenue`,
   nur bei `BASE === "/"`.
5. Drift-Tests in `test-inline-kopien.mjs`.
6. Doku + Handbuch-Anker, Version beim Merge gemeinsam in `src-tauri/Cargo.toml`,
   `src-tauri/tauri.conf.json`, `package.json`.

Reviews: `code-reviewer` Pflicht; `security-reviewer` kurz (PIN-Pad, localStorage, Navigation nur auf
feste Pfade). Kein Relay-Deploy nötig (Relay-Hülle blendet den Eintrag aus).
Ausarbeitung: `docs/features/_intake/meetingpoint-am-kiosk-tablet/3-how-to.md` (lokal, gitignoriert).
