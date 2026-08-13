# Echtzeit-Robustheit der Score-Strecke (Paket A) — Spezifikation

> Status: **abgestimmt 2026-08-13** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Turnier-Erfahrung (2-Hallen-Betrieb, „Hänger" + zu langsame Score-Anzeige)
> + belegte Ist-Analyse. Betroffene Crates: src-tauri, relay, relay-proto (+ Assets).
> ADR: [docs/adr/0016-monitor-push-transport.md](adr/0016-monitor-push-transport.md),
> [docs/adr/0017-reconnect-ownership.md](adr/0017-reconnect-ownership.md).

Teil des Clusters **Turnier-Robustheit** (Umbrella:
[docs/features/turnier-robustheit-cluster.md](turnier-robustheit-cluster.md)).
Diese Spec ist **Paket A**; Hebel B (Ergebnis-Puffer), C (Last-/Soak-Test),
D (Tote-Verbindungs-Erkennung) bekommen eigene Specs.

## Kontext / Problem

Reales 2-Hallen-Turnier: Haupthalle 12 Felder (LAN), Nebenhalle 6 Felder (Cloud,
**eigener Router**), je TVs + Tablets; ~36 Geräte; störanfälliges WLAN mit
hunderten Fremdgeräten. Zwei belegte Schmerzen auf der Strecke
Tablet → Server/Relay → TV:

1. **Latenz:** Tablet-Klick → TV-Anzeige dauert **bis 1 s** (Court-Monitor) bzw.
   **bis 5 s** (Feld-Übersicht). Ursache: die Anzeigen **pollen**
   (`monitor.html:1032` 1 s; `overview.html:544` 5 s), während Tablet→Server/Relay
   längst **Push** (WebSocket) ist und der Score sofort verarbeitet/gecached wird.
2. **Reconnect-Datenintegrität:** Verliert das zählende Tablet die Verbindung und
   kehrt im selben Spiel zurück, muss sein lokaler Stand die Wahrheit bleiben —
   **außer** ein anderes Tablet hat übernommen und weitergezählt, oder das Spiel
   wurde per Hand fertig eingegeben. Heute entscheidet ein `rev`-Zähler
   (`tablet.html:1319`), der **kein Ownership kodiert** und ein spät zurückkehrendes
   Alt-Tablet den legitimen Übernehmer überschreiben lassen kann.

Betroffen sind Turnierleitung (verliert Vertrauen bei zähen/springenden Anzeigen),
Schiedsrichter (Tablet muss verlässlich zählen) und Zuschauer (Liveticker/TV).

## Zielbild & Erfolgskriterien

- **A1 — Niedrig-latente Anzeige:** Score-Änderung erscheint auf Court-Monitor
  **und** Feld-Übersicht in **<200 ms** (Push-Weg), im LAN **und** Cloud, in
  beiden Hallen. Reißt der Push ab, erscheint sie spätestens **~250 ms** über den
  Poll-Fallback (kein Regress, keine Bedienung nötig).
- **A2 — Reconnect-Wahrheit:** Nach Tablet-Reconnect im selben Spiel setzt sich der
  lokale Tablet-Stand **deterministisch** durch, **außer** ein anderer Zähler hält
  den Feld-Slot und hat weitergezählt (dann tritt das Tablet zurück) **oder** das
  Match ist finalisiert (dann tritt es zurück und überbügelt das Hand-Ergebnis
  nicht). Verhalten ist testbar und ohne Bedienung selbstheilend.
- **Ohne Erklärung nutzbar:** Der Turnierleiter merkt nur „flüssiger und
  verlässlicher"; keine neuen Handgriffe. A2 ist per Schalter im laufenden Turnier
  zurückrollbar.

## Nicht-Ziele

- `preparation.html` und Werbung (weniger zeitkritisch — pollen weiter).
- Die 2-s-Match-/Feld-**Zuweisung** (eigener Mechanismus; höchstens Notiz).
- **TL-Eskalations-UI** bei Divergenz — bewusst verworfen zugunsten einer
  deterministischen Regel (stiller Verlierer akzeptiert, siehe Risiken).
- Score **inline** im Push (Nudge löst `fetch` aus; Inline-Optimierung
  zurückgestellt).
- Hebel B/C/D des Clusters (eigene Specs).

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - A1: `src-tauri/assets/monitor.html`, `overview.html` (Poll-Tuning + WS-Client);
    neue Monitor-WS-Route + Subscriber-Registry in `src-tauri/src/tablet/server.rs`
    (+ `tablet/state.rs`) und `relay/src/main.rs` (`Namespace`).
  - A2: reine Entscheidungsfunktion in `src-tauri/src/tablet/state.rs`; Aufrufe in
    `server.rs` (Identify/Reclaim) und `relay/src/main.rs` (`attach_tablet`);
    `src-tauri/assets/tablet.html` (`state_restore`-Handler folgt `authoritative`);
    Finalisiert-Tracking in `src-tauri/src/sync.rs`; Wire-Felder in
    `relay-proto/src/lib.rs`; Flag in `src-tauri/src/config.rs`.
- **Architekturregeln (R1–R6):**
  - **R2:** Court→Match-Zuordnung bleibt BTP-Wahrheit; A2 betrifft nur den
    **Live-Zählstand** des zugewiesenen Matches. Das Finalisiert-Flag stammt aus
    dem BTP-Status (`MatchStatus::Finished`).
  - **R3:** Push **und** Ownership müssen LAN (Server) und Cloud (Relay hinter
    nginx `/bts-relay/`) können. WS geht bereits durch den Proxy (Tablet-WS).
  - **R4:** „Ein aktives Tablet je Court" ist die **Grundlage** von „Slot-Halter
    gewinnt".
  - **R5:** Jedes eingehende Ergebnis läuft weiter durch `process_result`; das
    Finalisiert-Flag **ergänzt**, ersetzt nicht.
  - R1/R6 unberührt.
- **Konfiguration & Abwärtskompatibilität:** neues bool in `AppConfig`
  (`#[serde(default)]`), zur Laufzeit umschaltbar (A2-Rollback). **Das Tablet
  folgt dem server-gelieferten `authoritative`-Feld**; fehlt es (ältere App),
  greift per `serde(default)` der heutige `rev`-Zweig → Auto-Update-sicher. Relay
  additiv deploybar. `identifier`/Updater-Pfad unangetastet.
- **Datenschutz:** keine neuen personenbezogenen Felder; Nudge trägt nur
  CourtID + Sequenz, Ownership nur Geräte-ID (bereits vorhanden) + Epoch. Kein
  Geburtsjahr/Lizenz.
- **Abhängigkeiten:** keine neue Cargo-/npm-Dependency (WS-Stack, `tokio`,
  `serde` bereits da). Relay/nginx-Weg unverändert (WS bereits proxied, **kein**
  Ops-Eingriff nötig — SSE wurde genau deshalb verworfen).

## Akzeptanzkriterien

**A1 — Anzeige**
- [ ] Ein Punkt-Tipp erscheint auf dem Court-Monitor in <200 ms, wenn der
      Monitor-WS verbunden ist (LAN und Cloud).
- [ ] Dasselbe für die Feld-Übersicht (`overview.html`).
- [ ] Wird der Monitor-WS getrennt (WLAN-Abriss), zeigt die Anzeige den Stand
      weiter und holt binnen ~250 ms über den Poll-Fallback auf; bei WS-Rückkehr
      pausiert das Intervall-Poll wieder.
- [ ] Push und Poll erzeugen **kein** Flackern/Rückwärtsspringen (Nudge löst nur
      `fetch` auf die bestehende Poll-Route aus; veraltete Nudges via `lastSeq`
      verworfen).
- [ ] Reassignment/„Feld belegt"/Offline-Verhalten der Monitore bleibt
      unverändert (kein Regress).

**A2 — Reconnect-Wahrheit**
- [ ] Tablet fällt kurz aus (< STALE_AFTER) und kehrt zurück: niemand hat
      übernommen → lokaler Stand wird durchgesetzt (Server-Cache + TV +
      Liveticker ziehen nach).
- [ ] Tablet fällt lang aus (> STALE_AFTER), ein anderes Gerät übernimmt den Slot
      und zählt weiter; das alte Tablet kehrt zurück → es **tritt zurück** und
      überschreibt den Übernehmer **nicht** (auch wenn sein lokaler `rev` höher ist).
- [ ] Fremder Owner hat seit Übernahme **nicht** gezählt → der aktuelle Slot-Halter
      gewinnt deterministisch.
- [ ] Match ist in BTP finalisiert (`finalized`-Flag): das Tablet pusht keinen
      Score mehr und sendet kein Ergebnis; ein Hand-Ergebnis wird nicht überbügelt.
- [ ] Bei aktivem Legacy-Schalter verhält sich der Reconnect exakt wie heute
      (rev-Semantik) — Rollback ohne App-Neustart wirksam.
- [ ] Ältere App-Version ohne `authoritative`-Feld: unverändertes rev-Verhalten
      (kein Bruch durch Auto-Update).

## Tests

**Rust-Unit-Tests (TDD, Pflicht):**
- `reconnect_decision(...)`-Wahrheitstabelle: Slot frei → KeepLocal;
  Owner==Gerät → KeepLocal; fremder Owner + `scored_since_claim` → StandDown;
  fremder Owner ohne Score → Halter gewinnt; `finalized` → StandDown;
  Legacy-Flag → rev-Semantik.
- Epoch-Monotonie: neuer `claim_court`-Token > alter (erweitert
  `claim_court_tracks_holder_device`).
- **Broker-Routing des Push:** `record_score(court=5)` weckt genau die Subs von
  Court 5, nicht andere Courts/Namespaces; Namespace-Isolation im Relay; Nudge-
  `seq` monoton je Court; Subscribe/Unsubscribe-Lebenszyklus.
- **Finalisiert-Gate:** Score für ein finalisiertes Match wird in `handle_score`
  ignoriert (Stale-Filter erweitert); OnCourt→Finished setzt das Flag; TTL-Ablauf.
- Serde-Roundtrips: `StateRestore` (+ `authoritative`/`owner_epoch`/`owner_device`),
  `MatchBrief` (+ `finalized`) mit und ohne die neuen Felder.

**JS/E2E (Playwright):** Score ohne Poll-Tick sichtbar; WS-Kill → Poll übernimmt;
Alt-Tablet-Rückkehr mit/ohne Übernahme; nach `finalized`-Frame kein Push/Ergebnis.

**Manueller Turnier-Testfall:** 250-ms-Poll an ~36 (virtuellen) Geräten gegen den
Relay messen, bevor der Interim-Wert fixiert wird (Batterie/Datenvolumen der
Nebenhallen-TVs).

`cargo test` grün, `npm run build` fehlerfrei vor jedem Commit.

## Risiken & Rollback

- **A1-Push unter Last** (Nebenhalle/eigener Router/LTE): gedämpft durch das
  Nudge-Design (winzige Frames, eine Datenquelle) + 250-ms-Poll-Fallback — bei
  WS-Ausfall automatisch, kein Regress. Rollback: Client fällt bei WS-Fehler auf
  Poll; notfalls Monitor-WS ungenutzt lassen.
- **A2 ändert Reconnect-Verhalten für alle** (Auto-Update): Rollback über das
  Config-Flag **zur Laufzeit** (Legacy = altes rev-Verhalten). Alte Tablets ohne
  `authoritative` fallen per `serde(default)` auf rev zurück.
- **Divergenz „stiller Verlierer":** bei echter Doppel-Zählung gewinnt der
  aktuelle Slot-Halter still; der divergente Stand des anderen geht ohne Anzeige
  verloren. Bewusst akzeptiert (Determinismus > TL-Auflösung); in `docs/tablet.md`
  festgehalten.
- R2/R4/R5 gewahrt: Zuordnung bleibt BTP, ein aktives Tablet je Court,
  `process_result` validiert weiter.

## Offene Fragen / Annahmen

- **Feldname/Polarität des A2-Flags** (z. B. `reconnect_ownership` mit Default
  „neu aktiv" vs. `reconnect_legacy`) — final im ADR 0017. Prinzip steht: ein
  bool, `serde(default)`, runtime-umschaltbar, Default = neues Verhalten.
- **Interim-Poll-Wert** (~250 ms) wird durch die Turnier-Messung bestätigt/justiert.
- **Annahme:** Ein finalisiertes Match, das in BTP dem Feld zugewiesen bleibt,
  ist über den neuen `finalized`-Frame erkennbar; verlässt das Match das Feld
  (`MatchCleared`), greift ohnehin der bestehende „nicht mein Match"-Pfad.

## Betroffene Doku-Dateien

- `docs/court-monitor.md` (A1: Push-Kanal, Poll-Verhalten, Fallback).
- `docs/tablet.md` (A2: Reconnect-Wahrheit, Ownership, Divergenz-Regel,
  Finalisiert-Verhalten).
- `docs/cloud-relay.md` (Wire-Felder, Relay-Monitor-WS, Ownership im Cloud).
- Querverweis in `docs/multi-hall.md`; `docs/adr/0013…`, `0014…`;
  `docs/roadmap.md`-Eintrag; je Version `docs/changelog.md`.

## Umsetzungs-Hinweise

(Ergebnis der How-To-Phase — Details:
`docs/features/_intake/turnier-robustheit/3-how-to.md`.)

Reihenfolge kleiner, überprüfbarer Schritte:
1. **A1 Quick-Win:** Poll `monitor.html:1032` / `overview.html:544` auf ~250 ms.
2. **A1 Push (WS-Nudge):** Registry `monitor_subs` + Route `/monitor-ws?court={id}`
   (LAN) und `/{ns}/monitor-ws?court={id}` (Cloud); Broadcast in `record_score`/
   `forward_score`; Client-WS mit „Push aktiv → Poll pausiert" + Reconnect. →
   Version-Bump, auslieferbar.
3. **A2 Ownership (hinter Flag):** `reconnect_decision(...)` + `scored_since_claim`;
   `authoritative`/`owner_*` in `StateRestore`/`MatchBrief`; `tablet.html` folgt;
   Config-Flag.
4. **A2 Finalisiert:** per-Court „recently finalized" in `sync.rs`;
   `MatchBrief.finalized`; Tablet gated Push/Ergebnis. → Version-Bump.

- **ADR-Pflicht:** ADR 0016 (A1-Transport: WS-Nudge vs. SSE vs. Poll) und
  ADR 0017 (A2-Konfliktmodell: Ownership vs. rev vs. TL-Eskalation) vor der
  jeweiligen Umsetzung finalisieren.
- **Version gemeinsam** bumpen (`src-tauri/Cargo.toml` + `tauri.conf.json` +
  `package.json`; `relay-proto`/`relay` bei Wire-Änderung mit).
- **Review:** `code-reviewer` nach jeder Änderung; **`security-reviewer`** für den
  neuen Monitor-WS (Namespace-/CourtID-Validierung, Sub-Limits, Fan-out/DoS) und
  die Ownership-Direktive (Spoofing von `device_id`/Epoch — wessen Score gilt).
