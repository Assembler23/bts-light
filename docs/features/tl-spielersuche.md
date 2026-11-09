# Spielersuche in der Turnierleitungs-Oberfläche — Spezifikation

> Status: **abgestimmt 2026-11-09** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Idee der Turnierleitung vom 09.11.2026. Betroffene Crates: src-tauri, relay, relay-proto
> (kein `src/`-Frontend).
> ADR: [0081 — Personensuche am Turnier-PC statt im TL-Zustand](../adr/0081-tl-spielersuche-am-turnier-pc.md).

## Kontext / Problem

Am Turnierleitungstisch kommen ständig dieselben Fragen: „Wann spiele ich als Nächstes?", „Wie
ging mein Spiel aus, auf welchem Feld?", „Wo ist Schiedsrichter X eingeteilt, was hat er heute
schon geleitet?", „Spielt Y gerade?". Heute muss die Turnierleitung dafür Feldkacheln, die
Warteliste und die Beendet-Liste mit dem Auge absuchen — und findet trotzdem nicht alles:

- Die TL-Oberfläche trägt nur die **letzten 30** beendeten Spiele (`FINISHED_LIMIT`,
  `tablet/tl.rs`), in der Cloud noch weniger. Ein Spiel vom Vormittag oder Vortag ist unsichtbar.
- Beendete Spiele tragen **keine Schiedsrichter-Namen**.
- Die Warteliste ist gedeckelt (`QUEUE_LIMIT` = 120, Cloud 40/20/10/5) und blättert seitenweise.

Ein reiner Filter im Browser würde deshalb still Treffer verschweigen. Die Suche muss am
Turnier-PC über den ganzen BTP-Bestand laufen.

## Zielbild & Erfolgskriterien

Ein Suchfeld in der Kopfleiste der TL-Oberfläche (`/tl`, LAN und Cloud). Die Turnierleitung tippt
einen Namensteil und sieht innerhalb einer Sekunde alle Spiele, in denen diese Person als
**Spieler**, **Schiedsrichter (SR)** oder **Assistent (AR)** steht — gruppiert in
**Am Feld · Warteliste · Beendet** —, und springt per Tipp zur passenden Zeile.

Erfolgskriterien:
- Am Turnier findet die Turnierleitung jedes beendete Spiel einer Person, auch vom Vortag,
  ohne in Listen zu scrollen.
- Ein Spieler am Tisch bekommt seine nächste Spielzeit (Prognose) bzw. seinen Wartelistenplatz.
- Funktioniert ohne Erklärung: Feld sichtbar, Treffer selbsterklärend, Fehlerfälle als Klartext.
- Laufzeit der Suche am Turnier-PC < 20 ms bei ~1500 Spielen.

## Nicht-Ziele

- Suche im Desktop-Frontend der App (`src/`).
- Suche nach Verein, Nation oder Lizenznummer; Suche in Check-In-Daten.
- Suchverlauf oder gespeicherte Suchen.
- Live-Nachführen einer offenen Trefferliste (Standbild + „neu suchen").
- Aktionen (⋮-Menüs) direkt in der Trefferliste — Aktionen gibt es an der Zeile im Panel.
- Suche auf Slave-PCs (die Slave-Brücke hat keine TL-Routen).
- Spiele, die weder angesetzt (Warteliste) noch am Feld noch beendet sind.
- SR-Namen ohne eingeschalteten Schiedsrichter-Betrieb (keine Rohwerte `Official1/2` aus BTP).
- Zugriffsschutz über den Profil-Schalter (er blendet nur aus, siehe Datenschutz).

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - `relay-proto/src/lib.rs`: Konstanten, `RelayFrame::TlSearchRequest`,
    `HostFrame::TlSearchResult`, `TlDisplaySettingsWire.hide_search` (`hideSearch`).
  - `src-tauri/src/tablet/suche.rs` (**neu**): Falten, Matching, Gruppierung, Antwort-JSON.
  - `src-tauri/src/tablet/tl.rs`: verhaltensgleiche Extraktion aus `build_state_limited`
    (`felder_bauen`, `wartelisten_ordnung`, `prognose_rechnen` ohne Merge,
    `offenes_vorspiel`), `TlState.can_search`, Profil-Abbildung `hide_search`.
  - `src-tauri/src/config.rs`: `TlDisplaySettings.hide_search`.
  - `src-tauri/src/tablet/server.rs`: Route `POST /tl/api/search`.
  - `src-tauri/src/tablet/relay_client.rs`: Arm für `TlSearchRequest`.
  - `relay/src/main.rs`: Route `POST /{ns}/tl/api/search`, `search_pending`, Antwort-Arm.
  - `src-tauri/assets/tl.html`: Suchfeld, Trefferliste, Hinspringen, Profil-Häkchen.
  - `src/io/spielersuche.mjs` (**neu**) + `scripts/test-spielersuche.mjs`: reine Sprung- und
    Textlogik mit Inline-Kopie in `tl.html` und Drift-Test.
- **Architekturregeln:**
  - R1: unberührt (kein Desktop-Frontend beteiligt).
  - R2: Alle Daten stammen aus dem BTP-Snapshot bzw. den vorhandenen Stores; die Suche erfindet
    keine Zuordnung und verändert keinen Zustand (insbesondere kein `merge_predicted_starts`).
  - R3: LAN (`server.rs`) und Cloud (Relay → `relay_client.rs`) rufen **denselben** Handler
    `suche::search_json`. LanAndCloud: beide Wege parallel nutzbar.
  - R4/R5/R6: unberührt; die Suche ist lesend, Namespace = `install_id` wie bei allen TL-Routen.
- **Konfiguration & Abwärtskompatibilität:** Neues Feld `TlDisplaySettings.hide_search: bool`
  mit `#[serde(default)]` = `false` → in allen bestehenden Profilen ist die Suche **sichtbar**.
  Wire `hideSearch` ebenso mit Default. Ältere Versionen ignorieren das Feld. `identifier` und
  Updater-Pfad unangetastet.
- **Datenschutz:**
  - Neu im TL-Datenfluss: **SR-/AR-Namen an beendeten Spielen** — nur in der Suchantwort, nicht im
    `TlState`.
  - Suchantwort ist eine **Positivliste** (siehe Antwortformat). Nicht enthalten: Geburtsjahr,
    Nation, Setzplatz, SR-Verein, Sperrlisten, Check-In-Daten, Echo des Suchbegriffs.
  - Kandidaten offener Paarungen: nur als Label-Text, **ohne** Lizenznummer und Verein.
  - Lizenznummern (`team*_ids`) an Feld, Warteliste und Beendet wie im `TlState` — einziger Zweck
    der Link auf die badhub-Spielerseite.
  - Suchbegriff reist im **POST-Body**, nie in der URL (nginx-Access-Log); weder Turnier-PC noch
    Relay protokollieren ihn (Host-Logs gehen per Diagnose-Upload an badhub). Log-Zeile höchstens
    Trefferzahl und Dauer.
  - Die Suche ist ein Personen-Abfragewerkzeug („alle Spiele von X"). Sie steht hinter dem
    TL-Gerätezugang; der Profil-Schalter blendet nur aus und ist **kein** Zugriffsschutz —
    bewusst akzeptiert (ADR 0081). CLAUDE.md-Datenschutzabschnitt bekommt einen Absatz.
- **Abhängigkeiten:** Keine neue Cargo-/npm-Dependency (Akzent-Faltung per eigener Tabelle, keine
  `unicode-normalization`-Crate). Relay-Deploy nötig (automatisch bei Merge). Kein BTP-Protokoll-
  und kein badhub-Bezug.

## Verhalten im Detail

### Suchfeld
- `<input type="search">` im statischen Kopf der TL-Seite (nicht in `#halls`, damit Push-Renders
  Fokus und Bildschirmtastatur nicht zerstören), Schriftgröße ≥ 16 px, `maxlength` 64.
- Sichtbar genau dann, wenn `state.can_search` **und** das aktive Profil nicht `hideSearch` hat.
- Profil-Editor: Häkchen **„Spielersuche zeigen"** (nur angeboten, wenn `can_search`).
- Anfrage frühestens 300 ms nach dem letzten Tastendruck, erst ab 2 Zeichen (ohne Leerraum, nach
  Faltung). Höchstens eine Anfrage je Gerät gleichzeitig; eine neue verwirft die laufende
  (`AbortController`).

### Matching (am Turnier-PC)
- Faltung von Suchbegriff und Namen gleich: Kleinschreibung; `ß→ss`; `ä/ö/ü→a/o/u`, danach
  `ae/oe/ue→a/o/u`; weitere Akzente per Tabelle (é→e, ł→l, ñ→n, ç→c, ø→o, å→a, š→s, ž→z, æ→a, œ→o
  u. a.); `- , ' .` werden zu Leerzeichen, Leerraum zusammengefasst.
- Begriff in Wörter zerlegt (höchstens 6). **Alle** Wörter müssen als Teilstring im Namen
  **derselben Person** vorkommen, Reihenfolge egal. („max mül" findet „Max Müller";
  „Müller Schmidt" findet **nicht** das Doppel Müller/Schmidt.)
- Durchsucht werden je Spiel: Spielernamen beider Seiten; SR- und AR-Namen (nur bei
  eingeschaltetem Schiedsrichter-Betrieb); bei offenen Paarungen die Kandidatennamen aus dem
  Vorspiel (nur wenn die Anfrage `candidates: true` trägt).
- Ein Treffer je Spiel; `roles` nennt alle Rollen der gesuchten Person in diesem Spiel
  (`player`, `sr`, `ar`, `candidate`). Keine Gruppierung nach Person; gleichnamige Personen
  erscheinen als getrennte Spiele (Verein nur, wenn das Profil „Vereine anzeigen" hat).

### Gruppen, Reihenfolge, Deckel
- **Am Feld:** Spiele aktuell auf einem Feld (inkl. „wird geräumt"), sortiert nach Feld.
- **Warteliste:** spielbereite Spiele in der turnierweiten Wartelisten-Reihenfolge (dieselbe wie
  im Panel: manuelle Reihenfolge, Gerufene, Ausgenommene, Wunschfeld — mit Marke), danach offene
  Paarungen als Untergruppe **„möglicher Teilnehmer"**. Kandidaten nur, wenn das aktive Profil
  offene Paarungen **nicht** ausblendet.
- **Beendet:** alle beendeten Spiele mit Sieger aus dem BTP-Snapshot (auch Vortag), nach
  Spielnummer **absteigend**. Walkover/Aufgabe/Disqualifikation mit denselben Abzeichen wie die
  Beendet-Zeile. SR/AR eines beendeten Spiels aus `OfficialsStore::appearances` — ein Walkover
  zählt **nicht** als Einsatz und trägt keinen SR.
- Höchstens **50 Treffer je Gruppe** (spielbereite vor Kandidaten). Darüber hinaus:
  `more = N` und Text **„weitere N – bitte genauer suchen"**. Der Turnier-PC setzt den Deckel
  durch und kürzt bei Überschreiten von `MAX_TL_SEARCH_LEN` weiter (Halbierung), `more` zählt mit.

### Zeitangaben
- Warteliste: Prognose (`predicted_start_ms`) nur, wenn das Spiel auf **Platz ≤ 120**
  (`QUEUE_LIMIT`) liegt und die Prognose eingeschaltet ist; Wert aus derselben Rechnung wie
  `build_state_limited`, nie aus dem Speicher `predicted_starts` gelesen. Sonst
  **„Platz N in der Warteliste"**. In der Cloud wird die Prognose für Platz 41–120 ebenfalls
  gezeigt (kanonisch gleiche Werte).
- Beendet: Uhrzeit (`ended_ms`) **nur**, wenn die App das Spiel selbst laufen sah (gemessene
  Spielzeit liegt vor, `finished_times`); sonst keine Uhrzeit. Grund: BTP liefert kein
  Spielende, der Stempel „zuerst als beendet gesehen" ist nach Neuinstallation/Vortag falsch.
- Am Feld: „seit hh:mm" aus `on_court_since_ms`, Satzstand.

### Trefferliste
- Klappt unter der Kopfleiste auf (Overlay außerhalb von `#app`), überlebt Push-Renders.
- Kopf: **„Stand hh:mm"** (aus `server_now_ms`) und Knopf **„neu suchen"**. Kein Live-Nachführen.
- Hinweis **„Schiedsrichter-Betrieb aus"**, wenn `officials_managed == false`.
- Schließen per ✕, Escape oder Leeren des Feldes.

### Hinspringen
Ein Tipp auf einen Treffer wird zur Klickzeit gegen den aktuellen TL-Zustand aufgelöst; die
Trefferliste schließt, dann:
1. Zeile im DOM erreichbar → Panel ggf. aufblättern (`queueShown` erhöhen), zur Zeile scrollen
   (mittig), kurz markieren.
2. Panel zugeklappt → aufklappen wie der Panel-Pfeil (dauerhaft im Profil), Meldung
   **„Panel ‚…' aufgeklappt"**, dann wie 1.
3. Hallenfilter zeigt eine andere Halle → **kein** stilles Umstellen; Hinweis
   **„Liegt in Halle X – Filter umstellen?"** mit Knopf, der `setHall(X)` ruft.
4. Filter „nur spielbereite" verbirgt die Zeile → Hinweis mit Knopf **„Filter aus"**.
5. Zeile hinter dem Listen-Deckel (Warteliste > Cloud-/LAN-Deckel, Beendet > 30) oder Panel im
   Profil ausgeblendet → kein Sprung, Text **„nicht in der Liste sichtbar (Platz N)"**; bei
   beendeten Spielen bleiben **Punktverlauf** und **Zettel** (wenn `has_timeline`) direkt am
   Treffer erreichbar.

### Fehlerfälle (Klartext in der Trefferliste)
| Lage | Anzeige |
|---|---|
| < 2 Zeichen | keine Anfrage, keine Liste |
| 400 (zu lang / zu viele Wörter) | „Bitte kürzer suchen" |
| 401 | „Dieses Gerät ist nicht mehr freigegeben" |
| 429 | „Zu viele Anfragen – kurz warten" |
| 503 / Zeitüberschreitung | „Turnier-PC nicht erreichbar" |
| `has_data: false` (kein Snapshot, BTP getrennt) | „Keine Turnierdaten" |
| keine Treffer | „Keine Treffer" |

### Versionsschiefstand
- Neue Seite + alter Turnier-PC (Normalfall nach Merge, Relay deployt sofort): `can_search` fehlt
  → kein Suchfeld, **keine** Anfrage (sonst hinge sie 20 s am `TL_TIMEOUT`).
- Neuer Turnier-PC + altes Relay: Cloud-Seite ist die alte → kein Suchfeld; LAN unabhängig.

## Antwortformat (Wire)

Konstanten in `relay-proto`: `TL_SEARCH_MIN_LEN = 2`, `TL_SEARCH_MAX_LEN = 64` (Zeichen),
`TL_SEARCH_MAX_WORDS = 6`, `TL_SEARCH_HITS_PER_GROUP = 50`, `MAX_TL_SEARCH_LEN = 256 KiB`.

Frames (opak wie `OfficialDetail`):
```rust
RelayFrame::TlSearchRequest { #[serde(rename = "reqId")] req_id: u64, query: String, #[serde(default)] candidates: bool }
HostFrame::TlSearchResult  { #[serde(rename = "reqId")] req_id: u64, #[serde(default)] json: String } // leer = Fehler → 503
```

Request: `POST /tl/api/search` (LAN) bzw. `POST /bts-relay/{ns}/tl/api/search` (Cloud),
`Authorization: Bearer <token>`, Body ≤ 1 KiB: `{ "q": "müll", "candidates": true }`
(`candidates = !hideOpenMatches` des aktiven Profils). Antwort 200 mit `Cache-Control: no-store`;
400 bei Längen-/Wortverstoß (Relay prüft vorab, schickt dann **keinen** Frame); 401/429/503 wie
bei den anderen TL-Detailrouten; 413 bei Body > 1 KiB.

```json
{ "server_now_ms": 1731150000000, "has_data": true, "officials_managed": true,
  "court":    { "hits": [], "more": 0 },
  "waiting":  { "hits": [], "more": 0 },
  "finished": { "hits": [], "more": 12 } }
```
Positivliste je Treffer:
- Kern: `match_id, match_num, draw_name, round_name, class_label, discipline, team1, team2,
  team1_club, team2_club, team1_ids, team2_ids, hall, sr[], ar[], roles[]`.
- Am Feld: `court_id, court, on_court_since_ms, sets, clearing, has_timeline`.
- Warteliste: `position` (1-basiert), `predicted_start_ms?`, `predicted_uncertain, called,
  excluded, wish_court?, open, open_slot1_label, open_slot2_label`. Bei `open: true` keine
  `*_ids`, keine `*_club`.
- Beendet: `winner, sets, result` (`normal|walkover|retired|disqualified`), `court` (leer bei
  Papier-Ergebnis), `ended_ms?`, `has_timeline`.

## Akzeptanzkriterien

Suche und Treffer
- [ ] „mül" findet ein Spiel von „Max Müller"; ebenso „mueller", „muller", „MÜLLER".
- [ ] „max mül" und „mül max" finden „Max Müller"; „Müller Schmidt" findet das Doppel
      Müller/Schmidt **nicht**.
- [ ] „jose" findet „José", „lukasz" findet „Łukasz", „strasse" findet „Straße",
      „anna maria" findet „Anna-Maria".
- [ ] Ein beendetes Spiel vom Vortag (Spiel 1 der BTP-Datei, längst aus den letzten 30 gefallen)
      wird gefunden.
- [ ] Ein Spiel, das gerade am Feld läuft, erscheint unter „Am Feld" und **nicht** zusätzlich
      unter „Warteliste"; ein weiteres, wartendes Spiel derselben Person erscheint unter
      „Warteliste".
- [ ] Bei eingeschaltetem SR-Betrieb findet die Suche einen SR an seinem laufenden, wartenden und
      beendeten Spiel, `roles` enthält `sr` bzw. `ar`.
- [ ] Ein Walkover-Spiel trägt keinen SR und wird über den SR-Namen nicht gefunden.
- [ ] Bei ausgeschaltetem SR-Betrieb gibt es keine SR-Treffer und `officials_managed: false`; die
      Trefferliste zeigt „Schiedsrichter-Betrieb aus".
- [ ] Ein Kandidat einer offenen Paarung erscheint mit `roles: ["candidate"]`, `open: true`, ohne
      `*_ids` und `*_club`; mit `candidates: false` erscheint er nicht.
- [ ] Ist die Person Spieler und SR im selben Spiel, trägt der eine Treffer beide Rollen.
- [ ] 120 passende beendete Spiele → 50 Treffer, `more: 70`, Text „weitere 70 – bitte genauer
      suchen".
- [ ] Beendete Treffer stehen nach Spielnummer absteigend.

Zeiten
- [ ] Wartendes Spiel auf Platz 3 trägt dieselbe Prognose wie `build_state(...).queue`.
- [ ] Wartendes Spiel auf Platz 121 trägt keine Prognose, `position: 121`, Anzeige
      „Platz 121 in der Warteliste".
- [ ] Prognose aus → kein `predicted_start_ms`.
- [ ] Eine Suche verändert den Speicher `predicted_starts` nicht (vorher = nachher).
- [ ] Beendetes Spiel ohne gemessene Spielzeit hat kein `ended_ms`; mit gemessener Zeit trägt es
      `finished_ms` aus `match_times`.

Seite
- [ ] Suchfeld erscheint nur bei `can_search: true` und nicht ausgeblendetem Profil-Schalter;
      ein Bestandsprofil ohne `hideSearch` zeigt es.
- [ ] Ohne `can_search` (alter Turnier-PC) schickt die Seite keine Such-Anfrage.
- [ ] Fokus und Bildschirmtastatur bleiben erhalten, während Push-Updates die Seite neu zeichnen;
      die offene Trefferliste bleibt stehen.
- [ ] Tippen von „Müller" in normaler Geschwindigkeit löst genau eine Anfrage aus (Entprellung).
- [ ] Tipp auf einen Wartelisten-Treffer auf Platz 60 (LAN) blättert die Liste auf und scrollt zur
      Zeile.
- [ ] Tipp auf einen Treffer in Halle B bei Filter Halle A zeigt „Liegt in Halle B – Filter
      umstellen?"; der Filter ändert sich erst mit dem Knopf.
- [ ] Tipp auf einen Treffer im zugeklappten Panel klappt es auf und meldet „Panel ‚…' aufgeklappt".
- [ ] Tipp auf ein beendetes Spiel außerhalb der letzten 30 zeigt „nicht in der Liste sichtbar
      (Platz N)" und bietet Punktverlauf/Zettel, wenn vorhanden.
- [ ] Jeder Fehlerfall der Tabelle zeigt seinen Text; die übrige TL-Oberfläche bleibt bedienbar.

Transport, Sicherheit, Datenschutz
- [ ] LAN: ohne/mit falschem Token 401; „a" → 400; Body > 1 KiB → 413; gültige Suche → 200.
- [ ] Relay: Suche wird zum Host durchgereicht und beantwortet; fremdes Token wird abgewiesen;
      zu langer Begriff → 400 **ohne** Frame an den Host; alter Host (keine Antwort) → 503 nach
      `TL_TIMEOUT` und die Pending-Map ist leer; volle Pending-Map → 429; Host-Trennung leert
      `search_pending`; Antwort > `MAX_TL_SEARCH_LEN` → 503.
- [ ] Der Suchbegriff erscheint in keiner Log-Zeile von Turnier-PC oder Relay und nie in einer URL.
- [ ] Die Suchantwort enthält nur Felder der Positivliste (Wächter-Test), insbesondere kein
      Geburtsjahr, keinen Setzplatz, keine Nation, keinen SR-Verein, keine Sperrlisten.
- [ ] Suche über 1500 Spiele dauert am Turnier-PC < 20 ms (Lasttest, `#[ignore]`).

## Tests

TDD — Rust-Unit-Tests, `cargo test` (Workspace) und `cargo clippy --workspace --all-targets
-- -D warnings` grün, `npm run build` fehlerfrei:

- `tablet/suche.rs`: Faltung (Müller/Mueller/Muller, Straße, José, Łukasz, Bindestrich),
  Wortzerlegung und Grenzen (`SuchFehler` bei < 2 / > 64 Zeichen / > 6 Wörter), Matching je
  Person, Deckel + `more` + Kürzungsleiter gegen `MAX_TL_SEARCH_LEN` (Worst-Case-Fixture),
  Gruppierung (Feld nicht doppelt in Warteliste, Beendet nur mit Sieger, `result`-Abzeichen,
  Sortierung absteigend), Kandidaten + `candidates: false`, Prognose (Platz 3 gleich
  `build_state`, Platz 121 ohne, Store unverändert), SR nur bei Betrieb + Walkover-Regel,
  `ended_ms` nur gemessen, `has_data: false` ohne Snapshot, Lasttest `#[ignore]`.
- Wächter: `search_answer_fields_are_deliberately_allowed` (eigene Allowlist, Fixture deckt alle
  Gruppen, Kandidaten, SR an Beendet) und
  `search_answer_never_carries_personal_data_beyond_its_purpose`.
- `tablet/tl.rs`: bestehende `build_state`-Tests unverändert grün nach der Extraktion (eigener
  Commit); `can_search` in beiden Konstruktionswegen und in `ERLAUBT`; Wire↔Config-Roundtrip
  `hide_search`.
- `config.rs`: Bestandsprofil ohne `hide_search` lädt mit `false`.
- `tablet/server.rs`: 401/400/413/200.
- `relay/`: `search_passes_through_and_rejects_foreign_tokens` (Muster
  `official_details_pass_through_and_reject_foreign_tokens`), Timeout mit `tokio::time::pause`,
  400 ohne Frame, 429, Trennung, Übergröße.
- `relay-proto`: Serde-Roundtrips beider Frames; `hideSearch` fehlt → `false`.
- JS: `scripts/test-spielersuche.mjs` für alle `sprungZiel`-Fälle (Zeile, Halle, Filter
  „nur spielbereite", Panel zu, hinter Deckel, nur Detail) + Drift gegen die Inline-Kopie; als
  CI-Schritt in `.github/workflows/ci.yml`.
- Manuell: Browsertest der Seite am Mock bzw. lokal gestarteter App (Fokus über Push-Renders,
  Bildschirmtastatur auf Tablet), Feldtest am Turnier.

## Risiken & Rollback

- **Antwortgröße über Mobilfunk** (~100 KB bei 150 Treffern): Deckel 50, Entprellung, kein
  Live-Nachführen; der Host kürzt selbst, damit das Relay nicht still verwirft.
- **Last ohne Rate-Limit je Gerät:** Client-Disziplin (eine Anfrage je Gerät), `spawn_blocking`
  am Host, damit der Relay-Client Punktstände nicht aufhält; Relay-Grenzen 8 Geräte /
  16 offene Anfragen je Map (eigene `search_pending`-Map).
- **Push-Render zerstört Eingabe:** Input im statischen Kopf, Overlay außerhalb `#app`; Sprung erst
  nach `render()` + `requestAnimationFrame`.
- **Panel-Aufklappen wirkt profilweit** — bewusst wie der Pfeil.
- **Refactor von `build_state_limited`** kann das TL-Verhalten ändern → eigener Commit, gesamte
  Test-Suite als Netz.
- **Rollback:** im Turnier je Gerät per Profil-Häkchen ausblendbar; ältere Version installierbar,
  das Feld `hide_search` wird von ihr ignoriert; kein globaler Notschalter nötig.

## Offene Fragen / Annahmen

Keine offenen Fragen. Annahmen:
- Die Suche ignoriert den Hallenfilter (findet hallenübergreifend), Treffer tragen die Hallenmarke.
- `TlDevice.hall` wird nicht erzwungen — die Suche braucht daher keine Hallen-Sperre.
- Vereinsnamen reisen mit; die Seite zeigt sie nur bei „Vereine anzeigen" (wie im `TlState`).

## Betroffene Doku-Dateien

- `docs/turnierleitung-web.md` (Bedienung, Handbuch-Kapitel → `<!-- pruef: … -->`-Anker für
  „Spielersuche zeigen", „weitere N – bitte genauer suchen", „nicht in der Liste sichtbar",
  Mindestlänge/Deckel-Konstanten).
- `docs/cloud-relay.md` (Route, Frames, Deckel, Deploy-Reihenfolge, Versions-Matrix).
- `docs/schiedsrichter-management.md` (SR-Einsätze in der Suche, Walkover-Regel).
- `docs/oberflaechen.md` (neue Route in `tablet/server.rs`).
- `CLAUDE.md`: Zeile in der Doku-Tabelle („Spielersuche im TL-Web") und Absatz im
  Datenschutz-Abschnitt.
- `docs/roadmap.md`, `docs/changelog.md`; ADR 0081.

## Umsetzungs-Hinweise

Zwei PRs, jeweils Version gemeinsam bumpen (`src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`,
`package.json`), danach `code-reviewer` und **`security-reviewer`** (neuer User-Input, neue
Internet-Route, Personenabfrage).

**PR 1 — Turnier-PC, Wire, Relay** (`can_search = true`, ohne Seite folgenlos):
1. relay-proto: Konstanten, Frames, `hideSearch` + Tests.
2. config.rs: `hide_search` + Test.
3. tl.rs: Profil-Abbildung, `TlState.can_search` (beide Konstruktionsstellen, `ERLAUBT`).
4. tl.rs: Extraktion `felder_bauen` / `wartelisten_ordnung` / `prognose_rechnen` (Merge bleibt im
   Aufrufer) / `offenes_vorspiel` — eigener Commit, Suite grün.
5. `tablet/suche.rs`: Logik + Wächter + Lasttest; SR nur bei `officials_store().enabled()`
   (`appearances` prüft das nicht selbst!); Log nur Zahl + Dauer; überholten Doc-Kommentar an
   `TlFinished.finished_at_ms` berichtigen.
6. server.rs: `post(tl_search)` mit `DefaultBodyLimit::max(1024)`, `spawn_blocking`.
7. relay_client.rs: Arm `TlSearchRequest` → `spawn_blocking` → `TlSearchResult` (Fehler: leeres
   `json`).
8. relay/: `search_pending` (in `forget_tl_access` leeren), Route im `tl_an`-Block mit Body-Limit,
   Längenprüfung vor dem Frame, Antwort-Arm mit `MAX_TL_SEARCH_LEN` (nicht `MAX_TIMELINE_LEN`).
9. Doku `cloud-relay.md`, `changelog.md`.

**PR 2 — Seite:**
10. `src/io/spielersuche.mjs` (`sprungZiel`, `trefferZeile`) + Inline-Kopie + Drift-Test + CI.
11. Suchfeld im statischen Kopf, Sichtbarkeit über `can_search`/`hideSearch`, `messeKopf()`.
12. `#search-overlay`, „Stand hh:mm", „neu suchen", „weitere N", Hinweise, Escape.
13. Abruf: Entprellung, `AbortController`, `abrufMitFrist` POST, Statustexte, alles über `esc()`.
14. Hinspringen nach „Hinspringen" oben.
15. Profil-Editor-Häkchen „Spielersuche zeigen", `builtInProfile().display.hideSearch = false`.
16. Übrige Doku (Tabelle oben), CLAUDE.md, roadmap, changelog.

Reihenfolge der Auslieferung: PR 1 mergen (Relay deployt automatisch), PR 2 mergen; das Suchfeld
erscheint, sobald der Turnier-PC die Version aus PR 1 hat (Tag durch Admin).
