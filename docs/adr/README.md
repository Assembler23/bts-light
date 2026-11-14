# Architecture Decision Records (ADRs)

Kurze, versionierte Dokumente pro **bewusster, schwer reversibler** Entscheidung
(Tool-/Framework-Wahl, Protokoll-/Architektur-Schnitt, Release-Verfahren). **Nicht** für Alltägliches.

Format: [Nygard](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions) —
Vorlage in [`template.md`](template.md). Neue ADRs fortlaufend als `NNNN-kebab-titel.md` ablegen und
hier eintragen.

## Index

| Nr. | Titel | Status |
|---|---|---|
| [0001](0001-quality-gate-und-branch-protection.md) | Quality-Gate & Branch Protection | accepted |
| [0002](0002-ferne-halle-direkt-cloud-geraete.md) | Ferne Halle: Tablets & Monitore per Direkt-Cloud statt Slave-Multiplex | accepted |
| [0003](0003-azure-tts-vererbung-relay.md) | Azure-TTS-Konfiguration wird über den Relay an Cloud-Slaves vererbt | akzeptiert |
| [0004](0004-telefon-kopplungscode.md) | Kopplung ferner Hallen über kurzlebigen 8-stelligen Telefon-Code | akzeptiert |
| [0005](0005-lan-https-selbstsigniert.md) | LAN-Tablet-Server: HTTPS mit selbstsigniertem Zertifikat | akzeptiert |
| [0006](0006-master-identitaet-umziehen.md) | Master-Identität per Export/Import auf einen neuen PC umziehen | akzeptiert |
| [0007](0007-zaehltafelbediener.md) | Zähltafelbediener nach Vorbild Original-BTS, in zwei Phasen | akzeptiert |
| [0008](0008-auto-aussprache-serverseitig-badhub.md) | Automatische Aussprache-Vorschläge entstehen serverseitig bei badhub (opt-in) | akzeptiert |
| [0009](0009-hallen-checkin-persistenz-und-identitaet.md) | Hallen-Check-In: Persistenz in badhub, Turnier-Identität ist die turnier.de-GUID | accepted |
| [0010](0010-dependabot-automerge.md) | Dependabot-Updates mit Minor-/Patch-Umfang automatisch mergen | akzeptiert |
| [0011](0011-tl-web-schreibender-cloud-pfad.md) | Schreibender Cloud-Pfad für Turnierleitungs-Geräte: Whitelist fixierter Aktionen, der Host validiert | accepted |
| [0012](0012-tl-web-geraete-identitaet.md) | Geräte-Identität für Turnierleitungs-Geräte: host-ausgestellte, widerrufbare Tokens | accepted |
| [0013](0013-ergebniskorrektur-nur-ohne-folgespiel.md) | Ergebniskorrektur nur, wo nichts daran hängt | accepted (vorläufig) |
| [0014](0014-punktverlauf-expliziter-rally-frame.md) | Punktverlauf: expliziter Rally-Frame vom Tablet | accepted |
| [0015](0015-punktverlauf-datei-je-turnier.md) | Punktverlauf: eine JSON-Datei je Turnier beim Host | accepted |
| [0016](0016-monitor-push-transport.md) | Monitor-Push-Transport: WebSocket-Nudge statt SSE oder Poll | accepted |
| [0017](0017-reconnect-ownership.md) | Reconnect-Konfliktmodell: Ownership „Slot-Halter gewinnt" statt rev-Zähler | accepted |
| [0018](0018-ergebnis-weg-verlustsicher.md) | Ergebnis-Weg verlustsicher: Idempotenz, persistente Retry-Queue | accepted |
| [0019](0019-relay-last-soak-inprocess.md) | Relay-Last-/Soak-Test: In-Process, Multi-Thread, ehrliche Abgrenzung | accepted |
| [0020](0020-tote-verbindung-read-idle-tablet-stale.md) | Tote-Verbindungs-Erkennung: Read-Idle (Option A) + Tablet-Empfangs-Stale | accepted |
| [0021](0021-officials-ruecksync-eigenstaendiger-write.md) | Officials-Rücksync: eigenständiger Write, BTP gewinnt | accepted |
| [0022](0022-officials-turnierdaten-eigene-datei.md) | Officials-Turnierdaten: eigene turniergebundene Datei | accepted |
| [0023](0023-manuelle-spielreihenfolge-praefix-je-halle.md) | Manuelle Spielreihenfolge: Präfix je Halle, atomare Züge, ein Sortier-Helfer | superseded by [ADR 0026](0026-spielliste-eine-globale-reihenfolge-eine-liste.md) |
| [0024](0024-tl-panel-profile-verwaltung-im-web.md) | Panel-Profil-Verwaltung direkt in tl.html, zweite Ausnahme von der Setup-Regel | accepted |
| [0025](0025-tl-panel-profile-transport-persistenz.md) | TL-Panel-Profile: Hybrid-Transport, Persistenz installationsweit | accepted |
| [0026](0026-spielliste-eine-globale-reihenfolge-eine-liste.md) | Spielliste: eine globale Reihenfolge, eine Liste, Status an der Zeile | accepted |
| [0027](0027-spielzeit-stempel-hostseitig.md) | Spielzeit-Stempel entstehen host-seitig, nicht auf dem Tablet | accepted |
| [0028](0028-pause-haelt-bis-weiterspielen.md) | Satzpausen enden erst mit „Weiterspielen", nicht mit dem Countdown | accepted |
| [0029](0029-hallen-vorverteilung-eigener-store.md) | Hallen-Vorverteilung: eigener turniergebundener Store + `HallSource::Auto` | accepted |
| [0030](0030-halle-bindet-die-feldvergabe.md) | Die Halle bindet die automatische Feldvergabe (Constraint + Aufruf-Ersatz) | accepted |
| [0031](0031-hallen-farben-eigener-config-store.md) | Hallen-Farben: eigene `hall_colors`-Struktur statt `HallLayoutConfig`-Umbau | accepted |
| [0032](0032-hallen-farben-deterministische-auto-palette.md) | Hallen-Farben: deterministische Auto-Palette über die sortierte Hallenliste | accepted |
| [0033](0033-hallen-farben-hex-auf-dem-draht.md) | Hallen-Farben: Hex-String auf dem Draht, Palettenzwang nur am Schreibpunkt | accepted |
| [0034](0034-tl-web-push-transport.md) | TL-Web-Push: WebSocket-Nudge mit In-Band-Auth | accepted |
| [0035](0035-monitor-livestand-ordnung.md) | Monitor-Livestand: schmaler Abruf, Ordnung über `seq`, additive Nudges | accepted |
| [0036](0036-hallen-achse-im-messwert.md) | Hallen-Achse: Halle im Messwert stempeln statt zur Anzeigezeit nachschlagen | accepted |
| [0037](0037-zettel-ereignisse-eigener-strom.md) | Zettel-Ereignisse: eigener Frame, eigener Store, eigene Datei | accepted |
| [0038](0038-ereignisse-append-only.md) | Zettel-Ereignisse sind append-only; eine Rücknahme ist ein Eintrag, kein Löschen | accepted |
| [0039](0039-zettel-html-im-webview.md) | Der Zettel wird einmal am Host als HTML gerendert und im WebView gedruckt | accepted |
| [0040](0040-ansage-besetzung-einstellbar.md) | Ansage der Besetzung: je Gerät einstellbar statt turnierweit | accepted |
| [0041](0041-werbe-stil-je-bild.md) | Werbe-Hintergrund und Feldbezeichnung: Stil je Bild statt global | accepted |
| [0042](0042-stiller-druck-ueber-elementliste.md) | Stiller Druck über eine Elementliste, nicht über den WebView | accepted |
| [0043](0043-zettelblatt-nach-dbv-vorbild.md) | Das Zettelblatt folgt dem DBV-Bogen, nicht dem eigenen Raster | accepted |
| [0044](0044-sperrliste-turniergebunden.md) | Die Sperrliste gilt für ein Turnier | accepted |
| [0045](0045-fertig-warnung-serverseitig-gestempelt.md) | „Fertig, aber kein Ergebnis": Host stempelt, Seite rechnet | accepted |
| [0046](0046-wunschfeld-reserviert-ab-spielbereitschaft.md) | Das Wunschfeld reserviert ab Spielbereitschaft | accepted |
| [0047](0047-lan-tls-konkretisierung.md) | LAN-TLS: Crate-Wahl, Zertifikatsstrategie und Port-Koexistenz | accepted |
| [0048](0048-substrom-adressierung-traeger.md) | Ferne Halle: Träger-Verbindung, Substrom-Adressierung und lokale Terminierung | accepted |
| [0049](0049-kombi-ausrichtung-eigene-geraete-datei.md) | Kombi-Ausrichtung je Gerät: eigene Geräte-Datei statt Target-Feld | accepted |
| [0050](0050-verschiebe-modus-globales-einfuegeziel.md) | Das Einfügeziel im Verschiebe-Modus bleibt global, nicht filterbewusst | accepted |
| [0051](0051-offene-spiele-eigene-gedeckelte-liste.md) | Offene Spiele reisen als eigene, zuerst gekappte Liste | accepted |
| [0052](0052-beschriftung-offener-plaetze.md) | Offene Plätze: Kandidaten aus einer Ebene, neutraler Rückfall | accepted |
| [0053](0053-offene-spiele-in-der-manuellen-reihenfolge.md) | Offene Spiele nehmen an der manuellen Spielreihenfolge teil | accepted |
| [0054](0054-liveticker-kind-turnier-je-guid.md) | Liveticker: Kind-Turnier je turnier.de-GUID statt zusammengesetztem Schlüssel | accepted |
| [0055](0055-zaehltafel-anzeige-huelle-und-zuweisungsziel.md) | Zähltafel: Anzeige-Hülle als iframe-Container, Tafel als eigenes Zuweisungsziel (gegen 0049 abgegrenzt) | accepted |
| [0056](0056-zeilenfarbe-btp-fuehrt-aufrufmarke-weicht.md) | Zeilenfarbe: BTP führt, die Aufrufmarke weicht der Handfarbe | accepted |
| [0057](0057-update-ablauf-im-rust-kern.md) | Update-Ablauf im Rust-Kern, Wiederanlauf per Marker, stiller Installer beim Beenden | accepted |
| [0058](0058-eigene-kiosk-app-statt-fully-kiosk.md) | Eigene Android-Kiosk-App statt Fully Kiosk, Suche nach dem Pi-Muster | accepted |
| [0059](0059-setzplatz-als-text-aus-dem-parser.md) | Setzplatz als fertiger Text, aufgelöst im Parser | accepted |
| [0060](0060-upload-sperrt-btp-writes-schreib-gate.md) | TP-Upload sperrt BTP-Writes kurz (Schreib-Gate auf RwLock) | accepted |
| [0061](0061-takt-upload-lebenszyklus-schnellhash-anhalteregel.md) | Takt-Upload: Lebenszyklus am Liveticker, Schnell-Hash ohne Gate, Anhalte-Regel | accepted |
| [0062](0062-tp-upload-ereignis-ausloeser.md) | TP-Upload: Ereignis-Auslöser zieht den Takt vor, nur für Auslosung und Paarung, mit Drossel | accepted |
| [0063](0063-ansage-seiten-text-im-host-kern-vertont.md) | Ansage-Seiten: Text aus der Host-Oberfläche, Kern vertont (RAM, 60 s), Seite spielt nur ab | accepted |
| [0064](0064-erst-ansage-im-kern.md) | Erst-Ansage im Kern als Auftrag `court_assigned`; `MatchAnnouncer` nur am Ansage-Slave | accepted |
| [0065](0065-ansage-ton-aus-dem-kern-wasapi.md) | Ansage-Ton aus dem Rust-Kern (WASAPI über cpal), Gerät je Abspielvorgang neu aufgelöst | accepted |
| [0066](0066-windows-stimme-e3-im-kern.md) | Windows-Stimme (E3) im Kern, vorgezogen und gemeinsam für Turnier-PC und Ansage-Seiten | accepted |
| [0067](0067-mannschaften-berichtigt-kipp-ereignis.md) | „Mannschaften berichtigt" als Kipp-Ereignis; Tablet-Korrektur als eigener Undo-Schritt | accepted |
| [0068](0068-monitor-stil-als-attribut-am-feld-ziel.md) | Monitor-Stil als Attribut am Feld-Ziel (`serde(other)`), in-band über `MonitorState` | accepted |
| [0069](0069-stream-einblendung-eigene-seite.md) | Stream-Einblendung als eigene Seite auf dem Feld-Zustand je Feld, Namespace-Offenlegung akzeptiert | accepted |
| [0070](0070-nation-verein-auf-anzeigen-serverseitig.md) | Nation (Standard an) und Verein (Standard aus) auf Anzeigen serverseitig gefiltert, URL nur abschaltend | accepted |
| [0071](0071-meetingpoint-zustand-ein-store.md) | Meetingpoint: Aufrufe und Check-ins in einem turniergebundenen Store, Parken auf dem Feld | accepted |
| [0072](0072-sr-vorab-zuteilung-vorlaeufig.md) | SR-Vorab-Zuteilung als vorläufige Reservierung, BTP-Write erst mit Feld | superseded by 0080 |
| [0073](0073-strikter-modus-zusaetzliches-gate.md) | Strikter Modus „nur spielbereite" als zusätzliches Gate der Feld-Automatik | accepted |
| [0074](0074-meetingpoint-seite-unter-namespace.md) | Meetingpoint-Seite unter `/{ns}/` mit Request/Ack, Namespace-Offenlegung akzeptiert | accepted |
| [0075](0075-bedarf-der-aufruf-automatik.md) | Aufruf-Automatik: gemeinsamer Freiwerde-Helfer, Vorrat = gerufene vergebbare Partien | accepted |
| [0078](0078-besetzung-wartender-spiele-reserviert.md) | Die Besetzung wartender Spiele reserviert ihre Schiedsrichter | accepted |
| [0079](0079-rotationsmodi-schiedsrichter.md) | Rotationsmodi: zwei Reihenfolgen in A, abgeleitete Vorrolle, eine Wahl-Funktion | accepted |
| [0080](0080-rotation-teilt-beim-aufruf-zu.md) | Feste Rotations-Zuteilung beim Aufruf mit Herkunfts-Merker (löst 0072 ab) | accepted |
| [0081](0081-tl-spielersuche-am-turnier-pc.md) | Personensuche der Turnierleitung läuft am Turnier-PC (Positivliste, Fähigkeits-Flag) | accepted |
| [0082](0082-zettel-druck-beim-aufruf.md) | Zettel-Druck beim Aufruf: SR-Sicht, Nachdruck und Druck-Gedächtnis | accepted |
| [0083](0083-stand-autoritaet-nach-fortschritt.md) | Tablet-Stand: Autorität nach Fortschritt, Host-Sätze, Quellen-Regel (ergänzt 0017) | accepted |
| [0084](0084-undo-stand-format-v2.md) | Tablet-Zustand: Undo-Format v2, Mischversionen und Deckel | accepted |
| [0085](0085-spielende-ueberholt-parken-nachzuegler.md) | Spielende: „überholt“, geparktes Ergebnis und Nachzügler-Fenster (ergänzt 0018/0014/0038) | accepted |
| [0086](0086-ein-geordneter-sender-fuer-die-badhub-strecke.md) | Ein geordneter Sender für die badhub-Strecke: Snapshot-Ablieferung, Fehlerklassen, Backoff, Halt | accepted |
| [0087](0087-voller-tset-nur-bei-strukturaenderung.md) | Voller `tset` nur bei Strukturänderung; Lebenszeichen = letzter voller `tset`; Tablet-Punkte nur über den Punktweg | accepted |
| [0088](0088-anzeigen-bei-host-verlust-karenz-dann-503.md) | Anzeigen bei Host-Verlust: 20 s Karenz ab dem letzten Lebenszeichen, dann 503; Start-Karenz, Kopplungsseite bleibt | accepted |
| [0089](0089-lebensdauer-des-namespace-zustands-ohne-host.md) | Lebensdauer des Namespace-Zustands ohne Host: Schlüssel sofort, Turnierdaten 60 min, Tablet-Stand solange ein Tablet hängt | accepted |
| [0090](0090-ueberlast-am-relay-und-mindestschutz.md) | Überlast am Relay: fachliche vs. Transport-Deckel, Mengen-Takt, volle Kanäle trennen; Mindestschutz ohne Host-Anmeldung | accepted |
| [0091](0091-ergebnisse-im-host-ein-arbeiter-je-feld.md) | Ergebnisse im Host: ein Arbeiter je Feld, Quittung nur an die Sitzung des Auftrags (berührt R5, ADR 0018) | accepted |
| [0092](0092-stream-werbung-markierung-und-abriss.md) | Stream-Werbung: Markierung als eigene Datei, `inStream` im Fingerabdruck, `streamAds` additiv, `?zweck=stream` → 503 statt leer, eigene Seite | accepted |
| [0093](0093-ballzahl-reist-mit-dem-ergebnis.md) | Ballzahl: reist mit dem Ergebnis, lebt im Tablet-Stand, BTP-Wert gilt | accepted |
| [0094](0094-zeitplan-stand-prognose-gegen-plan.md) | Zeitplan-Stand: Prognose gegen Plan je Zeitfenster (ab zwei Spielen begonnen), gerechnet am Host, Panel standardmäßig unsichtbar | accepted |
