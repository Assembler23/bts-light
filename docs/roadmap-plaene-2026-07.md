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

**Plan (M):**
1. Farbwerte auf CSS-Variablen heben (`:root { --bg, --fg, --panel, … }`)
   — reine Fleißarbeit, keine Logik.
2. **Helles Theme als Standard** (dunkle Schrift auf hellem Grund ist bei
   minimaler Display-Helligkeit deutlich besser ablesbar und spart auf
   LCD-Tablets Energie, weil das Backlight runtergedreht werden kann);
   Umschalter im Tablet-Einstellungs-Menü (hell/dunkel), Wahl in
   localStorage.
3. Schriftgrößen anheben: Score-Ziffern und Namen ca. +30 %, Plus-Buttons
   moderat — auf 8-Zoll-Tablets gegentesten (kein Scrollen im
   Spielzustand!).
4. Kontrast-Check bei niedrigster Helligkeit am echten Turnier-Tablet.
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

Empfehlung: **A sofort dokumentieren, B als Umsetzung** (bestätigte
Praxis aus Tilos Betrieb), C nur bei Bedarf.

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

## 10. Übrige Punkte (bereits geplant/laufend)

- **Log-Review 20.07.2026** — Ablauf steht in
  [roadmap.md](roadmap.md#nach-dem-turnier-wochenende-stand-19072026).
- **Offizielles Release + Server-Cleanup + Azure-Key-Rotation** — Ablauf
  ebd.; kein eigener Plan nötig.
- **Master-Identität umziehen**, **Pi-Image-Untersuchung** — eigene
  Untersuchungen, Pläne folgen nach dem Log-Review (dessen Erkenntnisse
  fließen ein).
