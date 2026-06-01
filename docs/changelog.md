# Änderungsverlauf

Pro veröffentlichter Version die wesentlichen Änderungen. Die Versionen
werden über das Auto-Update (badhub.de) ausgeliefert; Tablet-Änderungen
erreichen den Cloud-Modus zusätzlich sofort über den Relay-Redeploy.

## v0.9.46

- **5 weitere Landesverbände als Preset.** Der Setup-Wizard bietet neben BVBB
  jetzt auch **BVRP, HBV, BBV, BWBV, NBV** als Ein-Klick-Ziel (eigene
  Liveticker-Adresse + Push-Token je Verband, einheitlicher Karten-Look).
- **Website-Einbettung mit Copy-Button.** Neue Dashboard-Karte
  „Website-Einbettung": zeigt den fertigen iFrame-Code für die Verbands-Website
  (WordPress) passend zum konfigurierten Turnier (`badhub.de/embed/live.php?t=…`,
  mit Auto-Höhe per postMessage) und kopiert ihn per Klick.
- **Hinweis für eigene Turniere.** Im manuellen Setup („Anderes Turnier") eine
  Infobox: für eine eigene Liveticker-Adresse vorab an info@badhub.de wenden.

## v0.9.45

- **Schnellere Selbstheilung nach Netzausfall.** Der Server-Timeout für tote
  Tablet-Verbindungen von 30 s auf **10 s** verkürzt. Da das jetzt kürzer ist
  als der Tablet-Watchdog (15 s), ist das Feld nach einem Router-/WLAN-Ausfall
  serverseitig schon frei, **bevor** sich das Tablet neu meldet – das „Feld
  wird bereits geschiedst"-Overlay erscheint dann gar nicht mehr und das
  Tablet belegt das Feld direkt selbst neu (kein manuelles „Übernehmen"). Auf
  gesunder Verbindung unkritisch: der Protokoll-Ping hält `last_seen` alle
  ~2 s frisch.

## v0.9.44

- **Zähltafelbediener-Hinweis auf dem Tablet-Spielzettel (Teil 2).** Bei der
  Seitenwahl zeigt das Tablet jetzt direkt, wer voraussichtlich die Zähltafel
  bedient: das Verlierer-Team des zuletzt auf diesem Feld beendeten Spiels
  („🧮 Zähltafel / Scoreboard: …"). `MatchBrief` trägt dafür ein neues Feld
  `scorekeeper` (vom Server aus `TabletState::scorekeeper`, LAN + Cloud),
  `#[serde(default)]` für Abwärtskompatibilität. Ergänzt Teil 1 (Übersicht in
  bts-light, v0.9.39). Kein Vorspiel auf dem Feld → kein Hinweis.
- **Pi-Court-Monitore: „German / English"-Übersetzungs-Pille unterdrückt.**
  Der Chromium-Kiosk läuft jetzt mit `--lang=de-DE`/`--accept-lang` und
  `--disable-features=Translate,TranslateUI` – Seite (deutsch) und UI-Sprache
  stimmen überein, sodass Chromium keinen Übersetzen-Hinweis mehr oben rechts
  einblendet. Wirkt nach erneutem `setup-monitor.sh` + Pi-Neustart.

## v0.9.43

- **TV-Anzeige verliert nach einem Netzausfall nicht mehr den Spielstand.**
  Sprang der TV nach einem kurzen Router-/Netzausfall auf 0:0 zurück (obwohl
  das Tablet weiterzählte) und kam nicht wieder, lag das an gleich mehreren
  Schwachstellen im Live-Score-Pfad. Behoben:
  - **Sticky Score:** Liveticker-Push und Felder-Übersicht vertrauten dem
    Tablet-Stand nur bei *offener* WebSocket-Verbindung – ein kurzer
    Aussetzer warf sie auf BTPs 0:0 zurück. Jetzt zählt der zuletzt
    gemeldete Stand für dasselbe Match unabhängig vom Verbindungsstatus
    (wie schon beim Feldmonitor); `verbunden` ist nur noch der Online-Indikator.
  - **Persistenz:** Der laufende Satzstand wird je Feld in `live-scores.json`
    gesichert und beim Start wieder geladen. Ein App-Neustart (Absturz,
    Standby) wirft den TV damit nicht mehr auf 0:0, bis das Tablet zurück ist.
    Atomar geschrieben (Temp-Datei + Rename), Schreiber serialisiert.
  - **Tote Verbindungen freigeben:** Bricht der Router weg, schickt der
    Browser oft kein „Close" – die Verbindung hing serverseitig und hielt das
    Feld „belegt", sodass das zurückkehrende Tablet ausgesperrt blieb. Der
    Server erkennt jetzt stille Verbindungen (Protokoll-Ping; >30 s ohne
    Lebenszeichen) und gibt das Feld frei.
  - **Selbstheilender Reconnect:** Hört das Tablet beim Wiederanmelden „Feld
    belegt", versucht es sich (wenn es das laufende Match hält) automatisch
    alle 4 s neu anzumelden und re-pusht nach erfolgreicher Übernahme sofort
    seinen Stand – ohne manuelles „Übernehmen". Ein echt fremdes Tablet
    behält das Feld; dann entscheidet weiter der Mensch.

## v0.9.42

- **Einzel- und Kombi-Anzeige einheitlich.** Drei Angleichungen:
  - Aufschlag-Punkt steht jetzt auf beiden Ansichten **vor der Flagge**
    (Punkt → Flagge → Name); vorher saß er auf der Einzel-Ansicht hinter
    dem Namen.
  - Flaggen einheitlich groß: feste Box + `object-fit:cover` auch auf der
    Kombi-Anzeige (vorher variable Breite je Seitenverhältnis).
  - Einzel-Ansicht hebt abgeschlossene Sätze jetzt auch **während des
    laufenden Spiels** den Satzsieger hell (weiß) hervor — wie die
    Kombi-Anzeige; vorher erst nach Spielende. Bei Aufgabe weiterhin keine
    Satz-Hervorhebung (letzter Satz unvollständig).

## v0.9.41

- **Einzel-Court-Ansicht: Aufschlag-Punkt spieler-genau im Doppel.** Auf
  dem Einzel-Feldmonitor (`monitor.html`) saß der gelbe Aufschlag-Punkt im
  Doppel/Mixed noch auf Team-Ebene (bei beiden Spielern). Jetzt steht er
  beim **konkret aufschlagenden Spieler** — dieselbe BWF-Logik wie auf der
  Kombi-Anzeige. Nutzt das vom Tablet berechnete `serving:{team,index}`;
  altes Tablet ohne die Info → Punkt beim ersten Spieler des Teams. Einzel
  unverändert.

## v0.9.40

- **Tablet-Auto-Reconnect (Heartbeat).** Das Tablet verbindet sich jetzt
  selbstständig neu, wenn der Server/Router kurz weg war — kein manuelles
  Seite-neu-Laden mehr nötig. Ein Watchdog (alle 5 s) sendet ein Ping und
  erkennt **tote Verbindungen auch dann, wenn der Browser kein `onclose`
  liefert** (Router weg → nur Stille): kam >15 s nichts vom Server, gilt
  die Verbindung als tot und wird neu aufgebaut. Backoff auf max. 5 s
  verkürzt (vorher 30 s). Der Watchdog ist der **einzige** Reconnect-
  Treiber (keine doppelten Sockets mehr).
  - `TabletMsg::Ping` / `ServerMsg::Pong` (relay-proto); LAN-Server
    *(server.rs)* und Cloud-Relay *(relay/main.rs)* antworten je sofort
    mit Pong.
- **Kombi-Anzeige: Feldnummer hervorgehoben.** Die Feldnummer am
  Bandanfang steht jetzt größer und als gelbes Badge (dunkler Text auf
  gelbem Block) — aus der Ferne sofort erkennbar.

## v0.9.39

- **Zähltafelbediener (Teil 1: bts-light-Übersicht).** bts-light merkt
  sich jetzt je Feld den **Verlierer des zuletzt dort beendeten Spiels**
  — das ist der voraussichtliche Zähltafelbediener fürs nächste Spiel.
  In der „Tablet-Spielzettel"-Übersicht steht er beim Feld mit
  Tablet-Symbol. Da BTP beendete Spiele nicht zuverlässig dem Feld
  zugeordnet behält, **trackt der Sync-Loop den Übergang OnCourt→Finished
  selbst** (kein Verlass auf BTP, keine externe DB — In-Memory pro Feld).
  - `TabletState.scorekeeper_by_court` + `SyncEngine.track_scorekeepers`
    (vergleicht zyklisch, welches Spiel ein Feld verlassen hat).
  - `CourtOverview.scorekeeper` (Verlierer-Namen), in TabletPanel angezeigt.
  - Teil 2 (Hinweis direkt auf dem Tablet-Spielzettel bei der Seitenwahl)
    folgt separat.

## v0.9.38

- **Aufschlag-Indikator spieler-genau im Doppel/Mixed.** Der gelbe Punkt
  steht jetzt beim **konkret aufschlagenden Spieler** (nicht mehr nur beim
  Team) und wechselt regelkonform: Bei geradem Punktestand des
  aufschlagenden Teams serviert der Spieler im rechten Aufschlagfeld, bei
  ungeradem der im linken; bei Side-out wechselt das Team. Das Tablet
  berechnet den Aufschläger (es kennt Positionen + Spieler-IDs) und legt
  `serving: {team, index}` in den `court_state`; `CourtOverview` trägt
  `serving_team` + `serving_player`, `combo.html` setzt den Punkt bei der
  richtigen Namens-Zeile. Einzel: Punkt beim einzigen Spieler. Alte
  Tablet-Stände ohne die Info → Team-Level-Fallback.

## v0.9.37

- **Fix: kein „Geistersatz" mehr nach Spielende.** Nach dem Match-Ende
  setzt das Tablet den laufenden Satz auf 0:0 zurück; `handle_score`
  hängte diesen leeren Satz an die Satzliste → in Kombi-/Übersicht-/
  Liveticker-Anzeige erschien ein zusätzlicher leerer Satz. Ein 0:0-Satz
  wird jetzt nicht mehr angehängt, wenn bereits Sätze gespielt sind
  (der allererste 0:0-Satz bleibt).
- **Fix: Monitor synct nach Netzwerk-Unterbrechung wieder.** Fiel der
  bts-light-Rechner kurz offline (Router/WLAN) und die Tablets zählten
  weiter, blieb der Kombi-Monitor nach dem Reconnect auf dem alten
  Stand. Das Tablet pusht jetzt beim Wiederverbinden (`ws.onopen`)
  sofort seinen aktuellen Satzstand + Spielzustand (Aufschlag/Pause) an
  den Server — Monitore + Liveticker holen damit den weitergezählten
  Stand vom Tablet zurück.
- **Kombi-Anzeige: Aufschlag-Indikator.** Vor dem aufschlagenden Team
  steht jetzt ein gelber Punkt (abgeleitet aus dem Tablet-Spielzustand:
  servingSide + teamOnSide). Zeigt auf einen Blick, welches Team
  aufschlägt; wechselt beim Aufschlagwechsel. `CourtOverview` trägt dazu
  ein `serving_team`-Feld (1/2/none).

## v0.9.36

- **Kombi-Anzeige: Ergebnis-Zahlen viel größer + ruhiger.** Die Satz-
  Zahlen skalieren jetzt mit der Feldzahl und nutzen die Bandhöhe aus
  (1 Feld ~30vh, 2 ~19vh, 3 ~13vh) — auf Distanz klar lesbar. Der
  „läuft"-Status (Punkt + Text) ist entfernt (redundant, kostete Platz);
  der laufende Satz wird nur noch farblich (gelb) markiert, **ohne
  Unterstrich**. Frei/Pause/TL/Behandlung bleiben als Status sichtbar.
- **Tablet: Zurück zur Aufstellung bei 0:0.** Wenn nach der Seiten-/
  Aufschlagwahl versehentlich zu schnell getippt wurde, führt der
  ↩-Button bei 0:0 (noch kein Punkt) zurück zur Aufstellung statt ins
  Leere. Das Button-Label wechselt dann zu „↩ Aufstellung ändern".

## v0.9.35

- **Fix: Auto-Update-Versionssprung repariert.** Ab v0.9.32 hatte der
  Versions-Bump (`package.json`/`tauri.conf.json`/`Cargo.toml`) nicht
  gegriffen — alle Builds v0.9.32–v0.9.34 trugen intern noch **0.9.31**.
  Folge: `latest.json` meldete eine neue Versionsnummer (aus dem Tag),
  der Installer war aber intern 0.9.31 → der Windows-Updater installierte
  faktisch wieder 0.9.31 und blieb in einer Update-Schleife. Mit v0.9.35
  stimmen Tag und interne Version wieder überein; das Update greift und
  bringt **alle** Fixes/Features aus v0.9.27–v0.9.35 auf einmal.
- **CI: Releases werden serialisiert** (`concurrency`-Group), damit nie
  zwei Publish-Jobs parallel ins Auto-Update-Verzeichnis schreiben und
  eine inkonsistente `latest.json` hinterlassen.

(Inhaltlich enthält 0.9.35 alle Änderungen seit 0.9.31: finishManually-
Push, Geräteliste sortiert/gruppiert, offline-Geräte entfernen.)

## v0.9.34

- **Offline-Geräte aus der Liste entfernen (X).** Offline-Monitore haben
  jetzt ein **X** zum Entfernen aus der „Court-Monitore"-Liste (vergisst
  den Live-Eintrag + löscht eine eventuelle Zuweisung). **Online-Geräte
  haben kein X** und werden auch server-seitig abgelehnt — sie kämen eh
  beim nächsten Poll zurück und sollen ihre Zuweisung nicht verlieren.
  Neuer Command `forget_monitor_device` (prüft `is_monitor_online`).

## v0.9.33

- **Fix: TV zeigt nach manuellem „Match beenden" den Endstand.**
  `finishManually()` pushte den finalen Stand nicht an Server/TV (wie
  zuvor schon `reopen()` nicht) → der Court-Monitor hing auf dem letzten
  Live-Stand. Ruft jetzt `sendScoreUpdate()` (Code-Review-Finding).
- **Court-Monitore-Übersicht: sortiert, gruppiert, offline unten.** Die
  Geräteliste in „Court-Monitore" ist jetzt aufgeräumt:
  - **Online-Geräte oben, offline darunter** unter einer „offline"-
    Trennlinie (ausgegraut) — keine Bereinigung nötig, störende
    Altgeräte rutschen nach unten.
  - Bei **mehreren Hallen** nach Halle gruppiert (Zwischenüberschrift).
  - Sortierung: **Felder zuerst (Feld 1 oben, dann 2, 3 …), dann
    Kombi-Felder, dann Info-/Werbe-TVs, dann unzugewiesene.**

## v0.9.32

- **Pausen-Countdown auf Tablet und TV synchron.** Das Tablet setzte
  `endsAt` mit seiner eigenen Uhr; der TV rechnet (seit v0.9.29) gegen
  die Server-Uhr → bei abweichenden Geräteuhren liefen die Countdowns
  5–6 s auseinander. Das Tablet holt jetzt per `/health` (neues Feld
  `serverNowMs`) seinen Uhr-Offset zum Server und setzt/zählt die Pause
  in **Server-Zeit** (`serverNow()`). Damit zeigen Tablet und TV
  denselben Wert. Offset wird beim Start und alle 30 s aktualisiert;
  ohne Verbindung Fallback auf die lokale Uhr.
- **Kombi-Anzeige lesbarer.** Die Satz-Zahlen sind deutlich größer
  (7vh, fett) und der laufende Satz stärker hervorgehoben (Glow). Im
  Doppel stehen die beiden Spieler eines Teams jetzt **untereinander**
  (A1 / A2) statt nebeneinander, **mit Flagge** je Spieler.
- **Court-Übersicht (`/info/overview`) zeigt jetzt Spielstände.** Je Feld
  beide Teams mit **Flagge**, Name(n) und **Satzstand** (gewonnene Sätze
  hervorgehoben, laufender Satz gelb) — vorher nur Teams + Status.
- **Court-Übersicht: dynamische Kachelgröße.** Das Feld-Raster passt die
  Spaltenzahl an die Feldanzahl an (1→1, 2→2, 3-4→2, 5-6→3 … bis 4) und
  füllt die Bildschirmhöhe (gleich hohe Zeilen). Bei wenigen Feldern
  (z. B. 4) große, bildschirmfüllende Kacheln statt kleiner Boxen oben.

## v0.9.31

- **Fix: TV übernimmt den Stand nach „Match wieder öffnen".** `reopen()`
  pushte den wiederhergestellten Stand nicht an den Server → der
  Court-Monitor hing auf dem alten beendeten Stand (zeigte z. B. 0:0 im
  laufenden Satz statt 20:17, und die alten Satz-Zahlen). `reopen()` ruft
  jetzt `sendScoreUpdate()` (wie `undo()`), der Server ersetzt die
  Satzliste, der TV zeigt beim nächsten 1-s-Poll den korrigierten Stand.
- **Neu: Korrektur direkt aus der Pause.** Im Pausen-Overlay (11er-/
  Satzpause) gibt es jetzt einen Button „↩ Korrektur — letzter Punkt
  zurück": bricht die Pause ab und nimmt den auslösenden Punkt zurück
  (z. B. wenn der Ball wiederholt werden muss und die Pause zu früh kam).
  Erscheint nur, wenn ein Punkt zum Zurücknehmen vorhanden ist.

## v0.9.30

- **Fix: „Match wieder öffnen" stellt den echten Stand auch nach einem
  Tablet-Reload her.** Die Undo-/Reopen-History wurde bewusst nicht
  persistiert. Endete ein Match automatisch (gewinnender Punkt) und das
  Tablet wurde danach neu geladen / reconnectete, war die History weg —
  `reopen()` konnte den letzten Stand (z. B. 20:1) nicht zurückholen und
  zeigte einen leeren `currentSet` (0:0) als zusätzlichen Satz. Die
  History wird jetzt mit in `localStorage` gesichert (auf 50 Snapshots
  gecappt) und beim Laden wiederhergestellt. „Match wieder öffnen" bringt
  damit den korrekten Stand + die korrekten Seiten zurück, und Korrektur
  per Undo funktioniert auch nach Pause/Reload (vorher war Undo bei
  leerer History gesperrt).

## v0.9.29

- **KRITISCHER Fix: Punkte landen nach „Match wieder öffnen" nicht mehr
  beim falschen Gegner.** `snapshot()`/`restoreSnapshot()` im Tablet-
  Spielzettel speicherten `teamOnSide` (welches Team auf welcher Seite
  steht) nicht. `swapSides()` (Satzende + Mid-Game-Switch bei 11 im
  Decider) flippt diese Zuordnung aber. Beim Undo/Wiederöffnen über eine
  solche Grenze blieb `teamOnSide` auf dem geflippten Stand, während
  `positions`/`currentSet`/`setsCompleted` zurückgesetzt wurden → die
  Team↔Seite-Zuordnung war gespiegelt und getippte Punkte gingen an den
  **falschen Gegner**. Jetzt wird `teamOnSide` (und `intervalDoneThisGame`)
  mit im Snapshot gesichert und korrekt wiederhergestellt. Alte, in
  localStorage liegende Snapshots ohne das Feld bleiben lesbar.
- **Fix: Pausen-Countdown + Match-Uhr auf dem TV stimmen wieder.** Der
  Court-Monitor (Pi) rechnete Pausen-Restzeit und Spieldauer mit seiner
  **eigenen** Uhr (`Date.now()`) gegen ein absolutes `endsAt`/`startedAt`
  vom Tablet. Pi Zero hat keine RTC und oft keine NTP-Synchronisation im
  Turnier-WLAN → die Uhr driftet, der Countdown war z. B. **+1 Minute**
  zu hoch (Tablet 1 min → TV 2 min). `MonitorState` trägt jetzt
  `serverNowMs` (Server-Zeit beim Poll); `monitor.html` rechnet relativ
  dazu statt zur Pi-Uhr. Fallback auf `Date.now()` bei alten Frames.

## v0.9.28

- **Kombi-Monitor Code-Review-Fixes (v0.9.27).**
  - `/combo/state` cappt die Felderzahl jetzt serverseitig auf **3** und
    entfernt **Duplikate** — eine manuell gebaute URL `?courts=1,1,1,…`
    kann das Band-Layout nicht mehr unleserlich machen.
  - `combo.html::setVal` vereinfacht (toter Parameter entfernt) +
    Fallback `0` statt `"undefined"` in der Satz-Zelle bei
    abweichendem Schema.
- **Chromium-Übersetzungsleiste auf den Pi-Monitoren aus.** Der
  Kiosk-Aufruf in `pi/setup-monitor.sh` bekommt
  `--disable-features=Translate --disable-translate` — damit erscheint
  die „German / English / Diese Seite übersetzen?"-Leiste oben rechts
  nicht mehr.

## v0.9.27

- **Kombi-Court-Monitor: bis zu 3 Felder auf einem Bildschirm.** Ein
  großer TV kann jetzt die Live-Spielstände von 2–3 Feldern gleichzeitig
  zeigen — als horizontale Bänder untereinander, je Feld Feldname,
  Disziplin, Status (läuft/Pause/TL/frei), beide Teams (Doppel-tauglich)
  und Satzstand mit hervorgehobenem laufendem Satz. So deckt man mit
  wenigen großen Bildschirmen viele Felder ab statt ein TV pro Feld.
  - Neue `MonitorTarget`-Variante `CourtCombo { court_ids }`
    (Wire-Form `{"kind":"court_combo","court_ids":[1,2,3]}`).
  - Neue Anzeige-Seite `combo.html` + Routen `/combo` und
    `/combo/state?courts=1,2,3` (filtert die Felder-Übersicht auf die
    gewählten CourtIDs, Reihenfolge = Band-Reihenfolge). 1-s-Poll,
    Pivot (`?rotate=`), Heartbeat wie die anderen Info-Seiten.
  - Zuweisung über einen **Kombi-Dialog** im „Court-Monitore"-Bereich:
    Dropdown-Eintrag „Felder wählen…" → Modal mit Feld-Checkboxen
    (2–3, Auswahl-Reihenfolge nummeriert). Aktive Kombi wird im
    Dropdown angezeigt.
  - Cloud-Modus: wie Info/Ad LAN-only (CourtCombo hat keine einzelne
    `court_id`, wird im Relay-Filter ausgeschlossen).

## v0.9.26

- **Schnellere Umstellung weg von Info-/Werbe-Anzeigen.** Ein Pi auf
  einer Info- oder Werbe-Seite (Courtübersicht, In Vorbereitung,
  Werbung) prüfte bisher nur **alle 30 s**, ob seine Zuweisung sich
  geändert hat — beim Umschalten zurück auf ein Feld (oder ein anderes
  Target) dauerte es entsprechend lang. Im LAN ist dieser Check ein
  winziger HTTP-GET; das Intervall ist jetzt auf **1 s** gesenkt
  (`overview.html`, `preparation.html`, `ad.html`) — gleich schnell wie
  `monitor.html`. Damit wirkt **jede** Umstellung im LAN binnen ~1 s,
  egal aus welcher Anzeige heraus.

## v0.9.25

- **Werbebilder mit Anzeigenamen.** In den Einstellungen → Werbebilder
  hat jedes Bild jetzt ein freies Textfeld für seinen Anzeigenamen
  (z. B. „Sommerfest 2026", „Sponsor Hauptbruecke"). Der Name wird in
  einer separaten JSON-Datei (`court-ad-labels.json`) persistiert und
  taucht in der „Werbung"-Sektion des Court-Monitor-Dropdowns statt
  des kryptischen `ad-1234567890.jpg` auf. Bilder ohne Label fallen
  auf den Dateinamen zurück. Beim Löschen eines Bilds wird der
  zugehörige Label-Eintrag mit aufgeräumt.
- **Tauri-Command `list_court_ads` ändert Rückgabetyp** von `Vec<String>`
  auf `Vec<CourtAd>` (`{file, label}`). Frontend nutzt jetzt `CourtAd[]`
  überall. Neuer Command `set_court_ad_label` zum Speichern.
- **MonitorTarget bleibt referenziert über `file`** (nicht Label) — eine
  Umbenennung in der UI bricht keine bestehenden Pi-Zuweisungen.

## v0.9.24

- **Default-Anzeige (Logo) übernimmt das App-Header-Design.** Statt des
  Badhub-Federball-PNGs zeigt der Pi jetzt das **gleiche Icon wie die
  bts-light-App selbst** (Dashboard-Header): Federball-Emoji 🏸 in einem
  dunklen Rounded-Square mit Schatten. Darunter Wordmark „badhub.de",
  darunter klein „BTS light". Dieselbe Atem-Animation wie vorher.
- **`fonts-noto-color-emoji` in `setup-monitor.sh`.** Pi OS Lite hat
  standardmäßig nur Mono-Schriften — ohne diese Font würde das Emoji
  als leeres Kästchen rendern. Wird beim ersten Setup-Lauf
  automatisch mit installiert. Auf Pis, die schon laufen, einmalig
  manuell nachziehen: `sudo apt-get install -y fonts-noto-color-emoji`
  und Chromium reloaden.
- **Unbenutztes Logo-PNG + Route entfernt** (`/assets/badhub-logo.png`,
  `BADHUB_LOGO_PNG`, `src-tauri/assets/badhub-logo.png`) — wurde nur in
  v0.9.23 kurz gebraucht und ist jetzt durch das Emoji-Design abgelöst.

## v0.9.23

- **Default-Anzeige für unzugewiesene Pis: Badhub-Logo Vollbild.**
  Statt der bisherigen Kopplungs-Karte mit großem Code zeigt ein Pi,
  der noch keinem Feld/Info-Target zugewiesen ist, jetzt das
  Badhub-Logo zentriert mit „badhub.de"-Wordmark darunter und einer
  sanften Atem-Animation. Sieht im Verleih-Set wie „läuft" aus, nicht
  wie „eingerichtet aber nichts darauf". Logo (PNG, 4 kB) ist in die
  bts-light-Binary eingebettet, neue Route `/assets/badhub-logo.png`.
- **„Identifizieren" zeigt jetzt den Device-Code Vollbild.** Der bisherige
  Identify-Overlay-Code (gelb, blinkend) bleibt — aber jetzt die einzige
  Stelle, an der der Code groß sichtbar wird. Operator klickt „Identifi-
  zieren" im Tool, der entsprechende Pi blendet seinen Code für 10 s
  (vorher 6 s) ein. Damit ist die Pi→Code-Zuordnung sauber bedienbar
  ohne den Code immer am TV anzuzeigen.

## v0.9.22

- **Online-Status auf Info-Pages korrigiert.** Der Pi auf einer
  Info-Page (Court-Übersicht, In Vorbereitung, Werbung) wurde in der
  „Court-Monitore"-Liste bisher als **offline** angezeigt, obwohl er
  problemlos läuft. Grund: `record_monitor_poll` lief nur in
  `/monitor/state`, das von Info-Pages aber nur alle 30 s gepollt wurde
  (Reassignment-Check) — der Server hat den Pi 24 von 30 s nicht
  gesehen, das Online-Fenster ist aber nur 6 s. Beim Entfernen oder
  Wechseln der Zuweisung dauerte es entsprechend lang, bis der Pi
  wieder als online angezeigt wurde.
- **Fix:** Die Info-State-Endpoints (`/info/ad/state`,
  `/info/preparation/state`, `/health`) akzeptieren jetzt einen
  optionalen `?device=<id>`-Query-Param. Wenn der gesetzt ist, zählt
  jeder dieser Polls als Lebenszeichen — der Pi gilt durchgehend als
  online. `ad.html`, `overview.html`, `preparation.html` schicken die
  Geräte-ID jetzt mit.
- **`ad.html` pollt schneller (5 s statt 60 s).** Neue Werbebilder
  erscheinen damit auch ohne Reboot/Reassignment auf dem Pi — und der
  schnellere Poll trägt direkt zum Online-Heartbeat bei.

## v0.9.21

- **Code-Review-Fixes zum Werbe-Target (v0.9.20).**
  - `read_assignments` parsed v3 jetzt **pro Eintrag** mit
    `serde_json::Value`-Zwischenstufe statt das ganze Map auf einmal.
    Schutz vor Datenverlust bei Downgrade: bisher hätte ein User, der
    eine Werbe-Zuweisung gesetzt hat und dann auf v0.9.18/v0.9.19
    zurückrollt, **alle** Court-Zuweisungen verloren (ein einziger
    unbekannter Eintrag → Map-Parse failed → leere Map). Jetzt: nur die
    unbekannten Einträge fallen weg, bekannte bleiben. Regressionstest
    in `monitor.rs`.
  - `ad.html`: `applyState` hat ein Dirty-Tracking — der 60-s-Pool-Poll
    triggert nicht mehr unnötig Cross-Fade auf das gleiche Bild und
    resettet auch nicht das Rotations-Intervall. Im `single`-Modus
    wird `showImage` nur bei tatsächlichem File-Wechsel gerufen.
  - `ad.html`, `overview.html`, `preparation.html`: bei
    Re-Assignment-Navigation (z. B. Pi wechselt von einem Info-Target
    zu einem anderen) wird der `?rotate=…`-Pivot-Param mitgenommen.
    Bisher ging die Rotations-Einstellung jedesmal verloren.

## v0.9.20

- **Werbe-Target im Court-Monitor-Dropdown.** Pis lassen sich jetzt
  nicht nur Feldern oder Info-Displays zuweisen, sondern auch direkt
  einer Werbe-Anzeige. Im „Court-Monitore"-Dropdown gibt es eine
  dritte Sektion „Werbung" mit zwei Modi:
  - **Rotierend:** alle hinterlegten Werbebilder im Wechsel, Intervall
    aus den Court-Monitor-Einstellungen (`ad_interval_s`).
  - **Einzelbild:** ein bestimmtes Werbebild Vollbild, dauerhaft.
  Wenn keine Werbebilder hinterlegt sind, ist die ganze Sektion
  ausgegraut. Neue Anzeige-Seite `assets/ad.html` mit Cross-Fade-
  Animation; Bilderpool wird alle 60 s frisch geholt, sodass das
  Hochladen neuer Bilder ohne Neustart wirkt.
- **`MonitorTarget` erweitert** um die Varianten `AdRotation` und
  `AdSingle { file }` (Wire-Form
  `{"kind":"ad_rotation"}` und `{"kind":"ad_single","file":"…"}`). Damit
  ist der Enum nicht mehr `Copy` — wo bisher `.copied()` reichte, ist es
  jetzt `.cloned()` (zwei Stellen angepasst, sonst transparent).
  `redirect_path()` liefert für Ad-Targets Pfad+Query
  (z. B. `/info/ad?mode=single&file=…`).
- **Reassignment-robust für Ad-Single.** Wechselt der Operator das
  Einzelbild eines Pis von `a.png` auf `b.png`, vergleicht `ad.html`
  beim 30-s-Poll den vollen Pfad+Query (nicht nur `pathname`) und
  navigiert auf das neue Bild. Kein Reload-Loop, kein Hängenbleiben
  auf dem alten Bild.

## v0.9.19

- **Code-Review-Fixes zur Info-Monitor-Zuweisung (v0.9.18).** Zwei
  Edge-Cases aus dem Review nachgezogen:
  - `read_assignments` migriert die alte v2-Datei jetzt **persistierend**
    nach v3 und schreibt das Ergebnis sofort auf Platte – Folge-Lesungen
    finden direkt v3 statt v2 erneut zu migrieren. Eine vorhandene aber
    **kaputte** v3-Datei (z.B. abgebrochener Schreibvorgang) ergibt
    bewusst eine leere Map statt auf v2 zurückzufallen; sonst hätte
    eine ältere v2 die jüngeren Info-Monitor-Zuweisungen überschrieben.
    Regressionstest in `monitor.rs`.
  - `monitor.html` prüft `redirectTo` **vor** `handleCommand`. Andersrum
    konnte ein anstehender `reload`-/`identify`-Command auf einer Seite
    feuern, die im selben Tick auf eine Info-HTML wegnavigiert –
    daraus resultierte ein Reload statt der Navigation.
- **Pi Zero 2 W: Chromium-Low-RAM-Warnung dauerhaft aus.** `setup-monitor.sh`
  setzt jetzt das `--no-memcheck`-Flag des Pi-OS-Chromium-Wrappers im
  Kiosk-Aufruf. Damit erscheint die "Less than 1 GB of RAM"-Splash auf
  Pi Zero 2 W nicht mehr; auf Geräten ≥ 1 GB ist das Flag ein No-Op.
  Heute live mit zwei Pi-Zero-2-W-Monitoren parallel verifiziert.

## v0.9.18

- **Info-Monitor-Zuweisung direkt aus dem Tool.** Die „Court-Monitore"-
  Seite hat ein erweitertes Dropdown: neben den Feldern (in den
  Mehr-Hallen-`optgroup`s) steht jetzt eine Sektion „Informationen" mit
  „Courtübersicht" und „In Vorbereitung". Wechseln zwischen Feld- und
  Info-Zuweisung passiert ohne SD-Karten-Editieren — der Pi merkt den
  Wechsel beim nächsten `/monitor/state`-Poll und navigiert sich selbst
  auf die richtige Seite. Auch der Rückweg (Info → Feld) klappt
  automatisch: die Info-Pages prüfen alle 30 s gegen `/monitor/state`,
  ob ihre Zuweisung sich geändert hat.
- **Datenmodell `MonitorTarget`** (Court | InfoOverview | InfoPreparation)
  ersetzt die reine CourtID-Zuweisung. Die Datei
  `monitor-assignments-v2.json` wird beim ersten Start nach
  `monitor-assignments-v3.json` migriert (jede CourtID → `Court`-Target);
  manuelles Eingreifen ist nicht nötig.

## v0.9.17

- **Info-Monitore: Court-Übersicht und In Vorbereitung.** Neben dem
  feld-bezogenen Court-Monitor (ein TV je Feld) liefert bts-light jetzt
  zwei Hallen-weite Info-Displays unter eigenen URLs aus —
  offline-fähig, direkt aus dem BTP-Snapshot, ohne Umweg über badhub.de:
  - `…/info/overview` zeigt **alle Felder** mit Status (frei, läuft,
    Behandlung, TL-Ruf), Paarung und Sätzen, bei Mehr-Hallen-Turnieren
    je Halle ein Abschnitt. Ideal für den TL-Tisch oder einen zentralen
    Eingangs-TV.
  - `…/info/preparation` zeigt die **gerufenen und eingeplanten Spiele**
    als Liste mit gold-Pille „In Vorbereitung", Halle und „vor X Min."
    pro Aufruf. Ideal als Meeting-Point-TV je Halle.
  Beide unterstützen `?halle=<Name>` (Hallen-Filter) und
  `?rotate=90|180|270` (Pivot-Monitor, dreht per CSS-Transform — keine
  OS-Anpassung am Pi nötig). Details:
  [docs/court-monitor.md → Info-Monitor](court-monitor.md).
- **`setup-monitor.sh` versteht Pi OS Lite.** Auf Lite installiert das
  Skript jetzt selbst den X-Stack (Xorg + matchbox-WM + Chromium),
  setzt Console-Autologin auf tty1 und richtet `.xinitrc` +
  `.bash_profile`-Hook so ein, dass beim Boot automatisch der Chromium-
  Kiosk startet. Auf Desktop bleibt der bisherige `.config/autostart`-
  Pfad. Non-interaktive Aufrufe (cloud-init, `curl | bash`) werden
  graceful unterstützt.

## v0.9.16

- **Hallen-Ansage für Spiele in Vorbereitung.** Im „In Vorbereitung"-Tab
  gibt es je gerufenem Spiel einen „Ansage"-Knopf: bts-light spielt dann
  eine gesprochene Ansage ab — Gong → „In Vorbereitung." → Disziplin →
  Paarung → „Bitte in *Halle X*." Nutzt die bestehende
  Ansage-Pipeline (Gong + Web Speech), Sprache aus den Ansage-
  Einstellungen oder automatisch (≥ Hälfte international ⇒ Englisch).
  `PreparationCandidate` trägt jetzt Disziplin und Einzel-Spielernamen
  inkl. Nationalitäten — Voraussetzung für die Ansage und Grundlage für
  die Auto-Sprachwahl. Der Knopf ist nur sichtbar, wenn die Ansagen
  aktiviert sind. Details: [docs/preparation.md](preparation.md),
  [docs/announcements.md](announcements.md).
- **Doku-Reorganisation.** Eigene Feature-Dokus für Spiele in Vorbereitung
  (`docs/preparation.md`) und für die Mehr-Hallen-Architektur als
  Gesamterzählung (`docs/multi-hall.md`); Querverweise in der
  `CLAUDE.md`-Datei-Map.

## v0.9.15

- **Court-Monitor: entschiedenes Match klar anzeigen — kein Geister-Satz.**
  Bei einem in zwei Sätzen entschiedenen Best-of-3 zeigte der Monitor noch
  eine leere dritte Satz-Spalte (0:0) als „laufenden Satz", als käme noch
  ein Satz. Jetzt: sobald das Tablet die Entscheidung meldet, rendert der
  Monitor nur die wirklich gespielten Sätze (etwaiger 0:0-Geister-Satz am
  Ende fällt weg), hebt je Satz das Gewinner-Team hell hervor und markiert
  die Sieger-Hälfte mit grünem Akzent und einer 🏆. Bei Aufgabe stammt der
  Sieger aus dem gespiegelten Tablet-Zustand (`retiredWinner`).
- **„In Vorbereitung" als Überschrift im Tablet-Panel.** Die Liste der
  gerufenen Spiele heißt jetzt „In Vorbereitung" statt „Aufgerufen" —
  konsistent zum Tab- und Liveticker-Namen.

## v0.9.14

- **Spiele „in Vorbereitung" aufrufen.** Neuer Tab „In Vorbereitung" im
  Tablet-Spielzettel: Die Turnierleitung wählt eingeplante Spiele aus und
  ruft sie in die Vorbereitung – bei Mehr-Hallen-Turnieren je Halle. Ein
  aufgerufenes Spiel erscheint auf der Aufruf-Anzeige des Livetickers
  (`/live?display=next`) hervorgehoben mit „vor X Min aufgerufen", damit
  die Spieler rechtzeitig in die richtige Halle gehen. Der Aufruf lässt
  sich zurücknehmen; kommt das Spiel aufs Feld, verschwindet er von
  selbst. BTP kennt keinen Vorbereitungs-Zustand – bts-light verwaltet
  ihn selbst, wie die Walkover-Vorschläge.

## v0.9.13

- **LAN und Cloud gleichzeitig.** Die Verbindungsart war bisher ein
  Entweder-oder. Für Zwei-Hallen-Turniere lässt sich jetzt **beides
  zusammen** aktivieren: die Haupthalle (mit bts-light + BTP) bindet ihre
  Tablets und Monitore lokal per LAN an, eine zweite Halle übers
  Cloud-Relay (Internet) — beides für dieselbe Turnier-Instanz. Im
  Einrichtungs-Assistenten sind LAN und Cloud nun zwei einzeln
  schaltbare Kacheln. Bei Doppelbetrieb zeigt der Tablet-Spielzettel je
  Feld beide QR-Codes (LAN und Cloud), die Court-Monitore-Seite beide
  Adressen, und die Geräteliste führt die Geräte beider Hallen zusammen.
  Reine LAN- oder reine Cloud-Turniere verhalten sich unverändert;
  bestehende Konfigurationen laden weiter.

## v0.9.12

- **Spielzettel: Zurück-Button im Setup war riesig.** Der „← Zurück ·
  Back"-Button im Aufstellungs-Assistenten füllte durch eine geerbte
  Flex-Regel die ganze Höhe des Fensters. Jetzt eine normal große
  Schaltfläche.

## v0.9.11

- **Court-Monitor: Spielernamen aus BTP exakt getrennt.** Der Monitor
  bezieht Vor- und Nachnamen jetzt direkt aus BTP, statt den Nachnamen am
  letzten Wort zu raten. Die Broadcast-Anzeige (Vorname klein, Nachname
  groß) stimmt damit auch bei mehrteiligen Nachnamen wie „van der Berg".

## v0.9.10

- **Installer legt die Firewall-Regel automatisch an.** Bei einer
  Neuinstallation richtet das Setup die eingehende Windows-Firewall-Regel
  für den Tablet-Server (Port 8088) selbst ein — die „Zugriff zulassen?"-
  Abfrage beim ersten Start entfällt. Es kommt einmalig eine
  Windows-Sicherheitsabfrage während der Installation. Greift nur bei der
  **interaktiven Installation**, nicht beim stillen Auto-Update — eine
  bestehende Installation bekommt die Regel also erst, wenn der Installer
  einmal von Hand ausgeführt wird.

## v0.9.9

- **Schließen beendet bts-light wirklich.** Das Fenster-Schließen-Kreuz
  beendet die App jetzt sauber, statt sie unsichtbar im Hintergrund
  weiterlaufen zu lassen — kein hängender Prozess mehr im Task-Manager.
  Läuft gerade ein Liveticker, fragt bts-light vorher zur Sicherheit
  nach. Für Hintergrundbetrieb das Fenster wie gewohnt minimieren.

## v0.9.8

- **Liveticker: Halle pro Feld im Push.** Der Liveticker-Push (`tset`)
  überträgt jetzt zu jedem Feld seine Halle — Grundlage für den nach
  Hallen getrennten Liveticker-Monitor auf badhub.de
  (`/live?display=monitor`). Noch keine sichtbare Änderung; die
  badhub-Seite folgt.

## v0.9.7

- **Mehr-Hallen-Unterstützung: Hallen sichtbar (Schritt 4–5/7).** Bei
  Turnieren in mehreren Hallen zeigt der Court-Monitor jetzt „Halle 2 ·
  Feld 6" statt nur des Feldnamens, das Tablet trägt dieselbe Bezeichnung.
  Die Felder-Übersicht, die QR-Code-Liste und die Geräte-Zuweisung im
  Dashboard sind nach Halle gruppiert. Ein-Hallen-Turniere bleiben
  unverändert — kein Hallen-Präfix, keine Gruppierung.

## v0.9.6

- **Mehr-Hallen-Unterstützung: Felder eindeutig per BTP-ID (Schritt 2–3/7).**
  bts-light unterscheidet Spielfelder jetzt über ihre stabile BTP-interne
  ID statt über den Feldnamen — durchgängig in Tablet-Server, Relay und
  Oberfläche. Damit verschmelzen bei Mehr-Hallen-Turnieren „Halle 1 ·
  Feld 1" und „Halle 2 · Feld 1" nicht mehr; alle Felder funktionieren
  unabhängig. Ein-Hallen-Turniere verhalten sich unverändert.
- **Einmalig nach diesem Update:** Die Court-Monitor-Geräte müssen ihren
  Feldern einmal neu zugewiesen werden (die alte Zuordnung hing am
  Feldnamen). Die Geräte erscheinen automatisch wieder in der Geräteliste.
  Tablets, die während des Updates geöffnet bleiben, einmal neu laden.

## v0.9.5

- **Tablet-Spielzettel: zwei Tabs.** Die Seite ist jetzt in „Übersicht"
  (Live-Stand aller Felder mit Tablet-Verbindung und Akku) und „QR-Codes"
  (Adressen zum Einrichten der Tablets) getrennt — übersichtlicher,
  gerade bei vielen Feldern.

## v0.9.4

- **Vorbereitung Mehr-Hallen-Unterstützung (Schritt 1/7).** bts-light liest
  jetzt die Standorte (Hallen) und die Feld-IDs aus BTP aus — Grundlage
  dafür, dass Turniere in mehreren Hallen künftig automatisch nach Halle
  getrennt angezeigt werden. Noch keine sichtbare Änderung; der Fahrplan
  steht in [roadmap.md](roadmap.md).
- **Diagnose-Log: Turnier-Topologie.** Das Log nennt bei jeder Änderung
  „N Hallen, M Felder, K Matches" — hilft bei Einrichtung und Fehlersuche.

## v0.9.3

- **Court-Monitor: Spielernamen im Broadcast-Stil.** Namen erscheinen
  jetzt zweizeilig — Vorname klein darüber, Nachname groß darunter, wie in
  Sport-Übertragungen. Lange Doppel-Namen bleiben dadurch aus der Distanz
  gut lesbar; die frühere Initialen-Kürzung entfällt. Details:
  [court-monitor.md](court-monitor.md).

## v0.9.2

- **Spielzettel: Zurück-Schritt im Match-Setup.** Der Aufstellungs-
  Assistent (Seitenwahl → Aufschlag → Annahme) hat ab Schritt 2 einen
  „← Zurück · Back"-Button. Eine falsch getippte Wahl lässt sich so
  korrigieren, ohne das Match neu zuweisen zu müssen.
- **Spielzettel: zweisprachige Beschriftung (DE/EN).** Titel und Hinweise
  des Setup-Assistenten erscheinen jetzt Deutsch und Englisch – für die
  wachsende Zahl internationaler Spieler:innen.
- Details: [tablet.md](tablet.md).

## v0.9.1

- **Court-Monitor: Spieldauer in der Kopfzeile.** Neben der Feldnummer
  zeigt der Monitor optional die laufende Spieldauer (Minuten, mit
  Stoppuhr-Symbol). Im Setup ein-/abschaltbar; sichtbar, sobald ein
  Tablet das Feld zählt.
- **Court-Monitor: Werbung im Leerlauf abschaltbar.** Neue Option
  „Werbung im Leerlauf anzeigen". Aus → ein freies Feld zeigt eine
  neutrale Leerlauf-Seite statt der Werbebilder.
- **Court-Monitor: lange Namen werden automatisch gekürzt.** Läuft ein
  Name über seine Spalte (häufig bei Doppeln mit langen internationalen
  Namen), kürzt der Monitor die Vornamen auf Initialen
  („Ajay Kumar Mandapati" → „A. K. Mandapati"); der Nachname bleibt voll.
- **Court-Monitor: Layout-Auswahl vorbereitet.** Das Anzeige-Layout ist
  jetzt im Setup wählbar (aktuell „A — Geteilt"); Grundlage für weitere
  Layouts. Abgeschlossene Sätze werden etwas größer dargestellt.
- Details: [court-monitor.md](court-monitor.md).

## v0.9.0

- **Court-Monitor: fester Name `bts-light.local` (mDNS).** Der Turnier-PC
  meldet sich im LAN-Modus unter dem festen Namen `bts-light.local` im
  Netz. Tablets und Court-Monitore erreichen ihn darüber, **ohne seine
  IP-Adresse zu kennen** – es braucht keine feste IP mehr, weder im
  Router noch am Laptop. Die Monitor-Adresse
  `http://bts-light.local:8088/monitor` ist damit in jedem Turnier-WLAN
  dieselbe – die Grundlage für ein Master-Image, das ohne Anpassung auf
  jedem Pi läuft. Details: [court-monitor.md](court-monitor.md).

## v0.8.2

- **Court-Monitor: Satzstand bleibt bei kurzem Tablet-Aussetzer stehen.**
  Schloss man am zählenden Tablet kurz den Browser, sprang der Monitor
  auf 0:0 und zeigte den Stand erst beim Wiederverbinden erneut. Ursache:
  ein erneutes Zuweisen desselben Matches (Tablet-Reconnect) setzte den
  gemerkten Satzstand zurück. Relay und LAN-Server halten jetzt den
  zuletzt bekannten Stand – zurückgesetzt wird nur bei echtem
  Match-Wechsel.
- Cloud-Monitor-Adresse korrigiert (`/bts-relay`-Pfad fehlte), Werbe-
  Upload-Limit am Server angehoben – beides bereits am Relay/Server
  ausgerollt.

## v0.8.1

- **Court-Monitor: stabile Geräte-ID per Pi-Seriennummer.** Der Pi-Kiosk
  übergibt jetzt die Hardware-Seriennummer als Geräte-ID. Damit lässt
  sich eine fertig eingerichtete SD-Karte beliebig auf weitere Pis
  klonen, ohne dass sich Geräte eine ID teilen – die Grundlage für ein
  „Master-Image" zur einfachen Verteilung. Anleitung:
  [pi-setup.md](pi-setup.md).

## v0.8.0

- **TV-Verwaltung für die Court-Monitore.** Monitore sind jetzt generische
  Geräte: Alle Raspberry Pis bekommen *dieselbe* Adresse (`…/monitor`) und
  zeigen beim Start einen Kopplungs-Code. Auf der neuen Seite
  **„Court-Monitore"** im Tool weist die Turnierleitung jedem Gerät ein
  Feld zu (jederzeit umstellbar), sieht den Online-Status und löst per
  Fernbefehl **„Identifizieren"** (Code groß einblenden) und **„Neu laden"**
  aus – in LAN und Cloud. Die feste Adresse `…/court/<Feld>/display`
  bleibt als Direkt-Variante erhalten. Details:
  [court-monitor.md](court-monitor.md).
- **Live-Vorschau der Anzeige-Optionen** im Court-Monitor-Setup –
  Disziplin/Runde/Spielnummer/Pausen-Timer wirken sofort sichtbar.
- Über-Dialog: Mitwirkende korrigiert (Tim Lehr; Philipp Hagemeister als
  „Visionär einer digitalen Turnierausrichtung").

## v0.7.0

- **Court-Monitor – TV-Anzeige am Spielfeld**: Pro Feld eine read-only
  Anzeige (Raspberry Pi, 32"–55"), die zwischen zwei Zuständen umschaltet:
  Werbung im Leerlauf, Match-Ansicht sobald ein Spiel aufs Feld kommt. Die
  Match-Ansicht („A — Geteilt") zeigt Spielernamen mit Landesflaggen, den
  Satzstand, die aufschlagende Mannschaft (eingefärbt) und einen
  Retro-Pausen-Countdown im Klappanzeigen-Stil. Werbebilder werden im Tool
  hochgeladen (ein gemeinsamer Satz für alle Felder); Wechsel-Intervall und
  Anzeige-Optionen sind einstellbar. Funktioniert im LAN- und im
  Cloud-Modus. Details: [court-monitor.md](court-monitor.md).

## v0.6.0

- **Sprachansagen für Feld-Aufrufe**: Wird in BTP ein Spiel auf ein Feld
  gezogen, sagt bts-light es über die PC-Lautsprecher an – Gong, Feld,
  Disziplin (Herren-/Dameneinzel, Herren-/Damendoppel, Mixed) und die
  Paarung. Deutsch, Englisch oder automatisch (Englisch, wenn mindestens
  die Hälfte der Spieler international ist); Stimmen und Tempo einstellbar.
  Details: [announcements.md](announcements.md).

## v0.5.0

- **Kampflose Wertung nach Aufgabe**: Gibt eine Mannschaft während eines
  Spiels auf und hat in derselben Disziplin noch weitere, ungespielte
  Spiele, blendet bts-light ein Fenster ein und schlägt vor, diese
  kampflos (Walkover) für den jeweiligen Gegner zu werten. Die
  Turnierleitung wählt die betroffenen Spiele aus und bestätigt – erst
  dann gehen sie mit `ScoreStatus = 1` nach BTP. Maßgeblich ist nur die
  Disziplin der Aufgabe; spielt ein Doppelpartner in einer anderen
  Disziplin mit anderem Partner, bleibt das unberührt.
- **Heartbeat**: bts-light meldet sich auch im Leerlauf alle 60 s beim
  Liveticker. So erkennt badhub.de ein laufendes Turnier zuverlässig als
  „live" – und kennzeichnet es als beendet, sobald bts-light geschlossen
  wird (kein Heartbeat mehr).
- **Versionsanzeige & Mitwirkende**: Fußzeile mit der installierten
  Version und ein „Über"-Dialog, der die Pioniere der BTS-Community
  würdigt – Philipp Hagemeister (Idee & Begründung), Tobias Lehr, letilo.

## v0.4.6

- **Kopier-Button** für die Tablet-Adressen in der Tablet-Spielzettel-
  Seite – die URL lässt sich jetzt in die Zwischenablage kopieren.
- Dieses Changelog angelegt.

## v0.4.5

- **Tablet-Übernahme mit laufendem Spielstand**: Das aktive Tablet
  spiegelt seinen Spielzustand laufend an den Server. Übernimmt ein
  anderes Gerät den Court, setzt es das laufende Spiel mit aktuellem
  Stand fort – statt bei 0:0 zu beginnen.
- Sieger-Wahl bei Aufgabe als große Buttons (vorher zu kleiner Text).

## v0.4.4

- **Spiel abbrechen / Aufgabe**: In der Behandlungspause beendet
  „Spiel abbrechen" das Match per Aufgabe – Teilstand wird übernommen,
  der Sieger manuell gewählt, das Ergebnis geht mit Status „retired"
  (`ScoreStatus = 2`) nach BTP.

## v0.4.3

- **Spieldauer** als MM:SS-Uhr in der Tablet-Kopfzeile.
- **Verletzungs-Button** (✚): unterbricht das Spiel, meldet es; das Feld
  wird in der bts-light-Felder-Übersicht hervorgehoben.
- **Turnierleitung-rufen-Button** (📣): Popup deutsch/englisch; Meldung
  erscheint app-weit in bts-light mit Feldnummer.
- **Tablet-Übernahme**: ein aktives Tablet pro Court; ein zweites Gerät
  zeigt „Feld wird bereits geschiedst" + Übernehmen.
- Zuvor (Zwischen-Deploys): Einzel-Court-Grafik-Fix (Name nicht doppelt),
  Ergebnis-Übermittlung mit automatischem Wiederholen bis zur Bestätigung.

## v0.4.2

- **Offizielle Pausen** (BWF): 60 s bei 11 Punkten, 120 s zwischen den
  Sätzen, je mit Countdown und „Weiterspielen".
- **Akkustand** der Tablets in der Felder-Übersicht (Android/Chrome).
- Moduswechsel LAN/Cloud greift sofort (Sync-Neustart beim Speichern).

## v0.4.1

- Oberflächen-Politur: Menü-/Button-Icons, Tooltips, modernere Optik.
- Cloud-Hinweis bei „Tablet-Spielzettel" für gesperrte Netze.

## v0.4.0

- **Cloud-Relay**: Tablets erreichen bts-light wahlweise direkt im LAN
  oder über einen Relay auf badhub.de. Der Cloud-Weg nutzt nur
  ausgehende Verbindungen und funktioniert auch hinter gesperrten
  Firmen-Firewalls. Umschaltbar im Setup. Details:
  [cloud-relay.md](cloud-relay.md).

## v0.1 – v0.3

Grundlagen: BTP-Anbindung (TP-Network-Protokoll), Badhub-Liveticker-Push,
Sync-Engine, Setup-Wizard und Dashboard, Auto-Update, digitaler
Tablet-Spielzettel im LAN, Diagnose-Logs, Single-Instance.
