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
| [0016](0016-monitor-push-transport.md) | Monitor-Push-Transport: WebSocket-Nudge statt SSE oder Poll | proposed |
| [0017](0017-reconnect-ownership.md) | Reconnect-Konfliktmodell: Ownership „Slot-Halter gewinnt" statt rev-Zähler | proposed |
| [0018](0018-ergebnis-weg-verlustsicher.md) | Ergebnis-Weg verlustsicher: Idempotenz, persistente Retry-Queue | proposed |
| [0019](0019-relay-last-soak-inprocess.md) | Relay-Last-/Soak-Test: In-Process, Multi-Thread, ehrliche Abgrenzung | proposed |
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
| [0034](0034-tl-web-push-transport.md) | TL-Web-Push: WebSocket-Nudge mit In-Band-Auth | proposed |
| [0035](0035-monitor-livestand-ordnung.md) | Monitor-Livestand: schmaler Abruf, Ordnung über `seq`, additive Nudges | proposed |
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
