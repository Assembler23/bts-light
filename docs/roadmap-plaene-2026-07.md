# Umsetzungspläne — Roadmap-Punkte nach dem Turnier-Wochenende (Juli 2026)

Je offener Roadmap-Punkt ein konkreter Umsetzungsplan: betroffene Dateien,
Aufwand (S/M/L), offene Entscheidungen. Grundlage: Code-Recherche vom
19.07.2026 (bts-light, badhub, Tilos Original-BTS). Priorisierung macht der
Mensch — die Reihenfolge hier ist **keine** Prio-Aussage.

> Quell-Liste: [roadmap.md](roadmap.md), Sektionen „Turnier-Wünsche
> 18./19.07." und „Nach dem Turnier-Wochenende".

---

## 1. Gezielter zweiter/dritter Aufruf — auch je Partei (Master + Slave)

**Ist-Zustand:** `PreparationCall` kennt nur `{match_id, location_id,
called_at_ms}` — **kein** Aufruf-Zähler, keine Partei-Info
(`tablet/state.rs:175`). Die Ansage spricht „In Vorbereitung → Disziplin →
Runde → Teams → Halle" (`io/announcer.ts:821` `buildPreparationSegments`).
Vom Slave zum Master existiert **kein Rückkanal**: Ansage-Slaves pollen nur
per HTTP (`relay_client.rs:401` `fetch_announce_state`), das Relay kennt
keine Slave→Host-Nachrichten.

**Plan (M, zwei Stufen):**

*Stufe 1 — Master (S):*
1. `PreparationCall` erweitern: `call_no: u8` (1/2/3) und
   `side: Option<CallSide>` (`Both | Team1 | Team2`); `add_preparation_call`
   erhöht `call_no` statt zu ersetzen.
2. `PreparationPanel.tsx`: je gerufenem Spiel Buttons „2. Aufruf",
   aufklappbar „nur {Team A}" / „nur {Team B}".
3. `announcer.ts`: neues Segment-Präfix „Zweiter Aufruf" / „Dritter und
   letzter Aufruf", bei `side` nur das fehlende Team nennen („Zweiter
   Aufruf für {Team}, bitte in {Halle}"). Azure-SSML-Variante analog.
4. Payload: `preparation_call_ts` bleibt der ERSTE Aufruf (Anzeige „vor X
   Min gerufen" springt nicht zurück); `call_no` optional mitschicken.

*Stufe 2 — Auslösung vom Slave (M, braucht Sicherheits-Entscheidung):*
1. Neuer Relay-Endpoint `POST /{ns}/slave/announce-request`
   (Body: match_id, call_no, side) → Relay reicht als neues
   `RelayFrame::AnnounceRequest` an den Host weiter.
2. Host validiert wie bei Ergebnissen (R5-Prinzip): Match muss in
   Vorbereitung/aufgerufen sein, sonst verwerfen. Der Host bleibt die
   einzige Instanz, die Ansagen tatsächlich auslöst — der Slave *bittet* nur.
3. Slave-UI: in der (künftigen) Slave-Spielübersicht (Plan 7) je Spiel der
   gleiche Aufruf-Button wie am Master.
4. **Entscheidung nötig (ADR):** Damit verlässt der Slave erstmals die
   reine Lese-Rolle (R4/R5). Vorschlag: Whitelist genau dieser einen
   Aktion, Validierung am Host, Rate-Limit im Relay.

## 2. „Nächste Spiele pro Halle"

**Ist-Zustand:** `BtpMatch` trägt `planned_time` auch für geplante Spiele
(`btp/model.rs:235`), aber `court_id` ist bei `Scheduled` typischerweise
leer — die Hallen-Info im Payload (`TsetCourt.hall`, `payload.rs:57/215`)
entsteht heute nur für Spiele, die schon **auf dem Feld** stehen; bei
Vorbereitungs-Aufrufen kommt die Halle aus dem Aufruf
(`preparation_hall`, `payload.rs:156`).

**Befund badhub (19.07.2026): Der Hallen-Filter existiert dort BEREITS.**
`/live?display=next&halle=<Name>` ist vollständig implementiert
(`public/assets/js/live.js:850` `filterUpcomingByHall`, Parameter-Parsing
`live.js:65`, Halle in der Überschrift, E2E-Test vorhanden). Er filtert auf
`upcoming_matches[].hall` — dieses Feld liefert bts-light aber heute nur
für Spiele **auf dem Feld** bzw. aus dem Vorbereitungs-Aufruf. Für „alle
angesetzten Spiele der Halle WR" fehlt allein die **senderseitige**
Hallen-Info. badhub-Code muss dafür nicht angefasst werden.

**Plan (M, fast nur bts-light):**
1. **Zuerst prüfen** (BTP-Mitschnitt vom Log-Review nutzen): Liefert
   `SENDTOURNAMENTINFO` das in BTP angesetzte Feld (Spalte „Feld"/„Spielort",
   z. B. `WR-6`) für **geplante** Matches mit? Falls ja: als
   `planned_court_id` **getrennt von `court_id`** parsen — `court_id`
   steuert `MatchStatus::OnCourt` (`model.rs:720`) und darf nicht für
   Ansetzungen missbraucht werden.
2. `payload.rs`: `upcoming`-Matches bekommen `hall` aus
   `planned_court_id → court_location_name()`; Fallback bleibt die
   Aufruf-Halle.
3. Nutzung sofort möglich: pro Halle eine eigene URL
   `…/live?t=…&display=next&halle=WR` für den Info-Monitor der Halle.
   Hinweis fürs Setup: Der next-Filter zeigt bewusst eine leere Liste,
   wenn kein Spiel der Halle ansteht (kein Fallback auf „alle" —
   gewolltes Verhalten laut badhub-Code).
4. Offline-Variante gratis dazu: `/info/preparation` (LAN-Info-Monitor)
   bekommt denselben Hallen-Filter über den bestehenden
   `?halle=`-Parameter.

## 3. Tablet: helles, akkuschonendes Styling + größere Schrift

**Ist-Zustand:** `tablet.html` ist durchgehend **dunkel und hartkodiert**
(`body background:#0f172a`, Z. 13; keine CSS-Variablen). Zentrale Größen:
Score 2.2 rem (Z. 123), Plus-Buttons 4.5 rem (Z. 82), Spielernamen
`clamp(.8–1.2rem)` (Z. 193).

**Warum das dunkle Design bei wenig Helligkeit versagt:** Auf
LCD-Tablets leuchtet das Backlight immer gleichmäßig — ein dunkles UI
lässt fast nichts davon durch. In einer hellen Sporthalle konkurriert der
dunkle Schirm zusätzlich mit Spiegelungen → die Schiedsrichter drehen die
Helligkeit hoch, und **die Helligkeit ist der Akkufresser**, nicht die
Pixelfarbe (das wäre nur bei OLED anders). Ein **helles UI kehrt das
um**: Weißer Grund nutzt das Backlight maximal aus, dieselbe Ablesbarkeit
gelingt mit deutlich niedrigerer Helligkeitsstufe.

**Plan (M):**
1. Farbwerte auf CSS-Variablen heben (`:root { --bg, --fg, --panel, … }`)
   — reine Fleißarbeit, keine Logik.
2. **Helles Maximal-Kontrast-Theme als Standard:** nahezu weißer Grund,
   nahezu schwarze Schrift (kein Mittelgrau!), Funktionsfarben als
   **großflächig gefüllte** Buttons statt farbiger Dünnschrift
   (links/rechts-Zuordnung z. B. über kräftige Rahmen + Füllung).
   Fette Schriftschnitte für Ziffern (dünne Strokes verschwimmen bei
   niedriger Helligkeit zuerst). Umschalter hell/dunkel im
   Tablet-Einstellungs-Menü, Wahl in localStorage.
3. Schriftgrößen anheben — ausdrücklich auch der **Spielstand**: aktuelle
   Score-Ziffern (heute 2.2 rem) und Satz-Historie deutlich größer,
   Namen ca. +30 %, Plus-Buttons moderat; generell alle Texte eine Stufe
   rauf. Auf 8-Zoll-Tablets gegentesten (kein Scrollen im
   Spielzustand!).
4. Abnahme-Kriterium: bei **20–30 % Display-Helligkeit** aus 1 m
   Abstand in heller Umgebung ablesbar (echtes Turnier-Tablet).
   Auslieferung wie gehabt: App-Release für LAN **und** Relay-Deploy für
   Cloud-Tablets.

## 4. „Zeit seit Aufruf" auf TVs und in bts-light

**Ist-Zustand:** Die Daten existieren komplett: `on_court_since_ms`
(1. Feld-Aufruf, `monitor.rs:176`) treibt schon die **Aufruf-Ampel** auf der
Einzelfeld-Anzeige (`monitor.html:622` `renderCallTimer`, „1. Aufruf → 2.
Aufruf → Letzter Aufruf"); `preparation_call_ts` steckt im Payload und wird
auf `/live?display=next` als „vor X Min aufgerufen" gezeigt.

**Plan (S):**
1. TV-Einzelanzeige: Chip um die absolute Zeit ergänzen („aufgerufen vor
   7 min") — nur `monitor.html`, Daten sind da.
2. Court-Übersicht (`overview.html`) + Vorbereitungs-Display
   (`preparation.html`): dieselbe Minuten-Angabe je gerufenem Spiel.
3. bts-light-App: In `PreparationPanel.tsx` und der Spielübersicht die
   Minuten seit Aufruf anzeigen (Ticker im Frontend, Zeitstempel kommen
   schon über die bestehenden Commands).
4. **Felderübersicht/Court-Übersicht `overview.html`** (Tilo 20.07.):
   die Multifeldanzeige um dieselbe „Zeit seit Aufruf" je Feld ergänzen
   — plus die **Pausenzeiten** (aus `court_state`/`pause`, die die
   Einzelanzeige schon kennt), damit die Turnierleitung Aufruf- und
   Pausenlage auf einen Blick sieht. Daten sind vorhanden; reine
   Anzeige-Erweiterung analog `renderCallTimer` (monitor.html:622).
5. **Tablet-Kopfzeile** (Tilo 20.07.): kleine „aufgerufen vor X min"-
   Angabe neben der Spieluhr am Tablet (`tablet.html`, `match-clock`-
   Bereich) — der Schiedsrichter sieht die Aufrufdauer ohne Blick auf
   TV/Backend. `on_court_since_ms` kommt bereits im MatchBrief mit.

## 5. Pausenuhr: Spielstand sichtbar lassen

**Ist-Zustand:** Die Pausenuhr **ist** bereits ein Overlay
(`monitor.html:194` `#timer-overlay`, Split-Flap-Optik) — sie liegt aber
vollflächig über dem Spielstand.

**Plan (S):** Overlay verkleinern statt neu bauen: Uhr als Banner im
oberen Drittel (oder halbtransparent), Satzstand + Namen bleiben darunter
lesbar. Nur CSS/Markup in `monitor.html`; Verhalten (`renderPauseTimer`,
Z. 786) unverändert. Mit dem echten TV-Abstand gegentesten.

## 6. HTTPS / Tablet-Akkustände auch im LAN

**Ist-Zustand bts-light:** Akku-Meldung existiert end-to-end
(`tablet.html:1208` `setupBattery` → WS `battery` → `TabletPanel.tsx:552`
`BatteryBadge` mit Farbstufen) — funktioniert aber nur, wo die Seite in
einem **Secure Context** läuft: Cloud-Tablets (https://badhub.de) ja,
LAN-Tablets (`http://IP:8088`) nein (Battery-API dort nicht verfügbar).
Fully-Kiosk-Geräte melden auch im LAN (eigene `window.fully`-API).

**Befund Original-BTS (19.07.2026):** Tilo löst das Problem **nicht**:
Default reines HTTP; HTTPS nur als „bring your own certificate"-Option
(`bts.js:104-152`, `config.json.default`), ohne Zertifikats-Erzeugung und
ohne Trust-Konzept für die Tablets; sein Installer setzt
`enable_https: false`. Seine Akku-Anzeige zeigt ohne Secure Context „N/A".

**Praxis-Hinweis von Tilo (19.07.2026):** Er hat HTTPS mit
selbstsigniertem Zertifikat betrieben und auf den Tablets im Browser
schlicht die Warnung weggeklickt („trotzdem vertrauen"). Das genügt:
Eine per Klick durchgelassene HTTPS-Seite gilt trotzdem als **Secure
Context** — Battery-API (und Wake Lock) funktionieren dann.

**Plan (Entscheidung als ADR, drei Optionen):**
- **Option A — Cloud-Weg als Akku-Kanal (S):** Tablets, die im
  LAN laufen, sind bei Turnieren mit Internet ohnehin meist parallel
  cloud-fähig (LAN+Cloud-Modus existiert). Kleinster Schritt: Doku +
  Setup-Hinweis „Akkustände brauchen den Cloud-Modus"; optional die
  Felder-Übersicht um den Hinweis „Akku n/v (LAN)" ergänzen. Kein
  Zertifikats-Betrieb, kein neuer Code im Server.
- **Option B — selbstsigniertes Zertifikat + Warnung wegklicken
  (Tilos Weg, M):** bts-light erzeugt beim ersten Start ein
  selbstsigniertes Zertifikat und bietet den Tablet-Server zusätzlich
  auf `https://…:8443` an (HTTP bleibt für Pis/Monitore bestehen).
  QR-Codes zeigen auf die HTTPS-URL; auf jedem Tablet einmal
  „Erweitert → trotzdem fortfahren". Kein CA-Handling, keine
  Trust-Store-Installation. Nachteile: abschreckende Warnung für
  Schiedsrichter beim Ersteinrichten, Ausnahme gilt je Gerät (und muss
  nach Zertifikatswechsel/IP-Wechsel neu bestätigt werden), manche
  Kiosk-Browser erlauben das Wegklicken nicht (Fully Kiosk meldet den
  Akku aber ohnehin über die eigene API).
- **Option C — echtes HTTPS mit lokaler CA (L, dauerhaft):** Eigene CA
  beim ersten Start erzeugen, Server-Zertifikat für die
  LAN-IP/`bts-light.local` ausstellen, CA-Zertifikat per QR/Download auf
  die Tablets bringen (einmalige Installation pro Gerät — im
  Verleih-Szenario realistisch, weil die Tablets mitverliehen werden).
  Aufwändig: Rotation, IP-Wechsel, Android-Trust-Store. Nur nötig, wenn
  die Warnung aus Option B im Betrieb stört.

**Entschieden (19.07.2026): Option B wird umgesetzt** —
[ADR 0005](adr/0005-lan-https-selbstsigniert.md). Umsetzung erst **nach**
dem Turnier-Wochenende, mit der übrigen Roadmap.

**Umsetzungs-Schritte Option B (M):**
1. Dependency-Check (dependency-auditor): `rcgen` (Zertifikats-Erzeugung)
   + `axum-server`/rustls-Anbindung — rustls ist über reqwest/
   tokio-tungstenite bereits im Baum.
2. Zertifikat beim ersten Start erzeugen (SANs: `bts-light.local`,
   aktuelle LAN-IPs, `localhost`; Gültigkeit lang, z. B. 10 Jahre) und im
   Config-Verzeichnis **persistieren** — die weggeklickte Warnung hängt
   am Zertifikat; ein Neustart darf sie nicht erneut auslösen.
3. Tablet-Server zusätzlich auf `:8443` (TLS) binden — gleicher
   axum-Router, HTTP `:8088` bleibt unverändert (Pis/Monitore).
4. QR-Codes/Tablet-URLs im Dashboard auf `https://…:8443` umstellen;
   Monitor-URLs bleiben HTTP.
5. Setup-Hinweis in der App + `docs/tablet.md`: Einmal-Bestätigung der
   Warnung je Tablet, Screenshot der Chrome-Dialogfolge.
6. Tests: Zertifikats-Erzeugung/-Wiederverwendung (Unit), Server startet
   mit beiden Ports; manuell: Battery-Badge erscheint für ein
   LAN-Tablet über HTTPS.
7. Windows-Firewall-Doku ergänzen (neuer Port 8443).

## 7. Spielübersicht für die Slave-Halle

**Ist-Zustand:** Der Cloud-Slave pollt heute `fetch_announce_state` und
`fetch_courts` (`relay_client.rs:401/481`) — Anzeige nur Geräte + Ansagen.
Das Relay hält aber je Feld bereits Match + Stand (`court_matches`,
`court_scores` im Namespace) und der Master pusht die Courts inkl. Halle.

**Plan (M):**
1. Relay: neuer Read-Endpoint `GET /{ns}/info/overview?hall=<name>` —
   liefert je Feld der Halle das laufende Match + Satzstand (Daten liegen
   im Namespace schon vor; nur JSON-View bauen).
2. Slave-UI: neue Seite „Spielübersicht" (React), pollt den Endpoint alle
   paar Sekunden — gleiche Optik wie die Master-Felderübersicht,
   read-only.
3. Anstehende Spiele der Halle dazu: kommt aus Plan 2 (Hallen-Filter im
   `upcoming`-Payload) — Reihenfolge daher: erst Plan 2, dann hier
   erweitern.
4. Aufruf-Buttons auf dieser Seite = Plan 1 Stufe 2.

## 8. Kopplungscode: 1 h Gültigkeit

**Plan (XS):** In PR #78 die Konstante der Code-Gültigkeit von 15 min auf
60 min setzen (+ Test + `docs/`-Erwähnung anpassen). Beim Rebase der
Release-Kette miterledigen.

## 9. Spielerprofil-Links auf `/live` teils defekt (badhub-Repo)

**Analyse-Ergebnis (19.07.2026):** Die Verlinkung ist **rein ID-basiert**
(BTS-Member-ID = `players.nu_licence_nr`), Namen/Umlaute spielen keine
Rolle. Ablauf: `live.js` sammelt Member-IDs → Batch-Auflösung über
`public/api/live_profile_resolve.php` → Link nur bei Treffer, sonst
Plain-Text. Ursachen fehlender Links, nach Wahrscheinlichkeit:

1. **Spieler ohne Member-ID im Push** (Gastspieler / in BTP ohne
   MemberID erfasst) — Auflösung findet gar nicht statt. Häufigster Fall,
   kein Bug.
2. **Spieler nicht in `players` importiert** (Verbands-Import deckt ihn
   nicht ab) — Auflösung liefert null.
3. **Klarer, gezielt behebbarer Bug — Normalisierungs-Asymmetrie:** Der
   Resolve-Endpoint validiert hart per Regex `^\d{1,4}-\d{1,8}$`
   (`live_profile_resolve.php:56`) und verwirft damit **A-Präfix-Lizenzen**
   (`A08-016853`) und **NU-IDs** (`NU895267`) — die Profilseite
   `spieler.php` löst genau diese über `normalizeLicenceNr()` aber auf.
   Ergebnis: Profil existiert, Link entsteht trotzdem nie → das erklärt
   „manche gehen, manche nicht".
4. **Mehrfach-Lizenz in mehreren Verbänden:** Resolve nimmt den ersten
   Treffer und kann auf den falschen Verband verlinken (dort 404).

**Fix-Plan (S, badhub-Repo):** `normalizeLicenceNr()` auch im
Resolve-Endpoint anwenden + Validierungs-Regex entsprechend lockern
(Punkt 3); danach mit echten Turnierdaten prüfen, wie viel von den
Restfällen (1)/(2) übrig bleibt. Punkt 4 nur angehen, falls real
beobachtet (z. B. Verband des Turniers bevorzugen).

## 10. TV-Leerlauf-Anzeige: große Feldnummer + badhub.de-Branding

**Wunsch (19.07.2026):** Steht auf einem Feld kein Spiel (typisch in der
Slave-Halle zwischen den Runden), soll der TV die **Feldnummer groß und
gut sichtbar** zeigen — und darunter groß **badhub.de** (doppelter
Nutzen: Orientierung in der Halle + Werbung).

**Plan (S):** Leerlauf-Zustand in `monitor.html` gestalten (der Zustand
„kein Match zugewiesen" existiert dort bereits, ist aber unscheinbar):
Feldnummer bzw. -name in sehr großer Schrift (vmin-basiert wie die
Spielernamen), darunter „badhub.de" als große Wortmarke; dezente Optik
passend zum Score-Design. Gilt automatisch für LAN- und Cloud-TVs
(gleiche Datei, Auslieferung wie gehabt App-Release + Relay-Deploy).
Gegencheck: Nach Spielende/Feldfreigabe muss der Wechsel zurück in den
Leerlauf sauber aussehen (kein Flackern mit der Ergebnis-Anzeige).

## 11. BTP-Rückschreibung: Übernahmen aus Tilos Original-BTS

**Analyse 19.07.2026** (vollständiger Vergleich:
[btp-write-vergleich-letilo.md](btp-write-vergleich-letilo.md)). Beide
Systeme sind seit 0.9.147 im Kern gleichwertig; drei Übernahmen lohnen:

- **P1 (S/M): `Highlight` für Vorbereitungs-Aufrufe** — Tilos Aufrufe
  sind in BTP sichtbar (Match-Feld `Highlight`), unsere nicht. Beim
  Aufruf Highlight:1, bei Rücknahme/Feld-Ruf Highlight:0 schreiben —
  ohne `Status`-Feld (v0.9.103-Falle). Turnierleitung sieht Aufrufe dann
  direkt in BTP.
- **P2 (M): Retry-Queue für Ergebnis-Writes** — von BTP nicht bestätigte
  Ergebnisse beim nächsten Kontakt nachschieben (Tilos
  `needsync`/`pushall`); Players-Checkout nur binnen 5 min.
- **P3 (S): Disqualifikation (`ScoreStatus 3`)** als dritte Option im
  Turnierleitungs-Dialog.
- Zurückgestellt: Check-in-Bits, Officials/Umpire, Shuttles, MatchOrder,
  eigenständige Spieler-Updates (Begründung im Vergleichs-Dokument).

## 12. Spielstand direkt eintragen (Tablet + Turnierleitung)

**Wunsch (19.07., Nutzer + Tilo):** Endstand eintippen, wenn niemand
live gezählt hat — und: findet sich mitten im Spiel doch ein Zähler,
trägt der den **Zwischenstand** ein und zählt ab da normal weiter. Der
Button ist **offen sichtbar** (Spieler müssen ihn zur Not selbst
bedienen können).

**Befund Tilos BTS:** Beides ist dort Funktion des externen
bup-Umpire-Panels („Edit mode"); der Server übernimmt bei jedem
score_update den kompletten Zustand **ungeprüft** (keine
Satzplausibilität) und schreibt über denselben BTP-Weg. Unser Ansatz ist
gleichwertig, aber strenger: die serverseitige `process_result`-
Validierung (R5) bleibt auch für die Direkteingabe aktiv.

**Plan (M):**
- *(a) Endstand am Tablet (S):* Button „Spielstand direkt eintragen" auf
  dem Match-Screen (UI-Vorlage `openFinishModal`, tablet.html:2059).
  Dialog mit zwei Zahlenfeldern je Satz; Client-Plausibilität über die
  vorhandene Satzregel `setWinnerSide` (tablet.html:1253, target/cap aus
  MatchBrief). Absenden über den **bestehenden** Weg (`pendingResult` →
  `trySubmitPending()` → POST /result → `process_result`) — kein neuer
  Server-Code.
- *(a2) Endstand aus der Turnierleitung (S/M):* Neuer Command
  `enter_result(match_id, sets)` nach dem Muster `confirm_walkover`
  (commands.rs:911); gleiche Plausibilitätsprüfung wie `process_result`
  (Prüf-Logik in gemeinsame Funktion auslagern). Dialog in der
  Spielübersicht, Optik wie WalkoverPanel.tsx. Deckt Spiele ab, die nie
  auf einem Feld standen.
- *(b) Zwischenstand am Tablet + weiterzählen (M):* Im selben Dialog
  Umschalter „Spiel läuft noch": abgeschlossene Sätze + aktueller
  Punktestand + Satz eingeben, danach die **bestehenden**
  Setup-Schritte `chooseSide` → `chooseServer` → (Doppel)
  `chooseReceiver` → `finalizeSetup` (tablet.html:1759) — die leiten
  Positionen/Aufschlagfeld bereits regelkonform aus der Score-Parität
  ab (BWF-Regel; fachliche Vorgabe: mehr als Aufschläger/Rückschläger/
  Satz muss niemand angeben). STATE-Befüllung per
  `applyPersistedState`-Muster; `intervalDoneThisGame`/
  `midGameSwitchDone` aus dem Stand ableiten. Ab da normaler Zähl-Flow.
- Doku: docs/tablet.md, changelog; Rust-Tests für die geteilte
  Plausibilitäts-Funktion. Auslieferung: App-Release + Relay-Deploy.

## 13. Klick-Delay am Tablet verkürzen

**Wunsch (19.07.):** Die Verzögerung zwischen Tipp auf +1 und sichtbarer
Punkteänderung ist zu lang. **Befund:** `touch-action: manipulation` ist
bereits gesetzt (kein 300-ms-Browser-Delay), aber die Plus-/Undo-Buttons
hören auf `click` (tablet.html:2882-2884) — das feuert erst beim
**Loslassen** des Fingers; zusätzlich laufen Voll-Render + `persistState`
(JSON + localStorage + WS) synchron im Tap-Pfad.

**Plan (S):**
1. Score-kritische Buttons auf `pointerdown` umstellen (Punkt zählt bei
   Berührung); Guard gegen nachlaufendes click-Doppelfeuer,
   `pointercancel` beachten. Modals/Einstellungen bleiben auf click.
2. Im Tap-Pfad nur STATE + minimales Score-DOM-Update sofort;
   `persistState()`/`sendScoreUpdate()` per `queueMicrotask` direkt
   danach (Persistenz-Garantie bleibt im selben Task-Umlauf).
3. Vorher/nachher am echten Turnier-Tablet messen (tlog-Timing).
4. Auslieferung: App-Release + Relay-Deploy (tablet.html doppelt).

## 14. Zähltafelbediener (Tabletoperator)

**Wunsch (19.07.):** Zähltafelbediener-Verwaltung wie in Tilos BTS.

**So macht es Tilos BTS (Recherche 19.07.):** Nach jedem regulär
beendeten Spiel kommt der **Verlierer** automatisch in eine
FIFO-Warteschlange (Option: ab Viertelfinale der Gewinner; Doppel
optional gesplittet; Walkover/Aufgabe erzeugen keinen Eintrag). Beim
Feld-Aufruf wird der am längsten Wartende dem Match zugewiesen —
bevorzugt auf dem Feld, auf dem er selbst gespielt hat. **Er wird mit
angesagt** („Tabletbedienung: {Name}"), es gibt eigene
Zweitaufruf-Ansagen. Absicherung: Bei Zuweisung wird er in BTP
**ausgecheckt** (kann nicht parallel eingeplant werden), nach dem Dienst
optional garantierte Mindestpause (Default 300 s). Verwaltung über eine
Warteliste im Admin (vorziehen/zurückstellen/entfernen/manuell
hinzufügen); auf den Court-Displays erscheint er nicht.

**Plan für bts-light (L — eigenes Feature, eigene docs/*.md):**
1. Warteschlange im Rust-State: Verlierer nach erfolgreichem
   `process_result` einreihen (Walkover ausgenommen), FIFO + manuelle
   Pflege-Commands.
2. Zuweisung beim Feld-Aufruf (`sync.rs auto_assign` + manuelles
   `assign_court`); bevorzugt aufs zuletzt gespielte Feld.
3. Ansage-Segment „Tabletbedienung: {Name}" (Feld- und
   Vorbereitungs-Ansage, announcer.ts) + Zweitaufruf-Button —
   verzahnt mit Plan 1 („2. Aufruf je Partei").
4. UI: Warteliste-Panel in der App; Anzeige am Match in der
   Felder-Übersicht.
5. BTP: Operator beim Spielende mit in den Players-Block; das
   Auschecken bei Zuweisung braucht Tilos eigenständiges
   Spieler-Update — der im BTP-Vergleich (Plan 11) zurückgestellte
   Punkt wird damit Bestandteil dieses Features.
6. Konfiguration minimal: Schalter „Zähltafelbediener verwalten" +
   Mindestpause-Sekunden; Tilos Spezialoptionen erst bei Bedarf.

## 15. Gong überlappt das erste Ansage-Wort

**Tilo-Befund 19.07.:** „Ansageton kommt manchmal zu spät und dann im
ersten Wort." **Ursache** (`io/announcer.ts`): Der Gong läuft auf der
Web-Audio-Uhr, sein Ende wird aber per festem `setTimeout` (1250/850 ms,
announcer.ts:86/115) signalisiert. Startet der AudioContext verzögert
(WebView2-Resume), klingt der Gong real später als der Timer → die
Sprache setzt in den Nachklang ein. Master, Announcer und Cloud-Slave
teilen den Code — ein Fix wirkt überall. (Tilos BTS hat keinen Gong,
verkettet aber seine Sprach-Teile über echte `onend`-Events — gleiches
Prinzip.)

**Plan (S):** `playGong()` am echten Audio-Ende auflösen (`onended` des
letzten OscillatorNode statt setTimeout); `ctx.resume()` vollständig
abwarten, bevor auf `ctx.currentTime` geplant wird; ~150 ms Atempause
zwischen Gong-Ende und TTS. Manuell auf Windows-WebView2 testen.

## 16. Matchball-Einfärbung in der Felderübersicht (nur Turnierleitung)

**Tilo-Idee 19.07.**, Scope-Entscheidung 20.07.: **nur** die
bts-light-Felderübersicht (Planung des nächsten Spiels), nicht die
Hallen-TVs. Bei Tilo existiert das Konzept nicht — eigener Entwurf.

**Plan (S):** Die Satzball/Matchball-Regel existiert bereits im Tablet
(`umpPointBadge`, tablet.html:2708: Führender ≥ target−1 und ≥1 vorn;
Matchball, wenn damit der entscheidende Satz fällt). `CourtOverview`
(state.rs:68) um `best_of`/`target_score`/`cap_score` ergänzen (reine
Tauri-Struktur, keine Wire-Kompatibilität nötig); in `TabletPanel.tsx`
die Regel als kleine TS-Funktion portieren und die Court-Karte
einfärben (Vorschlag: Gelb = Satzball, Rot/pulsierend + Badge =
Matchball — Farben bei Umsetzung abstimmen).

## 17. Altes Ergebnis am Feld bei Neu-Zuweisung (echte Lücke)

**Tilo-Befund 19.07.** („beim Neu-Zuweisen steht noch das alte Ergebnis
dran, erst beim Start springt es um") + identischer Live-Beleg HM-03 im
[Log-Review](turnier-log-review-2026-07.md). **Ursache:** Die
Reset-Guards existieren größtenteils (Relay leert `court_scores`/
`court_state` beim Match-Wechsel, main.rs:1449; Anzeigen nutzen
Tablet-Sätze nur bei passender `session.match_id`, state.rs:889/1018) —
aber ein Tablet, das noch im **alten** Match hängt (Doze/Reconnect),
sendet nach dem Aufwachen `score_update`/`state_sync` **ohne Match-ID**
und befüllt den frisch geleerten Cache wieder mit dem alten Stand.
Tilos BTS bestätigt den Fix-Ansatz: Er verwirft Score-Updates von
„stale panels", deren Match nicht mehr zum Court passt.

**Plan (S/M):**
1. `TabletMsg::ScoreUpdate` um `matchId` erweitern (relay-proto,
   `#[serde(default)]` → alte Seiten kompatibel); tablet.html sendet die
   Match-ID des gezählten Spiels mit.
2. Server (`handle_score`) und Relay (`forward_score`/
   `store_court_state`): Frames verwerfen, deren Match-ID nicht zum
   aktuellen Court-Match passt (beim `state_sync` steckt die Match-ID
   schon im State-JSON — parsen). Leere ID (alte Seiten) = Verhalten
   wie heute.
3. Ergibt: Neu-Zuweisung zeigt sofort 0:0/BTP-Stand; Nachzügler alter
   Matches können nichts mehr überschreiben (komplettiert den
   „nachlaufende Frames"-Fix aus 0.9.147). Erledigt zugleich den
   Log-Review-Auftrag „Score-Cache-Reset bei Match-Wechsel".
4. Tests: Relay „fremde matchId verworfen", state.rs analog,
   Serde-Roundtrip ±matchId. Auslieferung: App + Relay-Deploy.

## 18. Release-Seite: Versions-Downloads + Kompakt-Changelog

**Wunsch (20.07.2026):** Eine öffentliche Seite, auf der man jede
Version herunterladen kann und je Version die Änderungen kompakt sieht.

**Lage:** Beide Zutaten existieren schon — der Download-Bereich
`badhub.de/download/bts-light/` behält alle Versions-Installer
(lückenlos seit 0.4.6), und [changelog.md](changelog.md) pflegt die
Kompakt-Änderungen je Version (Commit-Pflicht laut CLAUDE.md). Es fehlt
nur die öffentliche Darstellung. Verwandter Alt-Punkt „Changelog pro
Version sichtbar machen" (roadmap.md → Geplant) geht hierin auf.

**Plan (S/M):**
1. **Generator im Release-Workflow:** Beim Tag-Release baut ein
   Script (Node/Python im Workflow) aus `docs/changelog.md` eine
   statische `index.html` für `download/bts-light/`: Tabelle je Version
   mit Datum, Download-Link (`BTS.Light_X.Y.Z_x64-setup.exe`) und den
   Changelog-Stichpunkten; neueste Version prominent oben („Aktuelle
   Version"). Upload zusammen mit Installer + latest.json (bestehender
   Deploy-Schritt).
2. **Auto-Update-Notes:** Denselben Changelog-Auszug der Version in
   `latest.json → notes` schreiben — das Update-Fenster in der App
   zeigt dann „Was ist neu" (erledigt den Alt-Punkt mit).
3. Alte Versionen: Liste aus den vorhandenen Exe-Dateien im
   Download-Verzeichnis generieren (Server-seitig einmalig erfasst oder
   im Workflow gepflegt); Versionen ohne Changelog-Eintrag nur mit
   Datum/Link.
4. Doku: docs/release.md (neuer Abschnitt „Release-Seite"), Test =
   Trockenlauf des Generators in CI (HTML entsteht, Links valide).
5. Hinweis: `public/download/` ist vom badhub-rsync ausgenommen — die
   Seite lebt wie die Exes nur auf dem Server; Deploy ausschließlich
   über den bts-light-Release-Workflow.

## 20. Feldnummer am Tablet prominent — auch vor Spielstart (Tilo 20.07.)

**Wunsch:** Tilo hatte zu Turnierbeginn 11 Spiele über die
Turnierleitung den Tablets zugeordnet und konnte danach nicht mehr
erkennen, welches Tablet an welchem Feld steht. Die Feldnummer steht am
Tablet nur klein in der Kopfzeile (`.court-label`, tablet.html:428/57)
und ist vor dem Spielstart neben „— kein Match —" leicht zu übersehen.

**Ist-Zustand:** `__COURT_LABEL__` wird serverseitig in die Kopfzeile
und den Einstellungsdialog ersetzt (tablet.html:428/679). Der
Leerlauf-/Wartezustand („kein Match") zeigt sonst nichts Großes.

**Plan (S, nur `tablet.html` — LAN + Cloud teilen die Datei):**
1. **Leerlauf-Großanzeige:** Solange dem Feld kein Match zugewiesen ist
   (`event-label` = „— kein Match —"), die **Feldnummer groß und
   zentral** im Zähltafel-Bereich einblenden (vmin-basiert wie die
   TV-Leerlaufanzeige, Plan 10) — darunter dezent „bereit". So ist aus
   Armlänge sofort erkennbar, welches Tablet welches Feld bedient.
   Sobald ein Match kommt, weicht die Großanzeige der normalen
   Zähltafel.
2. **Kopfzeile schärfen:** `.court-label` etwas größer/kontrastreicher,
   damit die Feldnummer auch während des Spiels ablesbar bleibt
   (verzahnt mit Plan 3 „größere Schrift").
3. Keine Server-/Protokolländerung; rein clientseitig. Test manuell am
   Tablet (LAN + Cloud). Auslieferung: App-Release + Relay-Deploy
   (tablet.html liegt doppelt).

Cluster D. Klein und risikoarm — guter Kandidat, um mit Plan 3 (helles
Theme + Schrift) gebündelt umgesetzt zu werden.

## 19. Übrige Punkte (bereits geplant/laufend)

- **Log-Review 20.07.2026** — Ablauf steht in
  [roadmap.md](roadmap.md#nach-dem-turnier-wochenende-stand-19072026).
- **Offizielles Release + Server-Cleanup + Azure-Key-Rotation** — Ablauf
  ebd.; kein eigener Plan nötig.
- **Master-Identität umziehen**, **Pi-Image-Untersuchung** — eigene
  Untersuchungen, Pläne folgen nach dem Log-Review (dessen Erkenntnisse
  fließen ein).
