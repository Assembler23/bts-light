# Roadmap & offene Punkte

Lebende Liste der offenen Arbeiten an bts-light. Erledigte Versionen stehen
im [changelog.md](changelog.md); hier steht, was **noch** ansteht.

> Stand: 2026-07-17, nach dem ersten Zwei-Hallen-Praxisturnier (v0.9.144).
> Die Prio-1-Punkte und Turnier-Wünsche stammen direkt aus diesem Einsatz.

## Prio 1 — Lehren aus dem Zwei-Hallen-Turnier (17.07.2026)

- **Turnier-Robustheit & Echtzeit-Latenz (Cluster)** *(Spec abgestimmt
  2026-08-13)*: Übersicht [features/turnier-robustheit-cluster.md](features/turnier-robustheit-cluster.md).
  Vier Hebel — **A** Echtzeit-Robustheit der Score-Strecke (niedrig-latente
  Anzeige per Push + Reconnect-Wahrheit „Slot-Halter gewinnt";
  [features/turnier-robustheit.md](features/turnier-robustheit.md), ADR 0016/0017),
  **B** lokaler Ergebnis-Puffer (Cloud), **C** Last-/Soak-Test (36 Geräte),
  **D** Tote-Verbindungs-Erkennung schärfen. Paket A ist spezifiziert und
  freigabebereit; B–D bekommen eigene Specs.
- **BTP-Ergebnis-Regression** *(Fix implementiert, wartet auf Release
  v0.9.145)*: Spiele wurden in BTP nicht automatisch beendet — `Status`
  fehlte seit v0.9.103 im Ergebnis-`SENDUPDATE`; zudem Ergebnis +
  Feldfreigabe jetzt in einem Request. **Vor dem Release am echten BTP
  gegenprüfen** (Spiel schließt automatisch; Aufgabe/Walkover; Check-in
  bei Feldzuweisung unverändert). Details: [btp_protocol.md](btp_protocol.md).
- **Master-Identität umziehen.** Ein Rechnertausch erzeugt eine neue
  `install_id` → alle gekoppelten Geräte (Slave, Pis, Tablets, TVs)
  verlieren still die Verbindung (Hauptursache des Turniertag-Chaos).
  Geführter Config-Export/-Import bzw. Identitäts-Übernahme im SetupWizard
  + Dashboard-Warnung, wenn bekannte Monitore länger offline sind.
  Seit der Zombie-Host-Ablösung (Cluster A3) zusätzlich die eigentliche
  Gegenmaßnahme bei geleakter `install_id` (Sicherheits-Abwägung in
  [cloud-relay.md](cloud-relay.md)).
- **Host-Ablösung sichtbar machen.** Wird der Host-Slot per
  Zombie-Ablösung übernommen (Cluster A3), sieht die Turnierleitung das
  heute nur im Relay-Log. Wunsch aus dem A3-Review: eine sichtbare
  Warnung in der App („dein Host-Slot wurde übernommen"), damit eine
  echte Fremd-Übernahme auffiele.
- **Slave-PC als eingebaute Monitor-Brücke.** bts-light im Cloud-Slave-Modus
  soll selbst auf `:8088` lauschen (`/health` + Redirect `/monitor[?device=…]`
  auf den Cloud-Monitor des Masters) — dann laufen die Bestands-Pis
  (Tilos Image, Subnetz-Scan) in der fernen Halle ohne Zusatz-Skripte.
  Ersetzt die Turnier-Notlösung (`pi-bridge`-Skripte auf Mac/Windows).

## Turnier-Wünsche (17.07.2026)

- ~~**Court-Monitor: Spielernamen deutlich größer**~~ → umgesetzt in
  v0.9.145 (`assets/monitor.html`; Cloud-TVs per Relay-Deploy).
- ~~**Ansage nennt die Klasse** („Herreneinzel A")~~ → umgesetzt in
  v0.9.145 (`model::class_label`, Details [announcements.md](announcements.md)).
- **Spielübersicht für die Slave-Halle**: laufende/anstehende Spiele der
  eigenen Halle am Slave sehen (Datenquelle: Relay), nicht nur
  Geräte-Anschluss + Ansagen.
- **„Spiele in Vorbereitung" vom Slave (erneut) aufrufen**: Rückkanal über
  den Relay nötig — Sicherheitsmodell beachten (Slave ist bisher bewusst
  read-only, R4/R5).

## Turnier-Wünsche (18./19.07.2026 — zweites Wochenende)

Aus dem laufenden Betrieb notiert (Turnierleitung + Beobachtungen).
**Umsetzungspläne je Punkt:** [roadmap-plaene-2026-07.md](roadmap-plaene-2026-07.md).

- **Gezielter zweiter/dritter Aufruf — auch je Partei.** Ist ein Spiel
  aufgerufen, aber nur eine Seite erschienen, soll die Turnierleitung
  einen **zweiten Aufruf nur für die fehlende Partei** auslösen können
  (Ansage z. B. „Zweiter Aufruf für …"). Gewünscht auf dem **Master und
  vom Slave aus** — hängt am selben Relay-Rückkanal wie der
  Vorbereitungs-Aufruf vom Slave (siehe oben, R4/R5 beachten).
- **„Nächste Spiele pro Halle"** (Idee von Nik, Turnierleitung): Eine
  Aufruf-/Nächste-Spiele-Liste **je Halle**. Der Hallen-Filter `&halle=…`
  existiert auf `badhub.de/live?display=next` bereits — es fehlt die
  senderseitige Hallen-Info an den **angesetzten** Spielen.
  ⚠️ *Nachgemessen 08.08.:* Die ursprüngliche Annahme („BTP führt den
  Spielort bereits an der Ansetzung, das kommt per `SENDTOURNAMENTINFO`")
  **trägt nicht**. In zwei echten Mitschnitten (914 Paarungen) hat ein noch
  nicht aufgerufenes `Match` **keine** `LocationID`; `CourtID` erscheint
  erst beim Aufruf, und `Draw`/`Event`/`Stage` tragen gar keinen Ortsbezug.
  Die Spalten „Feld"/„Spielort" **gibt es** im BTP-Spielplan-Export, im
  geprüften Turnier sind sie aber in allen 540 Zeilen leer. **Offen bleibt
  daher nur:** ob ein Turnier, das diese Spalten *pflegt*, sie auch über
  die Schnittstelle liefert — das lässt sich erst mit einem Mitschnitt
  eines solchen Turniers beantworten. Befund:
  [btp_protocol.md](btp_protocol.md), Regressionstest in `btp_capture.rs`.
  **✅ Auf anderem Weg gelöst (09.08.):** Die Turnierleitungs-Seite gibt
  einem wartenden Spiel den Spielort **von Hand** (Hallen-Wähler an der
  Zeile, `TlAction::SetHall`). Er wirkt auf Hallenfilter, Vergabe **und**
  `upcoming_matches[].hall` — damit greift `display=next&halle=…` auch in
  Turnieren, die ihre Aufrufe über BTP machen. Die Kaskade lautet jetzt
  Disziplin-Regel → Hand → Vorbereitungs-Aufruf (`assign::hall_for_match`).
  Siehe [turnierleitung-web.md](turnierleitung-web.md). Details:
  [roadmap-plaene-2026-07.md](roadmap-plaene-2026-07.md).
  **Schreibversuch gemessen (10.08.2026):** Ein `SENDUPDATE` mit
  `LocationID` wird von BTP mit `Result=1` beantwortet, der Wert aber
  verworfen — Rückschreibung nach BTP ist damit unmöglich, `SetHall`
  bleibt bewusst host-lokal. Befund: [btp_protocol.md](btp_protocol.md).
- **Tablet: helles, akkuschonendes Styling.** Das dunkle Design zwingt die
  Schiedsrichter, die Display-Helligkeit hochzudrehen → Akkus leeren sich
  schneller. Ziel: helles Theme bzw. ein Kontrast-Styling, das auch bei
  **minimaler Helligkeit** klar ablesbar ist.
- **Tablet-Schrift größer** — ausdrücklich auch der **Spielstand** und die
  Texte allgemein (analog zur TV-Vergrößerung aus v0.9.145).
- **TV-Leerlauf: Feldnummer groß + badhub.de-Branding.** Ohne laufendes
  Spiel (z. B. Slave-Halle zwischen den Runden) soll der TV die Feldnummer
  prominent zeigen, darunter groß „badhub.de" (Orientierung + Werbung).
- **Spielstand direkt eintragen (Tablet + Turnierleitung).** Endstand
  eintippen, wenn niemand gezählt hat; Zwischenstand eintragen und ab da
  live weiterzählen, wenn ein Zähler verspätet einsteigt (nur Aufschläger,
  im Doppel Rückschläger, plus Satz nötig — Positionen folgen der
  BWF-Paritätsregel). Button offen sichtbar.
- ~~**Klick-Delay am Tablet verkürzen.**~~ **Erledigt.** `pointerdown` und
  der verschobene Persist/Sync kamen mit Plan 13; die eigentliche Bremse war
  die 3-Sekunden-Sperre nach jedem Punkt (Schutz gegen Doppel-Taps) — seit
  09.08.2026 sind es 0,7 s.
- **Zähltafelbediener-Verwaltung** (wie Tilos BTS): Verlierer-Warteschlange,
  Zuweisung beim Feld-Aufruf, Mit-Ansage „Tabletbedienung: …",
  BTP-Auscheck, Mindestpause.
- **Vorbereitungs-/next-Monitor je Halle zeigte keine Spiele**
  (Turnier-Befund 19.07., nachgemeldet 20.07.): Der Browser-Monitor
  `…display=next&halle=…` blieb leer. **Diagnose:** `upcoming_matches[].hall`
  wird heute NUR gefüllt, wenn der Aufruf über bts-lights
  „Spiele in Vorbereitung" läuft (`preparation_hall`, payload.rs:156) —
  beim Turnier liefen die Aufrufe aber über BTP/mündlich → Hallen-Feld
  überall leer → der (funktionierende) badhub-Filter fand nichts, und
  die leere Liste ohne Fallback ist dort gewollt. **Dreiteiliger Fix:**
  (a) ~~Plan 2 — `planned_court_id` aus BTP parsen~~ → **nicht möglich**,
  BTP liefert an ungespielten Matches keinen Ort; **stattdessen erledigt**
  über den Hallen-Wähler der Turnierleitungs-Seite (siehe Punkt „Nächste
  Spiele pro Halle" oben); (b) P1 erweitern — BTP-`Highlight` nicht nur
  schreiben, sondern auch **lesen**, damit in BTP gemachte Aufrufe bei
  uns als „gerufen" erscheinen; (c) beim Umsetzen prüfen, wie das
  Original-BTS seine „upcoming"-Ticker-Anzeige speist
  (ticker_manager/highlight) — ggf. weitere Mechanik übernehmen.
- ~~**Matchball-Einfärbung in der Felderübersicht** (Tilo-Idee, nur
  Turnierleitung) — Plan 16.~~ → umgesetzt 2026-08-10 (App-Felderübersicht
  bereits zuvor; TL-Web-Abzeichen mit diesem Commit).
- **Altes Ergebnis bei Neu-Zuweisung** (Tilo + Log-Review HM-03):
  Match-ID in Score-Frames + Server-Filter gegen veraltete
  Tablet-Stände — Plan 17 (ersetzt den Log-Review-Punkt
  „Score-Cache-Reset").
- **badhub `/live?tab=done`: Tages-Filter** (Wunsch 19.07.), initial auf
  den aktuellen Tag. Die Beendet-Einträge tragen bereits `end_ts` →
  reines Frontend im badhub-Repo (`live.js`): nach Tag gruppieren/filtern,
  kleines Tages-Dropdown. **Achtung Befund 19.07.:** Nach einem
  App-Neustart stempelt bts-light ALLE schon beendeten Spiele mit
  frischem `end_ts` → für den Tages-Filter Zeitquelle prüfen/festigen.
- **Beendet-Liste: Aufgabe/kampflos kennzeichnen** (Befund 19.07.).
  In BTP direkt gewertete Aufgaben erscheinen im Ticker als „beendet"
  mit Teil-Spielstand (z. B. 14:16, 15:10, 0:0) und wirken fehlerhaft.
  Fix: `score_status` (Aufgabe/Walkover) aus dem BTP-Snapshot in die
  `recent_finished`-Einträge des Payloads übernehmen (bts-light) und im
  Ticker als Badge „Aufgabe"/„kampflos" anzeigen (badhub `live.js`).
- *Nice-to-have:* **Zeit seit Aufruf** auf den TVs **und** in bts-light
  anzeigen (die Aufruf-Uhr existiert am Cloud-Monitor bereits als
  Datenquelle: `on_court_since`/Aufruf-Zeitstempel).
- *Nice-to-have:* **Pausenuhr als Overlay.** Die Pausenuhr auf den TVs ist
  gut — der Spielstand soll dabei aber sichtbar bleiben (Overlay statt
  Vollbild-Wechsel).
- **Analyse (badhub-Repo): Spielerprofil-Links auf `/live` teils defekt.**
  Die Links auf Spielerprofile funktionierten schon einmal; aktuell gehen
  einige, andere nicht — Ursache klären (Namens-Matching?).
- **BTP-Rückschreibung: Übernahmen aus Tilos Original-BTS** (Analyse
  19.07., [btp-write-vergleich-letilo.md](btp-write-vergleich-letilo.md)):
  Aufrufe als `Highlight` nach BTP melden, Retry-Queue für nicht
  bestätigte Ergebnisse, Disqualifikations-Code — Pläne in
  [roadmap-plaene-2026-07.md](roadmap-plaene-2026-07.md), Punkt 11.
- **HTTPS für den LAN-Tablet-Server — Akkustände auch im LAN sehen.**
  Browser geben die Battery-API (`navigator.getBattery`) nur in
  **sicheren Kontexten** frei: Cloud-Tablets (https via badhub.de) melden
  ihren Akkustand an die Felder-Übersicht, LAN-Tablets (`http://IP:8088`)
  können das prinzipbedingt nicht. Damit die Turnierleitung **alle**
  Tablet-Akkus sieht, braucht der eingebettete Server HTTPS (Optionen
  bewerten: eigenes lokales Zertifikat + Vertrauensstellung auf den
  Tablets vs. alles über den Cloud-Weg — Entscheidung als ADR).

## Tilo-Feedback (20.07.2026 — Cluster-Zuordnung)

Fünf nachgereichte Punkte. Drei sind bereits geplant, zwei brauchen
Ergänzungen, einer ist neu (Plan 20):

| Tilos Punkt | Cluster | Status |
|---|---|---|
| Tablet-Schrift größer (Lesebrille) | **D** | ✅ geplant — Plan 3 (Schritt 3 hebt Größen inkl. Spielstand) |
| Spiel aus dem Backend beenden/finalisieren (vergessen/Abbruch) | **D** | ✅ geplant — Plan 12 a2 (`enter_result` aus der Turnierleitung) |
| Laufende Zeit nach Aufruf auf TV/Backend/**Tablet** | **C** | ⚠️ Plan 4 deckt TV + Backend — **Tablet-Anzeige ergänzt** |
| Multifeld-/Felderübersicht: Pausenzeiten **und** Zeit nach Aufruf | **C/E** | ⚠️ Plan 4 (Zeit) + Plan 5 (Pause) — **auf overview.html/Felderübersicht ausgeweitet** |
| Feldnummer am Tablet sichtbar, auch bei Erst-Zuweisung | **D** | 🆕 **neu — Plan 20** |

**Zum neuen Punkt (Plan 20):** Tilo hatte zu Beginn 11 Spiele über die
Turnierleitung den Tablets zugeordnet und konnte danach nicht mehr
sehen, welches Tablet an welchem Feld hängt — die Feldnummer ist am
Tablet zu unauffällig, besonders vor Spielstart. Plan:
[roadmap-plaene-2026-07.md](roadmap-plaene-2026-07.md) Punkt 20.
**Plan 4** bekommt zusätzlich die Tablet-Anzeige „Zeit seit Aufruf" und
die Zeit-/Pausenangabe in der Felderübersicht (`overview.html`).

## Nach dem Turnier-Wochenende (Stand 19.07.2026)

**Oberste Direktive (20.07.2026): Der erprobte Stand darf nicht mehr
kaputtgehen.** v0.9.147 lief das Turnier stabil (148/148 Ergebnisse) und
wird unverändert als offizielles Release konserviert. Daraus folgt für
ALLE weiteren Arbeiten:

- ~~**Regressionstests zuerst**~~ → **eingerichtet 20.07.2026:**
  [regression-suite.md](regression-suite.md) benennt die garantierten
  Kernpfade samt Tests (~240, via CI-Pflicht-Check `build` durchgesetzt)
  und die Regeln für jede Änderung. **Kein Feature-Merge, wenn die
  Suite rot ist.** Bekannte Lücken (Snapshot-Übernahme, tablet.html-JS)
  stehen dort mit Plan.
- Features/Fixes einzeln, klein, review't — nie gebündelt mit dem
  stabilen Release (auch #76/#78 kommen einzeln, wenn priorisiert).

### Cluster-Übersicht (Arbeitspakete, Stand 20.07.2026)

Die Pläne ([roadmap-plaene-2026-07.md](roadmap-plaene-2026-07.md)),
BTP-Übernahmen (P1–P3) und Log-Review-Fixes, sinnvoll gebündelt.
**Cluster A ist umgesetzt (v0.9.148, 20.07.2026)** — durchgestrichen:

| Cluster | Inhalt (Plan-Nr.) | Zweck |
|---|---|---|
| ~~**A — Stabilität & Regressionsschutz**~~ ✅ | ~~Regressions-Suite · Leer-Snapshot-Guard · Zombie-Host-Ablösung · Stale-Score-Filter (17) · BTP-Retry-Queue (P2) · Label-Kosmetik · Keep-Awake-/DNS-Doku~~ → **v0.9.148** | Erprobtes absichern — **erledigt** |
| **B — Release & Infrastruktur** | **Release-Seite (18, GESTARTET)** · App-Log-Rotation · LAN-HTTPS/ADR 0005 (6) · Code-Signing · CI-Wartung · Repo-Umbenennung | Auslieferung professionalisieren |
| **C — Aufrufe & Ansagen** | 2./3. Aufruf je Partei (1) · Highlight nach/aus BTP (P1) · Gong-Fix (15) · **Vorbereitungs-/next-Monitor je Halle (NEU, s. u.)** · Nächste Spiele pro Halle (2) · Zeit seit Aufruf — TV/Backend/**Tablet** + **Felderübersicht/Pausenzeiten** (4) | Der komplette Aufruf-Workflow |
| **D — Tablet-Bedienung** | Spielstand-Direkteingabe + **Backend-Finalisierung** (12) · Klick-Delay (13) · helles Theme + Schrift (3) · **Feldnummer prominent, auch vor Spielstart (20)** · Kopplungscode 1 h (8) | Schiedsrichter-Alltag |
| **E — Anzeigen & Ticker** | Pausenuhr-Overlay (5) · TV-Leerlauf-Branding (10) · Matchball-Färbung TL (16) · Aufgabe-Badge · Tages-Filter tab=done · Profil-Link-Fix (9) · Slave-Spielübersicht (7) | Sichtbarkeit für Halle & Publikum |
| **F — Große Features** | Zähltafelbediener (14) · Master-Identität umziehen · Disqualifikation (P3) · Azure-Key-Vererbung (#76) · Pi-Image-Untersuchung | Je ein eigenes Projekt |

Innerhalb eines Clusters teilen sich die Punkte Code-Stellen und
Testaufwand — sie sollten möglichst am Stück umgesetzt werden.
Cluster C und E hängen teils an Cluster A (Regressions-Suite zuerst).

Gesammelte Nacharbeiten, sobald das Turnier vorbei ist:

- ~~**Log-Review des Turnier-Wochenendes**~~ → **durchgeführt 20.07.2026**,
  Ergebnis: [turnier-log-review-2026-07.md](turnier-log-review-2026-07.md).
  Kernzahlen: 148/148 Ergebnisse OK (So), Reconnect-Fix mit
  Vorher/Nachher-Beweis (Sa 33× „belegt"/42 Übernahmen → So 0/1).
  Abgeleitete Fixes für die offizielle Version:
  1. **Leer-Snapshot-Guard** (2× leerer BTP-Snapshot am So → Massen-Reset).
  2. **Zombie-Host-Ablösung im Relay** (333× „Zweiter Host abgewiesen"
     in 17 min nach Netzwechsel — Host-Ping-Timeout analog Tablets).
  3. Keep-Awake-Empfehlung in tablet.md (140 Doze-Zyklen/Tag); Wake Lock
     später via ADR 0005.
  4. Score-Cache-Reset bei Match-Wechsel + leeres Hallen-Label im
     Ergebnis-Log (Kosmetik).
  5. DNS-Betriebshinweis (23 DNS-Ausfälle des Hallen-Routers am So).
- **Offizielles Release schnüren** (> 0.9.147, mit Auto-Update): Inhalte
  der TEST-Builds (BTP-Ergebnis-Fix, TV-Schrift, Klassen-Ansage,
  Slave-Brücke, 0.9.147 BTP-Felder + Tablet-Reconnect) plus der wartenden
  PRs #76 (Azure-TTS-Vererbung) und #78 (8-stelliger Kopplungscode).
  **Änderung am Kopplungscode: Gültigkeit 1 Stunde statt 15 Minuten.**
- **Server aufräumen:** nginx-Namespace-Rewrite (alte→neue Master-ID),
  Kurzlinks `wr1–6`/`wrtv1–6`, `pi-bridge-wr.ps1`, TEST-Exes im
  Download-Verzeichnis.
- **Azure-Speech-Key rotieren** (wurde während des Turniers im Klartext
  geteilt).
- **Pi-Kiosk-Image untersuchen:** Warum fahren frisch beschriebene Karten
  teils nicht hoch (Turnier-Befund; Tilos Image vs. unser Image).
- **Bug prüfen: Region-Feld am Slave nicht änderbar** (Azure-Ansagen).

## Mehr-Hallen-Unterstützung — Restposten

Die Mehr-Hallen-Architektur ist umgesetzt — CourtID-Identität, Hallen-
Gruppierung im UI, Liveticker-Hallen-Monitor (badhub), LAN+Cloud-
Parallelbetrieb (v0.9.4 – v0.9.13, Erzählung in
[multi-hall.md](multi-hall.md)). Geblieben ist ein technischer
Restposten:

- **Namens-Fallback entfernen.** Übergangs-Code, der Routing notfalls noch
  über den Feldnamen statt der CourtID erlaubt, kann nach mehreren stabilen
  Releases entfernt werden.

Geräte-Hinweis aus dem CourtID-Refactor: Tablet-/Monitor-Kopplungen mussten
einmalig neu zugewiesen werden (die alte Zuordnung hing am Feldnamen) —
gilt nur für Installationen, die schon vor v0.9.6 im Einsatz waren.

## Als Nächstes

- **Repo-Umbenennung** → Anzeigename „badhub BTP controller", GitHub-Repo
  `badhub-btp-controller`. Wichtig: Tauri-`identifier` `de.badhub.btslight`
  und der Updater-Pfad `download/bts-light/` bleiben **stabil**, sonst
  brechen bestehende Installationen beim Auto-Update. Der angezeigte
  `productName` kann separat und mit Bedacht wechseln.

## Umgesetzt, aber noch nicht abgenommen

- **Turnierleitungs-Weboberfläche („TL-Web")** — ausgeliefert mit
  **v0.9.176** (Schritte 1–13 der Spec). Bedienung und Grenzen:
  [turnierleitung-web.md](turnierleitung-web.md) · Spec mit ehrlicher Bilanz
  aller 49 Akzeptanzkriterien:
  [features/turnierleitung-web.md](features/turnierleitung-web.md) · ADR
  [0010](adr/0011-tl-web-schreibender-cloud-pfad.md),
  [0011](adr/0012-tl-web-geraete-identitaet.md),
  [0012](adr/0013-ergebniskorrektur-nur-ohne-folgespiel.md).

  **Was noch aussteht**, in der Reihenfolge, in der es im Betrieb auffällt:

  1. **Die manuelle Abnahme auf echten Geräten.** 25 der 49 Kriterien sind
     umgesetzt und im Code nachvollziehbar, aber nicht am Gerät
     nachgewiesen: iPad Safari und Android Chrome mit echten Fingern, zwei
     Geräte gleichzeitig am selben Feld, Relay-Neustart mitten im Betrieb,
     zehn Minuten Standby. Die Checkliste steht in der Spec.
  2. **Zähltafelbediener-Warteschlange** lässt sich aus der Seite nur
     ansehen, nicht umsortieren — der Host kann es, die Bedienung fehlt.
  3. **Beendete Spiele** fehlen in der Ansicht.
  4. **Der abschließende BTP-Versuch zur Ergebniskorrektur**
     ([btp_protocol.md](btp_protocol.md)) — er braucht ein Turnier, in dem
     BTP die nächste Runde nachweislich füllt.
  5. **Sichtprüfung der Geräteverwaltung** im laufenden Fenster.

## Spezifiziert (Spec liegt vor, Umsetzung noch nicht begonnen)

- **Hallen-Check-In** — Spieler bestätigen vor Beginn ihrer Spielklasse über
  eine öffentliche Webseite selbst, dass sie in der Halle sind; die
  Turnierleitung sieht **vor der Auslosung**, wer fehlt, und kann Fehlende
  gezielt ausrufen lassen. Spannt über zwei Repos (öffentliche Seite und
  Persistenz in badhub, Meldelisten-Push und Turnierleitungs-Sicht in
  bts-light), geschnitten in drei nacheinander lieferbare Stufen.
  Spec: [features/spieler-check-in.md](features/spieler-check-in.md) ·
  ADR: [adr/0009-hallen-checkin-persistenz-und-identitaet.md](adr/0009-hallen-checkin-persistenz-und-identitaet.md).
  **Vor Umsetzungsbeginn zu entscheiden:** ob die Check-In-Verwaltung in
  badhub für die Rolle `liveticker` freigeschaltet wird. Deren Sperre ist
  eine bewusste Rücknahme von 04/2026 (Zugangsdaten kommen seither per
  E-Mail), die Rollen-Infrastruktur ist aber intakt. Ohne Freischaltung
  müsste ein Superadmin die Anfangszeiten jedes Turniers pflegen.

## Geplant

- **Code-Signing des Windows-Installers.** Aktuell unsigniert → Windows
  zeigt beim ersten Start eine SmartScreen-Warnung. Optionen: Azure Trusted
  Signing vs. klassisches OV/EV-Zertifikat — Kostenentscheidung offen. Das
  Auto-Update ist davon unabhängig (eigenes Signaturschlüsselpaar).
- **CI-Wartung.** Die Release-/CI-Workflows nutzen Node-20-Actions
  (`actions/checkout@v4`, `actions/setup-node@v4`,
  `softprops/action-gh-release@v2`) — vor dem erzwungenen Node-24-Umstieg
  (ab 2026-06-02) aktualisieren. Außerdem leitet GitHub `windows-latest`
  ab 2026-06-15 auf `windows-2025` um — Build dort gegenprüfen.
- **Release-Seite: Versions-Downloads + Kompakt-Changelog** (Wunsch
  20.07., ersetzt den früheren Punkt „Changelog sichtbar machen"):
  Öffentliche Seite unter `download/bts-light/` mit allen Versionen
  (Installer liegen dort bereits lückenlos) und den Änderungen je
  Version aus changelog.md; beim Release automatisch generiert,
  Changelog-Auszug zusätzlich in `latest.json → notes` (Update-Fenster
  zeigt „Was ist neu"). Plan 18 in
  [roadmap-plaene-2026-07.md](roadmap-plaene-2026-07.md).
- **Feld-Raster per Drag & Drop anordnen.** Das Feld-Raster
  (Spaltenzahl + Start-Ecke + Schlange, [features/feld-raster.md](features/feld-raster.md))
  deckt rechteckige Hallen ab; für unregelmäßige Hallen wäre eine frei
  ziehbare Anordnung je Feld komfortabler. Bewusst verschoben beim
  Erstwurf — Persistenz je Feld statt je Halle, eigene Speicher-UI,
  Zusammenspiel mit wechselnden Feldzahlen.

## Datenverlust-Pfade in der Konfiguration (Befunde 07.08.2026)

Zwei **bestehende** Fehler, aufgefallen beim Review der TL-Web-Umsetzung.
Beide betreffen den erprobten Stand und sind bewusst **nicht** nebenbei
mitgeändert worden:

- **Beschädigte `config.json` → Assistent überschreibt sie.** `load_config`
  liefert bei jedem Parse-Fehler ein `Err`; das Frontend fängt das ab und
  zeigt den Einrichtungs-Assistenten mit der **Default**-Konfiguration
  (`App.tsx`, `defaultConfig()`). Der erste „Speichern & Starten"-Klick
  schreibt diese Defaults über die noch vorhandene, nur unlesbare Datei —
  inklusive **leerer `install_id`**, womit alle gekoppelten Geräte
  wegfallen (genau das Chaos aus dem Turniertag-Bericht). Begünstigt wird
  das durch `save_to`, das die Datei **nicht atomar** schreibt: ein Absturz
  oder Stromausfall mitten im Speichern hinterlässt eine abgeschnittene
  Datei. **Vorschlag:** atomar schreiben (temporäre Datei + `rename`),
  beschädigte Datei beiseitelegen statt überschreiben, und dem Nutzer
  ehrlich sagen, dass die alte Konfiguration nicht gelesen werden konnte —
  statt ihm einen harmlos aussehenden Ersteinrichtungs-Assistenten zu
  zeigen.
- **`locked_courts` geht beim Speichern von Einstellungen verloren.**
  `set_court_locked` schreibt die Sperrliste host-seitig in die
  `config.json`; der Einrichtungs-Assistent schickt beim nächsten Speichern
  seinen beim Öffnen aufgenommenen Stand zurück (`buildConfig`:
  `locked_courts: initialConfig.locked_courts ?? []`) und setzt sie damit
  zurück. Ablauf: Feld in der Übersicht sperren → in den Einstellungen
  irgendetwas speichern → Sperre weg. Für die TL-Geräteliste ist dieser
  Pfad bereits geschlossen (`keep_host_managed_fields` in `commands.rs`);
  `locked_courts` gehört auf demselben Weg dazu.

## Wünsche vom 11.08.2026

- **Punktverlauf-Graph pro Satz** — Spec freigegeben und in Umsetzung:
  [features/punktverlauf-graph.md](features/punktverlauf-graph.md)
  (ADR [0014](adr/0014-punktverlauf-expliziter-rally-frame.md),
  [0015](adr/0015-punktverlauf-datei-je-turnier.md)). Folge-Features
  (je eigene Spec, offen): badhub-Push + Anzeige auf badhub.de ·
  **ausgefüllte Schiedsrichterzettel als PDF drucken** (Hinweis vom
  11.08.2026; Datenbasis ist der Punktverlauf).

## Wünsche vom 10.08.2026 (nach dem v0.9.178-Test)

**TL-Web (bts-light, klein):**

- ~~**Spielliste: Disziplin, Runde und Gruppe einzeln ein-/ausblendbar** —
  drei weitere Schalter im Anzeige-Menü, je Gerät gespeichert (wie
  Spielnummer/Nationen).~~ → umgesetzt 2026-08-10.
- ~~**Bug: Drag & Drop auf Android-Tablets (Chrome) funktioniert nicht.**
  tl.html nutzt HTML5-Drag-Events, die auf Touch-Geräten nicht feuern;
  Antippen-dann-Feld-Antippen geht als gleichwertiger Weg. Fix: Drag auf
  Pointer-Events umstellen (oder Touch-Fallback).~~ → umgesetzt 2026-08-10
  *(am echten Android-Tablet bestätigt, siehe unten)*. Befund beim
  Umsetzen korrigiert: tl.html nutzte bereits
  Pointer-Events statt HTML5-DnD; der eigentliche Fehler war
  `touch-action: pan-y` auf der Zeile, das jede Wischbewegung sofort als
  Scrollen beanspruchte, bevor die 8-px-Schwelle greifen konnte.
  **Zwischenstand Long-Press (verworfen):** Erster Fix bewaffnete den
  Touch-Zug nach ~300 ms ruhigem Halten, mit einem `touchmove`-Listener zur
  Scroll-Unterdrückung. **Rückmeldung vom echten Android-Tablet: klappte nur
  in ca. 1 von 10 Versuchen** („zu schnell will er scrollen und verliert den
  Touch") — Chrome legt das erlaubte `touch-action` schon beim allerersten
  `touchstart` für die ganze Geste fest, ein natürliches Fingerzittern von
  wenigen Pixeln reichte, damit der Browser die Geste noch WÄHREND der
  300-ms-Wartezeit als Scrollen einstufte. Ein Long-Press auf einer
  scrollbaren Liste verliert dieses Wettrennen strukturell.
  **Umbau auf Zieh-Griff (Standard-Muster für mobiles Drag):** ein eigenes
  Griff-Element (⠿) an jeder ziehbaren Zeile/Kachel, `touch-action: none`
  STATISCH nur auf dem Griff selbst — die Geste beginnt gezielt dort, kein
  Wettrennen mehr, kein Long-Press, sofortiges Bewaffnen. Ein Tipp irgendwo
  sonst auf der Zeile bleibt unverändert scrollen + Antippen-dann-Antippen.
  **Gerätetest bestanden (10.08.2026, Android-Tablet/Chrome): „Zieh-Griff
  funktioniert jetzt super."**
- ~~**Alle Spielfeldkacheln sollen immer sichtbar sein, die Spielliste
  scrollt dafür separat** (rechts wie in „Spielliste darunter").~~ →
  umgesetzt 2026-08-10. Die Seite rollt seit dem festen Rahmen (`#app`
  spannt sich über `100dvh`/`100vh`) nicht mehr selbst; `main` teilt sich in
  zwei eigenständig rollende Bereiche. Die Felderbox misst nach jedem
  Neuzeichnen und bei jeder Größenänderung, ob sie überläuft, und schaltet
  dafür stufenweise `kacheln-kompakt` (kleinere Abstände/Schrift) dann
  `kacheln-mini` (deutlich kleiner, Abzeichen/Meta einzeilig) zu — die erste
  Stufe, die passt. Passt selbst die kleinste Stufe nicht (sehr viele Felder
  auf einem Telefon), wird die Felderbox als letzter Ausweg selbst rollbar.
- ~~**Spielliste per Ziehen vergrößern/verkleinern, je Gerät gespeichert.**~~
  → umgesetzt 2026-08-10. Trennsteg zwischen Feldern und Liste: zieht
  nebeneinander die Breite, gestapelt die Höhe (`localStorage`,
  `bts-tl-liste-breite`/`-hoehe`); Doppeltipp stellt die Automatik wieder
  her. Details: [turnierleitung-web.md](turnierleitung-web.md).
- ~~**Nationalflaggen fehlen in der TL-Sicht (nur Kürzel), und die Sicht
  „hüpft" sekündlich.**~~ → behoben 2026-08-10, eine Ursache für beides:
  Die ns-lose TL-Seite fand im Cloud-Betrieb keine Flaggen-Route
  (`/bts-relay/flags/…` gab 404), der `onerror`-Tausch der Platzhalter
  ließ die Listen bei jedem Poll springen. Relay liefert Flaggen jetzt
  auch ohne Namespace; tl.html merkt sich fehlgeschlagene Kürzel.


**Hallen-Check-In (überwiegend badhub-Repo; ändert die Spec
[features/spieler-check-in.md](features/spieler-check-in.md) — vor
Umsetzung kurz grillen, besonders die Doppel-Semantik):**

- **Doppel als Doppel kennzeichnen** auf der öffentlichen Check-In-Seite —
  Partner sichtbar zusammengehörig, auch wenn jede Person einzeln abhakt.
- **Zählung: ein Doppel zählt erst als anwesend, wenn beide da sind**
  (betrifft badhub-Anzeige UND die TL-Sicht/Ansagen in bts-light).
- **Suche zeigt Eingecheckte sofort grün hinterlegt** (öffentliche Seite).
- **Rückgängig bei Verklicken:** Undo-Symbol für Zeit x nach dem eigenen
  Check-In (öffentliche Seite + API).
- **Admin-Variante der Check-In-Seite:** Status generell änderbar,
  Spieler auf „abgemeldet" setzen usw. — neuer Status „abgemeldet" spannt
  über beide Repos; die TL-Sicht in bts-light kann heute schon
  Hand-Einchecken/Zurücknehmen.

## Feature-Wünsche

Von der Turnierleitung gewünscht, noch nicht eingeplant:

- **Aufgaben- & Walkover-Übersicht.** Eine Seite bzw. Kachel in bts-light,
  die während des Turniers alle Aufgaben und alle daraus gewerteten
  Walkovers auflistet — Überblick für die Turnierleitung.
- **Walkover zurücknehmen.** Eine kampflose Wertung wieder rückgängig
  machen können (Match in BTP zurück auf offen / `ScoreStatus = 0`),
  falls sie versehentlich oder falsch gesetzt wurde.
- **Tablet-Verbindungsanzeige im Cloud-Modus.** Schließt bts-light, bleibt
  das Tablet mit dem Relay verbunden und zeigt weiter „verbunden" — es
  erfährt nicht, dass der Host (bts-light) weg ist. Der Relay sollte den
  Tablets ein „Host offline"-Signal schicken, damit das Tablet ehrlich
  „Warte auf Turnier-PC" anzeigt.
- **Verbindungsweg je Gerät anzeigen (Parallelbetrieb).** Im
  LAN+Cloud-Modus pro verbundenem Gerät (Tablet, Court-Monitor) kenntlich
  machen, ob es bts-light über LAN oder über das Cloud-Relay erreicht —
  als Badge in der Felder-/Geräte-Übersicht. So sieht die Turnierleitung
  auf einen Blick, welchen Weg ein Gerät nutzt; hilft bei der Fehlersuche,
  wenn eine Halle hängt. (Der Relay/Server kennt den Weg ohnehin — er muss
  ihn nur je Gerät bis in die Übersicht durchreichen.)
- **Pausen-Buttons auf dem Tablet vereinheitlichen.** Die Buttons für
  Verletzungs-/Behandlungspause und der „Weiterspielen"-Button, mit dem
  eine laufende Pause beendet wird, sind uneinheitlich in Beschriftung,
  Größe und Anordnung. Über alle Pausen-Typen hinweg angleichen, damit
  die Bedienung im Spielbetrieb eindeutig ist.
- **Akkustand farblich kodieren (Tablet-Übersicht).** In der Felder-
  Übersicht soll der Tablet-Akkustand auf einen Blick zeigen, ob ein
  Tablet getauscht oder nachgeladen werden muss: **> 50 % grün**, **< 20 %
  rot**, dazwischen gelb. Schwellen am `TabletBattery.percent` in
  [`pages/TabletPanel.tsx`](../src/pages/TabletPanel.tsx); Ladezustand
  (`charging`) bleibt das bestehende Symbol.

## Court-Monitor — offene Punkte

Der Court-Monitor ist umgesetzt (v0.7.0–v0.9.0, [court-monitor.md](court-monitor.md),
[pi-setup.md](pi-setup.md), [pi-master-image.md](pi-master-image.md)).
Offen für das **Verleih-Set**-Konzept (Technik wird an Turnierleitungen
verliehen):

- **mDNS funktioniert auf Pi/avahi (verifiziert 2026-05-25).** Der seit
  Mai 2026 offene Entscheidungstest ist durchgeführt: ein Raspberry Pi mit
  Pi OS Lite (avahi-daemon) löst `bts-light.local` zuverlässig zu der IP
  des sendenden Geräts auf. Test-Setup: bts-light-Bekanntmachung
  (`_bts-light._tcp.local.` mit Hostname `bts-light.local.`, Port 8088)
  vom Mac aus per `dns-sd -P` simuliert → vom Pi aus mit
  `avahi-resolve -n bts-light.local` aufgelöst → IP korrekt empfangen,
  auch über die WLAN↔Ethernet-Bridge der FRITZ!Box hinweg. Damit ist die
  damalige Windows-PC-Fehlschlag-Beobachtung als reines
  Windows-mDNS-Client-Problem identifiziert; **bts-lights mDNS-Bekannt­
  machung in `tablet/mdns.rs` ist korrekt**. Konsequenz: das Master-Image
  bäckt `http://bts-light.local:8088/monitor` als Kiosk-Adresse ein, eine
  DHCP-Reservierung am Verleih-Router ist nicht notwendig (kann als
  Worst-Case-Rückfall jederzeit nachgezogen werden).
- **Master-Image erstellen + hosten.** Den „Golden Master"-Pi einmal auf
  echter Hardware bauen, die Karte als `bts-monitor.img.xz` sichern und in
  den Download-Bereich auf badhub.de legen. Ablauf: [pi-master-image.md](pi-master-image.md).
  Monitor-Adresse: **`http://bts-light.local:8088/monitor`** (durch den
  mDNS-Test oben bestätigt).
- **Hardware-Anforderung Pi Zero 2 W oder höher** (Hinweis 2026-05-25
  konkretisiert): Pi Zero W (1. Gen) und Pi Zero 2 W sehen physisch
  identisch aus, sind aber komplett verschiedene Chips. Pi Zero W (1. Gen,
  armv6 ARM1176JZF-S) hat **keine NEON-SIMD-Einheit**; modernes Chromium
  ist auf Debian Trixie / Pi OS Bookworm mit NEON als **Pflicht**
  kompiliert → Pi Zero W zeigt beim Start einen Hardware-Fehler-Dialog,
  ist als Court-Monitor **unbrauchbar**. Pi Zero 2 W (Cortex-A53), Pi 3,
  Pi 4 und Pi 5 haben alle NEON, dort läuft alles. 64-bit-Boot
  funktioniert nur ab Pi Zero 2 W (Symptom auf Pi Zero W: 7-Blink
  „kernel image not found"). Empfehlung für Verleih-Set-Hardware:
  Pi Zero 2 W (klein, günstig, ausreichend für den Kiosk) oder Pi 4
  (deutlich kraftvoller).
- **Info-Monitor: Routen + HTML ausgeliefert** (v0.9.17, 2026-05-25), **UI-
  Zuweisung offen.** Der Tablet-Server liefert jetzt zwei Hallen-Displays
  unter dedizierten URLs: `/info/overview` (Court-Übersicht, Hallen ×
  Felder × aktuelles Spiel) und `/info/preparation` (gerufene und
  eingeplante Spiele). Beide offline-fähig — Daten direkt aus
  `BtpSnapshot`, kein badhub.de nötig. URL-Parameter `?halle=<Name>` und
  `?rotate=90|180|270` unterstützt. Details
  [court-monitor.md → Info-Monitor](court-monitor.md). **Offen:**
  Zuweisung über die „Court-Monitore"-Seite (statt manuell die
  `bts-monitor-url.txt` zu bearbeiten) — Mock-Up des Dropdowns:
  ```
  Halle 1
    Feld 1
    Feld 2
  Halle 2
    Feld 1
    Feld 2
  Informationen
    Courtübersicht
    In Vorbereitung
  ```
  Setzt eine Erweiterung des `monitor_assignments`-Datenmodells voraus
  (Target = Court(i64) | InfoOverview | InfoPreparation) und ein
  zusätzliches Dropdown-Element im Frontend; der `/monitor`-Endpoint
  würde dann je Target-Typ die passende HTML zurückgeben.
- **Display-Rotation für Pivot-Monitore: URL-Parameter umgesetzt**
  (v0.9.17, 2026-05-25), **zentrale Steuerung offen.** `?rotate=90|180|270`
  am URL der Monitor-Seiten dreht die Anzeige per CSS-Transform — Pi-
  OS-seitig keine Änderung nötig. Das CSS rendert auch in Portrait
  sauber. **Offen:** Rotation als Geräte-Eigenschaft zentral aus bts-light
  pro Pi steuerbar (ohne `bts-monitor-url.txt` editieren zu müssen).
  Implementation: zusätzliches Feld `rotation: Option<u16>` in der
  Geräte-Zuweisung; bts-monitor.sh hängt `?rotate=…` an die URL an.
- **Online-Anleitung veröffentlichen.** [pi-setup.md](pi-setup.md) als
  echte Webseite (badhub.de) bereitstellen und **in bts-light verlinken**
  (Knopf „Einrichtungs-Anleitung" auf der Court-Monitore-Seite).
- **2-Felder-pro-TV-Modus.** Zwei benachbarte Felder auf einem großen TV
  (`…/display?courts=3,4`).

## Bekannte Einschränkungen / technische Schuld

- **Liga-Matches** (`PlayerMatches` in BTP) sind nicht abgedeckt — bts-light
  verarbeitet nur Einzel-/Doppel-Draws.
- **Spielsystem fest Best-of-3 bis 21.** BTP liefert das Spielformat im
  aktuellen Parser nicht zuverlässig; der Tablet-Spielzettel nimmt den
  Badminton-Normalfall an.
- **Liveticker-Staleness uneinheitlich.** Im `/live`-Picker fällt das
  „Live"-Badge nach 4 Min ohne Heartbeat weg, die Detailseite (`?t=`)
  zeigt „Nicht mehr live" erst nach 10 Min. Die 10-Min-Schwelle ist
  bewusst lose gehalten, solange Nicht-Heartbeat-Quellen (`letilo/bts`)
  pushen können. Angleichen, sobald bts-light die einzige Quelle ist.
- **Keine Frontend-Tests.** Der Rust-Kern ist per `cargo test` abgedeckt;
  die React-Seite (u. a. `announcer.ts` — Court-Phrase, Ansage-Segmente,
  Auto-Sprach-Regel) hat kein Test-Setup. badhub-tournament nutzt Vitest
  inkl. `announcer.test.ts` — das ließe sich übernehmen.
- **Alte Liveticker-Test-Turniere.** `lehiero`, `christian-zum-test` und
  die Legacy-Zeile `default` stehen in `liveticker_tournaments` noch auf
  `is_active = 1` und machen `/live` ohne `?t=` mehrdeutig. Im
  Liveticker-Admin auf inaktiv setzen.
- **`docs/ops/deployment.md` teils veraltet** (badhub-Repo): Der Abschnitt
  „Deploy: Produktion" beschreibt noch das KAS-`deploy_prod.sh`, obwohl
  Prod längst über `deploy_hetzner.sh` auf Hetzner läuft.
