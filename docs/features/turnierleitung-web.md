# Turnierleitungs-Weboberfläche („TL-Web") — Spezifikation

> Status: **Entwurf 2026-08-07** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Feature-Wunsch der Turnierleitung vom 2026-08-07.
> Betroffene Crates: `src-tauri/`, `relay/`, `relay-proto/`, `src/`.
> ADR: [0011 — Schreibender Cloud-Pfad für Turnierleitungs-Geräte](../adr/0011-tl-web-schreibender-cloud-pfad.md) ·
> [0012 — Geräte-Identität für Turnierleitungs-Geräte](../adr/0012-tl-web-geraete-identitaet.md) ·
> ADR zur Ergebniskorrektur folgt nach dem BTP-Experiment (Schritt 12).
> Verwandt: [feldvergabe-ausnahme.md](feldvergabe-ausnahme.md) (eigene Spec)
> ergänzt die Warteliste um einen Pause-Umschalter je Spiel, der es von der
> automatischen Feldvergabe ausnimmt — nutzt dieselbe `TlAction`-Infrastruktur.
> **Abgelöst in Teilen:** [tl-web-panelsystem.md](tl-web-panelsystem.md)
> (2026-08-15) ersetzt die hier beschriebenen, geräte-lokal in
> `localStorage` gehaltenen Anzeige-Einstellungen („Anzeige"-Klappmenü)
> durch benannte, server-seitige **Profile** und macht die neun Abschnitte
> der Seite zu einzeln ein-/ausblendbaren, umsortierbaren **Panels**.
> Akzeptanzkriterien dieser Spec, die sich auf das alte Anzeige-Menü oder
> auf `localStorage`-Persistenz der Anzeige-Schalter beziehen, gelten
> dadurch als abgelöst.

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
  Zwei benannte Ausnahmen: der Schalter für die automatische Feldvergabe,
  und (seit 2026-08-15) die **Panel-Profil-Verwaltung** — Anlegen,
  Bearbeiten, Löschen und Wählen der reinen Darstellungs-Profile läuft
  direkt in `tl.html`. Begründung und Abgrenzung:
  [ADR 0024](../adr/0024-tl-panel-profile-verwaltung-im-web.md) — Profile
  sind sicherheitsneutrale Anzeige-Präferenzen, anders als Zugänge und
  Verbindungen.
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
- `relay/src/main.rs` — Token→Namespace-Map, Routen `/tl`,
  `/tl/api/state`, `/tl/api/command`; 8-Geräte-Cap; Env-Not-Aus.
  *(Umgesetzt in Schritt 10. `/tl/lib/{file}` entfällt: Die Seite ist eine
  einzige Datei ohne nachgeladene Bausteine — ein Pfad weniger, der aus dem
  Internet erreichbar ist.)*
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
- ~~**Keine `member_id`/Lizenznummer im Browser.**~~ **Revidiert am
  17.08.2026, ausgeweitet am 18.08.2026** (Nutzer-Entscheidung, wie zuvor
  Nation 09.08. und Verein 12.08.): Wartelisten-Einträge, **laufende
  Feld-Kacheln und beendete Spiele** tragen die Lizenznummer
  (`team1_ids`/`team2_ids`) als Link-Ziel der badhub-Spielerseite
  (`/spieler/<Nr>/live` — die Nummer ist der **öffentliche** URL-Schlüssel
  genau dieser Seite); die Warteliste zusätzlich `blocked.player_keys`
  (Lizenznummer bzw. normalisierter Name) für die punktgenaue
  Namens-Färbung. Aufgeschriebener Zweck an allen drei Stellen:
  *Nachschlagen der Spielerhistorie auf der öffentlichen badhub-Seite
  während des Turniers* (Spec `tl-sicht-feinschliff`, Punkt 4). Die
  ursprüngliche Beschränkung auf die Warteliste war keine
  Datenschutz-Grenze, sondern schlicht die Stelle, an der zuerst verlinkt
  wurde. Spieler ohne Lizenznummer bleiben unverlinkt.
- ~~**Keine Nationalitäten.**~~ Revidiert am 09.08.2026 — als
  zuschaltbares ISO-Kürzel neben dem Namen (Standard: aus).
- **Kein Akkustand, keine `serving_*`-Felder** (nicht im Scope bzw. reine
  Zählhilfe).
- Spielernamen und Zähltafelbediener-Namen laufen über eine aus dem
  Internet erreichbare Seite. Gedeckt, weil der Zugang **tokengeschützt**
  ist, die Zahl der Geräte begrenzt und vom Master ausgestellt. Der Relay
  hält sie nur im RAM, kappt sie, persistiert sie nicht und **loggt sie
  nie** — dieselbe Regel wie beim Azure-Key.
- Ein **Datensparsamkeits-Test** prüft das serialisierte `TlState`
  (`the_state_never_carries_personal_data_beyond_its_purpose`): Das
  Geburtsjahr bleibt überall draußen, ebenso Check-In-Spielernamen sowie
  Sperrlisten und Stammverein der Schiedsrichter. Die Lizenznummern sind
  seit 18.08.2026 an allen drei Stellen **positiv** geprüft — fiele eine
  weg, wäre der Link dort tot, ohne dass es jemand merkte.
  **Achtung beim Nachrüsten:** Der zweite Wächter
  (`every_published_field_is_deliberately_allowed`) führt eine **flache**
  Feldnamen-Liste und schlägt deshalb nicht an, wenn ein bereits erlaubter
  Feldname in einer **weiteren** Struktur auftaucht — die Ausweitung vom
  18.08.2026 fing allein der Datensparsamkeits-Test.

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

> **Stand nach Schritt 13** (09.08.2026). Drei Zustände statt Häkchen, weil
> „umgesetzt" und „nachgewiesen" nicht dasselbe sind:
>
> - **[x]** durch einen Test oder eine Messung belegt
> - **[~]** umgesetzt und im Code nachvollziehbar, aber **nicht** am echten
>   Gerät bzw. im echten Turnier nachgewiesen
> - **[ ]** offen — mit Begründung dahinter
>
> Was offen ist, steht am Ende dieses Abschnitts noch einmal gesammelt.

**Zugang und Geräte**

- [~] In bts-light lässt sich ein TL-Gerät mit Namen anlegen; es erscheint
      ein QR-Code, dessen Scan die Seite auf dem Gerät geöffnet und
      angemeldet zeigt — ohne Eingabe eines Codes.
- [~] Die `install_id` ist an keiner Stelle des TL-Pfads sichtbar: nicht in
      der URL, nicht im Seitenquelltext, nicht in einer API-Antwort.
- [~] Das Token steht im URL-Fragment und taucht **nicht** im
      nginx-Access-Log auf.
- [~] Wird ein Gerät in bts-light entfernt, ist es spätestens **2 s**
      später gesperrt und zeigt „Zugang wurde entzogen" — ohne Neustart von
      App oder Relay, im LAN- **und** im Cloud-Modus.
- [x] Ein **neuntes** Gerät wird mit einer verständlichen Meldung
      abgewiesen; ein Gerät, das länger als 60 s weg war, gibt seinen Platz
      frei und ein neues kommt hinein.
- [x] Ein Token eines Turniers erreicht **niemals** den Host eines anderen
      Namespace.
- [~] Nach einem Relay-Neustart melden sich alle Geräte binnen Sekunden
      selbst wieder an; bis dahin werden Anfragen abgelehnt, nicht
      durchgelassen.
- [x] Ein Identitäts-Export enthält **keine** TL-Tokens.
- [ ] Bei `tl_web.enabled = false` (Default) ist die Seite nicht
      erreichbar, und der Relay kennt kein einziges Token dieses Turniers.
      → **Zweite Hälfte belegt, erste bewusst anders gelöst:** Die *Seite*
      wird immer ausgeliefert, genau wie `tablet.html` — sie ist eine leere
      Hülle, die ihren Zugang erst aus dem Adress-Fragment liest und ohne
      ihn nichts zeigt. Geschützt sind die Daten-Routen. Eine Sperre für die
      Hülle brächte nichts und machte das Koppeln unmöglich (man ruft die
      Adresse ja auf, *bevor* der Zugang gilt).
- [ ] Ist die Oberfläche **eingeschaltet, aber kein Gerät gekoppelt**, sagt
      die App das ausdrücklich („eingeschaltet, kein Gerät gekoppelt —
      Gerät hinzufügen"). Dieser Zustand entsteht regulär nach einem
      Identitäts-Umzug und sähe sonst wie ein funktionierendes Setup aus,
      während jede Anfrage abgewiesen wird.
      → **Offen.** Die Geräteverwaltung zeigt beide Angaben getrennt
      (Schalter „freigeschaltet" und „Noch kein Gerät gekoppelt"), nennt
      diesen Zustand aber nicht als das Problem, das er ist.
- [x] Ein Gerät koppeln und danach in den Einstellungen etwas anderes
      speichern lässt das gekoppelte Gerät **verbunden** (die
      Einstellungsseite darf ihren beim Öffnen aufgenommenen Stand der
      Geräteliste nicht zurückschreiben).
- [x] Ein Identitäts-Import lässt die am **neuen** PC bereits gekoppelten
      Geräte unangetastet.

**Ansicht**

- [~] Die Seite zeigt Felder, Spielliste, Zeiten, Live-Spielstände,
      Zähltafelbediener je Feld, beendete Spiele und Walkover-Vorschläge in
      **einer** Ansicht.
      → **Erfüllt (2026-08-10).** Der zugeklappte Abschnitt „Beendet"
      (`tl.html`) zeigt die zuletzt beendeten Spiele, neueste zuerst, mit
      Aufgabe-/kampflos-/disqualifiziert-Kennzeichnung. Am echten Gerät
      noch nicht nachgewiesen — deshalb `[~]` statt `[x]`.
- [x] Die Spielliste ist in drei Abschnitte gegliedert — *In Vorbereitung
      gerufen* (mit „seit X min"), *Spielbereit*, *Noch nicht bereit* — und
      innerhalb jedes Abschnitts exakt so sortiert wie die automatische
      Feldvergabe.
- [x] Die **Felder bleiben stehen**, während die Liste scrollt, und jedes
      Spiel nennt seine Klasse als `HE-C`/`HD-D`. *Beides kam aus der
      Bedienung am echten Turnier (09.08.): Bei 120 wartenden Spielen war
      nach zwei Wischern kein Feld mehr zu sehen — ein Spiel von unten ließ
      sich weder ziehen noch ablegen. Und „Gruppe 6" verrät nicht, worum es
      geht. Dabei fiel auf, dass auch das Auswahlband hinter der Kopfzeile
      verschwand: Beide klebten auf derselben Höhe.*
- [x] Ein nicht vergebbares Spiel bleibt sichtbar und nennt den Grund im
      Klartext samt Namen der blockierenden Spieler und der Uhrzeit, ab der
      es frei ist.
- [x] „Seit wann aufgerufen", „Feld frei seit", Pausen- und
      Vorbereitungs-Uhren zählen im Sekundentakt hoch und zeigen auf **allen**
      Geräten dieselbe verstrichene Zeit, auch wenn die Gerätezeit falsch
      geht. *Am laufenden System gemessen (09.08.): Gerätezeit um zwei
      Stunden verstellt → Anzeige lief unbeirrt weiter (1:26 → 1:33). Der
      erste Anlauf zeigte 2:03:53 und deckte den Fehler auf, dass die Uhr in
      ruhigen Phasen nie nachgezogen wurde — dort kommen ausschließlich
      304-Antworten, und deren Zweig rief `syncClock` nicht auf.*
- [~] Bei einem Mehr-Hallen-Turnier filtert **ein** Filter im Kopf Felder,
      Spielliste, Bediener-Warteschlange und beendete Spiele gemeinsam.
      Spiele ohne Hallenzuordnung erscheinen in einem eigenen, immer
      sichtbaren Abschnitt mit Anzahl — sie werden nie stillschweigend
      ausgeblendet.
- [x] Bei einem Ein-Hallen-Turnier wird kein Hallenfilter angeboten.
- [x] Der Filter überlebt einen Reload und lässt sich per Link teilen.
      *Gemessen: Tipp auf „Kyritzer" → `?halle=Kyritzer` in der Adresse,
      Feldzahl 18 → 12; nach einem Reload **ohne** Parameter blieb der Filter
      gesetzt (aus dem lokalen Speicher).*

**Bedienung**

- [~] Ein Spiel lässt sich per **Antippen, dann Feld antippen** zuweisen —
      auf iPad/Safari, Android/Chrome und Desktop. *Auf dem Desktop belegt
      (09.08.): Feld 02 übernahm das Spiel, Meldung „Feld 02 bekommt …", das
      Spiel verschwand aus der Warteliste. iPad und Android stehen aus.*
- [~] Dasselbe geht per **Ziehen**, auf denselben Geräten. *Auf dem Desktop
      belegt (09.08.): Zug auf Feld 06 → „Feld 06 bekommt …". iPad und
      Android stehen aus.*
- [~] Bricht ein Ziehvorgang ab, bleibt das Spiel ausgewählt und lässt sich
      durch Antippen des Feldes zuweisen — ohne dass der Nutzer etwas
      zurücksetzen muss.
- [x] Solange ein Spiel ausgewählt ist, zeigt ein festes Band oben, welches
      Spiel gewählt ist, und bietet Abbrechen; erlaubte Felder sind
      hervorgehoben, nicht erlaubte gedimmt — ein Tipp darauf **nennt den
      Grund**, statt ins Leere zu gehen. *Gemessen: Band „Gruppe 6 · G1 — …
      | jetzt ein Feld antippen | Abbrechen", 13 erlaubte und 5 gesperrte
      Felder; Tipp auf ein gesperrtes: „Auf diesem Feld läuft ein Spiel." —
      die Auswahl blieb bestehen.*
- [x] Ein Spiel lässt sich von Feld A auf Feld B umhängen; das geht als
      **ein** BTP-Schreibvorgang, es gibt keinen Zwischenzustand ohne Feld.
- [~] Ein Spiel lässt sich vom Feld herunternehmen; geht dabei ein
      laufender Spielstand verloren, kommt vorher eine Rückfrage.
- [~] Vorbereitungs-Aufruf setzen und zurücknehmen, zweiter und dritter
      Aufruf, Ergebnis nachtragen, Aufgabe/kampflos werten und die
      Zähltafelbediener-Warteschlange pflegen (vorziehen, entfernen,
      ergänzen) funktionieren aus der Seite heraus.
      → **Erfüllt (2026-08-10).** Der Host beherrscht alle drei
      Zähltafelbediener-Aktionen und prüft sie (`ScorekeeperAdvance`/
      `Remove`/`Add`); die Seite bietet jetzt auch die Bedienung dafür an
      (`tl.html`, Abschnitt „Zähltafel-Warteschlange"). Am echten Gerät noch
      nicht nachgewiesen — deshalb `[~]` statt `[x]`.
- [~] Die automatische Feldvergabe ist als Zustand sichtbar und lässt sich
      umschalten; nach einer Zuweisung von Hand pausiert sie sichtbar für
      60 s, damit sie dem Helfer nicht dazwischenfunkt.
- [x] Die Pause gilt **nur zur Laufzeit** und endet beim Stoppen/Starten der
      Übertragung. Sonst bliebe sie hängen, sobald das Gerät weg ist, das sie
      gesetzt hat — die Einstellungen sagten „an", vergeben würde trotzdem
      nichts, und es gäbe keinen Griff dagegen.

**Aufrufe und Ansagen**

- [x] Die Aufruf-Stufe ist auf **allen** Geräten gleich — inklusive der
      Desktop-App; sie zählt am Host, nicht im Browser. (Ursprünglich
      1./2./3.; seit der Profil-Option „Aufrufe unbegrenzt", 17.08.2026,
      zählt der Host über 3 hinaus, sobald ein Profil die Option führt —
      ab Stufe 4 spricht das Ansage-Gerät ohne Stufenwort.)
- [~] Eine aus TL-Web ausgelöste Ansage wird **genau einmal** und auf
      **genau einem** Gerät je Halle gesprochen — mit demselben Text, Gong
      und derselben Stimme wie eine Ansage aus der Desktop-App, inklusive
      „Tabletbedienung: …", wenn ein Bediener zugewiesen ist.
- [x] Ist in der Zielhalle kein Ansage-Gerät verbunden, meldet die Seite
      das im Klartext, die Aktion gilt trotzdem als ausgeführt, und die
      Aufruf-Stufe zählt hoch.
- [x] Eine Ansage, die länger als 60 s nicht abgespielt werden konnte,
      verfällt und wird nicht nachträglich gesprochen.

**Ergebnis und Korrektur**

- [x] Ein noch offenes Spiel lässt sich mit einem Endstand nachtragen.
- [~] Ein bereits gewertetes Spiel lässt sich überschreiben, **solange der
      Sieger in kein Folgespiel wirkt**; sonst wird es mit einer
      verständlichen Begründung abgelehnt.
- [x] In einer Gruppen-Auslosung wird eine Korrektur **nicht**
      fälschlicherweise blockiert.
- [x] Nach einer Korrektur überschreibt kein nachgereichter Schreibvorgang
      aus der Retry-Queue das frische Ergebnis.

**Konflikte und Fehlerfälle**

- [x] Weisen zwei Geräte gleichzeitig dasselbe Feld zu, gewinnt genau
      eines; das andere bekommt „Feld wurde gerade von jemand anderem
      belegt" **mit dem Namen des Spiels, das dort steht**, und die Ansicht
      springt auf den echten Stand. Beide Ansichten sind danach gleich.
      *Mit zwei Geräten am selben Turnier gemessen (09.08.): Gerät B auf
      eingefrorenem Stand („Feld 05 frei") bekam „Feld wurde gerade von
      jemand anderem belegt: Coën Corvin Van / Thomas Schulze."*
- [ ] Eine Aktion, die auf einer über 60 s alten Ansicht beruht, wird
      abgelehnt statt ausgeführt.
      → **Bewusst nicht so gebaut.** Der Stand der Ansicht (`viewRev`) reist
      mit und steht im Protokoll, aber es gibt keine Schwelle darauf: Die
      Revision steigt bei jeder Änderung — in einem vollen Turnier im
      Sekundentakt, in einer ruhigen Phase minutenlang gar nicht. Dieselbe
      Zahl bedeutete mal Sekunden, mal eine Viertelstunde; jede Grenze wäre
      geraten. Was ein veralteter Blick anrichten kann, fangen die
      fachlichen Prüfungen genauer ab: `expect` beim Feld, der beanspruchte
      Walkover-Vorschlag, die Ergebnisprüfung. Ein echtes Alterskriterium
      bräuchte einen Zeitstempel im Zustand statt einer Revisionsdifferenz.
- [x] Ein doppelter Tipp bei langsamer Verbindung erzeugt **genau eine**
      Zuweisung in BTP.
- [x] Bricht die Verbindung zum Relay ab, zeigt die Seite binnen 15 s ein
      rotes Band und **deaktiviert alle Schreibknöpfe**. *Im LAN gemessen
      (09.08.): Übertragung gestoppt → Band „Keine Verbindung zum
      Turnier-PC — Felder, Spielstände und Liste stehen auf dem letzten
      bekannten Stand." Die Sperre sitzt zentral in `send()`, nicht als
      `disabled` an 125 Knöpfen: Ein toter Knopf erklärt nichts, ein
      gedämpfter, der beim Tippen „nichts wurde gesendet" sagt, schon. Die
      Dämpfung kam aus dieser Messung — vorher sahen die Knöpfe voll
      bedienbar aus.*
- [~] Ist der Relay erreichbar, aber bts-light nicht, zeigt die Seite
      ausdrücklich „bts-light ist nicht verbunden" samt Alter der Daten —
      **nicht** „alle Felder frei".
- [x] Eine Aktion, die während eines Ausfalls ausgelöst wurde, wird
      **nicht** nachgereicht; die Seite meldet „nicht bestätigt" und
      aktualisiert, sobald sie wieder kann. *Gemessen: Tipp bei gestoppter
      Übertragung → „Keine Verbindung zum Turnier-PC — nichts wurde
      gesendet."; nach dem Wiederanlauf kam nichts nach.*
- [~] Nach 10 Minuten Standby zeigt die Seite beim Aufwecken sofort einen
      frischen Stand und keine falsch weitergelaufene Uhr.
- [~] Ein BTP-Neustart erzeugt keine Geisterzuweisungen; die Liste erholt
      sich von selbst.
- [x] Bestätigen zwei Geräte denselben Walkover-Vorschlag, bekommt das
      zweite „bereits verarbeitet" statt einer zweiten Wertung.
- [~] Ein Turnier **ohne** TL-Web verhält sich unverändert, auch mit
      aktualisiertem Relay.

**Nachvollziehbarkeit**

- [x] Jede ausgeführte Aktion wird im App-Log mit **Gerätename und Aktion**
      festgehalten — damit nach einem Turnier nachvollziehbar ist, wer was
      ausgelöst hat, und die Erfolgskriterien überhaupt prüfbar sind.
      *Beleg aus der Abnahme: `TL-Web [abnahme-a]: Spiel 1316 auf Feld 2
      (Ansicht 1)` gefolgt von `… ausgeführt`.*
- [x] Jede **abgelehnte** Aktion wird mit ihrem Grund protokolliert;
      Konflikte („Feld war schon belegt") sind im Log als solche zählbar.
      *Beleg: `TL-Web [abnahme-b]: Spiel 1311 auf Feld 5 abgelehnt (Feld
      wurde gerade von jemand anderem belegt: …)`.*
- [x] Im Log erscheint **kein** Gerätetoken — weder ganz noch teilweise.

### Was offen ist

**28 belegt, 19 umgesetzt aber nicht am echten Gerät nachgewiesen, 3 offen**
(Auszählung der Kriterien oben, „Zugang und Geräte" bis „Nachvollziehbarkeit";
die beiden folgenden Wiederholungs-Bullets zählen nicht mit — sie verweisen
nur auf Kriterien, die dort schon erfasst sind).

- [~] **Zähltafelbediener-Warteschlange lässt sich aus der Seite nicht
      pflegen.** **Erledigt (2026-08-10):** Vorziehen, Entfernen und
      manuelles Hinzufügen sind jetzt in `tl.html` bedienbar — siehe
      [turnierleitung-web.md](../turnierleitung-web.md), Abschnitt „Im
      Betrieb". Am echten Gerät noch nicht nachgewiesen (zählt daher zu den
      19, nicht zu den 28 belegten).
- [~] **Beendete Spiele fehlen in der Ansicht.** **Erledigt (2026-08-10):**
      Der zugeklappte Abschnitt „Beendet" zeigt die zuletzt beendeten
      Spiele, neueste zuerst, mit Aufgabe-/kampflos-/
      disqualifiziert-Kennzeichnung — siehe
      [turnierleitung-web.md](../turnierleitung-web.md), Abschnitt „Im
      Betrieb". Am echten Gerät noch nicht nachgewiesen (zählt daher zu den
      19, nicht zu den 28 belegten).

Die drei verbleibenden offenen in der Reihenfolge, in der sie im Betrieb
wehtun:

1. **„Eingeschaltet, aber kein Gerät gekoppelt"** wird nicht als Problem
   benannt. Der Zustand entsteht regulär nach einem Identitäts-Umzug und
   sieht aus wie ein fertiges Setup, während jede Anfrage abgewiesen wird.
2. **Keine Altersprüfung der Ansicht** — mit Begründung oben; die
   fachlichen Prüfungen decken die konkreten Fälle ab.
3. **Die Formulierung „Seite nicht erreichbar"** trifft die Umsetzung
   nicht; geschützt sind die Daten-Routen, nicht die leere Hülle.

**Die 19 mit [~] sind kein Formfehler.** Sie sind im Code nachvollziehbar
und größtenteils durch Unit-Tests gestützt, aber der Nachweis, auf den es
ankommt, fehlt: iPad Safari und Android Chrome mit echten Fingern, zwei
Geräte gleichzeitig am selben Feld, ein Relay-Neustart mitten im Betrieb,
zehn Minuten Standby. Das ist die manuelle Abnahme weiter unten — sie ist
**nicht** abgearbeitet.

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
2. ~~**Halle am noch nicht gerufenen Spiel.**~~ **Beantwortet (Schritt 15,
   09.08.2026) — und danach gelöst:** Weil BTP den Ort nicht liefert, setzt
   ihn jetzt die Turnierleitung selbst (Hallen-Wähler an der Zeile,
   `TlAction::SetHall`, Kaskade Regel → Hand → Aufruf). Der Befund, der
   dazu geführt hat: **In echten Daten kommt kein Spielort an.** Geprüft an
   zwei Mitschnitten (Ein- und Zwei-Hallen-Turnier, zusammen 914 echte
   Paarungen): Weder `Match` noch `Draw`, `Event` oder `Stage` tragen eine
   `LocationID`. Die einzige Ortsangabe ist `Court.LocationID` — ein **Feld**
   gehört zu einer Halle —, und `Match.CourtID` erscheint erst, wenn das
   Spiel dort steht (im Zwei-Hallen-Mitschnitt bei 5 von 36 Paarungen, je
   zusammen mit `StartTime`).

   **Einschränkung, damit der Befund nicht überdehnt wird:** Das Konzept
   existiert in BTP — der Spielplan-Export hat die Spalten „Feld" und
   „Spielort". Im geprüften Turnier (540 angesetzte Spiele) sind sie in
   *jeder* Zeile leer. Was ein Turnier liefert, das sie pflegt, ist damit
   **nicht** beantwortet; dafür braucht es einen Mitschnitt eines solchen
   Turniers.

   **Folge für dieses Feature:** Die Kaskade lautet jetzt
   Disziplin/Klasse-Regel → **von Hand gesetzt** → Vorbereitungs-Aufruf →
   unbekannt. Die Regel bleibt vorn (sie bindet auch die Vergabe), die Hand
   schlägt den Aufruf. Ein Test in `btp_capture.rs` hält den Befund fest und
   schlägt an, falls ein künftiger Mitschnitt doch eine Ansetzungs-Halle
   enthält — dann wäre die Handzuweisung nur noch der Notnagel.

   Der Ort wirkt auch auf `upcoming_matches[].hall` und damit auf den
   Liveticker-Filter `display=next&halle=…`, der bislang leer blieb, sobald
   ein Turnier seine Aufrufe über BTP machte (offener Roadmap-Punkt seit
   19.07.). Einen Ort zu setzen gilt dabei **nicht** als Aufruf.
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
