# Stream-Werbung — zweite OBS-Ebene für Sponsoren — Spezifikation

> Status: **abgestimmt 2026-11-12** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Idee vom 12.11.2026. Betroffene Crates: src-tauri / relay / relay-proto / src.
> ADR: [0092](../adr/0092-stream-werbung-markierung-und-abriss.md). Ergänzt ADR 0069
> (Score-Leiste als eigene Seite) und ADR 0041 (Werbe-Stil als eigene Datei).

## Kontext / Problem

Seit v0.9.328 gibt es die Score-Leiste für OBS (`/stream?courts=`). Wer ein Turnier streamt,
möchte auch die **Sponsoren** des Turniers einblenden, ohne die Bilder in OBS von Hand zu
pflegen. Die Werbebilder liegen schon in bts-light (Setup → Werbebilder) und laufen auf den
Hallen-Monitoren. Für den Stream fehlt eine transparente Ebene. Außerdem soll die
Turnierleitung wählen können, **welche** Bilder in den Stream dürfen: Nicht jeder Hallen-Sponsor
hat auch die Stream-Präsenz gekauft.

## Zielbild & Erfolgskriterien

- Im Panel **Court-Monitor** gibt es den Baukasten **Stream-Werbung (OBS)**. Dort wählt man
  die Darstellung, den Hintergrund und, beim Band, das Tempo, und kopiert die Adresse für das
  Hallennetz bzw. die Cloud in eine OBS-Browser-Quelle.
- Im Setup bekommt jedes Werbebild ein zweites Häkchen **Stream**, neben **Leiste**. Nur
  angehakte Bilder erscheinen im Stream.
- Drei Darstellungen: **rotierend** (ein Bild, Überblendung im Werbe-Intervall),
  **nebeneinander** (alle angehakten Bilder in einer Reihe), **Band** (die Bilder laufen
  endlos durch).
- Im Stream steht nie ein Fehlertext. Ein Relay-Deploy mitten in der Übertragung lässt die
  Sponsoren **stehen**.
- Erfolg beim nächsten gestreamten Turnier: Das Stream-Team bindet die Adresse einmal ein,
  und die Sponsoren laufen den ganzen Tag ohne Eingriff, auch über Relay-Deploys hinweg.

## Nicht-Ziele

- Keine Bildauswahl je Adresse. Die Auswahl gilt für alle Stream-Adressen gleichermaßen
  (Häkchen im Setup).
- Kein eigenes Wechsel-Intervall für die Rotation. Es gilt das Werbe-Intervall der Monitore
  (`ad_interval_s`).
- Keine Parameter für Lage und Größe. Die Seite füllt die OBS-Quelle; Größe und Lage stellt
  man in OBS ein.
- Keine Route über die Slave-Brücke. In einer fernen Halle gilt die Cloud-Adresse (wie bei
  der Score-Leiste, ADR 0069).
- Kein Klick- oder Sichtbarkeits-Tracking.
- `ad.html`, Sponsor-Leiste und Feld-Monitore verhalten sich unverändert, auch der
  Schalter **Werbung im Leerlauf anzeigen** (`show_ads`) wirkt **nicht** auf den Stream.
- Kein Hochladen eigener Stream-Bilder. Es gibt nur die vorhandenen Werbebilder.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - `src-tauri/src/tablet/monitor.rs`: Markierungsdatei `court-ad-stream.json`.
  - `src-tauri/src/commands.rs`: Command `set_court_ad_stream`, `CourtAd.in_stream`,
    Aufräumen in `remove_court_ad`.
  - `src-tauri/src/tablet/server.rs`: `streamAds` in `/info/ad/state`, Seite
    `/stream/werbung`.
  - `src-tauri/src/tablet/relay_client.rs`: `in_stream` im Upload und im Fingerabdruck.
  - `relay-proto`: `AdUpload.in_stream`.
  - `relay/`: `AdImage.in_stream`, `streamAds`/`streamVer`, `?zweck=stream` mit 503, Seite
    `/{ns}/stream/werbung`.
  - `src/`: Häkchen im `SetupWizard`, Baukasten im `CourtMonitorPanel`,
    `src/io/streamWerbung.mjs`, `streamWerbungLink` in `src/io/streamLink.mjs`.
  - Neues Asset `src-tauri/assets/stream-werbung.html`.
- **Architekturregeln:**
  - R1: Das Häkchen geht über den Tauri-Command `set_court_ad_stream`.
  - R2, R4, R5: nicht berührt (keine Match-Daten, keine Ergebnisse).
  - R3: LAN und Cloud sind beide bedacht; bei LanAndCloud lädt der Master hoch, wie heute.
  - R6: Die Cloud-Adresse enthält den Namespace. Das ist wie bei der Score-Leiste akzeptiert
    (ADR 0069, Punkt 5), das Handbuch sagt „nur ans eigene Stream-Team".
- **Konfiguration & Abwärtskompatibilität:**
  - Kein neues Feld in `config.rs`. Die Markierung liegt in einer **eigenen Datei**
    `court-ad-stream.json` im Werbe-Verzeichnis, als Menge von Dateinamen wie
    `court-ad-bar.json`.
  - Fehlt die Datei oder ist sie kaputt, ist die Menge leer und kein Bild steht im Stream.
  - Eine ältere Version ignoriert die Datei; ein Rollback ist folgenlos.
  - `identifier` und Updater-Pfad bleiben unangetastet.
- **Datenschutz:** Es fließen nur Werbebilder und ihre Hintergrundfarbe, keine
  Personendaten.
- **Abhängigkeiten:**
  - Keine neue Cargo- oder npm-Abhängigkeit.
  - Die OBS-Browser-Quelle (Chromium/CEF) kann CSS-Animationen und `fetch`.
  - Deploy-Reihenfolge: Der Relay wird beim Merge automatisch deployt, vor dem App-Tag.
  - Wire-Kompatibilität in beide Richtungen ist gegeben, weil es kein `deny_unknown_fields`
    gibt (siehe Risiken).

## Verhalten im Detail

### Adresse und Parameter

- Hallennetz: `http://<Turnier-PC>:8088/stream/werbung`
- Cloud: `https://badhub.de/bts-relay/<Kennung>/stream/werbung`

| Parameter | Werte | Vorgabe |
|---|---|---|
| `modus` | `rotation`, `reihe`, `band` | `rotation` |
| `hintergrund` | `transparent`, `kachel` | `transparent` |
| `tempo` (nur bei `band`) | `langsam` (40 px/s), `mittel` (80 px/s), `schnell` (140 px/s) | `mittel` |

Ein unbekannter Wert fällt auf die Vorgabe zurück. Der Baukasten lässt Vorgaben in der Adresse
weg und setzt `tempo` nur bei `modus=band`.

### Darstellung

- **Allgemein:**
  - Seite und Hintergrund sind transparent, die Fläche füllt die Quelle (100 vw × 100 vh).
  - Bilder werden in Höhe und Breite eingepasst (`contain`), nie verzerrt und nie
    beschnitten.
  - Fester Abstand zwischen Bildern 24 px; Innenrand der Kachel 8 px.
  - Die Reihenfolge der Bilder ist die der Werbebilder im Setup (nach Hochladezeit).
- **Kachel:**
  - Jedes Bild steht auf seiner im Setup gesetzten Hintergrundfarbe.
  - Ohne gesetzte Farbe gilt Schwarz `#000000`. Ein leerer Wert und `#000000` werden gleich
    behandelt.
  - Nur Werte der Form `#rrggbb` werden übernommen, alles andere wird schwarz.
- **rotierend:**
  - Zwei gestapelte Bilder mit weicher Überblendung, wie in `ad.html`.
  - Das Intervall ist `intervalS` aus dem Zustand (mindestens 1 s).
  - Bei genau einem Bild steht es fest, ohne Überblenden und ohne Zeitgeber.
- **nebeneinander:**
  - Alle Bilder stehen in einer Reihe in voller Quellhöhe.
  - Ist die Reihe breiter als die Quelle, werden alle gemeinsam verkleinert, bis sie passt.
  - Die Reihe steht mittig.
- **Band:**
  - Die Bilder laufen von rechts nach links endlos durch, per CSS-Animation `translateX`.
  - Damit die Schleife auch bei einem einzigen Bild nahtlos ist, wird die Bildfolge so oft
    wiederholt, bis sie die Quelle füllt, und dann verdoppelt.
  - Die Geschwindigkeit ist fest in px/s, also unabhängig von der Zahl der Bilder.
  - Das Band startet nur neu, wenn sich die Bildliste ändert. Ein gleichbleibender Abruf
    lässt es ungestört weiterlaufen.
- **Fensteränderung:** Maßstab und Band werden neu berechnet.

### Daten und Abruf

- Die Seite fragt alle 20 s `<BASE>info/ad/state?zweck=stream` ab, mit Frist und
  `cache: no-store`. Sie ist kein Gerät und schickt keinen Herzschlag.
- Neue Felder im Zustand (additiv, `ad.html` und die Sponsor-Leiste ignorieren sie):
  - `streamAds`: die Kennungen der angehakten Bilder in Setup-Reihenfolge, im LAN als
    Dateinamen, in der Cloud als Indizes. Im LAN wird gegen die vorhandenen Dateien gefiltert.
  - `streamVer` (nur Cloud): je Eintrag in `streamAds` die Inhaltsmarke des Bildes. Die Seite
    hängt sie als `?v=` an die Bild-Adresse. Rücken nach dem Löschen eines Bildes die Indizes
    nach, kann OBS so nicht für bis zu 60 s ein falsches Motiv aus dem Cache zeigen.
- **Entscheidung der Seite nach jedem Abruf:**
  - Antwort 200, und `streamAds` ist eine Liste: Diese Bilder werden gezeigt; eine leere
    Liste leert die Ebene.
  - Jede andere Antwort (503, 500, 404, Netzfehler, Zeitüberschreitung, fehlendes Feld
    `streamAds` von einem alten Relay, kaputtes JSON): Es bleibt, was steht.
  - Ein einzelnes Bild, das nicht lädt, wird ausgeblendet, ohne kaputtes Bildsymbol.
- **Relay mit `?zweck=stream`:**
  - 503, wenn die Lage „Start-Karenz", „Host weg" oder „Gesperrt" ist (vorhandenes
    `anzeige_lage`/`sperre`, ADR 0088).
  - 503 mit Rumpf `{"grund":"keine_daten"}`, wenn noch kein Monitor-Upload vorliegt: nach
    einem Relay-Neustart, nach Ablauf der Daten-Frist (ADR 0089) oder wenn der Host
    verbunden ist, aber noch nicht hochgeladen hat.
  - Ebenso 503, wenn kein Host verbunden ist und die Bildliste leer ist (Bilder bei einem
    hostlosen Turnier freigegeben).
  - **Ohne `zweck`** antwortet der Relay genau wie heute (200, leere Listen, wenn nichts da
    ist).
- **LAN mit `?zweck=stream`:** 503, wenn die Markierungsmenge nicht leer ist, die Bildliste
  aber leer. Das ist ein Lesefehler im Werbe-Verzeichnis, nicht die echte leere Auswahl;
  die echte Leere entsteht nie so, weil `remove_court_ad` die Markierung mit aufräumt.

### Häkchen im Setup

- In der Werbebild-Zeile steht neben **Leiste** ein Häkchen **Stream**, mit dem Tooltip
  „In der Stream-Werbung (OBS) zeigen".
- Umschalten ruft `set_court_ad_stream(file, inStream)`. Der Command prüft den Dateinamen
  (`is_safe_image_name`) und schreibt die Menge atomar.
- Der Command löst **keinen** badhub-Push aus (anders als `set_court_ad_bar`).
- Beim Löschen eines Werbebilds entfernt `remove_court_ad` auch seine Stream-Markierung.
- **Wirkung:**
  - Im Hallennetz beim nächsten Abruf (höchstens 20 s).
  - In der Cloud nach dem nächsten Fingerabdruck-Takt (30 s) und dem Upload, also nach
    höchstens rund 50 s.

### Cloud-Transport

- `AdUpload` bekommt `#[serde(rename = "inStream", default)] in_stream: bool`, positionsweise
  wie `inBar`.
- Der Host setzt es beim Upload aus `court-ad-stream.json`.
- Der Fingerabdruck (`ad_abdruck_zeile`) enthält das Flag. Ohne das würde ein reines
  Umhaken nie hochgeladen.
- Der Relay übernimmt das Flag in `AdImage.in_stream` und baut daraus `streamAds` und
  `streamVer`.

## Akzeptanzkriterien

- [ ] Ist im Setup bei Bild A und C „Stream" angehakt, bei B nicht, zeigt
      `/stream/werbung` (LAN) und `/{ns}/stream/werbung` (Cloud) genau A und C in dieser
      Reihenfolge.
- [ ] Ohne gesetztes Häkchen liefert der Zustand `streamAds: []` mit Status 200, und die
      Seite zeigt nichts, auch keinen Text.
- [ ] `modus=rotation` mit 2 Bildern wechselt im Werbe-Intervall mit Überblendung. Mit
      1 Bild steht das Bild ohne Zeitgeber.
- [ ] `modus=reihe` mit Bildern, die zusammen breiter als die Quelle sind, verkleinert alle
      gemeinsam. Nichts wird abgeschnitten, und die Reihe steht mittig.
- [ ] `modus=band` läuft nahtlos, auch mit 1 Bild. `tempo=langsam|mittel|schnell` ergibt
      40/80/140 px/s. Ein Abruf mit unveränderter Liste startet das Band nicht neu.
- [ ] `hintergrund=kachel` stellt jedes Bild auf seine Setup-Farbe; ohne Farbe auf
      `#000000`. `hintergrund=transparent` zeigt keine Fläche hinter den Bildern.
- [ ] Unbekannte Parameterwerte fallen auf die Vorgaben zurück (`rotation`, `transparent`,
      `mittel`).
- [ ] Nach einem Relay-Neustart (Namespace oder Monitor-Upload fehlt) antwortet
      `/{ns}/info/ad/state?zweck=stream` mit 503, und die Seite lässt die zuletzt gezeigten
      Bilder stehen.
- [ ] Ist der Host länger als die Karenz weg, oder ist die Daten-Frist abgelaufen, antwortet
      der Relay mit 503. Die Seite lässt die Bilder stehen.
- [ ] `/{ns}/info/ad/state` **ohne** `zweck` antwortet unverändert, auch ohne Upload 200 mit
      leeren Listen; `ad.html` und die Sponsor-Leiste merken keinen Unterschied.
- [ ] Ein Netzfehler oder eine Zeitüberschreitung beim Abruf lässt die Bilder stehen.
- [ ] Eine Antwort ohne `streamAds` (alter Relay) lässt die Bilder stehen.
- [ ] LAN: Ist die Markierung gesetzt, das Werbe-Verzeichnis aber nicht lesbar, antwortet der
      Abruf mit 503.
- [ ] Ein Häkchen „Stream" ändert den Fingerabdruck des Monitor-Uploads; die Cloud zeigt die
      neue Auswahl nach dem nächsten Takt.
- [ ] Ein Häkchen „Stream" löst keinen badhub-Push aus.
- [ ] Wird ein angehaktes Bild gelöscht, verschwindet seine Markierung aus
      `court-ad-stream.json`. Die Cloud zeigt danach unter einem nachgerückten Index kein
      falsches Motiv (`?v=` aus `streamVer`).
- [ ] `show_ads = false` lässt `streamAds` unberührt.
- [ ] Ein alter Upload ohne `inStream` wird am Relay als `false` gelesen. Ein neuer Upload an
      einen alten Relay wird ohne Fehler angenommen.
- [ ] `ad.html`, Sponsor-Leiste und Feld-Monitore zeigen dieselben Bilder wie vor der Änderung.
- [ ] Die Seite enthält kein `innerHTML` und keinen sichtbaren Text, und setzt Farben nur
      über `style.backgroundColor`.
- [ ] Der Baukasten erzeugt Adressen, die von der Seite zu denselben Optionen gelesen werden
      (Rundweg-Test).

## Tests

**Rust (TDD):**

- `monitor.rs`: `read_write_ad_stream_roundtrip`; fehlende oder kaputte Datei ergibt eine leere
  Menge.
- `commands.rs`:
  - `stream_markierung_wird_beim_loeschen_mitgeraeumt`, mit dem Kern als freie Funktion mit
    Pfad, ohne `AppHandle`;
  - `set_court_ad_stream_lehnt_unsichere_namen_ab`.
- `server.rs`:
  - `stream_ads_nur_vorhandene_dateien_in_setup_reihenfolge`;
  - `der_gemerkte_stream_marker_folgt_der_datei` (Umhaken bei gleicher Dateilänge);
  - `ad_state_ohne_stream_marken_liefert_leere_stream_liste`;
  - `stream_abruf_mit_marken_aber_ohne_bilder_ist_503`;
  - `stream_ads_unabhaengig_von_show_ads`;
  - `stream_werbung_seite_ersetzt_basis_und_wird_nicht_gecacht`.
- `relay-proto`: `ad_upload_in_stream_default_aus` (JSON ohne `inStream`) und
  `ad_upload_in_stream_roundtrip`.
- `relay_client.rs`: `der_abdruck_bemerkt_ein_stream_haekchen`.
- `relay/`:
  - `monitor_upload_exposes_stream_ads_und_version`;
  - `stream_ad_state_503_ohne_upload`;
  - `stream_ad_state_503_wenn_host_weg`;
  - `stream_ad_state_503_nach_relay_neustart`;
  - `stream_ad_state_503_wenn_bilder_freigegeben_ohne_host`;
  - `ad_state_ohne_zweck_bleibt_200_leer`;
  - `stream_werbung_seite_traegt_mount_basis`;
  - `stream_werbung_seite_unbekannter_namespace_404`.

**JS:**

- `scripts/test-stream-werbung.mjs` für `src/io/streamWerbung.mjs`:
  - Query-Vorgaben und Unsinn;
  - Entscheidung für 200, 304, 503, 500, 0 und fehlendes Feld;
  - Farbe leer, `#000000` und ungültig;
  - Bild-Adresse mit und ohne Version;
  - Maßstab für `reihe`;
  - Band-Wiederholungen bei 1 Bild;
  - Rundweg von `streamWerbungLink` zu den Optionen;
  - Seitenprüfungen: `var BASE = "__BASE__";`, kein `innerHTML`, Abruf enthält
    `zweck=stream`.
- `scripts/test-inline-kopien.mjs`: Drift der Inline-Kopie zum Modul.
- `scripts/test-abruf-frist.mjs`: die neue Seite in der Seitenliste.
- `cargo test` grün, `npm run build` fehlerfrei, Clippy mit `-D warnings`.

**Manuell:** OBS-Browser-Quelle 1920×120 mit allen drei Modi, im LAN und in der Cloud. Den
Relay neu starten, während die Quelle läuft: Die Bilder bleiben stehen.

## Risiken & Rollback

- **Häkchen kommt in der Cloud nicht an:** abgesichert durch das Flag im Fingerabdruck und
  einen Test dafür.
- **Wire in beide Richtungen kompatibel:**
  - Ein alter Relay verwirft `inStream` und kennt `/stream/werbung` nicht (404, OBS zeigt
    eine Fehlerseite). Deshalb wird der Relay mit PR 1 zuerst deployt.
  - Eine alte App am neuen Relay schickt kein Flag; die Stream-Ebene bleibt leer.
- **Upload-Grenzen:** Ein Bild jenseits von `MAX_UPLOAD_ADS = 24` bzw.
  `MAX_UPLOAD_TOTAL = 12 MB` fehlt nur in der Cloud. Bekannte Grenze, wird im Handbuch genannt.
- **Bilder-Budget am Relay erschöpft:** Die alten Bilder samt alten Häkchen bleiben stehen,
  wie heute bei `inBar`.
- **Geänderter Fingerabdruck:** Das neue Format löst nach dem Update einmal einen Upload aus.
  Unkritisch, weil jede Sitzung ohnehin mit leerem Abdruck beginnt.
- **Laufendes Turnier:** Die Änderung berührt keine Match- oder Ergebnis-Strecke. Die
  zusätzliche Last ist ein Abruf je 20 s je OBS-Quelle.
- **Rollback:**
  - Eine ältere App ignoriert `court-ad-stream.json`.
  - Ein Relay-Rollback lässt die Cloud-Adresse 404 antworten, ohne Einfluss auf andere
    Anzeigen.

## Offene Fragen / Annahmen

- Annahme: Die Reihenfolge der Werbebilder (Hochladezeit) genügt für den Stream. Ein
  Umsortieren gibt es im Setup nicht, und es ist kein Ziel.
- Annahme: Die px/s-Werte 40/80/140 und die Abstände 24/8 px sind ein guter Start. Sie sind
  Konstanten in `streamWerbung.mjs` und werden nach dem ersten Stream-Einsatz geprüft.

## Betroffene Doku-Dateien

- `docs/court-monitor.md` (öffentliches Handbuch): neuer Abschnitt „Stream-Werbung (OBS)"
  mit `pruef`-Ankern auf die Beschriftung des Häkchens in `SetupWizard.tsx`, die Route in
  `server.rs` und `relay/src/main.rs` und die Modus-Werte in `streamWerbung.mjs`. Dazu:
  Wirkzeit in der Cloud, Upload-Grenzen, keine Adresse in der fernen Halle, Adresse nur ans
  eigene Stream-Team.
- `docs/einstellungen.md`: Häkchen „Stream" mit Anker.
- `docs/oberflaechen.md`: Route und Baukasten.
- `docs/cloud-relay.md`: `AdUpload.inStream`, `streamAds`/`streamVer`, 503-Verhalten mit
  `?zweck=stream`.
- `docs/features/monitor-stile-und-stream.md`: Querverweis.
- `docs/changelog.md` je Version.
- `CLAUDE.md`: neue Tabellenzeile „Stream-Werbung", `stream-werbung` in die Asset-Liste der
  Handbuch-Tabelle.
- `docs/adr/0092-…` + `docs/adr/README.md`.

## Umsetzungs-Hinweise

Zwei PRs. Die Version legt erst der Merge fest (aktuell main v0.9.393). Reviews:
`code-reviewer` Pflicht; `security-reviewer` für den Command mit Dateinamen (PR 1) und die
Seite mit Farb- und URL-Bau (PR 2).

**PR 1 — Markierung, Wire, Zustand, 503** (Relay deployt automatisch beim Merge):

1. **`monitor.rs`:**
   - `AD_STREAM_FILE = "court-ad-stream.json"` mit `read_ad_stream`/`write_ad_stream`.
   - Gemeinsame private Mengen-Helfer, an die auch `read_ad_bar`/`write_ad_bar` delegieren.
     Die öffentliche API bleibt gleich.
2. **`commands.rs`:**
   - Pfad-Helfer `monitor_ad_stream_path`.
   - `set_court_ad_stream` nach dem Muster `set_court_ad_bar`, ohne badhub-Push.
   - `CourtAd.in_stream` in `list_court_ads`.
   - Aufräumen in `remove_court_ad`.
   - Registrierung in `lib.rs`.
3. **`server.rs`:**
   - Zwischenstand `ad_stream()` mit dem **Datei-Inhalt als Schlüssel** (Muster
     `ad_style()`), weil ein Umhaken die Länge erhält.
   - `streamAds` in `info_ad_state`.
   - LAN-503-Regel bei `zweck=stream`. `DeviceHeartbeat.zweck` gibt es schon.
4. **`relay-proto`:** `AdUpload.in_stream` (`serde default`, ohne `skip_serializing_if`).
   Alle Struktur-Literale in Tests ergänzen.
5. **`relay_client.rs`:**
   - `upload_monitor` setzt `in_stream`.
   - `ad_abdruck_zeile` bekommt den Parameter, Format
     `|{name}:{len}:{mtime}:{in_bar}:{in_stream}:{bg}:{zeigt_feld}`.
6. **Relay:**
   - `AdImage.in_stream`, Übernahme im Upload-Arm (Logo `false`).
   - `ad_bar_state` bekommt `Query<ZweckQuery>` und `HeaderMap`.
   - Bei `zweck=stream`: zuerst `sperre(anzeige_lage(..))`, dann `keine_daten_antwort()`
     (503, `no-store`, `Retry-After`) bei `monitor == None` bzw. bei Host fehlt + leere
     Bildliste.
   - `streamAds` und `streamVer` (`AdImage.etag` ohne Anführungszeichen) immer mitliefern.
7. **UI:** `types.ts` `in_stream`, `api.ts` `setCourtAdStream`, Häkchen „Stream" im
   `SetupWizard` neben „Leiste".
8. **Doku und Version:** `cloud-relay.md`, `einstellungen.md`, Changelog, CLAUDE.md-Zeile,
   ADR-Status auf accepted.

**PR 2 — Seite, Routen, Baukasten, Handbuch:**

1. **`src/io/streamWerbung.mjs`** (ES5-Stil, damit die Kopie wortgleich sein kann) und
   `scripts/test-stream-werbung.mjs`, test-first, mit CI-Schritt in `ci.yml`.
2. **`assets/stream-werbung.html`:**
   - `var BASE = "__BASE__";`, Inline-Kopie zwischen Markern.
   - `abrufMitFrist` aus `ad.html`.
   - Kein `innerHTML`.
   - Drift- und Fristtest eintragen.
3. **LAN:** `STREAM_WERBUNG_HTML` in `tablet/assets.rs`, `stream_werbung_page` (`no-store`),
   Route `/stream/werbung` neben `/stream`.
4. **Relay:**
   - `include_str!`, Handler nach dem Muster `stream_page` (`valid_namespace`,
     `monitor_base`), Route `/{ns}/stream/werbung`.
   - **Das Asset in die `paths` von `.github/workflows/relay-deploy.yml` aufnehmen**, sonst
     wird `test-relay-deploy-pfade.mjs` rot.
5. **Baukasten:** `streamWerbungLink` in `src/io/streamLink.mjs` und der eigene Block
   `StreamWerbungBaukasten` in `CourtMonitorPanel.tsx` nach `StreamBaukasten`. Er braucht keine
   Felder; Segment-Knöpfe für Darstellung, Hintergrund und, nur beim Band, Tempo; Links für LAN
   und Cloud.
6. **Handbuch und Doku:** `court-monitor.md` mit Ankern, `oberflaechen.md`, Querverweis,
   Changelog.

**Fallen:**

- Den LAN-Zwischenstand nicht an `(mtime, len)` schlüsseln.
- Ein fehlendes `streamAds` heißt „behalten", nicht „leer".
- Das Band nur bei geänderter Liste neu starten.
- Bei einem Bild die Kopie wiederholen, nicht nur verdoppeln.
- `show_ads` nicht auswerten.
- Der Tauri-Parameter `in_stream` kommt als `inStream` aus dem Frontend.
