# Roadmap & offene Punkte

Lebende Liste der offenen Arbeiten an bts-light. Erledigte Versionen stehen
im [changelog.md](changelog.md); hier steht, was **noch** ansteht.

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
  — **✅ (#207/#208)**; LAN-Server-Last als Folge-Erweiterung. **D**
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
- ~~**`locked_courts` geht beim Speichern von Einstellungen verloren.**~~
  **Erledigt in v0.9.258** (Spec `tl-web-felder-sperren`):
  `keep_host_managed_fields` schützt die Sperrliste jetzt wie die
  TL-Geräteliste. Der Fehler wog schwerer als gedacht — mit der Bedienung aus
  der Halle wäre der Ablauf gewesen: kaputtes Feld sperren, jemand speichert
  am PC eine Einstellung, Sperre still weg, Automatik legt ein Spiel darauf.
  Ein Test hält den Pfad offen (`keep_host_managed_fields_preserves_the_locked_courts`).

## Wünsche vom 23.08.2026

- **Felder sperren im TL-Web** — Spec freigegeben und umgesetzt (v0.9.258):
  [docs/features/tl-web-felder-sperren.md](features/tl-web-felder-sperren.md),
  [ADR 0044](adr/0044-sperrliste-turniergebunden.md). Schloss nebenbei den
  `locked_courts`-Datenverlust oben.
- **Warnung bei scheinbar fertigem Spiel** — Spec freigegeben und umgesetzt
  (v0.9.259): [docs/features/tl-warnung-fertiges-spiel.md](features/tl-warnung-fertiges-spiel.md),
  [ADR 0045](adr/0045-fertig-warnung-serverseitig-gestempelt.md).
- **Feldauswahl für die Automatikvergabe** — ein Spiel soll ein Wunschfeld
  bekommen können, auf das die Automatik wartet (Finalspiele steuern). Von
  Hand woanders hinlegen bleibt möglich, mit Rückfrage. Spec steht noch aus.

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
- **Das Turnierlogo reist in jedem vollen `tset` mit** (`sync.rs`,
  Base64, bis 2,7 MB). Ein voller `tset` geht mindestens jede Minute als
  Lebenszeichen raus und zusätzlich immer dann, wenn der Diff bei
  mehreren geänderten Matches zum Vollstand degeneriert — bei vielen
  Feldern also alle paar Sekunden. Weglassen ging bisher nicht: Ein
  `tset` ersetzt bei badhub den kompletten Snapshot-Datensatz
  (`liveticker_state.snapshot_json`), ein fehlendes Feld löschte das Logo
  also, und der Liveticker blendete es beim nächsten 5-s-Poll aus —
  ebenso die Check-In-Seite, wenn dort kein eigenes Branding-Logo
  hinterlegt ist (Recherche 18.08.2026).
  **Umstellung in drei Schritten, Reihenfolge zwingend:**
  1. ✅ **v0.9.226:** bts-light schickt die Logo-Felder auch leer mit
     (`""` statt weglassen) — sonst wäre ein Logo nach Schritt 2 nicht
     mehr löschbar. Gegen das heutige badhub verhaltensgleich, deshalb
     gefahrlos vorab.
  2. **badhub-PR #473** (offen): `liveticker_logo_uebernehmen()` gibt den
     Feldern den Vertrag „weglassen = unverändert, `""` = löschen" — den,
     den `checkin_branding_apply()` schon hat. **Braucht einen Deploy.**
  3. ⚠️ **v0.9.227 gebaut, DARF ERST NACH SCHRITT 2 AUSGELIEFERT WERDEN:**
     bts-light schickt das Logo nur noch bei Änderung (`Option`-Felder,
     Marke aus Turnier + Bildinhalt, Auffrischung alle 10 Min, Stempel
     erst nach geglücktem Push). Ohne den badhub-Deploy würde ein
     weggelassenes Logo dort als „kein Logo" gelten.
  Zu beachten: Im badhub-Repo liegt (lokal, ungepusht) der Umbau
  `feat/turnierlogo-zentralisierung` — Logo als inhaltsadressierte Datei,
  im Snapshot nur noch `tournament_logo_url`. Er löst diesen Punkt
  **nicht** von allein (ein `tset` ohne Logo lässt die URL schlicht aus
  dem Snapshot fallen) und kollidiert an derselben Codezeile mit #473;
  beim Auflösen muss die Übernahme **oben** stehen, sonst holt sie den
  Base64-Blob zurück.

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
