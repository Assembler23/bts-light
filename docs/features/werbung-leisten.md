# Sponsor-Leiste — kleine Werbung neben dem Turnierlogo

**Status:** Freigegeben (Entscheidungen unten). Umsetzung phasenweise, Phase 1
zuerst.

**Entscheidungen (2026-08-12):**
- **Reihenfolge:** Phase 1 zuerst (eigener PR/Release), dann 2, dann 3.
- **Tablet:** einbeziehen — im LAN-Teil von Phase 1 (tablet.html spricht den
  Host direkt an), Cloud-Tablet folgt in Phase 2.
- **badhub-Auslöser (Phase 3):** automatisch **beim Speichern** der
  Einstellungen (einmalig, nicht laufend).

## Ziel

Die vorhandenen **Werbebilder** sollen **alternativ klein in der oberen
Leiste** diverser Anzeigeseiten erscheinen — **neben dem Turnierlogo**. Pro
Bild einstellbar. Umfang über beide Repos (bts-light-Bildschirme **und**
badhub-Seiten Check-in/Zeitplan).

## Geklärte Anforderungen (aus dem Grill)

- **Pro-Bild-Schalter**: jedes hochgeladene Werbebild bekommt einen Haken
  „auch klein in der Leiste zeigen" (kein globaler Ein/Aus-Schalter).
- **Darstellung**: Sponsorbild(er) **neben** dem Turnierlogo, **kein
  Rotieren**; in der Regel **1–2** Bilder.
- **Monitor-Leerlauf** (freies Feld): bleibt **Vollbild**-Werbung; die kleine
  Leisten-Werbung kommt **nur während eines laufenden Spiels** dazu.
- **Turnierlogo sparsamer übertragen**: heute reist es base64 in jedem vollen
  `tset`-Heartbeat mit — künftig möglichst **einmalig beim Speichern**.
- **Seiten**: Tablet, Monitor, Spielanzeigen (Feldübersicht/Vorbereitung),
  badhub Check-in, badhub Zeitplan.

## Ist-Zustand (gemessen)

**bts-light — Werbebilder:** Dateien in `court-ads/` (nicht in der Config),
Labels in `court-ad-labels.json`. `CourtMonitorConfig` hat nur `show_ads` /
`ad_interval_s`. Draht: `AdUpload{content_type,data}` → Relay; `MonitorState.ads`
= Dateinamen (LAN) bzw. Indizes (Cloud). Auslieferung LAN `/ads/{file}`, Cloud
`/{ns}/ads/{idx}`. Genutzt heute nur von `monitor.html` (Vollbild-Rotation im
Leerlauf) und `ad.html`. **Keine Rollen-/Größentrennung** — flache Liste.

**bts-light — Turnierlogo:** `AppConfig.tournament_logo` (`LogoConfig{data,
mime,background_color}`). Transport **nur** an den badhub-Liveticker, **nur** im
vollen `tset` (`sync.rs`, `Update::Full`) als `event.tournament_logo*`. Volle
`tset` bei Erstlauf, struktureller Änderung oder Heartbeat > 60 s. Deltas tragen
es bewusst nicht. **Kein Transport zum Relay, zu keiner bts-light-Anzeigeseite.**
`monitor.html`/`overview.html`/`preparation.html`/`tablet.html` kennen es nicht.

**bts-light — Kopfleisten:** `overview.html` und `preparation.html` haben je
`<header class="bar">` (Flex, Statuspunkt + Titel + Halle/Turnier, **kein
Bild**) — ideal für ein kleines `<img>`. `monitor.html` Match-Ansicht hat eine
`bar` (Court/Uhr/Timer/Disziplin, kein Logo). `tablet.html` `<header>` ist eng
(Undo/Injury-Buttons links/rechts).

**Cloud-Grenze:** Info-Targets (Übersicht/Vorbereitung) sind laut
`relay_client.rs` heute **LAN-only** (Cloud-Wire kennt nur Court-Targets, TODO
im Code). Im Cloud-Modus sind diese beiden Seiten also vorerst nicht erreichbar.

**badhub — Check-in/Zeitplan:** `/checkin/{uuid}`, `/checkin/{uuid}/zeitplan`,
Logo via `/checkin/{uuid}/logo`. Kopf `<header class="ci-head">` (Logo +
Titel), Kiosk zusätzlich obere Tab-Leiste. Turnierlogo kommt schon an (tset-
Snapshot base64 **oder** manueller Upload nach `public/assets/logos/
checkin_{uuid}.{ext}`). **Sponsor-Bilder existieren nicht** — Neuentwicklung.
Empfehlung: **eigener Datei-/Upload-Weg** statt das 5-MB-Snapshot aufzublähen.
Regelkonform (R1): bts-light → badhub-API (Bearer) → DB/Datei → Blade liest nur
lokale DB. „Zeitplan" = Check-in-Anfangszeiten je Klasse (kein Match-Spielplan).

## Datenmodell-Erweiterung

- **Pro-Ad-Flag `in_bar: bool`.** Ablage: `court-ad-labels.json` von
  `{datei: label}` zu `{datei: {label, in_bar}}` erweitern (abwärtskompatibel:
  alter String = Label, `in_bar=false`). Neuer Command `set_court_ad_bar`.
- **Turnierlogo als Anzeige-Ressource.** Neue LAN-Route `/info/logo` (liefert
  `config.tournament_logo` mit `Cache-Control`, analog `/info/club-logo`).
- **Bar-Ads-Auswahl** in `MonitorState`/`/info/*/state`: die als `in_bar`
  markierten Dateien/Indizes gesondert ausweisen (`bar_ads`), damit die Leiste
  nicht die Vollbild-Rotation anfasst.

## Umsetzung — Phasen (klein, überprüfbar)

### Phase 1 — bts-light Monitor + Spielanzeigen + Tablet (LAN) — **UMGESETZT (v0.9.189)**

1. ✅ `in_bar`-Flag: Store `court-ad-bar.json` (`monitor.rs` `read/write_ad_bar`),
   Command `set_court_ad_bar` (`commands.rs`), SetupWizard-Häkchen „Leiste" je
   Bild, `list_court_ads` liefert `in_bar`, Aufräumen beim Löschen.
2. ✅ `/info/logo`-Route (`server.rs`, aus `config.tournament_logo`) + `barAds`
   + `hasLogo` in `/info/ad/state`.
3. ✅ Leiste rendern: `overview.html` + `preparation.html` + `monitor.html`
   Match-`bar` + `tablet.html`-Kopf — Turnierlogo + 1–2 `in_bar`-Ads
   (`<img>`, `onerror`-Rückfall). Monitor-Leerlauf unverändert (Vollbild),
   Tablet auf schmalen Geräten ausgeblendet.
4. ✅ Rust-Test `read_write_ad_bar_roundtrip`. `docs`: `court-monitor.md`,
   `changelog`.

### Phase 2 — Cloud (Monitor + Tablet) — **UMGESETZT (v0.9.190)**

1. ✅ `AdUpload.in_bar` + `MonitorUpload.logo` (`relay-proto`, serde-default-
   kompatibel) → Relay `MonitorBundle` (Logo als `AdImage`, `in_bar` je Ad).
   Neue Relay-Routen `/{ns}/info/logo` und `/{ns}/info/ad/state` (barAds als
   **Indizes**, `hasLogo`). Sparsam: `monitor_fingerprint` um Logo + `in_bar`
   erweitert, Upload bleibt änderungs-gegated.
2. ✅ Cloud-**Tablet** und -**Monitor** nutzen dieselben Phase-1-HTML-Snippets
   (BASE-relativ) — mit den neuen Relay-Routen greift die Leiste jetzt auch im
   Cloud-Modus. Keine HTML-Änderung nötig.
3. Übersicht/Vorbereitung im Cloud: **Nicht-Ziel** (Cloud-Info-Targets noch
   nicht ausgebaut, `relay_client.rs`-TODO) — bleiben LAN-only.
4. ✅ `relay-proto`-Serde-Roundtrip + Default-Kompat-Test
   (`ad_upload_in_bar_and_logo_default_to_off`).

### Phase 3 — badhub Check-in + Zeitplan (Cross-Repo)

1. **Sparsame Übertragung**: Turnierlogo aus dem Heartbeat-`tset` herausnehmen;
   stattdessen bts-light lädt Logo **und** 1–2 Sponsor-Bilder **einmalig beim
   Speichern** (bzw. per Knopf) an einen **neuen badhub-Endpoint** hoch
   (Bearer, wie `live_update.php`). Ablage `public/assets/logos/
   sponsor_{uuid}_{n}.{ext}` (Datei = Wahrheit, wie der manuelle Logo-Upload).

   ✅ **Sponsor-Push umgesetzt (v0.9.194):** Beim Umschalten des „Leiste"-Häkchens
   (`set_court_ad_bar`) schiebt bts-light die markierten Bilder als roh-Base64
   (max 4, alphabetisch, `checkin_sponsor_max()`-konform) an
   **`POST /api/checkin-branding`** — Bearer = Liveticker-Passwort, Body
   `{"sponsors":[…]}`, **keine GUID** (badhub löst das Turnier über das Passwort
   auf). Additiv/feuer-und-vergiss: ohne badhub-Passwort passiert nichts, und
   HTTP 404/400 bedeutet „badhub kennt den Endpunkt noch nicht" (nur Log). Die
   URL wird aus `config.badhub.url` abgeleitet (letztes Pfadsegment ersetzt).
   Code: `commands.rs collect_bar_sponsors_b64`/`push_bar_sponsors_to_badhub`,
   `badhub/push.rs checkin_branding_url`/`push_checkin_branding`,
   `badhub/payload.rs CheckinBrandingMessage`.

   ✅ **Logo-Push umgesetzt (v0.9.195):** Ändert der Operator das Turnierlogo,
   schiebt bts-light es beim Speichern (`save_config`, nur bei echter Änderung)
   über **denselben** Endpunkt an badhub (`CheckinBrandingMessage.logo`, roh-
   Base64; leer = löschen). Die Nachricht ist **feld-unabhängig**: das Sponsor-
   Häkchen sendet nur `sponsors`, das Speichern nur `logo` — beide `Option`, ein
   `None`-Feld wird weggelassen und lässt badhub das andere unberührt (spart die
   bis 2 MB Logo-Bytes bei jedem Sponsor-Toggle). badhub speichert es als
   `checkin_{uuid}.{ext}`; `turnierLogoUrl()` bevorzugt die Datei bereits vor dem
   tset-Snapshot. Code: `commands.rs push_logo_to_badhub`/`spawn_branding_push`.
   **Noch offen (separater PR nach badhub-Deploy):** das Logo aus dem
   Heartbeat-`tset` nehmen (`sync.rs`) — erst sicher, wenn badhub es nachweislich
   aus der Datei ausliefert, sonst verlieren Check-In/Zeitplan das Logo.
2. badhub-Auslieferung: Endpunkt(e) analog `/checkin/{uuid}/logo`
   (Inhalts-Validierung, kein SVG, `nosniff`, Cache). nginx-Block „kein PHP in
   `public/assets/logos/`" muss vor Rollout stehen.
3. Anzeige in `ci-head` (Handy) und Kiosk-Tab-Leiste + `checkin_zeitplan`-Kopf.
4. badhub-Tests (`tests/integration/*_db_test.php` + CI-Wiring), Doku
   `features/hallen_checkin.md`; bts-light-Payload-Schema in
   `docs/features/spieler-check-in.md` nachziehen.

## Nicht-Ziele

- Keine Sponsor-Werbung auf der Vollbild-Liveticker-Seite (`public/live.php`)
  in dieser Iteration.
- Kein Rotieren in der Leiste (bewusst statisch, 1–2 Bilder).
- Kein Ausbau des Cloud-Info-Target-Protokolls für Übersicht/Vorbereitung,
  solange nicht ausdrücklich gewünscht (Phase 2, optional).

## Offene Design-Entscheidungen (für die Freigabe)

1. **Reihenfolge/Schnitt**: Empfehlung — Phase 1 zuerst ausliefern (sichtbarer
   Nutzen im Hallennetz), dann 2, dann 3. Alternativ alles bündeln.
2. **Tablet**: kleines Logo+Sponsor trotz engem Kopf aufnehmen, oder Tablet
   vorerst weglassen?
3. **badhub-Upload-Zeitpunkt**: „beim Speichern der Einstellungen" automatisch,
   oder ein expliziter Knopf „Logo/Sponsoren an badhub senden"?

## Risiken

- Payload/Datenmenge: der Datei-Upload-Weg (Phase 3) vermeidet das Aufblähen
  des Liveticker-Snapshots; Logo-Minimierung reduziert die laufende Last.
- Cloud-Übersicht/Vorbereitung: ohne Protokoll-Ausbau nicht erreichbar — sauber
  als Nicht-Ziel markiert, kein stiller Teil-Zustand.

## Nachtrag 18.08.2026 — Datenmenge im Betrieb (v0.9.225)

Das oben genannte Risiko ist im Hallenbetrieb tatsächlich eingetreten, nur an
anderer Stelle als erwartet: nicht im Liveticker-Snapshot, sondern in der
laufenden Auslieferung an die Anzeigegeräte. Beide Bild-Routen standen auf
`Cache-Control: no-store`, und weil die Vollbild-Werbung ihr Motiv alle zehn
Sekunden wechselt, lud jedes Gerät dabei jedes Mal die vollen Bilddaten neu.
Behoben durch Kennung (`ETag`) plus fünf Minuten Cache-Frist auf allen vier
Routen (LAN und Cloud, Werbebild und Logo) sowie einen Änderungsabgleich in
der Sponsor-Leiste, damit ihr Minuten-Takt nicht bei jedem Durchlauf neue
`<img>` anlegt. Einzelheiten und die Begründung gegen `immutable`:
[../court-monitor.md](../court-monitor.md).

**Weiterhin offen:** Das Turnierlogo reist als Base64 in **jedem vollen
`tset`** an badhub mit (`sync.rs`). Weglassen ist derzeit nicht möglich — ein
`tset` ersetzt bei badhub den kompletten Snapshot-Datensatz, ein fehlendes
Feld löscht das Logo also und blendet es im Liveticker sowie auf der
Check-In-Seite aus. Erst wenn badhub fehlende Logo-Felder als „unverändert"
behandelt (so, wie es `checkin_branding_apply` für den Branding-Weg bereits
tut), darf bts-light es nur noch bei Änderung mitschicken.
