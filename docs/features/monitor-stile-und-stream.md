# Monitor-Stile und Stream-Einblendung — Spezifikation

> Status: **freigegeben 2026-10-31** (via Brief → Grill → How-To → Review).
> Quelle: Wunsch vom 31.10.2026, „die Ansichten aus bup/BTS auch für unsere Monitore".
> Betroffene Crates: `relay-proto/` (`MonitorTarget`, `MonitorState`, `MonitorConfig`,
> `MonitorPlayer`), `src-tauri/` (Config, Monitor-Projektion, LAN-Server, Slave-Brücke, Assets),
> `relay/` (Routen, Projektion), `src/` (Court-Monitor-Panel, Setup, `io/`-Module).
> ADRs: [0068 — Monitor-Stil als Attribut am Feld-Ziel](../adr/0068-monitor-stil-als-attribut-am-feld-ziel.md) ·
> [0069 — Stream-Einblendung als eigene Seite auf dem Feld-Zustand](../adr/0069-stream-einblendung-eigene-seite.md) ·
> [0070 — Nation und Verein auf Anzeigen serverseitig gefiltert](../adr/0070-nation-verein-auf-anzeigen-serverseitig.md).

## Kontext / Problem

Der Feld-Monitor (`monitor.html`) kennt genau ein Aussehen. In Hallen mit hellem Licht, an
Beamern oder für Zuschauer weit weg vom Feld passt das nicht immer. Die Turnierleitung kennt
aus bup (`phihag/bup`, `js/displaymode.js`) über zwanzig Darstellungen und vermisst einige
davon. Außerdem gibt es für Livestreams keine Score-Einblendung: Wer ein Feld mit OBS streamt,
hat keinen Spielstand im Bild.

**Lizenz:** bup und bts haben keine Lizenz (`license: null`, Forks ebenso). Übernommen wird
**nur die funktionale Anordnung** (Teamzeilen, Satzspalten, großer laufender Stand). Kein Code,
kein CSS, keine Grafik, keine Farbwerte, keine Schriftmaße.

Heute zeigt der Monitor die Flagge **immer**, sobald BTP eine Nation liefert. Einen Schalter
gibt es nicht. Den Verein zeigt er nie.

## Zielbild & Erfolgskriterien

**Etappe E1: Hallen-Stile.** Der Turnierleiter wählt im Court-Monitor-Panel je Feld-Monitor
einen Stil:

| Stil | Wert | Aussehen |
|---|---|---|
| Standard | `standard` | wie heute |
| International | `international` | dunkle Teamzeilen, je Satz eine Spalte, Flagge und Aufschlagmarke an der Zeile |
| Riesen-Stand | `riesen` | Stand des laufenden Satzes bildschirmfüllend, klein darüber Satzstand und Kurznamen |
| Nur Spieler | `spieler` | Namen (mit den Plätzen am Namen), Aufschlag-Punkt ● und Annehmer-Ring ○, kein Stand |
| Hell | `hell` | Aufbau wie Standard, heller Grund, dunkle Schrift |
| Nur Stand | `stand` | Satzstände und laufender Stand groß, keine Namen; Gegenstück zu „Nur Spieler" |

Dazu kommen zwei turnierweite **Plätze am Namen** (Nachtrag 01.11.2026, ersetzt die zwei
Häkchen): **Vor dem Namen** — Nation (Flagge) *(Standard)*, Vereinslogo, ohne — und **Unter dem
Namen** — Vereinsname, Nation (Text), ohne *(Standard)*. Die Anordnung gilt am Feld-Monitor;
der Datenbedarf (Nation/Verein überhaupt senden?) folgt daraus für alle Anzeigen.

**Nachtrag 01.11.2026 — zwei Monitore je Feld und Annehmer:** „Nur Spieler" links und „Nur
Stand" rechts teilen den Bildschirm gleich auf (Kopf, Fuß, Trenner bleiben), damit Team 1 auf
beiden Monitoren auf derselben Höhe steht. „Nur Spieler" zeigt dafür den Aufschlag (Punkt und
Balken). Das Tablet schickt zusätzlich `receiving: {team, index}` (diagonal gegenüber dem
Aufschläger, `computeReceiving`) im gespiegelten Stand; der Feld-Monitor markiert den Annehmer
mit einem Ring ○. Host und Relay reichen den Stand unverändert durch, eine Wire-Änderung ist
dafür nicht nötig. Ältere Tablet-Seiten schicken kein `receiving`; dann leitet der Monitor den
Annehmer nach derselben Regel aus `positions`/`servingSide`/`teamOnSide`/`currentSet`/`match` ab
(`receivingInfo` in `monitor.html`).

**Etappe E2: Stream-Einblendung.** Unter `/stream?courts=…` gibt es eine transparente Seite für
die OBS-Browser-Quelle, in LAN und Cloud. Mit einem Feld zeigt sie eine Score-Leiste
(Namen, Satzstand, laufender Stand, Aufschlag), mit 2–3 Feldern die Felder untereinander.

Erfolgskriterien:

- Ein Turnierleiter stellt einen Pi-TV im Panel von „Standard" auf „Riesen-Stand". Der TV
  wechselt beim nächsten Abruf (≤ 4 s), ohne die Seite neu zu laden.
- Die Ziffern im Stil „Riesen-Stand" sind mindestens 45 vmin hoch.
- Ein Stream-Team bindet `/stream?courts=3` als OBS-Browser-Quelle ein. Über dem Video steht
  die Leiste ohne Hintergrund, und ein Punkt am Tablet erscheint dort so schnell wie am
  Feld-Monitor.
- Nach dem Update sehen alle bestehenden Monitore unverändert aus, Flaggen eingeschlossen.

## Nicht-Ziele

- Keine Übernahme von bup-Code, -CSS, -Grafiken, -Farbwerten oder -Maßen.
- Keine Mannschaftskampf-Ansichten (bup `teamcourt`, `teamscore`, `streamteam`).
- Keine Farbschemata und keine freie Farbwahl. „Hell" ist ein eigener Stil.
- Keine Einstellseite für den Stream, keine Parameter für Position, Farbe oder Breite. Größe und
  Lage legt man in OBS fest.
- Kein Stil für Tafel, Kombi, Übersicht, Vorbereitung und Sieger. Der Nation- und der
  Vereinsschalter filtern dort aber die **Daten** (siehe Datenschutz).
- Kein eigener Zugangsschlüssel für die Stream-URL.
- Keine Stream-Route über die Slave-Brücke. OBS in einer fernen Halle nimmt die Cloud-Adresse.
- Mehr als 3 Felder in der Einblendung.

## Betroffene Komponenten / Architekturregeln / Daten

- **R1:** Panel und Setup sprechen nur über die bestehenden Commands (`assign_monitor`,
  `save_config`). Kein neuer Netzwerkzugriff aus React.
- **R2:** Alle Daten kommen aus dem vorhandenen Monitor-Zustand. Kein neuer BTP-Zugriff, und die
  Einblendung schreibt nie etwas zurück.
- **R4/R6:** Die Stream-URL in der Cloud enthält den Namespace (`install_id`). Das ist bewusst
  akzeptiert (ADR 0069). Das Handbuch sagt: „URL nur an das eigene Stream-Team weitergeben."
- **R5** bleibt unberührt.

**Wire (`relay-proto`), alle Felder mit Serde-Default:**

| Typ | Neu | Default / Form |
|---|---|---|
| `MonitorStil` | Enum `Standard, International, Riesen, Spieler, Hell, Stand` + `#[serde(other)] Unbekannt`, `snake_case` | `Standard`; `ist_standard()` gilt für Standard **und** Unbekannt |
| `MonitorTarget::Court` | `stil: MonitorStil` | `default`, `skip_serializing_if = ist_standard` → `{"kind":"court","court_id":5}` bleibt bytegleich |
| `MonitorState` | `stil: MonitorStil` | `default` |
| `MonitorConfig` | `showNations: bool`, `showClubs: bool` (aus den Plätzen abgeleitet), `nameDavor: String`, `nameDarunter: String` | `true` bzw. `false`; Plätze leer → weggelassen, Seite leitet aus den Schaltern ab |
| `MonitorPlayer` | `club: Option<String>` | `default`, `skip_serializing_if = is_none` |

**Config (`config.rs` `CourtMonitorConfig`):** `name_davor` (`flagge`|`logo`|`ohne`, Standard
`flagge`), `name_darunter` (`verein`|`nation`|`ohne`, Standard `ohne`); `nation_an()`/`verein_an()`
leiten den Datenbedarf ab. Das Feld `layout` bleibt lesbar, wird aber als veraltet markiert und nicht mehr
angeboten. Der Stil wohnt **nicht** in der Config, sondern an der Zuweisung (ADR 0068).

**Datenschutz:** Gefiltert wird auf dem Server (ADR 0070):

- Nation nur bei `nation_an()`, Verein (Name und Logo) nur bei `verein_an()`.
- Das gilt für den Feld-Zustand in LAN (`build_monitor_state`) und Relay (`monitor_player`), für
  LAN-`/health`, Cloud-`/health` und `/combo/state`. Die heute ungenutzt mitreisenden
  `team*_clubs` werden bei Verein aus geleert.
- Eine URL kann Nation, Verein und Logo **nur abschalten**, nie einschalten.
- Kein Geburtsjahr. Setzplätze wie bisher.

## Verhalten im Detail

### E1: Stil am Feld-Monitor

- **Panel:** Steht ein Gerät auf „Feld X", erscheint daneben ein eigenes Auswahlfeld **Stil**
  mit den fünf Stilen. Wechselt das Gerät auf ein anderes Feld, bleibt der Stil. Wechselt es auf
  Tafel, Info, Werbung oder Kombi, verfällt er. Zurück auf einem Feld gilt „Standard".
- **Gerätemodus** (`/court/display`, zugewiesen): Wirksam ist allein `MonitorState.stil`. Ein
  `?stil=` in der URL wird ignoriert.
- **Fester Modus** (`/court/{id}/display`): Wirksam ist allein `?stil=<wert>`.
- **Unbekannter oder fehlender Wert:** In beiden Modi gilt `standard` (Allowlist in
  `src/io/monitorStil.mjs`). Der Wert landet nie roh in DOM oder CSS.
- **Wechsel:** Der Stil wird über die Zustands-Marke übertragen, also gibt es kein Neuladen. Die
  Seite setzt `data-stil` am Wurzelelement.
- **Leerlauf-Werbung, Pausenbanner, Aufruf-Uhr, Identifizieren, Ansage-Modus** verhalten sich in
  allen Stilen wie heute. Im Stil „Riesen-Stand" liegt das Pausenbanner wie heute oben über dem
  Stand.
- **Nur Spieler:** Nach Spielende bleiben die Namen stehen, bis BTP das Feld räumt.
- **Kurznamen** („Riesen-Stand", „International"): Sie werden aus `given`/`family` gebildet, sonst
  aus `name`. In der Cloud sind `given`/`family` leer, deshalb ist die Ableitung aus `name` Pflicht.
  Regel: Kurzname = `family`. Fehlt er, bei einem Komma in `name` („Nachname, Vorname") der Teil
  vor dem Komma, sonst das letzte Wort von `name`. Ist das Ergebnis leer, gilt der volle `name`.
  Diese Regel setzt `kurzname()` in `src/io/monitorStil.mjs` um, und der Test deckt jeden Fall ab.
- **Verein:** Den Namen zeigt die Seite unter dem Spielernamen (Platz „Unter dem Namen"), das Logo
  vor ihm in der Box der Flagge (Platz „Vor dem Namen"). Das Logo kommt einheitlich über
  `BASE + "info/club-logo?name=<verein>"`:
  - LAN: Host-Endpoint wie heute.
  - Cloud: neue Relay-Route `/{ns}/info/club-logo`. Sie antwortet mit einem relativen 302 auf den
    badhub-Resolver `/api/v1/club-logo?name=<kodiert>`, den Namen kodiert und auf 120 Zeichen
    gekappt.
  - Fehlt das Logo, blendet `onerror` es aus; der Platz bleibt stehen, damit die Namen fluchten.
- **Slave-Brücke:** `court_display` reicht die Query weiter, damit `?stil=` erhalten bleibt.

### E1: Plätze im Setup

Im Abschnitt Court-Monitor des Setup-Assistenten erscheinen zwei Auswahlfelder: **Vor dem
Namen** (Nation (Flagge) · Vereinslogo · ohne; Standard Flagge) und **Unter dem Namen**
(Vereinsname · Nation (Text) · ohne; Standard ohne). Das Auswahlfeld „Layout" entfällt. Die
Anordnung gilt am Feld-Monitor; der abgeleitete Datenbedarf turnierweit für Feld-Monitor,
Tafel, Übersicht, Kombi und Stream, in LAN und Cloud.

### E2: Stream-Einblendung (`stream.html`)

- **Routen:** LAN `/stream?courts=<ids>`, Cloud `/{ns}/stream?courts=<ids>`.
- **`courts`:** kommagetrennte Feld-IDs. Nur Ganzzahlen, Duplikate fallen weg, gekappt bei 3.
  - 1 Feld: Einzelleiste.
  - 2–3 Felder: Leiste mit einer Zeile je Feld.
  - Leere oder ungültige Liste: komplett durchsichtig.
- **Optionen, nur abschaltend:** `nation=0`, `verein=0`, `logo=0` (Turnierlogo). Ohne
  Parameter ist das Turnierlogo an, Nation und Verein folgen dem Server.
- **Inhalt je Feld:**
  - Feldname
  - Namen, im Doppel beide
  - Flagge und Verein laut Server und URL
  - Satzstand je abgeschlossenem Satz
  - laufender Stand groß
  - Aufschlagmarke
  - Setzplatz wie am Monitor
- **Hintergrund:** `html`/`body` transparent. Nur die Leiste selbst hat eine Fläche.
- **Feld ohne Spiel:** Die Zeile entfällt. Ohne ein einziges Spiel ist die Seite durchsichtig.
- **Nie im Bild:** Werbung, Pausenbanner, Identifizieren, Offline-Hinweis, Kopplungsseite,
  Aufruf-Uhr. Die Seite meldet sich nicht als Gerät an (keine Geräte-ID).
- **Daten:** je Feld der Feld-Zustand (`/court/{id}/state` bzw. `/{ns}/court/{id}/state`) mit
  ETag. Dazu eine `monitor-ws` ohne Feldfilter als Nudge, `seq` je Feld und der Herzschlag wie am
  Monitor.
- **Ausfall:**
  - Als „Daten" zählen eine 200, eine 304 oder ein Herzschlag. Eine Relay-Antwort mit leerem
    `courtLabel` (Host weg) zählt nicht.
  - Der letzte Stand bleibt stehen. Nach **60 s** ohne Daten wird die Seite durchsichtig, ohne
    Fehlertext. Kommen wieder Daten, erscheint die Leiste wieder.

## Akzeptanzkriterien

**E1**

1. Eine gespeicherte Zuweisung `{"kind":"court","court_id":5}` wird als Feld 5 mit Stil
   „Standard" gelesen und genauso bytegleich zurückgeschrieben.
2. `{"kind":"court","court_id":5,"stil":"riesen"}` wird gelesen, und der Gerätezustand trägt
   `stil: "riesen"`.
3. `{"kind":"court","court_id":5,"stil":"zukunft"}` wird als Feld 5 gelesen (Stil Unbekannt, wirkt
   wie Standard). `MonitorControl` mit diesem Eintrag bleibt am Relay lesbar, es gibt kein 422.
   Beim Zurückschreiben fehlt `stil`.
4. Ein Relay mit dem Wire-Stand vor E1 (Spiegel-Enum im Test) liest eine Zuweisung mit `stil`
   als `Court`.
5. Ein Stilwechsel im Panel ändert die Zustands-Marke. Der TV wechselt ohne Neuladen.
6. Feste URL `/court/5/display?stil=hell` zeigt „Hell". `?stil=quatsch` zeigt „Standard". Im
   Gerätemodus ändert `?stil=hell` nichts.
7. Feldwechsel im Panel bei Stil „International" behält „International". Wechsel auf Tafel und
   zurück ergibt „Standard".
8. Eine Config ohne die neuen Felder ergibt Nation an, Verein aus. Alle Monitore zeigen die
   Flaggen wie vorher.
9. Bei Nation aus enthält kein Feld-Zustand, keine `/health`-Antwort (LAN und Cloud) und kein
   `/combo/state` eine Nationalität.
10. Bei Verein aus enthält keine dieser Antworten einen Vereinsnamen (`club`, `team*_clubs`).
    Bei Verein an trägt `MonitorPlayer.club` den Namen.
11. Ein Host-Upload ohne die neuen Config-Felder führt am Relay zu Nation an, Verein aus.
12. `GET /{ns}/info/club-logo?name=TSV%20X` antwortet 302 mit `Location:
    /api/v1/club-logo?name=TSV%20X`. Ein ungültiger Namespace ergibt 404, ein überlanger Name wird
    gekappt.
13. Über die Slave-Brücke bleibt `?stil=` bei der Weiterleitung erhalten.
14. Das Panel zeigt das Stil-Feld nur bei Feld-Zielen. Setup zeigt die zwei Häkchen, das
    Auswahlfeld „Layout" ist weg.

**E2**

15. `/stream?courts=3` zeigt die Einzelleiste von Feld 3 auf transparentem Grund.
16. `/stream?courts=1,2,2,3,4` zeigt die Felder 1, 2, 3 (Duplikat raus, Kappung 3).
    `courts=abc` ergibt eine durchsichtige Seite.
17. `?nation=0` blendet Flaggen aus, auch wenn der Server sie liefert. Bei Nation aus am Server
    bleibt die Flagge trotz fehlendem oder `nation=1` aus.
18. Ein Punkt am Tablet erscheint in der Einblendung über den WS-Nudge, ohne auf den
    Fallback-Takt zu warten.
19. Bei getrenntem Host zeigt die Einblendung den letzten Stand und ist nach 60 s durchsichtig.
    Es erscheint kein Fehlertext. Mit dem nächsten Datensatz ist sie wieder sichtbar.
20. Die feste Cloud-Route `/{ns}/court/{id}/state` antwortet mit ETag und bei unveränderter Marke
    mit 304.
21. `/{ns}/stream` mit ungültigem Namespace ergibt 404.
22. Die Inline-Kopien von `monitorSeq`/`pushHealth` in monitor, tafel, overview und stream sind
    wortgleich zum Modul (Drift-Test).

## Tests

- **relay-proto:**
  - `court_ohne_stil_serialisiert_bytegleich`
  - `court_mit_stil_roundtrip`
  - `unbekannter_stil_liest_als_unbekannt_und_schreibt_nichts`
  - `monitor_control_mit_unbekanntem_stil_bleibt_lesbar`
  - `alter_relay_liest_neuen_court` (Spiegel-Enum)
  - `monitor_config_ohne_neue_felder_nation_an_verein_aus`
  - `monitor_player_club_default_und_skip`
- **src-tauri:**
  - `alte_config_ohne_schalter_nation_an_verein_aus`
  - `build_monitor_state_ohne_nation_schalter_keine_nationalitaet`
  - `verein_nur_mit_schalter`
  - Datenschutz-Wächter `monitor_state_traegt_verein_und_nation_nur_mit_schalter` (Muster
    `the_state_never_carries_personal_data_beyond_its_purpose`)
  - `read_assignments_behaelt_court_mit_unbekanntem_stil`
  - `geraete_state_traegt_stil_feste_route_nicht`
  - `stilwechsel_aendert_marke`
  - `health_und_combo_ohne_vereine_und_nationen_bei_schalter_aus`
  - Slave-Brücke: Query bleibt
  - E2: `/stream` liefert HTML mit `no-store` und ersetztem `__BASE__`
- **relay:**
  - `relay_geraet_court_mit_stil`
  - `alter_host_upload_ohne_schalter_zeigt_nation_ohne_verein`
  - `host_schaltet_nation_ab_relay_filtert` (Feld-Zustand und `/health`)
  - `club_logo_leitet_relativ_um_und_kodiert`
  - E2: `feste_state_route_antwortet_304_bei_bekannter_marke`, `stream_route_unbekannter_namespace_404`
- **node (scripts/):**
  - `test-monitor-stil.mjs`: Allowlist; Gerätemodus schlägt URL; Unbekanntes wird `standard`;
    `zielMitStil` behält den Stil beim Feldwechsel; `kurzname` (alle Fälle); Inline-Kopie wortgleich
  - `test-stream-ansicht.mjs`: `courtsAusQuery`, `optionenAusQuery` nur abschaltend, `datenFrisch`
    60 s, `zaehltAlsDaten` (leeres `courtLabel`)
  - `test-inline-kopien.mjs`: alle Kopien von `monitorSeq`/`pushHealth`/`monitorStil`/`streamAnsicht`
  - `check-asset-syntax.mjs`
- **Sichtprüfung:** jeder Stil in LAN und Cloud auf 1080p-TV und Pi; Stream in OBS (Chromium-
  Browserquelle) über einem Testbild, mit 1 und 3 Feldern.
- **Last:** `scripts/last-monitor.mjs` mit drei Stream-Quellen à 3 Felder gegen das Relay.

## Risiken & Rollback

- **Relay-Rollback nach dem E1-Release:** Der Stil fällt in der Cloud auf Standard (kein
  Einfrieren), aber „Nation aus" wird am alten Relay ignoriert. Dann stehen in der Cloud wieder
  Flaggen. Das ist eine Datenschutz-Folge und gehört in `docs/cloud-relay.md`. Vereine bleiben aus.
- **App-Downgrade:** Zuweisungen bleiben, Stile gehen verloren, Flaggen sind wieder immer an.
- **Relay-Rollback nach E2:** `/{ns}/stream` antwortet 404, OBS zeigt eine nicht transparente
  Fehlerseite. Das Handbuch nennt das.
- **Relay-Last:** bis zu 12 Abrufe/s je Stream-Quelle mit Projektionsbau unter dem
  Namespace-Schloss, gemildert durch ETag/304. Ist die Messung schlecht, kommt eine
  Kombi-Zustandsroute am Relay als Ausbau (ADR 0069, Alternative).
- **Drift-Test startet rot,** falls die vorhandenen Kopien schon abweichen. Dann zuerst angleichen,
  im selben PR.
- **Größe von `monitor.html`:** Die Logik kommt in `monitorStil.mjs`, die Stil-Zweige bleiben
  dünn (CSS plus eine Zeilen-Renderfunktion für „International").

**Deploy-Reihenfolge:** Beim Merge deployt der Relay automatisch, danach folgt der App-Tag (Admin).
In der Cloud wirkt `?stil=` sofort. Der Gerätestil und die Schalter wirken erst mit dem neuen Host;
ein alter Host bedeutet Standard-Stil, Nation an, Verein aus. E2: Relay zuerst ist unkritisch.

## Offene Fragen / Annahmen

- Annahme: OBS nutzt die Chromium-Browserquelle (transparenter Hintergrund wird unterstützt).
- Annahme: Die Turnierlogo-Route `/info/logo` gibt es in LAN und Relay (`server.rs:598`,
  `relay/src/main.rs:754`).
- Keine offenen Fragen.

## Betroffene Doku-Dateien

- diese Spec; `docs/roadmap.md`; `docs/changelog.md` je Release
- `docs/court-monitor.md`:
  - Abschnitt „Layout" durch „Stile" ersetzen
  - neu: „Stream-Einblendung (OBS)", „Nation und Verein"
  - Endpunkte, Flaggen, Lizenzhinweis
- `docs/oberflaechen.md`: Stream-Einblendung, Hinweis „URL nur ans eigene Stream-Team"
- `docs/einstellungen.md`: Layout-Zeile raus, zwei Häkchen mit Standardwerten
- `docs/cloud-relay.md`:
  - Routen `/{ns}/stream`, `/{ns}/info/club-logo`
  - ETag der festen State-Route
  - Deploy und Rollback E1/E2
- `docs/features/monitor-livestand-push.md`: `stream.html` als Abnehmer von S1–S4/S6
- Handbuch-`pruef:`-Anker:
  - Stil-Beschriftungen in `CourtMonitorPanel.tsx`
  - Häkchen-Texte in `SetupWizard.tsx`
  - Routen in `server.rs`/`relay/`
  - Werte in `monitorStil.mjs`/`streamAnsicht.mjs`
  - Standardwerte in `config.rs`
- `CLAUDE.md`:
  - Zuordnungszeile „Monitor-Stile & Stream-Einblendung"
  - Asset-Muster um `stream`
  - Datenschutz-Satz zur Nation auf Monitoren (Standard an, abschaltbar, serverseitig)

## Umsetzungs-Hinweise

**E1 (ein PR, eigener Release):**

1. relay-proto-Typen und Tests (siehe oben)
2. `config.rs` (Felder, Default, `to_monitor_config`, `layout` veraltet)
3. `tablet/monitor.rs` Filter und Wächter
4. `server.rs`: `monitor_device_state` setzt `stil` aus dem Court-Ziel (die feste Route nie);
   `uebersicht_json` vor dem Cache-Bau und `combo_state` filtern Nation und Verein
5. Relay:
   - Geräte-Arm (`Court { court_id, stil }`, main.rs ~1140) setzt `stil`
   - `build_monitor_state`/`monitor_player` und `overview_health` filtern nach `namespace.monitor.config`
   - Route `info/club-logo`
6. `slave_bridge.rs:250`: Query weiterreichen
7. `src/io/monitorStil.mjs` (+ `.d.mts`): `STILE`, `stilAusQuery`, `wirksamerStil`, `zielMitStil`, `kurzname`;
   `types.ts`, Panel-Select (getrennt vom Options-Schlüssel `court:<id>`, Falle aus ADR 0049),
   Setup-Häkchen
8. `monitor.html`: `data-stil`, CSS je Stil, Zeilen-Renderfunktion „International",
   Riesen-Element immer befüllt und nur per CSS sichtbar, Vereinsname und -logo, Inline-Kopie
9. Doku, ADRs, Handbuch; `test-handbuch-fakten.mjs` grün

Nur zwei Nicht-Test-Stellen matchen `Court { court_id }` (relay main.rs ~1140, server.rs ~1648),
dazu `MonitorTarget::court_id()`.

**E2 (ein PR, eigener Release):**

1. `src/io/streamAnsicht.mjs` und Test
2. `assets/stream.html` und `test-inline-kopien.mjs`
3. LAN-Route `/stream` (`render_stream_html`)
4. Relay `/{ns}/stream`; die feste `monitor_state` auf `monitor_state_response` umstellen
   (ETag/304)
5. `check-handbuch-aktuell.mjs` Muster, Doku, ADR

**Pflicht nach jeder Code-Änderung:** code-reviewer. Zusätzlich security-reviewer für die
Logo-Weiterleitung (URL-Handling) und die URL-Parameter von `stream.html`.
