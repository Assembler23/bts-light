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

## 11. Übrige Punkte (bereits geplant/laufend)

- **Log-Review 20.07.2026** — Ablauf steht in
  [roadmap.md](roadmap.md#nach-dem-turnier-wochenende-stand-19072026).
- **Offizielles Release + Server-Cleanup + Azure-Key-Rotation** — Ablauf
  ebd.; kein eigener Plan nötig.
- **Master-Identität umziehen**, **Pi-Image-Untersuchung** — eigene
  Untersuchungen, Pläne folgen nach dem Log-Review (dessen Erkenntnisse
  fließen ein).
