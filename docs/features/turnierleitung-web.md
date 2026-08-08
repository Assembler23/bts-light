# Turnierleitungs-Weboberfläche („TL-Web") — Spezifikation

> Status: **Entwurf 2026-08-07** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Feature-Wunsch der Turnierleitung vom 2026-08-07.
> Betroffene Crates: `src-tauri/`, `relay/`, `relay-proto/`, `src/`.
> ADR: [0010 — Schreibender Cloud-Pfad für Turnierleitungs-Geräte](../adr/0010-tl-web-schreibender-cloud-pfad.md) ·
> [0011 — Geräte-Identität für Turnierleitungs-Geräte](../adr/0011-tl-web-geraete-identitaet.md) ·
> ADR zur Ergebniskorrektur folgt nach dem BTP-Experiment (Schritt 12).

## Kontext / Problem

Die gesamte **schreibende** Turnierleitung ist heute an einen Rechner
gebunden: die Tauri-Desktop-App. Felderübersicht
(`src/pages/FieldOverviewPage.tsx`), Vorbereitungs-Aufrufe
(`src/pages/PreparationPanel.tsx`) und der 2./3. Aufruf (v0.9.174) laufen
dort über Tauri-Commands (**R1**). Im Browser existieren bisher nur die
**read-only** Info-Monitore `/info/overview` und `/info/preparation`; der
Cloud-Slave ist bewusst read-only (**R4/R5**).

Beim Zwei-Hallen-Turnier (17.–19.07.2026) war genau das der Engpass: Die
zweite Halle konnte nichts auslösen, und die Turnierleitung stand gebunden
am PC, während Helfer mit Tablets durch die Halle liefen. Mehrere
Roadmap-Punkte zielen bereits in diese Richtung — Cluster C (2./3. Aufruf
je Partei, Zeit seit Aufruf), Cluster E (Slave-Spielübersicht, Plan 7),
Cluster D (Backend-Finalisierung, Plan 12).

## Zielbild & Erfolgskriterien

**Zielbild:** Eine Weboberfläche, die mehrere Helfer gleichzeitig bedienen
— im Hallennetz und über das Internet. Spiele werden per Ziehen **oder**
Antippen auf Felder gelegt, umgehängt und heruntergenommen; Zeiten,
Spielstände und Aufrufe sind auf einen Blick sichtbar; Ansagen werden
ausgelöst, aber an genau einem Gerät je Halle gesprochen.

**Erfolgskriterien** (messbar am nächsten Mehr-Hallen-Turnier):

1. Die zweite Halle vergibt ihre Felder **selbst**, ohne Ruf an den
   Turnier-PC — nachweisbar an Feldzuweisungen, deren auslösendes
   TL-Gerät der fernen Halle zugeordnet ist.
2. Mindestens zwei Personen arbeiten über das ganze Turnier parallel,
   ohne dass eine Zuweisung stillschweigend überschrieben wird: **null**
   Fälle „Spiel stand plötzlich woanders" im Turnier-Log; jede abgelehnte
   Zuweisung ist als Konflikt protokolliert.
3. Ein Helfer, der die Seite zum ersten Mal öffnet, weist ohne Erklärung
   ein Spiel zu — geprüft an einer Person aus der Turnierleitung, die die
   Entwicklung nicht kennt.
4. Kein Rückschritt am erprobten Stand: die Regressions-Suite bleibt grün,
   und Turniere **ohne** TL-Web zeigen kein verändertes Verhalten.

## Nicht-Ziele

- **Kein Ersatz der Desktop-App.** Sie bleibt Herz und Rückfallebene:
  BTP-Verbindung, Setup-Assistent, Geräte-Kopplung, Diagnose.
- **Kein Setup und keine Einstellungen aus dem Web** — BTP-Verbindung,
  Verbindungsmodus, Passwörter, Gerätekopplung bleiben in der Desktop-App.
  Einzige Ausnahme: der Schalter für die automatische Feldvergabe.
- **Kein Zugriff auf Diagnose, Logs oder Log-Upload** aus dem Web.
- **Kein Start/Stopp des Liveticker-Pushs** aus dem Web.
- **Keine Schiedsrichter-Anzeige.** BTP führt zwar `Officials` und
  `Official1ID`/`Official2ID` (`docs/btp_protocol.md:159,165,188`), aber in
  **beiden** echten Turnier-Mitschnitten des Repos kommt `Official`
  **null**-mal vor. Wird erst gebaut, wenn an einem echten Turnier belegt
  ist, dass die Felder gepflegt werden.
- **Keine Geräte-/Akku-Übersicht** (Tablets, Monitore) in diesem Feature.
- Kein Check-In, kein Punktezählen (bleibt der Tablet-Spielzettel).
- **Keine Browser-Sprachausgabe.** Die Seite gibt keinen Ton von sich.

## Betroffene Komponenten / Architekturregeln / Daten

**Crates/Komponenten**

- `relay-proto/src/lib.rs` — neue Wire-Typen `TlAction`, `CourtExpectation`,
  `TlResponse`, `TlErrorCode`, `TlState`; neue Frames `HostFrame::TlAuth |
  TlState | TlAck` und `RelayFrame::TlCommand`.
- `relay/src/main.rs` — Token→Namespace-Map, Routen `/tl`, `/tl/lib/{file}`,
  `/tl/api/state`, `/tl/api/command`; 8-Geräte-Cap; Env-Not-Aus.
- `src-tauri/src/tablet/tl.rs` **(neu)** — `authorize`, `state`, `execute`;
  von LAN-Server **und** Relay-Client aufgerufen (Muster `process_result`).
- `src-tauri/src/tablet/assign.rs` **(neu)** — aus `sync.rs` extrahierte,
  reine Guard-Funktionen (`court_occupied_by`, `check_assign`, `check_free`).
- `src-tauri/src/tablet/server.rs` — LAN-Routen, `build_result_update` mit
  Überschreib-Option.
- `src-tauri/src/tablet/relay_client.rs` — `TlAuth`/`TlState` pushen
  (fingerabdruck-gesteuert), `TlCommand` behandeln.
- `src-tauri/src/tablet/state.rs` — `call_stages`, `pending_assign`,
  `announce_jobs` (Verallgemeinerung von `freetext`).
- `src-tauri/src/sync.rs` — ruft die extrahierten Guards auf; respektiert
  `pending_assign`.
- `src-tauri/src/btp/model.rs` — rohe `from1`/`from2` (Baumkante).
- `src-tauri/src/config.rs` — `TlWebConfig`.
- `src-tauri/assets/tl.html` **(neu)** — die Seite selbst.
- `src/io/callTimer.mjs`, `src/io/hallRule.mjs`, `src/io/tlDrag.mjs`
  **(neu)** — geteilte, framework-freie Logik; React nutzt dieselben Dateien.
- `src/pages/` — Geräteverwaltung (anlegen, QR, widerrufen), Umstellung des
  Desktops auf die serverseitigen Aufruf-Stufen.
- `.github/workflows/relay-deploy.yml` — neue Asset-Pfade in den Trigger.

**Architekturregeln**

- **R1 bleibt gültig.** Die React-Desktop-UI spricht den Kern weiterhin
  ausschließlich über Tauri-Commands. TL-Web ist **keine** React-Seite,
  sondern ein eigener Client am Tablet-Server/Relay — derselbe Weg, den
  `tablet.html` und `monitor.html` schon gehen.
- **R2 unangetastet.** TL-Web erfindet keine Court→Match-Zuordnung; jede
  Aktion geht über den Host nach BTP und wird von dort zurückgelesen.
- **R3 erfüllt.** Die Seite wird von **beiden** Pfaden ausgeliefert:
  eingebetteter Server (`0.0.0.0:8088`) und Cloud-Relay. Im reinen
  LAN-Modus funktioniert sie ohne Internet.
- **R4 wird erweitert, nicht gebrochen.** TL-Geräte sind eine **dritte
  Client-Klasse**: sie landen nie in `Namespace.tablets`, übernehmen nie
  eine Court-Session, senden nie `TabletMsg`. Neu in R4 aufzunehmen:
  *höchstens 8 tokenauthentisierte Turnierleitungs-Geräte je Namespace*.
- **R5 gilt unverändert.** Jede Mutation existiert **genau einmal** in
  `tablet/tl.rs` und wird dort validiert — LAN- und Cloud-Pfad teilen sie
  sich, exakt wie `process_result` heute.
- **R6 wird verschärft.** Die `install_id` verlässt den Master für TL-Web
  **nicht**. TL-Routen sind namespacefrei; der Relay schlägt den Namespace
  über das Gerätetoken nach.

**Konfiguration & Abwärtskompatibilität**

- Neu: `AppConfig.tl_web: TlWebConfig { enabled: bool (Default false),
  devices: Vec<TlDevice { id, token, label, created_at_ms, hall }> }`,
  `#[serde(default)]` auf Feld **und** Struct (Muster `CheckinConfig`).
- Bestehende `config.json` bleibt ohne Änderung lesbar; ein Migrationstest
  weist das nach.
- Tauri-`identifier` `de.badhub.btslight` und Updater-Pfad
  `download/bts-light/` bleiben **unangetastet**.

**Datenschutz**

- Kein Geburtsjahr — kommt in keiner View-Struct vor und darf nicht
  nachgerüstet werden.
- **Keine `member_id`/Lizenznummer im Browser.** Die Spieleridentität
  (`player_key`) bleibt am Host; TL-Web bekommt nur das *Ergebnis* der
  Verfügbarkeitsprüfung (`blocked_reason`, `ready_at_ms`,
  `blocking_players` als Namen).
- **Keine Nationalitäten.** Die existieren allein für die automatische
  Sprachwahl der Ansage — da TL-Web nicht spricht, entfallen sie.
- **Kein Akkustand, keine `serving_*`-Felder** (nicht im Scope bzw. reine
  Zählhilfe).
- Spielernamen und Zähltafelbediener-Namen laufen über eine aus dem
  Internet erreichbare Seite. Gedeckt, weil der Zugang **tokengeschützt**
  ist, die Zahl der Geräte begrenzt und vom Master ausgestellt. Der Relay
  hält sie nur im RAM, kappt sie, persistiert sie nicht und **loggt sie
  nie** — dieselbe Regel wie beim Azure-Key.
- Ein **Datensparsamkeits-Test** prüft das serialisierte `TlState` gegen
  `member_id`, Geburtsjahr und Nationalitäten und schlägt fehl, wenn jemand
  ein solches Feld nachrüstet.

**Abhängigkeiten**

- BTP-Protokoll: rohe `From1`/`From2` müssen zusätzlich geparst werden.
- Relay hinter nginx: **keine nginx-Änderung nötig** (`/bts-relay/` wird
  pauschal geproxyt).
- **Keine neue Cargo- oder npm-Abhängigkeit.** Das Ziehen wird auf
  Pointer-Events selbst gebaut (~120–180 Zeilen); `dnd-kit` ist React-only
  und damit unbrauchbar, `SortableJS` müsste ohne Bundler vendored werden
  (Lizenzpflege ohne `npm audit`) und ist für Listen-Sortierung gebaut.
  Zufall für Tokens kommt aus `crypto.randomUUID()` — derselbe Weg wie bei
  der `install_id`.

## Akzeptanzkriterien

**Zugang und Geräte**

- [ ] In bts-light lässt sich ein TL-Gerät mit Namen anlegen; es erscheint
      ein QR-Code, dessen Scan die Seite auf dem Gerät geöffnet und
      angemeldet zeigt — ohne Eingabe eines Codes.
- [ ] Die `install_id` ist an keiner Stelle des TL-Pfads sichtbar: nicht in
      der URL, nicht im Seitenquelltext, nicht in einer API-Antwort.
- [ ] Das Token steht im URL-Fragment und taucht **nicht** im
      nginx-Access-Log auf.
- [ ] Wird ein Gerät in bts-light entfernt, ist es spätestens **2 s**
      später gesperrt und zeigt „Zugang wurde entzogen" — ohne Neustart von
      App oder Relay, im LAN- **und** im Cloud-Modus.
- [ ] Ein **neuntes** Gerät wird mit einer verständlichen Meldung
      abgewiesen; ein Gerät, das länger als 60 s weg war, gibt seinen Platz
      frei und ein neues kommt hinein.
- [ ] Ein Token eines Turniers erreicht **niemals** den Host eines anderen
      Namespace.
- [ ] Nach einem Relay-Neustart melden sich alle Geräte binnen Sekunden
      selbst wieder an; bis dahin werden Anfragen abgelehnt, nicht
      durchgelassen.
- [ ] Ein Identitäts-Export enthält **keine** TL-Tokens.
- [ ] Bei `tl_web.enabled = false` (Default) ist die Seite nicht
      erreichbar, und der Relay kennt kein einziges Token dieses Turniers.
- [ ] Ist die Oberfläche **eingeschaltet, aber kein Gerät gekoppelt**, sagt
      die App das ausdrücklich („eingeschaltet, kein Gerät gekoppelt —
      Gerät hinzufügen"). Dieser Zustand entsteht regulär nach einem
      Identitäts-Umzug und sähe sonst wie ein funktionierendes Setup aus,
      während jede Anfrage abgewiesen wird.
- [ ] Ein Gerät koppeln und danach in den Einstellungen etwas anderes
      speichern lässt das gekoppelte Gerät **verbunden** (die
      Einstellungsseite darf ihren beim Öffnen aufgenommenen Stand der
      Geräteliste nicht zurückschreiben).
- [ ] Ein Identitäts-Import lässt die am **neuen** PC bereits gekoppelten
      Geräte unangetastet.

**Ansicht**

- [ ] Die Seite zeigt Felder, Spielliste, Zeiten, Live-Spielstände,
      Zähltafelbediener je Feld, beendete Spiele und Walkover-Vorschläge in
      **einer** Ansicht.
- [ ] Die Spielliste ist in drei Abschnitte gegliedert — *In Vorbereitung
      gerufen* (mit „seit X min"), *Spielbereit*, *Noch nicht bereit* — und
      innerhalb jedes Abschnitts exakt so sortiert wie die automatische
      Feldvergabe.
- [ ] Ein nicht vergebbares Spiel bleibt sichtbar und nennt den Grund im
      Klartext samt Namen der blockierenden Spieler und der Uhrzeit, ab der
      es frei ist.
- [ ] „Seit wann aufgerufen", „Feld frei seit", Pausen- und
      Vorbereitungs-Uhren zählen im Sekundentakt hoch und zeigen auf **allen**
      Geräten dieselbe verstrichene Zeit, auch wenn die Gerätezeit falsch
      geht.
- [ ] Bei einem Mehr-Hallen-Turnier filtert **ein** Filter im Kopf Felder,
      Spielliste, Bediener-Warteschlange und beendete Spiele gemeinsam.
      Spiele ohne Hallenzuordnung erscheinen in einem eigenen, immer
      sichtbaren Abschnitt mit Anzahl — sie werden nie stillschweigend
      ausgeblendet.
- [ ] Bei einem Ein-Hallen-Turnier wird kein Hallenfilter angeboten.
- [ ] Der Filter überlebt einen Reload und lässt sich per Link teilen.

**Bedienung**

- [ ] Ein Spiel lässt sich per **Antippen, dann Feld antippen** zuweisen —
      auf iPad/Safari, Android/Chrome und Desktop.
- [ ] Dasselbe geht per **Ziehen**, auf denselben Geräten.
- [ ] Bricht ein Ziehvorgang ab, bleibt das Spiel ausgewählt und lässt sich
      durch Antippen des Feldes zuweisen — ohne dass der Nutzer etwas
      zurücksetzen muss.
- [ ] Solange ein Spiel ausgewählt ist, zeigt ein festes Band oben, welches
      Spiel gewählt ist, und bietet Abbrechen; erlaubte Felder sind
      hervorgehoben, nicht erlaubte gedimmt — ein Tipp darauf **nennt den
      Grund**, statt ins Leere zu gehen.
- [ ] Ein Spiel lässt sich von Feld A auf Feld B umhängen; das geht als
      **ein** BTP-Schreibvorgang, es gibt keinen Zwischenzustand ohne Feld.
- [ ] Ein Spiel lässt sich vom Feld herunternehmen; geht dabei ein
      laufender Spielstand verloren, kommt vorher eine Rückfrage.
- [ ] Vorbereitungs-Aufruf setzen und zurücknehmen, zweiter und dritter
      Aufruf, Ergebnis nachtragen, Aufgabe/kampflos werten und die
      Zähltafelbediener-Warteschlange pflegen (vorziehen, entfernen,
      ergänzen) funktionieren aus der Seite heraus.
- [ ] Die automatische Feldvergabe ist als Zustand sichtbar und lässt sich
      umschalten; nach einer Zuweisung von Hand pausiert sie sichtbar für
      60 s, damit sie dem Helfer nicht dazwischenfunkt.
- [ ] Die Pause gilt **nur zur Laufzeit** und endet beim Stoppen/Starten der
      Übertragung. Sonst bliebe sie hängen, sobald das Gerät weg ist, das sie
      gesetzt hat — die Einstellungen sagten „an", vergeben würde trotzdem
      nichts, und es gäbe keinen Griff dagegen.

**Aufrufe und Ansagen**

- [ ] Die Aufruf-Stufe (1./2./3.) ist auf **allen** Geräten gleich —
      inklusive der Desktop-App; sie zählt am Host, nicht im Browser.
- [ ] Eine aus TL-Web ausgelöste Ansage wird **genau einmal** und auf
      **genau einem** Gerät je Halle gesprochen — mit demselben Text, Gong
      und derselben Stimme wie eine Ansage aus der Desktop-App, inklusive
      „Tabletbedienung: …", wenn ein Bediener zugewiesen ist.
- [ ] Ist in der Zielhalle kein Ansage-Gerät verbunden, meldet die Seite
      das im Klartext, die Aktion gilt trotzdem als ausgeführt, und die
      Aufruf-Stufe zählt hoch.
- [ ] Eine Ansage, die länger als 60 s nicht abgespielt werden konnte,
      verfällt und wird nicht nachträglich gesprochen.

**Ergebnis und Korrektur**

- [ ] Ein noch offenes Spiel lässt sich mit einem Endstand nachtragen.
- [ ] Ein bereits gewertetes Spiel lässt sich überschreiben, **solange der
      Sieger in kein Folgespiel wirkt**; sonst wird es mit einer
      verständlichen Begründung abgelehnt.
- [ ] In einer Gruppen-Auslosung wird eine Korrektur **nicht**
      fälschlicherweise blockiert.
- [ ] Nach einer Korrektur überschreibt kein nachgereichter Schreibvorgang
      aus der Retry-Queue das frische Ergebnis.

**Konflikte und Fehlerfälle**

- [ ] Weisen zwei Geräte gleichzeitig dasselbe Feld zu, gewinnt genau
      eines; das andere bekommt „Feld wurde gerade von jemand anderem
      belegt" **mit dem Namen des Spiels, das dort steht**, und die Ansicht
      springt auf den echten Stand. Beide Ansichten sind danach gleich.
- [ ] Eine Aktion, die auf einer über 60 s alten Ansicht beruht, wird
      abgelehnt statt ausgeführt.
- [ ] Ein doppelter Tipp bei langsamer Verbindung erzeugt **genau eine**
      Zuweisung in BTP.
- [ ] Bricht die Verbindung zum Relay ab, zeigt die Seite binnen 15 s ein
      rotes Band und **deaktiviert alle Schreibknöpfe**.
- [ ] Ist der Relay erreichbar, aber bts-light nicht, zeigt die Seite
      ausdrücklich „bts-light ist nicht verbunden" samt Alter der Daten —
      **nicht** „alle Felder frei".
- [ ] Eine Aktion, die während eines Ausfalls ausgelöst wurde, wird
      **nicht** nachgereicht; die Seite meldet „nicht bestätigt" und
      aktualisiert, sobald sie wieder kann.
- [ ] Nach 10 Minuten Standby zeigt die Seite beim Aufwecken sofort einen
      frischen Stand und keine falsch weitergelaufene Uhr.
- [ ] Ein BTP-Neustart erzeugt keine Geisterzuweisungen; die Liste erholt
      sich von selbst.
- [ ] Bestätigen zwei Geräte denselben Walkover-Vorschlag, bekommt das
      zweite „bereits verarbeitet" statt einer zweiten Wertung.
- [ ] Ein Turnier **ohne** TL-Web verhält sich unverändert, auch mit
      aktualisiertem Relay.

**Nachvollziehbarkeit**

- [ ] Jede ausgeführte Aktion wird im App-Log mit **Gerätename und Aktion**
      festgehalten — damit nach einem Turnier nachvollziehbar ist, wer was
      ausgelöst hat, und die Erfolgskriterien überhaupt prüfbar sind.
- [ ] Jede **abgelehnte** Aktion wird mit ihrem Grund protokolliert;
      Konflikte („Feld war schon belegt") sind im Log als solche zählbar.
- [ ] Im Log erscheint **kein** Gerätetoken — weder ganz noch teilweise.

## Tests

**Rust (`cargo test --workspace`, Pflicht-Check)**

- `relay-proto`: Serde-Roundtrip je `TlAction`-Variante; fehlendes
  `expect`-Feld ergibt `CourtExpectation::Any`; Roundtrips für `TlCommand`,
  `TlAck`, `TlAuth`, `TlState`, `TlResponse` inkl. Fehlercodes.
- `relay`: Token löst zum eigenen Namespace auf · unbekanntes Token
  abgewiesen · `TlAuth`-Push **ersetzt** die Token-Menge (Widerruf) ·
  Tokens fallen beim Host-Disconnect weg · Kommando erreicht den Host und
  der Ack löst die wartende Anfrage · ohne Host definierter Fehler ·
  sauberer Timeout · neuntes Gerät abgewiesen · veralteter Geräteplatz nach
  60 s frei · **Token eines Namespace erreicht nie einen fremden Host** ·
  `TlState` gespeichert, gekappt, ausgeliefert · ETag stabil bei
  unverändertem Stand · unbekannte Frame-Variante killt die Verbindung
  nicht · Routen fehlen bei gesetztem Not-Aus.
- `config.rs`: alte Config ohne `tl_web` lädt mit Default **aus** ·
  Speichern/Laden-Roundtrip · **Identitäts-Export enthält keine Tokens**.
- `tablet/assign.rs`: belegtes/gesperrtes Feld · Match steht schon woanders
  · jede `CourtExpectation`-Kombination · `Any` verhält sich exakt wie der
  heutige Desktop-Pfad · Reservierung blockt das zweite Gerät und läuft nach
  TTL ab.
- `sync.rs`: **alle 16 bestehenden `auto_assign_*`-Tests bleiben
  unverändert grün** (Beleg, dass die Extraktion nichts verändert) · neu:
  automatische Vergabe überspringt ein von Hand reserviertes Feld.
- `tablet/tl.rs`: `authorize` akzeptiert/verweigert/widerruft · lehnt alles
  im Slave-Modus ab · lehnt alles bei ausgeschaltetem Flag ab · Revision
  ändert sich nur bei echter Änderung · Ansage-Auftrag ist hallen-gescoped,
  verfällt nach 60 s, gekappt · Warnung ohne Ansage-Gerät, Aktion gilt
  trotzdem · Aufruf für ein Spiel, das nicht auf dem Feld steht, abgelehnt ·
  Nachruf ohne Vorbereitungs-Aufruf abgelehnt · die Aufruf-Stufe bleibt nie
  hinter der Uhr zurück · Aufruf-Stufe zählt je
  (Feld, Match) und wird beim Match-Wechsel zurückgesetzt ·
  **Datensparsamkeits-Test** · Sortierung identisch zur automatischen
  Vergabe · `blocked_reason`/`ready_at_ms` inkl. BTP-Setting 1303 und
  Config-Override · Hallen-Kaskade · Wiederholung derselben `opId` schreibt
  nicht doppelt · **eine Korrektur mit derselben `opId` gilt nicht als
  Wiederholung** (der Fingerabdruck trägt die ganze Nutzlast, und er hat
  keinen Sammelzweig — eine neue Aktion bricht den Übersetzer, statt lautlos
  in der Idempotenz zu landen) · ein Walkover ohne schreibbaren Kandidaten
  wird abgelehnt und der Vorschlag bleibt stehen · den Vorschlag bekommt nur
  **ein** Gerät, und er kommt zurück, wenn gar nichts geschrieben wurde.
- `server.rs`/`btp`: Korrektur blockiert, wenn das Folgespiel läuft oder
  gewertet ist · erlaubt ohne Folgespiel · **erlaubt in der
  Gruppen-Auslosung** · Überschreiben verlangt ein ausdrückliches Flag ·
  Parser behält rohe `from1`/`from2` · Nachfolger-Suche findet das
  Folgespiel · Capture-Regression gegen beide Fixtures.

**Node (`.mjs`, bestehendes CI-Muster)**

- `tlDrag.mjs`: Tipp→Tipp weist zu · zweiter Tipp hebt die Auswahl auf ·
  Abbruch lässt die Auswahl stehen · Bewegung unter 8 px zählt als Tipp ·
  im Wartezustand ignoriert das betroffene Feld weitere Tipps.
- `callTimer.mjs`: Stufen an den konfigurierten Schwellen · negative
  Differenz ergibt 0 · Formatierung über 60 Minuten.
- `hallRule.mjs`: identische Ergebnisse wie der Rust-Test — **derselbe
  Testdatensatz in beiden Sprachen**.
- `gamePoint.mjs`: bestehende Tests bleiben grün.

**Kein Vitest/DOM-Harness in diesem Feature.** Vier `.mjs`-Module decken
die risikoreiche Logik ohne neue Abhängigkeit ab; ein zweiter Test-Stack
wäre ein eigenes Projekt. Vitest bleibt Roadmap-Punkt — TL-Web wäre der
erste Nutznießer.

**Manuelle Abnahme (Teil der Definition-of-Done).** Ungetestet bleiben
DOM-Rendering, Layout auf echten Tablets, echte Touch-Gesten in Safari und
Chrome, `elementFromPoint` unter Zoom, Verhalten nach Standby und bei
echtem Paketverlust. Checkliste: iPad Safari · Android Chrome · Desktop, je
einmal Tipp-Tipp **und** Ziehen · zwei Geräte auf dasselbe Feld · WLAN
aus/an · bts-light beenden · 10 min Standby · BTP-Neustart · zwei Geräte
auf denselben Walkover · Doppeltipp bei gedrosseltem Netz · **ein Spiel vom
Feld wählen und es dort zu Ende gehen lassen** (die Auswahl muss verfallen,
statt „Ergebnis eintragen" auf die inzwischen aufgerufene Begegnung zu
richten) · **einen Walkover-Kandidaten abwählen und eine Abfrage abwarten**
(das Kästchen darf sich nicht selbst wieder anhaken) · Ansage genau
einmal je Halle · Auto-Vergabe räumt eine Handzuweisung nicht um ·
Zwei-Hallen-Filter inklusive Gruppe „ohne Hallenzuordnung".
`npm run build` fehlerfrei.

## Risiken & Rollback

| Risiko | Wirkung | Gegenmaßnahme |
|---|---|---|
| BTP-Verhalten beim Überschreiben einer Wertung unbekannt | Korrektur zerschießt den Turnierbaum | Erst nach dem BTP-Experiment freischalten; bis dahin konservativ blockieren |
| Der Relay-Deploy ist **global** (ein Binary für alle Installationen) | Ein Fehler im neuen Schreibpfad träfe Turniere ohne TL-Web | Ohne Host-Opt-in kennt der Relay kein Token → jede Anfrage endet abgewiesen, **bevor** neuer Code Zustand berührt; zusätzlich Not-Aus per Umgebungsvariable ohne Rebuild; Kompatibilitätstests in beide Richtungen; **Relay vor Client ausrollen** |
| Token-Diebstahl über eingeschleusten Seitencode | Fremdzugriff auf den Schreibkanal | Keine Fremd-Skripte, CSP, Namen ausschließlich als Text eingesetzt; Token nur im Fragment und lokalen Speicher |
| Zwei Geräte im selben Zeitfenster | Doppelvergabe trotz Prüfung | Geteilte Reservierung mit Verfallszeit, gelesen auch von der automatischen Vergabe |
| Aufruf-Stufen divergieren zwischen Desktop und TL-Web | „Zweiter Aufruf" doppelt oder übersprungen | Stufen wandern an den Host, Desktop wird **im selben Schritt** umgestellt |
| Gerätegrenze sperrt echte Helfer aus | Turnierleitung ausgesperrt | Veraltete Plätze **vor** der Grenzprüfung räumen; klare Meldung statt stiller Verdrängung |
| Identitäts-Umzug nimmt Tokens mit | Der alte PC bliebe schreibberechtigt | Beim Export entfernen, durch Test abgesichert |
| „Alles in einem Zug" | Ein unreviewbarer Riesen-PR | 15 einzeln prüfbare Schritte; Schritte 1–9 laufen **ohne** Relay-Deploy |

**Rollback:** Die Änderung ist zurückrollbar. Config bleibt lesbar (neue
Felder haben Defaults), eine ältere App-Version liest sie unverändert. Der
Relay lässt sich auf das vorherige Binary zurücksetzen; der Not-Aus per
Umgebungsvariable wirkt sofort ohne Rebuild. Da TL-Web ohne ausdrückliche
Aktivierung unerreichbar ist, ist „aus" jederzeit ein gültiger Zustand.

## Offene Fragen / Annahmen

1. **Ergebniskorrektur, Fall „Nachfolger existiert, Sieger dort eingesetzt,
   aber noch nicht gestartet".** Setzt BTP den Sieger sofort beim Werten in
   den nächsten Slot, wäre eine strenge Auslegung praktisch „nie
   korrigierbar außer im Finale und in Gruppen". Ebenso offen: ob BTP den
   Baum bei einem Überschreiben neu rechnet. **Vorgehen:** Experiment an
   einem Test-BTP in Schritt 12, Ergebnis als Fixture einfrieren, danach
   eigenes ADR. **Bis dahin blockiert dieser Fall.**
2. **Halle am noch nicht gerufenen Spiel.** Die Kaskade startet mit der
   Disziplin/Klasse-Regel und dem Aufruf; die beste Quelle wäre der von BTP
   an der Ansetzung geführte Spielort (Roadmap-Plan 2). Ob die vorhandenen
   Mitschnitte ihn enthalten, ist **nicht** verifiziert — deshalb
   nachgelagert (Schritt 15), analog zum Vorgehen bei den Schiedsrichtern.
3. **Annahme:** Ein Gerät je Halle mit eingeschalteter Ansage genügt als
   „Ansage-Gerät". Das entspricht dem heutigen Betriebsmodell (ein Master,
   ein Slave je Halle). Wird in einer Halle künftig mehr als ein
   sprechendes Gerät betrieben, braucht es eine ausdrückliche Auswahl.
4. **Annahme:** 8 gleichzeitige Geräte reichen. Die Grenze ist eine
   Konstante und ohne Protokolländerung anhebbar.
5. **Annahme:** Die Karenz von 60 s, in der die automatische Vergabe nach
   einer Handzuweisung pausiert, ist im Betrieb angenehm. Falls sie stört,
   ist sie ein Zahlenwert, kein Umbau.
6. **Zu prüfen bei der Umsetzung:** mögliche Routen-Kollision zwischen
   `/tl/api/*` und den bestehenden Namespace-Routen im Relay; Ausweichweg
   ist ein eigenes Präfix.
7. **Unbekannte Aktion an einem älteren Host** (aus dem Review zu Schritt 1,
   bewusst offen gelassen): Schickt ein neueres Gerät eine Aktion, die
   dieser Host noch nicht kennt, scheitert schon das Zerlegen des ganzen
   Frames — es wird still verworfen, und der Absender bekommt **weder
   Erfolg noch Absage**, hängt also bis zum Zeitablauf. Der Host antwortet
   auf bekannte Frames bereits mit einer Absage (`Unsupported`); für den
   unbekannten Fall muss der Schritt, der den Kanal baut, die
   Vorgangsnummer **auch aus einem nicht zerlegbaren Frame** ziehen und
   beantworten. Gehört zur Fehlerbehandlung des Kanals, nicht zu den
   Wire-Typen.

## Betroffene Doku-Dateien

Im selben Commit zu pflegen (Tabelle in `CLAUDE.md`):

- **`docs/features/turnierleitung-web.md`** (diese Spec) — bleibt die
  fachliche Referenz; die Betriebsdoku entsteht als **eigene**
  `docs/turnierleitung-web.md`.
- `docs/cloud-relay.md` — Gerätetoken, TL-Routen, Rollen, Gerätegrenze.
- `docs/tablet.md` — geteilte Routen des eingebetteten Servers.
- `docs/preparation.md` — Vorbereitungs-Aufruf aus TL-Web.
- `docs/walkover.md` — Bestätigung aus TL-Web.
- `docs/zaehltafelbediener.md` — Anzeige und Warteschlangen-Pflege.
- `docs/announcements.md` — Ansage-Aufträge, ein Ansage-Gerät je Halle,
  serverseitige Aufruf-Stufen.
- `docs/btp_protocol.md` — rohe `From1`/`From2`, Überschreiben einer
  Wertung.
- `docs/multi-hall.md` — TL-Geräte als dritte Client-Klasse, Hallen-Kaskade.
- `docs/regression-suite.md` — neue Kernpfade.
- `docs/roadmap.md` — erledigte Punkte streichen (Cluster C/E-Überschneidungen).
- `docs/changelog.md` — je veröffentlichter Version.
- `CLAUDE.md` — R4 um die dritte Client-Klasse ergänzen; Doku-Tabelle um
  TL-Web erweitern.

## Umsetzungs-Hinweise

*Erst nach Freigabe relevant.* Vollständiger Plan mit Belegen:
`docs/features/_intake/turnierleitung-web/3-how-to.md`.

**Kernentscheidungen:** Vanilla-Seite `tl.html` (wie `tablet.html`) mit
geteilten `.mjs`-Logikmodulen · host-ausgestellte, widerrufbare
Gerätetokens, vom Relay nur gespiegelt · Whitelist fixierter Aktionen, der
Host validiert (R5) · Antwort synchron über das erprobte
`req_id`/`oneshot`-Muster der Ergebnismeldung · Polling mit Revision/ETag
statt Push-Fanout · Guards aus `auto_assign` in reine Funktionen extrahiert
· Ansagen als strukturierte Aufträge über den vorhandenen Kanal, nie als
Freitext.

**Reihenfolge** (Schritte 1–9 ohne Relay-Deploy, damit TL-Web im Hallennetz
läuft, bevor ein globales Binary angefasst wird):
Wire-Typen → Config → Guards extrahieren → Hallen-Kaskade und Lesemodell →
`tl.rs` und LAN-Routen → Seite lesend → Tipp-Tipp mit Konfliktprüfung, dann
Ziehen → restliche Aktionen → Ansage-Aufträge und Aufruf-Stufen → Relay →
Cloud-Seite am Host → BTP-Experiment und Korrektur → Geräteverwaltung im
Desktop → Doku und ADRs → nachgelagert der geplante Spielort.

**Reviews:** `code-reviewer` nach **jeder** Code-Änderung (Pflicht).
`security-reviewer` verbindlich für die Schritte zu Gerätetoken,
Relay-Routen, Schreibkanal und Seiten-Auslieferung — das Feature bringt
neuen Nutzereingaben-Pfad **und** Authentifizierung.

**Version** gemeinsam bumpen in `src-tauri/Cargo.toml`,
`src-tauri/tauri.conf.json` und `package.json`.

**Nicht vergessen:** `.github/workflows/relay-deploy.yml` um die neuen
Asset-Pfade erweitern — sonst wird die Seite nie ausgeliefert.
