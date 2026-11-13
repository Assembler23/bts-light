# Roadmap & offene Punkte

Lebende Liste der offenen Arbeiten an bts-light. Erledigte Versionen stehen
im [changelog.md](changelog.md); hier steht, was **noch** ansteht.

> Dies ist die allgemeine Repo-Roadmap für bts-light. Wie zurückgestellte Themen
> klassifiziert, auf Dubletten geprüft und geroutet werden, steht in
> `~/.claude/AGENTS.md` → *Roadmap & Backlog (FUTURE ROADMAP RULE)*.
> Cross-Repo-Code-Struktur gehört nach `badhub/docs/arch/code-structure-roadmap.md`,
> Agentic-/Harness-Themen nach
> `~/ai-harness/prepared/agentic-health-check-2026-08-27.md`.
> Cross-Repo-Punkte werden **genau einmal** geführt — im Repo mit der führenden
> Umsetzung; abhängige Repos nur als `DEPENDS_ON`/`AFFECTS`.

> Stand: 2026-07-17, nach dem ersten Zwei-Hallen-Praxisturnier (v0.9.144).
> Die Prio-1-Punkte und Turnier-Wünsche stammen direkt aus diesem Einsatz.

## Prio 1 — Lehren aus dem Zwei-Hallen-Turnier (17.07.2026)

- **Turnier-Robustheit & Echtzeit-Latenz (Cluster)**: Übersicht
  [features/turnier-robustheit-cluster.md](features/turnier-robustheit-cluster.md).
  Vier Hebel — **A** Echtzeit-Robustheit der Score-Strecke (niedrig-latente
  Anzeige per Push + Reconnect-Wahrheit „Slot-Halter gewinnt";
  [features/turnier-robustheit.md](features/turnier-robustheit.md), ADR 0016/0017)
  — **✅ umgesetzt v0.9.196/197 (#204/#205)**. **B** Ergebnis-Weg verlustsicher
  (Idempotenz + persistente Retry-Queue; [features/ergebnis-puffer.md](features/ergebnis-puffer.md),
  ADR 0018) — **✅ v0.9.198 (#206)**. **C** Last-/Soak-Test des Relay-Brokers
  (In-Process, [features/last-soak-test.md](features/last-soak-test.md), ADR 0019)
  — **✅ (#207/#208)**; die LAN-Server-Last ist am **20.08.2026 gemessen** und
  war die letzte offene Frage daraus: Bei **fünffacher Ziellast** (220 Geräte,
  1232 Abrufe/s, 9 MB/s) braucht der Turnier-PC **75 % eines einzelnen Kerns**
  und liefert p95 104 ms, ohne einen einzigen Fehler — der Server ist nicht der
  Engpass, das Hallen-WLAN ist es. Zahlen und Vorbehalte:
  [regression-suite.md](regression-suite.md). **D**
  Tote-Verbindungs-Erkennung schärfen (Host-Client Read-Idle + Tablet↔Relay-
  Empfangs-Stale; [features/tote-verbindungen.md](features/tote-verbindungen.md),
  ADR 0020) — **✅ umgesetzt v0.9.199**.
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
  uns als „gerufen" erscheinen — **Lesen seit v0.9.277 vorhanden**
  (Spec `features/tl-zeilenfarbe.md`: `Highlight` ist BTPs Zeilenfarbe
  0–6, Gelb = 1 = unsere Aufrufmarke); ein in BTP gelb markiertes Spiel als
  „gerufen" zu deuten bleibt offen, weil Gelb dort auch eine Handfarbe sein
  kann (ADR 0056); (c) beim Umsetzen prüfen, wie das
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
- ✅ **Zeit seit Aufruf** auf den TVs (v0.9.54/156), in bts-light und
  seit v0.9.278 auch am **Tablet** (Kopfzeile, ab der Zuweisung bis zur
  bestätigten Aufstellung; [tablet.md](tablet.md)).
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
| Laufende Zeit nach Aufruf auf TV/Backend/**Tablet** | **C** | ✅ Plan 4 (TV + Backend) · Tablet-Kopfzeile seit v0.9.278 |
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

- **Ergebnis-Write ohne CourtID löscht den Spielort.** Messung 09.11.2026
  (`docs/btp_protocol.md`): BTP übernimmt die CourtID aus jedem Write mit
  `Official*`. `update_request` mit `free_court_id: None` (kampflos aus
  TL-Web/Desktop, Wiederholungspfad in `sync.rs`) trägt `Official*` ohne
  `CourtID` — stand das Spiel doch auf einem Feld, ist der Spielort in BTP
  danach weg. Folge gering (Spiel ist beendet); Abhilfe: CourtID aus dem
  Snapshot bzw. frisch gelesen mitschicken. Nachtrag (Messung 09.11.2026,
  v0.9.369): Dasselbe gilt für `Highlight` — jeder Match-Write ohne das
  Element setzt die Farbe auf 0.
- **Handfarbe überlebt den Ruf aufs Feld nicht.** Feldvergabe und Freigabe
  (`court_assign_request` mit `Courts`-Block) tragen kein `Highlight`, BTP
  setzt die Farbe dabei auf 0 (Messung 09.11.2026). Für die gelbe
  Aufrufmarke ist das gewollt; eine von der Turnierleitung gesetzte andere
  Farbe geht aber mit verloren. Abhilfe: Handfarben (≠ Gelb) frisch gelesen
  mitschicken. **Gleiche Ursache, schwerer:** Feldvergabe und Freigabe ohne
  lokal bekannte Besetzung (`officials_for_write` → `None`, also
  `assign_court`, `free_court`, `plan_court_action`, `auto_assign`) schicken
  kein `Official1ID/2ID`. `officials_for_write` liefert `None` auch, sobald
  BTP eine eigene Besetzung bestätigt hat (`confirm` lässt die lokale Absicht
  los) — betroffen ist also **jede** bestätigte SR/AR-Besetzung, auch die
  beim Aufruf geschriebene (ADR 0080), nicht nur eine von Hand in BTP
  gesetzte. Sie ist weg, sobald das Spiel aufs Feld geht oder freigegeben
  wird; ob die Rotation sie danach wieder einträgt (im Log vom 09.11.2026
  folgte nach dem Ruf aufs Feld ein erneuter SR-Write), ist nicht geprüft.
  Für den Zuweisungs-Write mit `Courts`-Block ist das nicht eigens gemessen,
  folgt aber aus der Messung. Der Kommentar in `commands.rs::free_court`
  („Besetzung nicht anfassen") stimmt dann nicht. Abhilfe wie beim
  Schiedsrichter-Write: frisch lesen (plus Merkstand
  `btp::eigene_writes`) und die Besetzung mitschicken; vorher kurz messen.
- **Tablet-Kiosk-App: Kiosk-Verhalten ohne Gerätebesitzer (Fire OS 8).**
  Befund 08.09.2026: Auf Fire OS 8 ist der Gerätebesitzer nie erreichbar —
  Amazons Kindersicherung ist auch direkt nach einem Werksreset Profile
  Owner, `dpm set-device-owner` scheitert immer. „Bildschirm anheften" ohne
  Besitzer funktioniert seit v0.9.285 wieder (Home/Zuletzt gesperrt), sofern
  die Kindersicherung „Touch-Funktion deaktivieren" aus hat. **Autostart
  seit v0.9.285 gelöst** über `device_config put activity_manager
  default_background_activity_starts_enabled true` im Skript (Feldtest:
  Fokus nach jedem Neustart auf der App). Den Fixier-Dialog, den Android
  ohne Besitzer bei jedem Anheften zeigt, bestätigt seit v0.9.285 der
  Bedienungshilfe-Dienst `BestaetigungsDienst` (per adb eingeschaltet).
  Was auf Fire OS bleibt: Home und Zuletzt sind nur durch das Anheften
  gesperrt (Ausstieg über die Android-Geste), und nach „Kiosk verlassen"
  holt nichts die App zurück — falls das je stört, könnte derselbe Dienst
  nach dem Muster LauncherHijack die App beim Erscheinen des Launchers
  zurückholen. Kein akuter Bedarf.
- **Repo-Umbenennung** → Anzeigename „badhub BTP controller", GitHub-Repo
  `badhub-btp-controller`. Wichtig: Tauri-`identifier` `de.badhub.btslight`
  und der Updater-Pfad `download/bts-light/` bleiben **stabil**, sonst
  brechen bestehende Installationen beim Auto-Update. Der angezeigte
  `productName` kann separat und mit Bedacht wechseln.

## Umgesetzt, aber noch nicht abgenommen

- **Zettel-Druck schon beim Aufruf in die Vorbereitung** — Spec:
  [features/zettel-druck-bei-aufruf.md](features/zettel-druck-bei-aufruf.md) (freigegeben
  10.11.2026), ADR 0082 — neue Auswahl „Drucken: bei der Feldvergabe / schon beim Aufruf";
  nur mit SR, Nachdruck bei SR-Wechsel bis zur Feldvergabe (20 s stabil), Warnung bei
  entferntem SR, kein Schwall beim Umschalten. **umgesetzt v0.9.372 + v0.9.373, Feldtest offen**: Nachdruck-Frist und
  „ungültig"-Hinweis an echtem Drucker prüfen.

- **Spielersuche im TL-Web** — Spec:
  [features/tl-spielersuche.md](features/tl-spielersuche.md) (freigegeben
  09.11.2026), ADR 0081 — **v0.9.368,
  Feldtest offen**: Suchfeld in der Kopfleiste findet Spieler und SR/AR am
  Feld, in der ganzen Warteliste und in allen beendeten Spielen. Zu prüfen:
  Bildschirmtastatur auf Fire-Tablets, Sprung bei zugeklapptem Panel.

- **Ansage in Teilen vertonen** — Spec:
  [features/ansage-in-teilen.md](features/ansage-in-teilen.md) (freigegeben
  09.11.2026) — **umgesetzt v0.9.366, Hörtest offen**: klingt die
  Klebestelle in der Halle unauffällig, schwankt die Stimme je Dienst
  zwischen den Teilen, wie viel Zeichen spart es an einem echten Turniertag.

- **Kiosk-App: Akku auf der Wartekarte + Drei-Finger-Geste** — Spec:
  [features/kiosk-akku-und-ausgang.md](features/kiosk-akku-und-ausgang.md)
  (freigegeben 06.11.2026) — **v0.9.343, Feldtest auf Fire HD offen.**

- **„Zuletzt gespielt" in der Turnierleitungs-Liste** — Spec:
  [features/tl-zuletzt-gespielt.md](features/tl-zuletzt-gespielt.md)
  (freigegeben 06.11.2026) — **v0.9.342, Feldtest offen.** Profil-Häkchen,
  Marke „vor N min" je Seite an wartenden und gerufenen Spielen. Später
  denkbar: Einfärbung nach Schwelle; Altbestand bei Einrichtung mitten im
  Turniertag nicht als „gerade eben" zeigen.

- **Reihenfolge der gerufenen Partien** (Meetingpoint / In Vorbereitung) —
  Spec: [features/meetingpoint-reihenfolge.md](features/meetingpoint-reihenfolge.md)
  (freigegeben 07.11.2026), ADR 0077 — wer zuerst gerufen ist, kommt zuerst
  aufs Feld; Gerufene im TL-Web und am Desktop umsortierbar.

- **Spielfolge-Anzeige** (laufende + kommende Spiele, Laufband) — Spec:
  [features/spielfolge-anzeige.md](features/spielfolge-anzeige.md), ADR 0076 —
  **v0.9.341, Feldtest offen.** Nachzuholen: Ansage-Modus, Cloud-Etappe,
  Prognose-Zeit serverseitig formatieren (Kiosk ohne Zeitzone).

- **Schiedsrichter in der TL-Web-Warteliste fest einteilen** — Spec:
  [features/tl-warteliste-schiedsrichter.md](features/tl-warteliste-schiedsrichter.md)
  (freigegeben 08.11.2026), ADR 0078 — Einteilung über das ⋮-Menü der Warteliste,
  Rotation reserviert die Besetzung wartender Spiele (auch BTP-vorab), Umteilen
  gegen BTP, Walkover zählt nicht als Einsatz. **Umgesetzt v0.9.350–v0.9.355**
  (Menüpunkt + Dialog in v0.9.355). Offen: **Feldtest** (LAN, Cloud, alter Host,
  offene Paarung, Vorabzettel) und die **BTP-Messung**, ob BTP `Official1ID` an
  einem Spiel ohne Feld übernimmt (mit `CourtID 0` bzw. ohne CourtID-Element).

- **Weitere Sprachdienste der Ansage (ElevenLabs, OpenAI, Gemini)** — Spec:
  [features/ansage-elevenlabs.md](features/ansage-elevenlabs.md) (freigegeben
  02.11.2026, erweitert 03.11.2026) — **umgesetzt v0.9.336, Hörtest beim
  Turnier offen**. Auswahl „Sprachdienst" (Azure · ElevenLabs v2/v3/v4/Flash
  v2.5 · OpenAI · Gemini 3.8 Flash/Flash-Lite) auf der Seite Ansagen. Offen: welcher
  Dienst die Namen am besten trifft, ob v3/v4/Flash/OpenAI/Gemini auf den
  Satzpunkt eine Pause machen, Latenz in der Halle, Kosten pro Turniertag;
  OpenAI und Gemini sind **noch nie gegen den echten Dienst gelaufen** (kein
  Schlüssel), die Modell-IDs `gpt-4o-mini-tts` und
  `gemini-3.8-flash(-lite)-tts` (Interactions API) sind ungeprüft. Danach entscheiden, welche
  Dienste bleiben (dann Datenschutz-Abwägung festhalten).

- **Meetingpoint: Einchecken je Partie und „spielbereit"** — Spec:
  [features/meetingpoint-spielbereit.md](features/meetingpoint-spielbereit.md)
  (freigegeben 02.11.2026, ADR 0071–0075) — **umgesetzt v0.9.330 als Sammel-PR, Feldtest offen; Relay vor App deployen**. Gerufene Partien checken sich am
  Meetingpoint-Gerät je Halle einzeln ein (Spieler, optional SR); TL sieht den
  Status, strikte Feld-Automatik vergibt nur spielbereite Partien; SR-Rotation
  schon beim Aufruf; Aufruf-Automatik (fest N / Bedarf aus Restzeit);
  Fehlende-Warnung + Ansage; Feld-Check am Zähl-Tablet. Aufrufe überleben
  künftig Neustarts. Alles standardmäßig aus.

- **Tablet-Kiosk-App für Android / Fire-Tablets — umgesetzt, Feldtest
  offen.** Spec: [features/tablet-android-kiosk-app.md](features/tablet-android-kiosk-app.md),
  ADR [0058](adr/0058-eigene-kiosk-app-statt-fully-kiosk.md). Aus dem
  Nutzer-Wunsch vom 06.09.2026: Eine eigene, dünne Kotlin-App in `android/`
  findet den Turnier-PC wie die Pi-Monitore selbst (gemerkte IP →
  Subnetz-Scan → mDNS), zeigt die Felder-Lobby im Vollbild und sperrt das
  Tablet als Gerätebesitzer — ohne Fully-PLUS-Lizenz und ohne Adresse je
  Tablet. Bedienung: [tablet-android-app.md](tablet-android-app.md). Nur
  echte Hardware klärt noch:
  1. **Device Owner auf Fire OS:** `dpm set-device-owner` mit frischem
     Tablet ohne Amazon-Konto. Wird es verweigert → weiche Sperre
     dokumentieren, Alternative prüfen.
  2. **Amazon-WebView-Stand:** reicht die Chromium-Version für
     `tablet.html` (Module, `fetch`, Pointer-Events)? Ggf.
     Mindestversion in die Doku.
  3. **Lock-Task auf Fire OS:** Statusleiste wirklich weg? Fire-Launcher
     unterdrückt? Sperrbildschirm aus?
  4. **Boot-Zeit** bis zur Lobby, mit und ohne gemerkte IP.
  5. **Scan-Dauer** im Hallen-WLAN (254 Adressen, Blöcke von 30).
  6. **Akku-Badge** in der Übersicht sichtbar.
  7. Ein Turniertag parallel zu einem Fully-Tablet.

- **Mehrere Liveticker je Verband (v0.9.276)** — Spec:
  [features/liveticker-mehrere-turniere-je-verband.md](features/liveticker-mehrere-turniere-je-verband.md),
  ADR [0054](adr/0054-liveticker-kind-turnier-je-guid.md).
  Aus der Nutzer-Frage vom 04.09.2026 (zwei BVBB-Turniere am selben
  Wochenende): badhub hielt bisher genau **einen** Stand je Zugangsschlüssel,
  und das Verbands-Preset teilt ein Passwort — zwei parallele Installationen
  überschrieben sich alle 60 s gegenseitig. badhub legt seither beim ersten
  Push mit neuer turnier.de-GUID automatisch ein **Kind-Turnier** unter dem
  Verbandszugang an (`bvbb-<8 Hex>`); das gilt aber nur für Lesepfade, die den
  Kindschlüssel schon kennen — Konsumenten, die nur den Verbandsschlüssel
  kennen (Check-In-Zuordnung, Badge, Live-Seite, Branding-Push,
  Check-In-Admin-Rechte), lösen das
  Kind an je einer Stelle auf (Nachtrag ADR 0054, Final-Review fand fünf
  solche Stellen).
  In bts-light ist die GUID jetzt Pflichtfeld (aus dem Check-In- in den
  Turnier-Abschnitt „1 · Liveticker-Ziel" gezogen), reist in allen
  Push-Nachrichten mit (`tset`, `sched`, `tupdate_match`, `centry_list`,
  `checkin-branding`), Aushang und Dashboard verlinken mit `&g=<GUID>` direkt
  aufs Turnier. **bts-light-Seite umgesetzt (v0.9.276); badhub-Seite: bereits
  gemergt und deployt (Migration 208, badhub-PR #621).** Offen gelassen:
  eigener Check-In-PIN je Kind. Feldtest am nächsten Doppel-Wochenende offen
  (zwei Installationen mit Preset „BVBB" und zwei GUIDs gegen das deployte
  badhub).

- **Zähltafel fürs Tablet + Anzeige-Hülle** — Spec:
  [features/zaehltafel-anzeige-huelle.md](features/zaehltafel-anzeige-huelle.md),
  ADR [0055](adr/0055-zaehltafel-anzeige-huelle-und-zuweisungsziel.md).
  Aus der Idee vom 04.09.2026: ein zweites Tablet am Feld zeigt nur den
  Spielstand — zwei große Punktzahlen, Satzstand klein, Aufschlag-Punkt —
  ohne das zählende Tablet zu stören. Reines Tafel-Layout `tafel.html`
  (`/court/{id}/tafel`, auch als Court-Monitor-Zuweisung „Zähltafel – Feld X")
  plus Tablet-Hülle `anzeige.html` mit Zahnrad/PIN, Layout-Wahl (Tafel,
  Feld-Monitor, Übersicht, Vorbereitung), Spiegeln, Feldwechsel, Wake-Lock.
  Einstiege im Zahnrad-Menü und im Belegt-Overlay des Zähl-Tablets. Zwei PRs
  (Tafel + Ziel, dann Hülle). Feldtest auf iPad ist Abnahmekriterium (iframe).
  PR 1 (Tafel + Zuweisungsziel) umgesetzt in v0.9.274, PR 2 (Hülle +
  Einstiege) umgesetzt in v0.9.275. Offen: Feldtest (iPad-iframe!) und die
  Slave-Brücke (Tafel-Route in `slave_bridge.rs`). Offen außerdem: ein
  generischer Wächter `scripts/test-inline-kopien.mjs`, der die Inline-Kopien
  aller `src/io`-Module in den Assets gegen die Module diffed
  (Security-Review #328); Router-Tests für `/anzeige` und `/court/{id}/tafel`
  (kein Oneshot-Harness im Repo).

- **Kombi-Ausrichtung je Monitor (v0.9.270)** — ob die Felder einer
  Kombi-Anzeige über- oder nebeneinander stehen, wird **je TV** im bestehenden
  Kombi-Dialog gewählt statt global im Setup; der globale Schalter
  `combo_vertical` ist verschwunden (einmalige Migration beim Start). Die
  Ausrichtung wohnt in einer eigenen Geräte-Datei — `relay-proto` und Relay
  bleiben unberührt — und reist im laufenden `/combo/state`-Poll mit, schaltet
  also **ohne Seitenaufbau** um. Nimmt den `rotate`-Fehler in `combo.html` mit
  (hochkant montierte Kombi-TVs navigierten im Sekundentakt neu).
  Spec: [features/kombi-ausrichtung-je-monitor.md](features/kombi-ausrichtung-je-monitor.md) ·
  ADR [0049](adr/0049-kombi-ausrichtung-eigene-geraete-datei.md).
  **Offen:** Feldtest der nicht automatisierbaren Kriterien — drei Felder
  nebeneinander (Lesbarkeit), Gerät offline beim Umstellen, Downgrade.

- **Hintergrundfarbe und Feldbezeichnung je Werbebild (v0.9.247)** — das
  Leerlauf-Vollbild am Court-Monitor bekommt je Bild eine eigene
  Hintergrundfarbe; auf Wunsch steht die Feldbezeichnung darüber (Bild wird
  dafür verkleinert, Schriftfarbe rechnet der Host).
  Spec: [features/werbung-hintergrund-und-feld.md](features/werbung-hintergrund-und-feld.md) ·
  ADR [0041](adr/0041-werbe-stil-je-bild.md).
  **Offen:** Feldtest an einem Turnier (LAN **und** Cloud), insbesondere die
  Rotation mit gemischten Häkchen.

- **Ansage der Besetzung einstellbar (v0.9.246)** — Schiedsrichter fehlten in
  der automatischen Feldansage ganz, die Zähltafelbedienung wurde nur bei
  echter Zuweisung genannt. Beides ist behoben und über zwei Häkchen
  abschaltbar. ADR [0040](adr/0040-ansage-besetzung-einstellbar.md).
  Der Cloud-Ansage-Slave zog in v0.9.248 nach (SR/AR fielen dort nur bei der
  Umwandlung heraus, übertragen wurden sie längst).
  **Offen:** Feldtest — insbesondere in einem Zwei-Hallen-Turnier über Cloud.

- **Automatische Hallen-Vorverteilung** — die vordersten x Spiele bekommen
  automatisch eine Halle im Verhältnis der entsperrten Felder (gemischt,
  fortlaufend nachgefüllt); Halle bindet die Feldvergabe, Auto-Spiele
  brauchen keinen Aufruf mehr; Spieler sehen die Halle früh (Monitore,
  badhub `display=next&halle=…`).
  Spec: [features/hallen-vorverteilung.md](features/hallen-vorverteilung.md) ·
  ADR [0029](adr/0029-hallen-vorverteilung-eigener-store.md) ·
  ADR [0030](adr/0030-halle-bindet-die-feldvergabe.md).
  **Offen:** Feldtest am Mehr-Hallen-Turnier.

- **Spielzeiten-Messung & Startzeit-Prognose (Etappen A+B, v0.9.206)** —
  Brutto-/Nettozeit je Match host-seitig gemessen (`match-times.json`,
  ADR 0027), BTP-`Duration` neustartfest und auf allen Pfaden, Prognose
  („dran ca. hh:mm") + Panel „Spielzeiten" in TL-Web, SetupWizard-Abschnitt.
  Spec: [features/spielzeiten-prognose.md](features/spielzeiten-prognose.md) ·
  Bedienung: [spielzeiten-prognose.md](spielzeiten-prognose.md).
  Etappe C (v0.9.207): Pausen-Countdown + Überziehung in TL-Web, Tablet
  hält die Pause bis „Weiterspielen" (ADR 0028), Behandlungspause sichtbar.
  **Offen:** Feldtest am Testturnier (Erfolgsmaß E12: ±10 min bei ≥70 %,
  Auswertung über die „Prognose-Kontrolle"-Zeilen im Diagnose-Log);
  Relay-Deploy verteilt tablet.html/tl.html automatisch beim main-Merge.

- **TL-Web Panelsystem** — Grundfassung released mit v0.9.203 (PR #218,
  gemergt, Relay-Deploy gelaufen). **Zweite Runde umgesetzt 15.08.2026,
  noch nicht released** (Version 0.9.204 vorbereitet): Die 6 Panels
  (Felder, Walkover, Zähltafel, Schiedsrichter, Spiele, Beendete Spiele)
  sind einzeln aus-/einblendbar, **zuklappbar** (mit Vorschau im Kopf),
  umsortierbar, einer von **1–3 Spalten** zuordenbar und höhenverteilbar;
  benannte, server-seitige **Profile** (an der Geräte-Identität hängend,
  turnierübergreifend) ersetzen das verstreute `localStorage`-Anzeige-Menü;
  dazu ein 3-stufiges Abzeichen-System. Zusätzlich in derselben Runde:
  die manuelle Spielreihenfolge ist jetzt **hallenübergreifend** (ADR
  0026, löst ADR 0023 in der Hallen-Frage ab), die vier
  Wartelisten-Unterabschnitte sind zu einem Panel „Spiele" mit Status als
  Zeilen-Abzeichen zusammengeführt, Feldkachel und Spielzeile haben
  erweiterte ⋮-/⋯-Menüs (u. a. „Nach oben schieben", „Ergebnis
  eintragen", 2. Aufruf je Partei am Feld). Spec:
  [features/tl-web-panelsystem.md](features/tl-web-panelsystem.md)
  (Nachtrag am Ende) · ADR [0024](adr/0024-tl-panel-profile-verwaltung-im-web.md) ·
  [0025](adr/0025-tl-panel-profile-transport-persistenz.md) ·
  [0026](adr/0026-spielliste-eine-globale-reihenfolge-eine-liste.md).

  **Blocker vor dem ersten echten Einsatz:** Das in der Spec verlangte
  **dedizierte Testturnier** steht aus. Es ersetzt hier den sonst
  üblichen Sicherheitsnetz-Fallback — die Datei wurde vollständig
  ersetzt, es gibt bewusst keinen Umschalter zurück auf die alte
  Oberfläche, und ein Rollback mitten im Turnier ist praktisch nicht
  durchführbar. Checkliste in der Spec (iPad Safari · Android Chrome ·
  Wandmonitor; Profil anlegen/bearbeiten/löschen/wählen; Profilwechsel
  übersteht Reload; gelöschtes zugewiesenes Profil fällt auf Standard
  zurück; WLAN aus/an bei offenem Editor; BTP-Neustart; zwei Geräte
  bearbeiten gleichzeitig dasselbe Profil; Steg bindet an den nächsten
  sichtbaren Nachbarn bei ausgeblendetem Zwischen-Panel).

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

- **Ballzähler am Tablet** — Spec:
  [features/tablet-ballzaehler.md](features/tablet-ballzaehler.md)
  (freigegeben 13.11.2026, ADR 0093; BTP-Messung mit
  `tests/btp_shuttles_probe.rs` am 13.11.2026: **Zweig B** — jeder Match-Write
  muss `Shuttles` mittragen). Bälle am Zähl-Tablet
  hochzählen; die Zahl geht mit dem Ergebnis als `Match.Shuttles` nach BTP,
  das TL-Web zeigt sie, die Turnierleitung kann sie beim Eintragen/Korrigieren
  angeben. Schalter im Setup, Standard aus. Vier PRs: Messwerkzeug (gebaut), dann
  PR 1 mit Relay-Deploy vorab.
- **Stream-Werbung (zweite OBS-Ebene)** — Spec:
  [features/stream-werbung.md](features/stream-werbung.md)
  (freigegeben 12.11.2026, ADR 0092). Transparente Sponsoren-Ebene für OBS unter
  `/stream/werbung`: rotierend, nebeneinander oder als Band (Tempo in drei Stufen),
  Hintergrund transparent oder Kachel in Bildfarbe. Auswahl per Häkchen „Stream“ je
  Werbebild im Setup. Relay-Neustart lässt die Bilder stehen (503 statt leer). Zwei PRs,
  PR 1 mit Relay-Deploy vorab.

- **Tablet-Stand robust** — Spec:
  [features/tablet-stand-robust.md](features/tablet-stand-robust.md)
  (freigegeben 11.11.2026; ADR 0083–0085). Paket 1 des
  Kommunikations-Audits: Ersatz-Tablet übernimmt den gesicherten Stand statt 0:0 (Autorität
  nach Fortschritt, Einstieg mit Host-Sätzen, LAN+Cloud über Kreuz), Undo-Format v2 (≤ 40 KB,
  nie still verworfen), geparktes statt blockierendes Alt-Ergebnis (Host-Antwort „überholt"),
  Voll-Sync + 120-s-Nachzügler-Fenster für Verlauf und Zettel. Fünf PRs —
  **alle umgesetzt** (v0.9.374–378, 11.11.2026); offen sind die Release-Tags
  und der Feldtest (Ersatzgerät mitten im Satz, Neustart des Turnier-PCs,
  Ergebnis bei getrenntem WLAN, Karte bei getrenntem Internet).
- **badhub-Push entkoppeln** — Spec:
  [features/badhub-push-entkoppeln.md](features/badhub-push-entkoppeln.md)
  (freigegeben 11.11.2026; ADR 0086/0087). Paket 2 des
  Kommunikations-Audits: ein geordneter Sender-Task für alles an badhub (der Sync-Zyklus
  wartet nie auf das Internet), Fehlerklassen je Kanal mit Backoff 5–60 s ohne Logo, Halt bei
  401/403 bis zum Speichern, 429 nach `Retry-After`, voller `tset` nur bei Strukturänderung
  und alle 60 s, Tablet-Punkte genau einmal, Punktverlauf wertet `verworfen` aus,
  Log-Flanken für BTP und Strecke. PRs 1, 2a, 2b, 3, 4 (PR 5 nur nach Messung) — **PR 1 und 2a
  umgesetzt** (v0.9.381: Fristen, Fehlerklassen, Halt, Drossel, Backoff ohne Logo, Log-Flanken;
  v0.9.382: eigener Sender-Task, der Zyklus wartet nicht mehr auf badhub) sowie **PR 4**
  (v0.9.383: Punktverlauf wertet `verworfen` aus, Änderungsmarken); offen PR 2b und 3 — beide
  nach einer Messung im Feld; kein badhub-Deploy nötig. badhub-Folgearbeit (gzip, Sperre im
  `tupdate_match`-Zweig, Reihenfolge-Prüfung) steht beim Kommunikations-Audit, Paket 2.
- **Relay härten** — Spec:
  [features/relay-haerten.md](features/relay-haerten.md)
  (freigegeben 11.11.2026; ADR 0088–0091). Paket 3 des
  Kommunikations-Audits: keine toten Feld-Slots, Größen- und Mengendeckel auf allen
  WebSocket-Wegen, begrenzte Kanäle (ein voller Kanal trennt den Empfänger), Host-Schleife
  mit einem Ergebnis-Arbeiter je Feld (träges BTP wirft den Turnier-PC nicht mehr ab),
  Verbindungsaufbau mit 10-s-Frist und Backoff-Automat, Anzeigen zeigen nach Host-Verlust
  20 s weiter und dann „Keine Verbindung zum Turnier-PC", Turnierdaten verfallen nach
  60 min (Sprachdienst-Schlüssel sofort), Mindestschutz gegen Schein-Turniere
  (`MemoryMax`, Bilder-Tor, Gesamt-Budget), neue Dashboard-Meldung „Ein anderer PC ist mit
  diesem Turnier verbunden". Neun PRs (1, 2, 9 Host ohne Relay-Deploy; 3–8 Relay in zwei
  Merge-Fenstern) — **alle neun PRs umgesetzt** (v0.9.379: Host-Schleife entkoppelt; v0.9.380: Verbindungsaufbau mit Frist und Backoff-Automat; v0.9.384: ein Feld je Tablet-Verbindung, Halterschaft, Feld-Deckel; v0.9.385: Größendeckel und Mengen-Takt; v0.9.386: begrenzte Kanäle; v0.9.387: Bilder-Tor, Bilder-Budget, Log; v0.9.388: Host-Verlust auf den Anzeigen, Close-Grund; v0.9.389: Daten-Verfall nach 60 min; v0.9.390: Dashboard-Meldung „anderer PC"). Offen: Merges in zwei Fenstern, Handgriff `ops/bts-relay.service` am Server, Feldtest; aus den Reviews vermerkte Restpunkte stehen je PR in der Spec. Offen dazu: Handgriff `ops/bts-relay.service` am Server. Ausgelagerte Punkte stehen beim
  Kommunikations-Audit, Paket 3.
- **Meetingpoint am Kiosk-Tablet** — Spec:
  [features/meetingpoint-am-kiosk-tablet.md](features/meetingpoint-am-kiosk-tablet.md)
  (freigegeben 09.11.2026). Eintrag „Meetingpoint" in der Feldwahl `/felder` (nur wenn an, hinter der
  PIN, belegt kein Feld), Knopf „Zurück zum Meetingpoint" nach WLAN-Rückfall, „Zur Feldwahl" im
  Hüllen-Menü. Nur LAN-Master, keine Auto-Rückkehr.

- **SR und AR schon beim Aufruf in Vorbereitung zuteilen** — Spec:
  [features/sr-ar-beim-aufruf.md](features/sr-ar-beim-aufruf.md) (freigegeben 09.11.2026),
  ADR 0080 (löst 0072 ab) — Rotation teilt beim Aufruf fest zu (auch ohne Meetingpoint),
  Ansage nennt SR/AR, Nachrücken still, Freigabe bei Rücknahme. Nach `sr-rotationsmodus`
  PR 1+2. Folgeidee: Erkennen, dass ein Official selbst in einem laufenden oder gerufenen
  Spiel steht.

- **Rotationsmodus Schiedsrichter/Aufschlagrichter** — Spec:
  [features/sr-rotationsmodus.md](features/sr-rotationsmodus.md)
  (freigegeben 09.11.2026, ADR 0079). Drei Einteilungsprinzipien im Setup: A getrennte
  SR-/AR-Listen, B abwechselnd (Standard, behebt das Kleben der Paar-Rollen), C Rollenwechsel
  je Person nach dem zuletzt beendeten Spiel. Rolle je Person (beides / nur SR / nur AR),
  Meetingpoint-Vorab-SR folgt demselben Modus, Vorschau „als Nächstes SR/AR". Fünf PRs, nur
  PR 5 (TL-Web zwei Listen) braucht einen Relay-Deploy vorab.

- **Monitor-Stile und Stream-Einblendung** — Spec:
  [features/monitor-stile-und-stream.md](features/monitor-stile-und-stream.md)
  (freigegeben 31.10.2026, ADR 0068/0069/0070). Wunsch: die Ansichten aus bup auch für unsere
  Monitore, als Neubau (bup ohne Lizenz). **E1:** Stile je Feld-Monitor im Panel
  (Standard, International, Riesen-Stand, Nur Spieler, Hell, Nur Stand) + turnierweite Plätze
  vor/unter dem Namen (Flagge/Logo/ohne, Vereinsname/Nation/ohne), serverseitig gefiltert;
  Annehmer-Ring am Monitor. **E2:** transparente OBS-Einblendung
  `/stream?courts=` für 1–3 Felder, LAN und Cloud. Je Etappe ein PR und ein Release.

- **Ausgabegerät für Ansagen** — Spec:
  [features/ansage-ausgabegeraet.md](features/ansage-ausgabegeraet.md)
  (freigegeben 31.10.2026, ADR 0065/0066). Auslöser Senior-Cup 31.10.2026: Ansage kam
  nicht über die Klinke an die Hallenanlage. Ton komplett aus dem Rust-Kern
  (cpal/WASAPI), Gerät je Ansage neu aufgelöst; Windows-Stimme (E3 aus
  „Ansage im Browser") vorgezogen; Anzeige und selbstauflösende Banner;
  Gerätewahl je PC. **PR 1 umgesetzt (v0.9.318, #416)**, **PR 1b umgesetzt
  (v0.9.320)**, **PR 2 umgesetzt (v0.9.321)** · **PR 1** Ton aus dem Kern + Windows-Stimme + Anzeige ·
  **PR 1b** OneCore-WAV für die Ansage-Seiten · **PR 2** Gerätewahl.
  Notfall-Schalter „Alter Abspielweg" drei Versionen nach PR 1 entfernen.

- **Seiten und Mannschaften am Zähl-Tablet korrigieren** — Spec:
  [features/tablet-seiten-korrigieren.md](features/tablet-seiten-korrigieren.md)
  (freigegeben 30.10.2026, ADR 0067), **umgesetzt v0.9.319** — offen: Tag,
  Feldtest. ⇄-Knopf ohne PIN: „Seiten tauschen" (alles
  wandert mit) und „Mannschaften tauschen" (nur Namen, gemeldeter Stand kippt;
  im Doppel Neuabfrage der Aufstellung; neues Ereignis `teams_corrected`).

- **Sprachansagen im Browser (Ansage-Seite)** — Spec:
  [features/ansage-im-browser.md](features/ansage-im-browser.md) (freigegeben
  30.10.2026, PR #405). **E1 umgesetzt (v0.9.311, ADR 0063)**, **E1b
  umgesetzt (v0.9.313)**, **E2 umgesetzt (v0.9.314, ADR 0064 — Erst-Ansage
  `court_assigned` im Kern; eigener Feldtest nötig, Rollback nur per Version)**;
  offen E3 (nach Messung), E4, der Feldtest von E1 auf Fire-Tablet,
  Android-Handy und iPhone und der Feldtest von E1b am echten Pi
  (Autoplay-Flag, HDMI-Ton, Stimme nur mit Azure). Ansagen auf Pi/Tablet/Handy ohne
  Windows-App: Turnier-PC vertont (Azure, später OneCore), Seite `/ansage`
  spielt nur ab; Pi am TV über einen Ansage-Modus der Monitor-Seiten.
  - **Etappen:** E1 LAN-Seite + TL-Aufträge/Freitext/Desktop-Knöpfe ·
    E1b Monitor-Modus + Pi-Autoplay · E2 Erst-Ansage im Kern ·
    E3 Offline-Stimme OneCore (**vorgezogen in
    [ansage-ausgabegeraet](features/ansage-ausgabegeraet.md)**, ADR 0066) ·
    E4 Cloud mit eigenem Lese-Schlüssel.

- **TP-Datei während des Turniertags an badhub** — Spec:
  [features/tp-datei-upload.md](features/tp-datei-upload.md),
  [ADR 0060](adr/0060-upload-sperrt-btp-writes-schreib-gate.md).
  Aus der Idee vom 26.10.2026. bts-light kopiert die **offene** TP-Datei
  (Messung `btp_tp_match_probe.rs`, PR #390: lesbar trotz `.ldb`,
  `Match.ID` = `PlayerMatch.id`, GUID in `Settings.LinkCode`) und sendet sie
  gzip-komprimiert an `POST /api/tp-upload`; badhub baut daraus ein
  vorläufiges Turnier (`tournaments.source='tp_live'`) und mischt beim
  Rendern die Liveticker-Stände in Spielliste, Baum und Tabellen.
  - **Stufe 1 — ✅ umgesetzt:** badhub B1–B4 auf Prod (26.10.2026,
    badhub-PRs #1045/#1046/#1055/#1057), bts-light L1 in **v0.9.300**
    (Dateiwahl im Setup, Schreib-Gate, Knopf im Dashboard, Handbuch
    [tp-datei-upload.md](tp-datei-upload.md)).
  - **Stufe 2b Ereignis-Auslöser — ✅ umgesetzt in v0.9.305:** Auslosung
    oder gefüllte Paarung zieht die Takt-Prüfung auf ~30 s vor, abschalt-
    und drosselbar; Spec [features/tp-ereignis-upload.md](features/tp-ereignis-upload.md),
    [ADR 0062](adr/0062-tp-upload-ereignis-ausloeser.md).
  - **Stufe 2 — ✅ umgesetzt in v0.9.301:** Automatik, die nur solange der
    Liveticker läuft und nur bei geänderter Datei sendet (ungegateter
    SHA-256, Halt mit Wiederanlauf, Nachversuch 90 s × 2); Spec
    [features/tp-takt-upload.md](features/tp-takt-upload.md),
    [ADR 0061](adr/0061-takt-upload-lebenszyklus-schnellhash-anhalteregel.md).
    Standard aus.
  - **Turnier-Kennung aus der Datei — ✅ umgesetzt in v0.9.302:** GUID per
    Byte-Muster `LinkCode` aus der TP-Datei (kein Jet-Parser); Übernahme im
    Setup, Ablehnung fremder Dateien vor dem Senden (Ergänzung in der
    Takt-Spec). Offen: Datei über den BTP-Prozess selbst finden.
  - **Reihenfolge:** badhub B1–B4 zuerst (`AFFECTS` badhub), dann bts-light L1 — eingehalten.
  - **Vor dem Feldtest:** Messung A2 (Kopie bei BTP-Eingaben), Jet-Nachlauf.
    Lizenzfrage aus badhub-ADR 0047 **geprüft 28.10.2026** (badhub
    `docs/drafts/tp-datei-weitergabe-einschaetzung.md`): keine Einschränkung im
    vorliegenden EULA; offen sind eine schriftliche Bestätigung von Visual
    Reality, ein Freigabe-Satz des BVBB-Spielausschusses und die Ergänzung von
    AVV § 13.2 und Datenschutzerklärung (Projektleitung badhub). Für
    BVBB-Turniere vertretbar, für andere Verbände erst nach der AVV-Ergänzung.

- **Setzplätze anzeigen** — Spec:
  [features/setzplaetze-anzeigen.md](features/setzplaetze-anzeigen.md),
  [ADR 0059](adr/0059-setzplatz-als-text-aus-dem-parser.md).
  Aus der Nutzerfrage vom 12.10.2026. BTP liefert die Setzung an
  `StageEntry.Seed1/Seed2` (Messung `btp_seed_probe.rs`).
  - **Anzeige:** `[1]`, `[3/4]` auf Monitoren (ohne Schalter), im TL-Web
    (Profil-Schalter, Standard an) und im Desktop-Check-In.
  - **Etappen:** PR 1 bts-light, PR 2 Payload an den badhub-Liveticker und
    den Check-In, PR 3 Anzeige in badhub (`AFFECTS` badhub).

- **Offene Paarungen in der TL-Spielliste** — Spec:
  [features/tl-offene-paarungen.md](features/tl-offene-paarungen.md),
  ADR [0051](adr/0051-offene-spiele-eigene-gedeckelte-liste.md) /
  [0052](adr/0052-beschriftung-offener-plaetze.md) /
  [0053](adr/0053-offene-spiele-in-der-manuellen-reihenfolge.md).
  Aus der Nutzer-Idee vom 29.08.2026: Spiele, deren Teilnehmer noch nicht
  feststehen, fehlen in der TL-Liste komplett, obwohl BTP sie führt — am
  2-Hallen-Mitschnitt tragen 22 von 36 Spielzeilen mindestens einen offenen
  Platz. Künftig erscheinen sie eingereiht, mit den Kandidaten aus dem Vorspiel
  beschriftet („Müller oder Schmidt" → „aus Spiel 42" → „noch offen"), per
  Geräteschalter ausblendbar (Standard: an). Sie nehmen an der manuellen
  Reihenfolge teil, aber **nicht** an Feldvergabe, Vorbereitungs-Aufruf und
  Startzeit-Prognose. Umsetzung in 14 Schritten, Ziel v0.9.273.
  **Vorbedingung:** Messung, ob BTP Freilose als eigene Spielzeile liefert
  (Schritt 0) — an den vorhandenen Fixtures nicht entscheidbar.
  Reihenfolge: **vor** Spur B des Verschiebe-Modus umsetzen.

- **Spiele umsortieren ohne langes Ziehen (TL-Web)** — Spec:
  [features/tl-spielliste-sprung-umsortieren.md](features/tl-spielliste-sprung-umsortieren.md),
  [ADR 0050](adr/0050-verschiebe-modus-globales-einfuegeziel.md).
  Aus der Feldtest-Rückmeldung vom 26.08.2026 („das Verschieben zusammen mit dem
  Scrollen ist hakelig", Wartelisten von 50–60 Spielen). **Spur A** — die
  Zieh-Geste selbst geschmeidig machen — ist bereits umgesetzt (v0.9.271, siehe
  Nachtrag in [features/spielliste-manuelle-reihenfolge.md](features/spielliste-manuelle-reihenfolge.md)).
  Diese Spec ist **Spur B**: ein Verschiebe-Modus, der die gehaltene Geste
  auflöst — Spiel über das ⋮-Menü merken, **normal** scrollen, Zielzeile
  antippen. Dazu wird der seit ADR 0026 vorhandene, aber undokumentierte Befehl
  „↑ Nach oben schieben" als sichtbarer Knopf an die Zeile geholt; der Grill
  hatte ergeben, dass Fall 1 zum Teil ein **Auffindbarkeitsproblem** war.
  Bewusst ohne Serveränderung: kein Tauri-Command, kein `TlAction`, keine
  Config-Felder — reines `assets/tl.html` + `src/io/queueGap.mjs`. Offen ist,
  ob der Feldtest von Spur A abgewartet wird; möglicherweise nimmt das
  geschmeidige Ziehen den Schmerz allein.

- **Verschlüsselte LAN-Strecke (https + wss)** — Spec:
  [features/lan-tls-verschluesselt.md](features/lan-tls-verschluesselt.md).
  Löst die nie implementierte Entscheidung [ADR 0005](adr/0005-lan-https-selbstsigniert.md)
  ein: zusätzlicher TLS-Port 443 (Rückfall 8443) mit selbstsigniertem,
  persistiertem Zertifikat,
  **8088 bleibt offen** (Bestands-Pis unberührt). Bringt den Tablets den Secure
  Context und damit die **Akku-Anzeige im LAN-Betrieb**. Zwei neue Crates
  (`rcgen`, `tokio-rustls`). Bewusst kein Pi-Rollout, kein Zwangs-Umschalten —
  ein Origin-Wechsel im laufenden Turnier würde unbestätigte Ergebnisse aus dem
  `localStorage` verlieren. Voraussetzung für die
  **Transport-Bündelung in der fernen Halle** (siehe nächster Punkt).

- **Transport-Bündelung in der fernen Halle (Multiplexer)** — Spec:
  [features/ferne-halle-transport-buendelung.md](features/ferne-halle-transport-buendelung.md).
  Der Slave-PC wird vom 302-Weiterleiter zum lokalen Terminator: Tablets,
  Court-Monitore und Übersichtsanzeigen sprechen nur noch mit ihm, ihr
  **WebSocket**-Verkehr läuft gebündelt über **eine** Trägerverbindung zum Relay
  (HTTP bleibt außen vor — sonst drohte Head-of-Line-Blocking durch die 12-MB-
  Werbebilder). Holt die in [ADR 0002](adr/0002-ferne-halle-direkt-cloud-geraete.md)
  aufgeschobene **Transport-Hälfte** von „Weg B" nach, **ohne** Offline-Pufferung.
  Bewusst dumm: kein Zustandsspiegel, kein schreibender Rückkanal, TL-Web bleibt
  Direkt-Cloud. Kern: eigener `mpsc`-Kanal je Substrom, damit die acht
  `same_channel`-Stellen und mit ihnen R4, ADR 0017 und ADR 0020 unverändert
  gültig bleiben. Config-Default **aus** (Opt-in). Setzt die TLS-Spec voraus.
  Spec **freigegeben 25.08.2026**, Entscheidung in
  [ADR 0048](adr/0048-substrom-adressierung-traeger.md); Umsetzung noch nicht
  begonnen. Reihenfolge: Relay-Etappen zuerst — sie sind rückwärtskompatibel
  und für sich wirkungslos, und der Relay deployt bei jedem main-Merge.

- **Schiedsrichterzettel vorab und automatisch drucken — VOLLSTÄNDIG
  umgesetzt** (E1–E6, v0.9.249). Zwei Wege aufs Papier: ein **Leerzettel** für
  Spiele der Warteliste (Kopf vorgedruckt, Raster von Hand zu führen; Knöpfe in
  TL-Web und in der Desktop-Warteliste) und ein **stiller Autodruck** bei der
  Feldvergabe an einen einstellbaren Drucker — nur für Spiele, denen ein
  Schiedsrichter zugeordnet ist, höchstens ein Blatt je Spiel, auch über
  App-Neustarts hinweg. Dazu der **Umbau des Blatts auf den DBV-Bogen**
  (sechs Blöcke à 33 Spalten, A/R-Spalte, Satzergebniskasten; Turnierlogo statt
  Verbandsmarke; Marker W/F/R/D; der Vermerk „kein amtlicher Beleg" entfällt).
  **E1** Blatt als Elementliste, **E2** Vorabzettel-Modus (Wire + Routen),
  **E3** Knöpfe in Desktop und TL-Web, **E4** stiller GDI-Druck +
  Druckerauswahl, **E5** Autodruck mit persistentem Druck-Gedächtnis,
  **E6** Abschluss.
  Spec: [features/schiedsrichterzettel-autodruck.md](features/schiedsrichterzettel-autodruck.md) ·
  ADR [0042](adr/0042-stiller-druck-ueber-elementliste.md) ·
  [0043](adr/0043-zettelblatt-nach-dbv-vorbild.md).

  **Offen und nur am Gerät prüfbar:** ein echter Papierdruck auf einem
  Laserdrucker (bisher gegen „Microsoft Print to PDF" nachgewiesen: A4 quer,
  Ränder stimmen) und der Turnier-Feldtest der Automatik — insbesondere, ob
  die Zettel früh genug am Feld liegen. Relay-Deploy vor dem Tag: E2 erweitert
  den `scoresheet_request`-Frame.

- **Ausgefüllte Schiedsrichterzettel drucken — VOLLSTÄNDIG umgesetzt** (E1–E8,
  v0.9.244). **E1** Wire-Typen neben dem Punktverlauf, **E2** `SheetStore`
  (append-only, vereinigt statt zu ersetzen), **E3** Ingest über LAN, Cloud und
  Relay, **E4** Projektion auf das Zellenraster, **E5** ein Renderer und drei
  Lesepfade, **E6** Erfassung am Tablet, **E7** Ausgabewege in Desktop und
  TL-Web, **E8** Abschluss. Karten überleben jetzt einen Gerätewechsel und
  landen auf einem druckbaren Blatt — **internes Turnier-Archiv, kein amtlicher
  Beleg**. Bedienung: [schiedsrichterzettel.md](schiedsrichterzettel.md) ·
  Spec: [features/schiedsrichterzettel-druck.md](features/schiedsrichterzettel-druck.md) ·
  ADR [0037](adr/0037-zettel-ereignisse-eigener-strom.md) ·
  [0038](adr/0038-ereignisse-append-only.md) ·
  [0039](adr/0039-zettel-html-im-webview.md).

  **Offen und nur am Gerät prüfbar:** der Druck-Test unter Windows-WebView2 und
  Android-Chrome sowie ein Turnier-Feldtest mit parallel geführtem Papierbogen.
  Fällt das Seitenbild aus, ist ADR 0039 neu zu bewerten.

- **Monitor-Livestand per Push — VOLLSTÄNDIG umgesetzt** (S0–S7, v0.9.235–242,
  PRs #256–#264). **S0** Messung (Zähler, 10-Sekunden-Log-Zeile,
  `GET /debug/perf` nur LAN, Lastskript `scripts/last-monitor.mjs`), **S1**
  Antwortcache mit 250-ms-Hart-TTL und ETag/304, **S2** entprellte
  `live-scores.json`, **S3** Zuweisungs-Nudge in Host und Relay, **S4** `seq`
  in den Voll-Antworten, **S5** Teil-Patch statt Board-Neuaufbau, **S6**
  Herzschlag + neue Gesundheits-Definition + 4-s-Schalter (Default aus),
  **S7** schmaler Abruf `/health?court=<id>`.

  **Nachmessung vom 19.08.2026 (v0.9.242, 20 belegte Felder, Debug-Build):**
  Im Leerlauf **−99 % Nutzdaten** (1,13 MB/s → 0,01 MB/s, 99 % der Antworten
  sind „nichts Neues") und **−87 % Vollberechnungen** (74,0 → 9,8 je Sekunde);
  mit gesetztem Schalter **−93 % Abrufe** (75,4 → 5,3/s). Im Spielbetrieb mit
  20 zählenden Tablets: **Latenz Punkt → Anzeige p50 15 ms / p95 68 ms** gegen
  eine 300-ms-Grenze, und `persist_scores` läuft **56-mal für 222 Punkte** mit
  6,6 ms statt bei jedem Punkt mit 20,0 ms. Die Ausbaustufe „Nutzlast im
  Nudge" wird damit **nicht** gebaut — ihr Auslösekriterium waren mehr als
  20 Requests/s je Übersichts-Gerät, gemessen sind 7,45/s.

  **S8** (v0.9.243) kam nach der Nachmessung dazu: Die Bestätigung „nichts
  Neues" gab es nur im Hallennetz, in der Cloud lief jeder Abruf mit vollem
  Rumpf (0,61 gegen 0,01 MB/s).

  **Offen:** die Pi-Zeile (`ovRenderMessen` auf einem echten Kiosk) — Feldtest,
  kein Code. Der Release-Tag (die App ist seit v0.9.226 nicht getaggt). Und
  **S9** ist inzwischen umgesetzt (v0.9.245): Das Relay führt jetzt eine
  Anzeige-Revision und legt die fertige Übersicht darunter ab — die
  Bestätigung spart damit nicht mehr nur Bytes, sondern auch die Rechnung.
- **Cloud-Aufruf-Uhr driftet** (vorbestehend, gefunden 19.08.2026): Die
  Übersicht rechnet `on_court_since_ms` (Stempel des **Turnier-PCs**, vom Host
  hochgeladen) gegen `serverNowMs` (Uhr des **Relays**). Beide Uhren laufen auf
  verschiedenen Rechnern; „Zeit seit Aufruf" driftet im Cloud-Betrieb um deren
  Differenz. Sauber wäre, den Stempel beim Eintreffen auf die Relay-Uhr
  umzurechnen oder den Versatz mitzuliefern.
  Fundstelle: `relay/src/main.rs` `overview_health`.
- **Monitor-Livestand per Push** — ein gezählter Punkt soll nur noch kosten,
  was er wert ist. Heute weckt jeder Punkt jede Übersichts-Anzeige, die
  daraufhin den Zustand **aller** Felder holt und ihr Board komplett neu
  aufbaut (grob 1,6–8 MB/s WLAN bei 20 Feldern × 20 TVs für ~20 Byte
  Information). Sieben Etappen, Reihenfolge durch eine Vorab-Messung
  gesteuert: Perf-Zähler + Lastskript (S0), Antwortcache für `/health` (S1),
  Entprellung von `persist_scores` — heute ein Vollschreibvorgang **je
  Punkt** (S2), Zuweisungs-Nudge (S3, schließt die beiden offenen
  A1-TODOs), `seq` in den Voll-Antworten als Ordnung zwischen Push und
  Abruf (S4), Teil-Patch statt Board-Neuaufbau (S5), neue Definition von
  „Push-Kanal ist gesund" mit sichtbarem Heartbeat und 4-s-Fallback
  (S6, Config-Schalter, Default aus), schmaler Abruf `/health?court=<id>`
  (S7). „Nutzlast im Nudge" ist bewusst **zurückgestellt** und wird nur bei
  Verfehlen der Nachmess-Schwellen gebaut.
  Spec: [features/monitor-livestand-push.md](features/monitor-livestand-push.md) ·
  ADR [0035](adr/0035-monitor-livestand-ordnung.md).
- **Feinschliff Turnierleitungssicht (18.08.2026)** — vier unabhängige
  Punkte aus dem Turnierbetrieb: (1) die Spielzeiten-Statistik wird
  **mehrachsig** (Klasse · Disziplin · Halle · Klasse×Disziplin), die Achse
  ist eine Profil-Einstellung; die Halle wird dafür beim E4-Stempel im
  Messwert festgehalten und bleibt **reine Anzeige** (Prognose unberührt).
  (2) **Nachruf für Zähltafelbediener** am Feld-Aufruf („… bitte als
  Tabletbedienung melden") — der seit ADR 0007 offene Baustein; der
  Vorbereitungs-Aufruf bleibt bewusst außen vor. (3) Neue TL-Web-Ansage
  **„Feld X. Bitte mit dem Spielen beginnen."**, die die Aufruf-Zählung
  **nicht** anfasst. (4) **Spielerlinks** auch in laufenden Feld-Kacheln und
  bei beendeten Spielen — hebt die Beschränkung der Lizenznummern auf die
  Warteliste auf (beide Wächter-Tests werden angepasst und begründet).
  Spec: [features/tl-sicht-feinschliff.md](features/tl-sicht-feinschliff.md) ·
  ADR [0036](adr/0036-hallen-achse-im-messwert.md) (Punkt 1) ·
  Nachtrag an ADR [0007](adr/0007-zaehltafelbediener.md) (Punkt 2).
  **Umsetzung in vier PRs, Reihenfolge 4 → 3 → 1 → 2** (Nutzer-Vorgabe:
  Spielerlinks und Spielbeginn-Ansage zuerst). Punkt 2 und 3 brauchen
  **Relay-Deploy vor dem Client-Tag** (neue `TlAction` → altes Relay
  antwortet 422); der Punkt-3-PR trägt zusätzlich die Absicherung, die
  einen Ansage-Slave mit älterem Stand vor dem Verlust der ganzen
  Auftragscharge schützt.

- **Hallen-Farben** — jede Halle eines Mehr-Hallen-Turniers bekommt eine
  Farbe (Auto-Palette, deterministisch alphabetisch; Übersteuerung per
  Palettenton auf der Felderübersicht), sichtbar als Marke neben jeder
  Hallen-Nennung: Desktop, TL-Web, Monitor-Seiten (LAN + Cloud) und
  badhub-Aushang (display=next + display=monitor; badhub-Anzeige als
  Folge-PR im badhub-Repo). Farbe nie einziger Informationsträger;
  Ein-Hallen-Turniere bleiben unberührt.
  Spec: [features/hallen-farben.md](features/hallen-farben.md) ·
  ADR [0031](adr/0031-hallen-farben-eigener-config-store.md) ·
  [0032](adr/0032-hallen-farben-deterministische-auto-palette.md) ·
  [0033](adr/0033-hallen-farben-hex-auf-dem-draht.md).
- **Schiedsrichtermanagement** — BTP-Schiedsrichterliste in BTS Light:
  SR/AR je Spiel zuweisen (Client + TL-Web, auch bei laufendem Spiel),
  Konflikt-Warnungen (Verein/Sperr-Spieler, nie blockierend), automatische
  Rotation mit Pausen und manueller Reihenfolge (getrennt SR/AR),
  feldweise Schalter inkl. Tabletbediener-Vergabe (drei Mischformen),
  Ansagen, Tablet-Anzeige (LAN + Cloud + ferne Halle), Rücksync nach BTP
  bei **jeder** Zuweisungsänderung. **Messung erledigt 13.08.2026**
  (`btp_officials_probe.rs`): kein Verein am Official ⇒ Pflege in
  BTS Light; Official1=SR/Official2=AR; Writes werden angenommen
  (asynchron ≤1 s).
  Spec: [features/schiedsrichter-management.md](features/schiedsrichter-management.md) ·
  ADR [0021](adr/0021-officials-ruecksync-eigenstaendiger-write.md) ·
  ADR [0022](adr/0022-officials-turnierdaten-eigene-datei.md).
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

- **Linux-Variante (Entwickeln zuerst, Betreiben später).** Der Kern ist
  plattformneutral, BTP dagegen ein Windows-Programm — ein bts-light auf
  Linux spricht immer ein BTP auf einem anderen Rechner an (`btp.host` ist
  frei konfigurierbar, das geht heute schon). Vier Stufen:
  - **Stufe 0 — Linux als Test-Plattform (umgesetzt 15.09.2026):** CI-Job
    `linux` (`ubuntu-22.04`, Tauri-System-Pakete) mit Clippy `-D warnings`
    und `cargo test --workspace`. Hält den `not(windows)`-Zweig grün
    (GDI-Druck → `NichtUnterstuetzt`, SSID → `iwgetid`), der seit v0.9.249
    nie gebaut worden war. Der Windows-Job bleibt die Release-Referenz.
  - **Stufe 1 — Linux-Paket im Release:** zweiter Build-Job `build-linux`
    in `release.yml` (`.AppImage` + `.sig`, `.deb`), Eintrag
    `linux-x86_64` in `latest.json`, Download-Zeile auf der Release-Seite.
    Kein Firewall-Hook (nur NSIS); bei `ufw` müssen 8088/8443 von Hand auf
    — Doku.
  - **Stufe 2 — Auto-Update auf Linux:** Der Einbau ist selbst gebaut
    (`update.rs`: Installer laden, `/S /UPDATE` starten). Auf Linux gibt es
    keinen Installer; der Tauri-Updater tauscht beim AppImage die Datei.
    Braucht einen `cfg`-Zweig und eine Entscheidung, ob „Beim Beenden
    einbauen" (ADR 0057) für AppImage nachgebaut oder ausgeblendet wird.
    `.deb` bekäme kein Auto-Update (Paketmanager).
  - **Stufe 3 — Zettel-Autodruck über CUPS:** zweiter Ausgabeschritt
    Elementliste (`blatt.rs`) → PDF → `lp -d <Drucker>`, Druckerliste über
    `lpstat -a`; deckt später auch macOS ab. Bis dahin geht auf Linux nur
    der Browser-Druck (Vorabzettel, TL-Web), und der Setup-Abschnitt sollte
    das sagen statt eine leere Druckerliste zu zeigen.
  - Bleibt: Messwerkzeuge (`tests/*_probe.rs`) und neue BTP-Mitschnitte
    brauchen Windows. Stufen 1–3 vor dem Code durch `/idee` (Spec
    `docs/features/linux.md`); macOS ist dieselbe Arbeit plus
    Apple-Signierung (Developer-Konto) — ohne die widerspricht es dem
    Plug-and-play-Anspruch.

- **Update ohne jede Lücke (Stufe 3): Prozess-Übergabe oder Dienst +
  Oberfläche.** Seit v0.9.279 (Spec
  [features/update-im-turnierbetrieb.md](features/update-im-turnierbetrieb.md),
  ADR 0057) ist die Update-Lücke eine Sache von ~20 s: Paket vorgeladen,
  Übertragung läuft danach von selbst wieder an, Feldstempel überleben,
  „Beim Beenden einbauen" als Alternative. Was bleibt, ist der Prozess-
  Neustart selbst: LAN-Server (8088/8443), Relay-WebSocket, BTP-Verbindung
  und Monitor-Sockets sind für Sekunden weg. Echte Nullzeit hieße, diese
  Verbindungen an einen neuen Prozess zu übergeben (Socket-Handover unter
  Windows) oder den Kern als Dienst vom Fenster zu trennen (die Oberfläche
  startet neu, der Dienst nicht — und der Dienst selbst bräuchte dann
  wieder einen Übergabe-Mechanismus). Beides ein großer Umbau; Nutzen
  gegenüber Stufe 1+2 erst messen (Feldtest: wie oft wird überhaupt
  mitten im Turnier aktualisiert?), dann entscheiden.
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
- **Handbuch Stufe 2 — die fehlenden Kapitel.** *Status: **abgeschlossen
  06.09.2026** · Trigger: Wunsch vom 05.09.2026, nach Freigabe von Stufe 1
  (PR #333).* Alle vier Themen sind geschrieben und veröffentlicht; dazu kamen
  Suche, Stichwortverzeichnis und ein aufgabenorientierter Einstieg.
  Ursprünglicher Wortlaut zur Nachvollziehbarkeit:* Stufe 1 veröffentlicht die vorhandene Doku unter
  `badhub.de/download/bts-light/handbuch/`. Gewünscht ist darüber hinaus ein
  Handbuch, in dem **nachlesbar** ist:
  - ~~was die **Tablets** können~~ — **erledigt 06.09.2026**:
    [tablet-bedienen.md](tablet-bedienen.md), aus Sicht der Person am Feld;
  - ~~**was welche Einstellung in der Software bewirkt**~~ — **erledigt
    06.09.2026**: [einstellungen.md](einstellungen.md), alle Abschnitte der
    Einstellungsseite plus Wartung, mit Standardwerten,
    Sichtbarkeitsbedingungen und der Frage, ab wann eine Änderung wirkt;
  - ~~wie der **Master-/Slave-Betrieb** funktioniert~~ — **erledigt
    06.09.2026**: [master-slave.md](master-slave.md), aus Sicht der
    Turnierleitung statt der Architektur, mit den vier häufigsten
    Missverständnissen;
  - ~~was die **Oberflächen** jeweils können~~ — **erledigt 06.09.2026**:
    [oberflaechen.md](oberflaechen.md), Überblick über App-Menü,
    Hallen-Geräte und öffentliche Seiten.

  Dazu **erledigt 06.09.2026**: [erste-schritte.md](erste-schritte.md)
  (Installation, Voraussetzung in BTP, erster Start, Beenden vs. Minimieren,
  Updates im Turnierbetrieb) und die Wartungsseite als Teil von
  [einstellungen.md](einstellungen.md). Offen bleibt ein Überblick über die
  Oberflächen (welcher Bildschirm zeigt was).
  **Der Aufwand liegt im Schreiben, nicht in der Technik** — die Strecke
  Markdown → Website → Deploy steht. Neue Kapitel entstehen als normale
  `docs/*.md` und werden in [handbuch.json](handbuch.json) eingetragen; es
  bleibt bei einem Doku-Ort. Entscheidungen und Grenzen:
  [features/handbuch-website.md](features/handbuch-website.md).
- **Handbuch: kürzere Adresse (nginx).** *Status: zurückgestellt · Priorität:
  niedrig · Trigger: Messung nach dem ersten Deploy, 06.09.2026.* Heute muss
  `…/download/bts-light/handbuch/index.html` ausgeschrieben verlinkt werden —
  die Verzeichnis-URL liefert 403 (`index`-Direktive greift für den neuen
  Unterordner nicht). Eine nginx-Änderung (`index index.php index.html;` für
  diesen Pfad, oder ein `alias` von `/bts-light/`) macht daraus eine
  vorzeigbare Adresse. **Server-Änderung mit eigener Freigabe** — über
  `/server-change`, nicht nebenbei.
- **Handbuch: Screenshots.** *Status: zurückgestellt · Priorität: niedrig ·
  Trigger: erst wenn die Textfassung von Stufe 2 steht.* Für Vollabdeckung
  rund 45–60 Aufnahmen. Die Web-Oberflächen (Tablet, TL-Web, Monitor, Aushang,
  Werbung) sind reine HTML-Seiten des eingebetteten Servers und damit gegen
  einen Mock-Stand automatisierbar; nur die Tauri-Fenster bleiben Handarbeit.
  Bewusst nachgelagert: Screenshots veralten mit jedem UI-Umbau, und der
  Pflegeaufwand ist der eigentliche Dauerposten des Handbuchs.
- **Feld-Raster per Drag & Drop anordnen.** Das Feld-Raster
  (Spaltenzahl + Start-Ecke + Schlange, [features/feld-raster.md](features/feld-raster.md))
  deckt rechteckige Hallen ab; für unregelmäßige Hallen wäre eine frei
  ziehbare Anordnung je Feld komfortabler. Bewusst verschoben beim
  Erstwurf — Persistenz je Feld statt je Halle, eigene Speicher-UI,
  Zusammenspiel mit wechselnden Feldzahlen.
- **Anstoß der Suche aus `tablet.html` über die JS-Brücke
  (Tablet-Kiosk-App).** Bekommt der Turnier-PC mitten im Turnier eine neue
  IP-Adresse, sieht die Tablet-Kiosk-App das nicht von selbst — die Suche
  läuft bewusst nur bei App-Start, WLAN-Wechsel, Ladefehler oder Handgriff,
  nie als Hintergrund-Takt. Ein Anstoß direkt aus der geladenen Seite über
  die `fully`-JS-Brücke, sobald sie „Verbindung verloren" erkennt, wäre die
  saubere Lösung, bräuchte aber eine Änderung an `tablet.html` — bewusst aus
  der ersten Fassung ausgeklammert.

## Kommunikations-Audit (Befunde 10.11.2026)

Durchsicht aller Strecken — Tablet ↔ Host, Anzeige-Geräte ↔ Host/Relay,
Host ↔ BTP/badhub, Relay + TL-Web — auf Robustheit, Performance und
Datensparsamkeit. **Reine Code-Lektüre, nichts ausgeführt oder gemessen**;
Größen- und Bandbreitenangaben sind gerechnet. Die Kernstrecken
(Ergebnis-Übermittlung mit Idempotenz und Platten-Queue, Monitor-Push S0–S9,
TL-Web-Zugang, TP-Upload) waren ohne Befund. Zeilennummern: Stand main
`536d8b2`.

Drei Arbeitspakete, in dieser Reihenfolge; dazu Einzelpunkte.

### Paket 1 — Tablet-Stand

**Spec:** [features/tablet-stand-robust.md](features/tablet-stand-robust.md)
(freigegeben 11.11.2026; ADR 0083–0085). Der Grill hat die Befunde
nachgeprüft und teils korrigiert — maßgeblich ist die Spec, die Stichpunkte hier sind der
Audit-Stand.

- **Ersatz-Tablet an freiem Feld startet bei 0:0 und überschreibt den
  gesicherten Stand.** Fällt ein Tablet mitten im Spiel aus, gibt der Server
  den Slot nach 10 s (LAN) bzw. 15 s (Relay) frei. Ein frisches Gerät bekommt
  dann `authoritative=true` (`state.rs` `reconnect_decision`, Regel „Halter
  vorher leer") und setzt im Ownership-Zweig seinen leeren Stand durch
  (`tablet.html:1857`); das folgende `match_assigned` lädt 0:0, und der
  `state_sync` überschreibt den Cache. Verlauf, Aufschlag, Positionen und
  Ereignisse sind weg. ADR 0017 beschreibt „frisches Gerät übernimmt
  Server-Stand" nur für den Legacy-Zweig. **Vorschlag:** lokal nur
  durchsetzen, wenn ein lokaler Stand zum selben Match existiert, sonst
  adoptieren; Test „frisches Gerät, freier Slot".
- **`state_sync` schickt je Punkt den vollen Zustand; in der Cloud wird er ab
  etwa dem dritten Satz still verworfen.** Der Snapshot trägt `history` (50
  Undo-Stände), jeder davon eine Vollkopie von `rallyLog` und `events`
  (`tablet.html:3180`). Nachgemessen (11.11.): 61–66 KB am Ende eines normalen
  Dreisätzers, im Schiri-Modus 81 KB ab Mitte Satz 2 und bis 590 KB.
  Über `MAX_STATE_LEN` (64 KB) verwirft das Relay kommentarlos
  (`relay/src/main.rs:3736`): Cloud-Monitore zeigen alten Aufschlag/alte
  Pause, eine Übernahme bekommt einen viele Punkte alten Stand. Im LAN
  30–70 KB je Punkt über das Hallen-WLAN plus derselbe String ins
  `localStorage`; `set_court_state` hat dort gar keinen Deckel. Unterhalb der
  Grenze reicht das Relay den vollen Zustand an jeden Cloud-Monitor (der Host
  verschlankt im LAN, das Relay nicht). **Vorschlag:** `rallyLog`/`events`
  nicht in jeden Undo-Stand kopieren (Schnittposition reicht) oder `history`
  nicht spiegeln; Verwerfen loggen; im Relay verschlanken.
- **Ein nicht mehr zustellbares `pendingResult` überlebt den Matchwechsel und
  blockiert das nächste Ergebnis.** `submit` bricht bei gesetztem Auftrag ab
  (`tablet.html:4387`); weder `loadMatch` noch `clearMatch` räumen ihn, und
  losgelassen wird er nur bei `finalized` desselben Matches. Trägt die TL das
  Ergebnis von Hand ein und kommt das Tablet erst nach Ablauf der 5-min-Netze
  (`RESULT_IDEMPOTENCY_TTL`, `FINALIZED_TTL`) zurück, läuft der Retry endlos,
  und am Ende des Folgespiels ist „Ergebnis übermitteln" gesperrt — auch nach
  Reload. **Vorschlag:** fremden Auftrag in `loadMatch`/`clearMatch`
  loslassen und loggen; Host antwortet `permanent`, wenn das Match schon
  einen Sieger hat.
- **Cloud: nach einem Abriss Host↔Relay friert der Punktverlauf des laufenden
  Spiels ein.** Ballwechsel und Zettel-Ereignisse aus der Lücke verwirft das
  Relay (kein Puffer, `relay/src/main.rs:3930 ff.`); danach lehnt der Host
  jedes weitere `rally`-Frame ab (`timeline.rs`, Lückenprüfung), bis zufällig
  ein Voll-Sync kommt. Graph, badhub-Verlauf und Zettel werden abgeschnitten
  finalisiert; eine Karte aus der Lücke erreicht den Host nie. **Vorschlag:**
  im `match_assigned`-Zweig für dasselbe Match `sendRallySync()` +
  `sendMatchEventSync()`; Voll-Sync vor `submit`.

### Paket 2 — badhub-Push entkoppeln

**Spec:** [features/badhub-push-entkoppeln.md](features/badhub-push-entkoppeln.md)
(freigegeben 11.11.2026; ADR 0086/0087). Der Grill hat die Befunde
nachgeprüft und teils korrigiert — maßgeblich ist die Spec, die Stichpunkte hier sind der
Audit-Stand. Korrigiert sind vor allem: verzögert wird der *nächste* Zyklus (die Halle läuft
im Zyklus vor den Pushes), das Überholen kommt von zwei unabhängigen Sendern, der
Punktverlauf verliert still statt zu blockieren, und gzip fällt aus (siehe unten).

**badhub-Folgearbeit** (eigenes Repo, nicht Teil der Spec): badhub nimmt am
Liveticker-Endpunkt kein gzip an (roh an `json_decode`); im `tupdate_match`-Zweig fehlt eine
Sperre bzw. Transaktion (parallele Punkte verschiedener Felder können sich wegschreiben);
es gibt keine Reihenfolge-Prüfung (`rid` wird nur zurückgespiegelt). bts-light entschärft
die beiden letzten Punkte clientseitig durch die serielle Strecke; komprimieren kann es
erst, wenn badhub gzip annimmt.

- **Hängendes badhub streckt den Sync-Zyklus von 5 s auf bis zu ~75 s.** Vier
  Pushes (`tset`, Meldeliste, `sched`, `tverlauf`) laufen nacheinander in
  `run_once` (`sync.rs:2937–2957`), jeder mit 15 s Gesamt-Timeout und ohne
  `connect_timeout` (`badhub/push.rs:67`). Bei totem Uplink leiden
  Feldvergabe, SR-Rotation, Autodruck, Erst-Ansage und `set_snapshot` — also
  der reine LAN-Betrieb. **Vorschlag:** Pushes in einen eigenen Task („neuester
  Stand gewinnt"); mindestens `connect_timeout` 3–5 s und Additives
  überspringen, wenn der `tset` am Transport scheiterte.
- **Der Fehlerpfad sendet die teuerste Nutzlast im schnellsten Takt.**
  `on_failure` setzt `last_pushed = None` (`sync.rs:2997`) → alle 5 s voller
  `tset`, samt Logo (bis ~2,7 MB Base64), solange es nicht gestempelt ist.
  Kein Backoff, auch nicht bei 401. Auf schmalem Uplink passt das nie in die
  15 s. **Vorschlag:** exponentieller Backoff, lange Pause bei 401/403,
  Wiederholung ohne Logo, Timeout an die Körpergröße koppeln.
- **Im Normalbetrieb geht fast jeder Zyklus als unkomprimierter Vollstand
  hinaus.** Mehr als ein geändertes Match ergibt einen vollen `tset`
  (`badhub/diff.rs:73`), kein gzip — grob 100–400 KB alle 5 s, obwohl die
  Punkte schon einzeln als `tupdate_match` gingen. `sched` geht minütlich
  voll und ohne Diff. **Vorschlag:** N kleine Updates statt Vollstand; gzip;
  Hash-Vergleich für `sched`.
- **BTP- und Push-Fehler des Sync-Loops stehen nie im Diagnose-Log**
  (`SyncOutcome` landet nur im Dashboard-Status). **Vorschlag:**
  flankengesteuerte Zeile bei ok → Fehler → ok.
- **Veralteter Stand kann einen frischeren Tablet-Punkt kurz überholen**
  (BTP-Rückschreibungen zwischen `apply_tablet_scores` und Push); heilt im
  nächsten Diff.
- **Punktverlauf:** `badhub/verlauf.rs` klont und hasht alle 5 s die
  Timelines aller Matches; eine von badhub mit anderem Status als 400/404
  abgelehnte `tverlauf`-Nachricht wird endlos wiederholt und blockiert die
  übrigen.

### Paket 3 — Relay härten (Relay-Deploy vor Client)

**Spec:** [features/relay-haerten.md](features/relay-haerten.md)
(freigegeben 11.11.2026; ADR 0088–0091). Grill und How-To haben die
Befunde nachgeprüft und teils korrigiert — maßgeblich ist die Spec, die Stichpunkte hier
sind der Audit-Stand. Korrigiert sind vor allem: Der Namespace verfällt heute schon
ereignisgetrieben (es fehlt nur ein Zeit-Verfall); ein sofortiges `503` ließe bei jedem
Wackler des Turnier-PCs alle Fernseher blinken (deshalb 20 s Karenz); ein blankes
`tokio::spawn` bräche die Reihenfolge der Ergebnisse (deshalb ein Arbeiter je Feld);
`max_message_size` allein trennt, statt zu verwerfen (deshalb fachliche und
Transport-Deckel getrennt); nginx-Limits kommen nicht.

**Aus dem Paket ausgelagert — offene Punkte** (nicht Teil der Spec):

- **Host-Authentifizierung am Relay.** `host-ws` ist ohne Anmeldung; mit Kenntnis der
  `install_id` bleibt der Host-Slot nach 15 s Stille übernehmbar, und Schein-Hosts können
  die Namespace-Tabelle füllen. Die eigentliche Gegenmaßnahme, braucht ein eigenes
  Auth-Modell samt Schlüsselverteilung an bestehende Installationen → `/idee`.
- **nginx-Limits als Ops-Schritt.** `limit_conn`/`limit_req` je IP träfen eine ganze Halle
  im Störfall; wenn überhaupt, dann sehr großzügig und getrennt von einem Code-PR. Dazu
  gehört die Frage, ob das nginx-Zugriffslog für `/bts-relay/` die volle `install_id`
  weiter nennen soll (das Relay-Log kürzt sie mit der Spec auf 8 Zeichen).
- **`MAX_TL_WS_VORAUTH` je IP statt prozessweit.** Der Relay müsste erstmals der
  weitergereichten Client-Adresse vertrauen und eine IP-Tabelle mit Verfall führen; alle
  TL-Geräte einer Halle teilen eine Adresse.
- **TL-Zustand nur bauen, wenn ein Cloud-Gerät zusieht.** Braucht ein neues Signal
  Relay → Host mit Deploy-Reihenfolge, Rollback-Verhalten und Wartezeit beim ersten Blick.
- **Zettel über langsamen Uplink.** Ein 5-MB-Zettel über eine Leitung unter etwa
  2,8 Mbit/s wirft den Host weiter ab (gerechnet): Der Relay stempelt nur vollständige
  Nachrichten, ein Pong kann die laufende nicht überholen. Wege: große Host-Nachrichten in
  WebSocket-Fragmente teilen (ohne Protokoll-Änderung, per Test zu belegen) oder Stückelung
  im Protokoll.
- **Host-Verlust am Tablet anzeigen.** Die Tablet-Seite ignoriert einen fehlenden
  Turnier-PC bis zum Ergebnis; die Anzeigen zeigen ihn mit der Spec nach 20 s.
- **Kleiner:** Feld-Schloss für den LAN-Ergebnisweg (im Cloud-Pfad ordnet die Spec je
  Feld); Budget für Tablet-Zustände über alle Namespaces; Größen-Deckel am lokalen
  WebSocket der Slave-Brücke.

- **Mehrfaches `identify` hinterlässt tote Feld-Slots; der Namespace verfällt
  nie.** Jedes weitere `identify` mit anderer `courtId` belegt ein Feld mehr
  (`relay/src/main.rs:2699`), `abraeumen` löst nur das letzte (`:2809`). Ein
  Fremder kann so `MAX_NAMESPACES` dauerhaft füllen; neue Hosts werden bis
  zum Relay-Neustart abgewiesen. **Vorschlag:** altes Feld vor neuem
  `identify` lösen oder ablehnen; geschlossene Sender ausfegen; Namespaces
  ohne Host kurzlebig.
- **Keine Größengrenzen auf den WebSocket-Wegen.** Nirgends
  `max_message_size`; `forward_score` speichert `sets_history` ungekappt
  (`:3850`), `MatchAssigned` legt Felder ohne Zahl- und Längendeckel an; alle
  Kanäle sind `unbounded`. **Vorschlag:** Deckel je Route, Satzliste ≤ 10,
  Felder je Namespace deckeln, `limit_conn`/`limit_req` in nginx.
- **Host-Schleife wartet Ergebnis-Write und Uploads inline ab.**
  `RelayFrame::Result` → `process_result(...).await`
  (`relay_client.rs:553`); bei trägem BTP liest die Schleife >15 s nichts,
  das Relay wirft den Host ab (`HOST_STALE`) — trifft alle Cloud-Felder. Auch
  Werbe-Upload und Control-Sync laufen inline; `monitor_fp` ist je Sitzung
  lokal (Neu-Upload nach jedem Reconnect). **Vorschlag:** `tokio::spawn` wie
  bei `TlCommand`.
- **Verbindungsaufbau des Hosts ohne Frist, Backoff ohne Jitter**
  (`relay_client.rs:116–118`): `connect_async` kann hinter einem Captive
  Portal ewig hängen; der Backoff wird schon beim Handshake zurückgesetzt
  (Dauer-Reconnect im 1–2-s-Takt bei Abweisung); nach jedem Relay-Deploy
  kommen alle Hosts gleichzeitig.
- **Anzeigen zeigen ohne Hinweis einen eingefrorenen Stand, wenn der Host weg
  ist, aber noch ein Tablet oder eine TL-Seite am Relay hängt.**
  `overview_health`, `build_monitor_state`, `meetingpoint_state`,
  `preparation_state` prüfen `host` nicht. **Vorschlag:** `503` bei
  fehlendem Host — die Offline-Blende der Seiten greift dann von selbst.
- **Nach Host-Abgang bleiben Turnierdaten im RAM und abrufbar** (Namen,
  Azure-Schlüssel, Bilder), solange ein Tablet oder TL-Tab verbunden ist.
  **Vorschlag:** `azure_tts` sofort verwerfen, `tl_subs` schließen, nach
  Karenz leeren.
- **Kleiner:** ein globales Schloss für alle Turniere mit Kopien darunter;
  Relay-Log mit `install_id` im Klartext und ohne `max_log_files`;
  `MAX_TL_WS_VORAUTH` prozessweit statt je IP; Host baut den TL-Zustand alle
  2 s, auch wenn kein Cloud-Gerät zusieht.

### Einzelpunkte — Datensparsamkeit

- **LAN-`/health` und `/combo/state` liefern die komplette `CourtOverview`**
  (`server.rs:1416`, `state.rs:131`) an jeden im Hallen-WLAN: `sr`/`ar`,
  `sr_id`/`ar_id`, `official_warn` (Ableitung aus den Sperrlisten),
  `scorekeeper`, `battery`, `locked`. Übersicht und Kombi nutzen keines
  davon; das Relay baut hier schon eine Positivliste. **Vorschlag:** eigene
  Projektion + Wächter-Test.
- **Verein und Nation gehen immer ans Tablet** (`match_brief`), unabhängig
  von den Schaltern; die Nation nutzt das Tablet nie. Der Brief liegt damit
  auch im `state_sync`, im Relay-Cache und im `localStorage`.
- **Badhub-URL ohne HTTPS-Zwang:** Freitextfeld, weder Schema noch Host
  geprüft — betrifft Bearer-Passwort, Pushes und den TP-Upload. Der
  Push-Client folgt außerdem Redirects.
- **Mögliche Namen im Log:** `state.rs` schreibt bei unlesbarer
  Ansage-Auftragsliste die ersten 120 Zeichen der Antwort (können SR-/AR-
  Namen enthalten).

### Einzelpunkte — Anzeigen und Sonstiges

- **Meetingpoint friert nach hängendem Check-in ein:** POST ohne Frist
  (`meetingpoint.html:339`), solange sperrt `unterwegs` das Neuzeichnen der
  ganzen Seite (`:377`), der Status bleibt „aktuell".
- **Slave-Brücke verschluckt `If-None-Match` auf `/health`**
  (`slave_bridge.rs:207`) — die ferne Halle bekommt nie ein `304`; der
  `reqwest`-Client dort hat kein Timeout.
- **Kombi-Anzeige läuft an allen S-Etappen vorbei:** Sekundentakt ohne ETag
  und Nudge, voller DOM-Neuaufbau, `combo_state` baut je Abruf die ganze
  Übersicht (die Spec `monitor-livestand-push` behauptet das Gegenteil).
- **`/monitor/state` liest die Zuweisungsdatei bei jedem Abruf von der
  Platte;** `preparation`/`spielfolge`/`winners`/`ad` springen schon beim
  ersten Leer-Lesen weg (keine Entprellung wie `DEASSIGN_AFTER`).
- **Vorbereitung und Meetingpoint klonen je Abruf den ganzen BTP-Snapshot;**
  die Vorbereitungsliste ist ungedeckelt (Seite zeigt 10); die Feldwahl holt
  alle 15 s den vollen Meetingpoint-Rumpf nur für den Statuscode.
- **Rest-Abrufe ohne Frist:** `anzeige.html` (`ladeFelder`, „Zum Zählen"),
  Sponsor-Leiste in vier Seiten.
- **Sieger-Monitor** rechnet alle 2 s alle Podien, zeigt nur die gewählte
  Disziplin.
- **Log-Upload** verliert die letzten bis zu 10 Minuten vor dem Beenden und
  den Rest der Vortagsdatei; ohne gzip und Größendeckel, blockierendes Lesen.
- **Sprachdienste:** kein `connect_timeout`, Client je Anfrage neu; bei
  „Ansage in Teilen" und totem Netz 30–60 s bis zur Ersatzstimme.
- **BTP-Lesen** ohne Größendeckel (`btp/client.rs`, `read_to_end` + gzip).

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
- ~~**`locked_courts` geht beim Speichern von Einstellungen verloren.**~~
  **Erledigt in v0.9.258** (Spec `tl-web-felder-sperren`):
  `keep_host_managed_fields` schützt die Sperrliste jetzt wie die
  TL-Geräteliste. Der Fehler wog schwerer als gedacht — mit der Bedienung aus
  der Halle wäre der Ablauf gewesen: kaputtes Feld sperren, jemand speichert
  am PC eine Einstellung, Sperre still weg, Automatik legt ein Spiel darauf.
  Ein Test hält den Pfad offen (`keep_host_managed_fields_preserves_the_locked_courts`).

## Wünsche vom 06.09.2026

- ~~**Pause der Feld-Automatik überlebt Neustart und Update nicht.**~~ —
  **erledigt in v0.9.323**: Die Pause steht turniergebunden in der Config
  (`auto_assign_paused` + `auto_assign_paused_tournament`, Muster ADR 0044),
  wird beim Start geladen und ist vor dem Speichern der Einstellungen
  geschützt (`keep_host_managed_fields`). Nebenbei behoben: Auch jedes
  Speichern der Einstellungen hob die Pause auf (Sync-Neustart). Griff ohne
  TL-Gerät seit v0.9.326: Balken „Automatik pausiert" mit Knopf
  „Fortsetzen" in der Desktop-App (`set_auto_assign_paused`, derselbe Weg
  `persist_auto_assign_pause` wie die TL-Aktion).
  Bedienung: [turnierleitung-web.md](turnierleitung-web.md).

## Wünsche vom 23.08.2026

- **Felder sperren im TL-Web** — Spec freigegeben und umgesetzt (v0.9.258):
  [docs/features/tl-web-felder-sperren.md](features/tl-web-felder-sperren.md),
  [ADR 0044](adr/0044-sperrliste-turniergebunden.md). Schloss nebenbei den
  `locked_courts`-Datenverlust oben.
- **Warnung bei scheinbar fertigem Spiel** — Spec freigegeben und umgesetzt
  (v0.9.259): [docs/features/tl-warnung-fertiges-spiel.md](features/tl-warnung-fertiges-spiel.md),
  [ADR 0045](adr/0045-fertig-warnung-serverseitig-gestempelt.md).
- **Feldauswahl für die Automatikvergabe** — Spec freigegeben und umgesetzt
  (v0.9.262): [docs/features/tl-wunschfeld.md](features/tl-wunschfeld.md),
  [ADR 0046](adr/0046-wunschfeld-reserviert-ab-spielbereitschaft.md).
  Reserviert wird ab Spielbereitschaft — ein Endspiel, dessen Spieler noch
  im Halbfinale stehen, hält das Hauptfeld nicht leer.
- **Tablet merkt eine veraltete Fassung** — Spec umgesetzt (v0.9.266 stiller
  Abgleich, v0.9.268 Fernbefehl „⟳ Tablets"):
  [docs/features/tablet-version-abgleich.md](features/tablet-version-abgleich.md).
  Kein ADR nötig. Wichtig für die Deutung des Feldtests: Der berichtete
  „Hänger" bis zum Leeren der Browserdaten war **nicht** der HTTP-Cache
  (`no-store` steht seit dem ersten Commit, die Seiten sind selbstenthaltend),
  sondern sehr wahrscheinlich das hängende `pendingResult` im `localStorage`
  — behoben mit v0.9.254. Endgültig belegt ist das nicht: Die Tablet-Logs der
  betroffenen Geräte gibt es nicht mehr, die Suche nach dem genauen
  Ablehnungsgrund ist eingestellt (24.08.2026). Offen: Feldtest beider Wege.

## Wünsche vom 11.08.2026

- **Punktverlauf-Graph pro Satz** — Spec freigegeben und in Umsetzung:
  [features/punktverlauf-graph.md](features/punktverlauf-graph.md)
  (ADR [0014](adr/0014-punktverlauf-expliziter-rally-frame.md),
  [0015](adr/0015-punktverlauf-datei-je-turnier.md)). Folge-Features
  (je eigene Spec, offen): badhub-Push + Anzeige auf badhub.de ·
  ~~**ausgefüllte Schiedsrichterzettel drucken**~~ → **spezifiziert**
  2026-08-19, siehe unten.

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
- **Server-Suche über badhub als weiterer Rückfall** (Idee 04.09.2026).
  Heute findet das Master-Image (Kiosk-Skript aus `pi/setup-monitor.sh`) den
  Turnier-PC in dieser Reihenfolge: gemerkte IP → Subnetz-Scan des
  eigenen /24 auf `:8088/health` (1–9 s) → mDNS mit Timeout. Idee: bts-light
  hinterlegt beim Liveticker-Push seine **lokale IP** bei badhub; der Pi
  fragt dort einmalig nach, welche lokalen IPs gerade aktiv sind.
  Zuordnung ohne Kennung/Token über die **öffentliche Absender-IP**:
  badhub merkt sich das Paar (öffentliche IP, lokale IP) und antwortet
  nur mit Einträgen derselben öffentlichen IP → fremde Netze sehen
  nichts. Nutzen liegt **nicht** im Tempo (der Scan ist schon schnell),
  sondern im Fall, den der Scan nie abdeckt: PC und Pi in
  **verschiedenen Subnetzen** (zwei Router, Gast-WLAN). Einordnung als
  Rückfall hinter Cache und Scan, vor mDNS. Offene Punkte: Pis ohne
  Internet (Verleih-Router ohne Uplink) bleiben auf Scan/mDNS angewiesen;
  Carrier-Grade-NAT bei Mobilfunk-Hotspots macht die Zuordnung unscharf;
  HTTPS am Pi ohne Echtzeituhr scheitert bis zum NTP-Abgleich still →
  Wiederholung nach Zeitabgleich nötig. Betroffen: badhub-Endpoint,
  Push-Payload, Pi-Kiosk-Skript. Vor der Umsetzung `/idee`.
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
- **Neues Pi-Image für Zero 2 W und 3 B+** (09.11.2026, löst das gemeinsame
  BTS-Image ab — die BTS-Unterstützung ist ausgebaut, damit entfällt auch der
  Firmware-Tausch für den Pi 3 B+ Rev. 1.4). Emuliert gebaut
  (`pi/build-image-docker.sh`, Bookworm Lite 2026-10-06) und als
  `pi-image/bts-light-pi.img.xz` hochgeladen; Release-Seite angepasst.
  **Offen: Probestart auf echter Hardware** (das Image lief nie auf einem
  Pi) und Feldtest: CEC-Einschalten je TV-Marke. Der Nur-Lese-Modus ist seit
  v0.9.397 wieder ausgebaut (Ärger am Zero 2 W und am Pi 3). Das alte BTS-Image liegt auf dem Server als
  `bts-light-pi-bts-2026-06.img.xz`.
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
- **Online-Anleitung veröffentlichen — Veröffentlichung erledigt, Verlinkung
  offen.** [pi-setup.md](pi-setup.md) steht seit dem Handbuch (PR #333) als
  echte Webseite unter `badhub.de/download/bts-light/handbuch/pi-einrichten.html`.
  **Offen bleibt:** aus bts-light heraus darauf verlinken (Knopf
  „Einrichtungs-Anleitung" auf der Court-Monitore-Seite).
- **2-Felder-pro-TV-Modus.** Zwei benachbarte Felder auf einem großen TV
  (`…/display?courts=3,4`).

## Bekannte Einschränkungen / technische Schuld

- **Azure-Schlüssel offen am Relay** (Befund 29.10.2026, Grill zur
  Ansage-Seite): `/{ns}/info/announce/state` liefert die geerbte
  Azure-Config samt Schlüssel an jeden, der die `install_id` kennt
  (`relay/src/main.rs` `announce_state`, so gewollt seit ADR 0003). Die
  `install_id` ist zugleich Tablet-Zugang. Eigener Fix nötig (z. B. Erbe nur
  über die authentifizierte Slave-Kopplung); die Ansage-Seite nutzt diesen
  Weg bewusst nicht.
- **Azure-Audio mit Spielernamen dauerhaft auf der Platte** (Befund
  29.10.2026): `azure_tts.rs` cacht jede Ansage als MP3 in
  `app_data_dir/tts-cache/` ohne Verfall. Verfall/Räumen beim Turnierwechsel
  prüfen.

- **Kein Framing-Schutz für die Zähl-Seite** `/court/{id}` **im LAN**
  (Port 8088/8443 setzen weder `X-Frame-Options` noch `frame-ancestors`):
  Clickjacking auf die Punkte-Knöpfe wäre möglich; die Ergebnis-Abgabe
  bleibt durch `process_result` (R5) validiert. **In der Cloud besteht die
  Lücke nicht:** Der nginx vor `/bts-relay/` sendet `X-Frame-Options:
  SAMEORIGIN` und eine CSP mit `default-src 'self'` (geprüft 04.09.2026 per
  `curl -I` auf `…/court/1/tafel`, `…/anzeige`, `/bts-relay/health`) — das
  erlaubt genau die Anzeige-Hülle (#328, same-origin-iframe) und sperrt
  fremde Seiten aus. Für den LAN-Server wäre ein `frame-ancestors 'self'`
  im `router()` das Gegenstück — eigener PR.
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
- **MOVED → badhub.** Zwei Punkte dieser Liste waren badhub-Arbeit und
  werden dort primär geführt (Owner-Regel: ein Punkt, ein Repo) —
  `badhub/docs/roadmap.md`, Abschnitt *Backlog*:
  *Alte Liveticker-Test-Turniere deaktivieren* (`lehiero`,
  `christian-zum-test` und die Legacy-Zeile `default` stehen in
  `liveticker_tournaments` noch auf `is_active = 1` und machen `/live`
  ohne `?t=` mehrdeutig) und *`docs/ops/deployment.md` ist veraltet*
  (beschreibt noch das KAS-`deploy_prod.sh` statt `deploy_hetzner.sh`).
  Hier bewusst kein zweiter aktiver Task.
- **Das Turnierlogo reiste in jedem vollen `tset` mit** (`sync.rs`,
  Base64, bis 2,7 MB) — **erledigt, Stand 2026-08-28.** Ein voller `tset`
  ging mindestens jede Minute als Lebenszeichen raus und zusätzlich immer
  dann, wenn der Diff bei mehreren geänderten Matches zum Vollstand
  degenerierte. Weglassen ging nicht, solange ein `tset` bei badhub den
  kompletten Snapshot-Datensatz ersetzte: ein fehlendes Feld löschte das
  Logo (Recherche 18.08.2026). Die dreistufige Umstellung ist komplett
  durch — nachgemessen, nicht angenommen:
  1. ✅ **v0.9.226** — Logo-Felder werden auch leer mitgeschickt
     (`""` statt weglassen), damit ein Logo nach Schritt 2 löschbar bleibt.
  2. ✅ **badhub-PR #473**, gemergt am 2026-08-18:
     `liveticker_logo_uebernehmen()` gibt den Feldern den Vertrag
     „weglassen = unverändert, `""` = löschen" — liegt in
     `lib/live_update_lib.php` auf badhub-`main`.
  3. ✅ **v0.9.227** — das Logo geht nur noch bei Änderung raus
     (`Option`-Felder, Marke aus Turnier + Bildinhalt, Auffrischung alle
     10 Min, Stempel erst nach geglücktem Push); in `sync.rs` als
     `logo_in_tset_legen()` / `logo_gesendet`. Ausgeliefert — der Stand
     ist inzwischen v0.9.272.
  Der lokale badhub-Umbau `feat/turnierlogo-zentralisierung`, der an
  derselben Codezeile kollidierte, existiert nicht mehr. Kein Restpunkt.

## Schiedsrichtermanagement — umgesetzt (v0.9.201)

Die Spec [features/schiedsrichter-management.md](features/schiedsrichter-management.md)
ist vollständig umgesetzt (Doku: [schiedsrichter-management.md](schiedsrichter-management.md)).
Offen bleiben der Feldtest an einem Turnier mit Schiedsrichtern und — für
den Cloud-Weg — der Relay-Deploy auf badhub.de vor dem Client-Release.

## Spiele von der automatischen Feldvergabe ausnehmen — umgesetzt, noch nicht released

Die Spec [features/feldvergabe-ausnahme.md](features/feldvergabe-ausnahme.md)
ist vollständig umgesetzt: Turnierleitung kann ein einzelnes Spiel per
Knopfdruck (TL-Web und Turnier-PC) temporär von `sync.rs::auto_assign`
ausschließen, bis es manuell reaktiviert wird oder das Match endet;
manuelles Zuweisen bleibt unberührt. Persistenz nach dem ADR-0022-Muster
(eigene turniergebundene Datei `excluded-matches.json`). Doku:
[turnierleitung-web.md](turnierleitung-web.md). Offen bleiben der
Feldtest an einem laufenden Turnier und der Versions-Bump für ein Release.

## Spielliste per Drag&Drop manuell sortierbar — umgesetzt, noch nicht released

Die Spec [features/spielliste-manuelle-reihenfolge.md](features/spielliste-manuelle-reihenfolge.md)
ist vollständig umgesetzt (ADR [0023](adr/0023-manuelle-spielreihenfolge-praefix-je-halle.md)):
Turnierleitung zieht spielbereite, noch nicht gerufene Spiele in eine
eigene, je Halle getrennte Reihenfolge (Präfix-Mechanik — jeder Zug
speichert die effektive Reihenfolge vom Hallenanfang bis zum neuen Platz
des gezogenen Spiels, alles danach folgt weiter BTPs eigener Reihenfolge).
Wirkt an allen fünf Sortier-Stellen (`assign::resolve_and_sort_key`,
gemeinsamer Helfer, abgesichert durch
`tests/queue_order_consistency.rs`) sowie bei der automatischen
Feldvergabe. Globaler Reset-Knopf (TL-Web + Desktop). Messung 14.08.2026
(`btp_displayorder_probe.rs`, gegen TEST Köpi-Cup) belegt: `DisplayOrder`
lässt sich nicht nach BTP zurückschreiben (stiller No-Op wie
`LocationID`) — die Reihenfolge bleibt rein host-lokal. Persistenz nach
dem ADR-0022-Muster (`queue-order.json`). Doku:
[btp_protocol.md](btp_protocol.md), [preparation.md](preparation.md).
Offen bleiben der Feldtest an einem Mehr-Hallen-Turnier, der manuelle
Touch-/Drag-Test auf einem echten Tablet (wie schon bei der
Schiedsrichter-Reihenfolge ausstehend) und der Versions-Bump für ein
Release.
