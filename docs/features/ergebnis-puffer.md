# Ergebnis-Weg verlustsicher (Cluster-Hebel B) — Spezifikation

> Status: **abgestimmt 2026-08-13** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Robustheits-Cluster + belegte Ist-Analyse. Betroffene Crates:
> src-tauri, relay (+ tablet.html). ADR:
> [docs/adr/0018-ergebnis-weg-verlustsicher.md](adr/0018-ergebnis-weg-verlustsicher.md).

Teil des Clusters **Turnier-Robustheit**
([Umbrella](turnier-robustheit-cluster.md), **Hebel B**). Paket A (#204/#205) ist
gemergt.

## Kontext / Problem

Die belegte Ist-Analyse hat die ursprüngliche Prämisse („Ergebnis-Klick hängt bis
20 s") **korrigiert**: Ein Ergebnis-Puffer existiert bereits auf zwei Ebenen —
das **Tablet** puffert (`pendingResult` in localStorage, nicht-blockierender
5-s-Auto-Retry bis `ok:true`; überlebt Reload/Crash/Cloud-Aussetzer), und der
**Host** puffert (BTP-Retry-Queue mit 30-s-Flush). Die UI blockiert **nicht**.

Der Grill deckte jedoch drei echte Lücken auf, davon eine als **schon heute
existierender Bug**:

1. **Endlos-Retry + Doppel-Write (Bug):** Nach einem erfolgreichen Ergebnis-Write
   räumt der Server das Feld. Ein Wiederholungs-POST (etwa weil die ack verloren
   ging) bekommt dann „Kein Match auf diesem Court" → **nie `ok:true`** → das
   Tablet löscht `pendingResult` **nie** und retryt endlos; trifft der zweite
   POST ein, während das Feld noch belegt ist, laufen **zwei BTP-Writes** parallel.
2. **Träge Retries:** Der Client-`fetch` hat kein Timeout → eine Cloud-Anfrage
   kann bis 20 s (`RESULT_TIMEOUT`) in der Promise stehen, bevor der 5-s-Retry
   startet.
3. **Flüchtige Host-Queue:** Die BTP-Retry-Queue liegt nur im RAM → ein
   Host-App-Neustart mit gefüllter Queue verliert Ergebnisse (Fall: Tablet hat
   `ok:false` bekommen und wurde selbst ausgeschaltet).

Betroffen: Schiedsrichter/Turnierleitung (verlorene oder endlos „hängende"
Ergebnisse), Zuschauer (Liveticker).

## Zielbild & Erfolgskriterien

- **(c) Idempotenz:** Ein Wiederholungs-POST für ein Match, dessen **identisches**
  Ergebnis bereits geschrieben wurde, wird mit `ok()` quittiert → das Tablet
  löscht `pendingResult`, der Retry stoppt. Kein Endlos-Retry, kein
  schädlicher Doppel-Write.
- **(a) Flotte Retries:** Ein Cloud-Aussetzer beim Absenden führt zu einem
  Client-Retry innerhalb weniger Sekunden (Abort-Backstop), ohne Klick-Blockade
  (unverändert) und ohne Verlust.
- **(b) Neustart-fest:** Ein Host-App-Neustart mit gefüllter Retry-Queue verliert
  **kein** Ergebnis mehr — die Queue wird (turnier-gegated) von Platte geladen und
  weiter geflusht.
- **Ohne Erklärung:** Der Turnierleiter merkt nur „Ergebnisse kommen zuverlässig
  an"; keine neuen Handgriffe, keine neue UI.

## Nicht-Ziele

- **Kein Relay-Puffer** — der Relay bleibt zustandslos.
- **Kein Umbau** des Client-localStorage-Puffers oder der Retry-Intervalle.
- Keine Änderung der R5-Validierung für **neue** Ergebnisse (c fügt nur einen
  Idempotenz-Zweig für **bereits geschriebene** hinzu).
- Keine Durabilität gegen **Stromausfall/OS-Crash** (nur App-Neustart; `write`
  ohne `fsync`).
- Keine neue Ergebnis-UI, keine neuen Config-Felder.
- Per-Match-In-Flight-Guard gegen echt-paralleles Doppel-Schreiben (identischer
  Payload → BTP-idempotent; optionales Future-Hardening).

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - (c) `src-tauri/src/tablet/server.rs` `process_result` (~1435) — Idempotenz-
    Zweig; nutzt den bestehenden `direct_btp_write_since`-Merker (`state.rs:687`).
  - (a) `src-tauri/assets/tablet.html` `trySubmitPending` (~2987) —
    `AbortController`-Timeout.
  - (b) `src-tauri/src/tablet/state.rs` — Persist-DTO, `persist_btp_retry`/
    `load_btp_retry`, Aufrufe in `queue_btp_retry`/`clear_btp_retry`/
    `set_snapshot`; `src-tauri/src/commands.rs` — Pfad-Helper + Startup-Wiring.
  - Relay: `relay/src/main.rs` `RESULT_TIMEOUT` 20 → 8 s.
- **Architekturregeln (R1–R6):**
  - **R5:** `process_result`-Validierung bleibt voll aktiv für neue Ergebnisse;
    der Idempotenz-Zweig quittiert **nur** einen Retry mit **identischen**
    entscheidenden Feldern `(sets, team1_won, score_status)` innerhalb eines
    kurzen TTL — ein abweichender Payload fällt weiter auf Fehler.
  - **R3:** (c)/(a) gelten LAN **und** Cloud (beide Wege laufen durch
    `process_result`); (b) ist Host-seitig (puffert LAN- wie Cloud-Ergebnisse).
  - R2 unberührt (BTP bleibt die Wahrheit); R1/R4/R6 unberührt.
- **Konfiguration & Abwärtskompatibilität:** keine neuen Config-Felder (interne
  Konstanten). `btp-retry.json` wird nur von der neuen Version geschrieben; alte
  Version ignoriert sie, neue toleriert ihr Fehlen (leere Queue). Auto-Update-
  sicher; der Turnier-Guard verhindert Fremd-Turnier-Replay nach Neustart.
- **Datenschutz:** Die Persist-Datei trägt `MatchUpdate`-Felder inkl.
  BTP-`player_ids` (Spieler-**IDs**, keine Namen/Geburtsjahr) + `tournament_name`.
  Im App-Datenverzeichnis, kein Namensbezug — vertretbar.
- **Abhängigkeiten:** keine neue Cargo-/npm-Dependency (serde, write-atomic-Muster
  vorhanden). Relay unabhängig deploybar.

## Akzeptanzkriterien

**(c) Idempotenz**
- [ ] Ein erfolgreicher Ergebnis-Write räumt das Feld; ein **identischer**
      Wiederholungs-POST liefert danach `ok:true` (nicht „Kein Match auf diesem
      Court") → das Tablet löscht `pendingResult`.
- [ ] Ein Wiederholungs-POST mit **abweichenden** Sätzen auf ein geräumtes/
      gewechseltes Feld liefert weiter **Fehler** (R5, keine Falsch-Bestätigung).
- [ ] Ein **genuin neues** Ergebnis auf einem belegten Feld durchläuft die normale
      Validierung/Schreibung (Idempotenz greift nicht).
- [ ] Liegt der letzte Direkt-Write länger als das Idempotenz-TTL zurück, liefert
      ein Retry wieder **Fehler** (eine echte spätere Korrektur wird nicht
      abgewürgt).

**(a) Client-Timeout**
- [ ] Bleibt der Ergebnis-`fetch` hängen, bricht der Client nach dem Backstop-
      Timeout ab und startet den 5-s-Retry; `pendingResult` bleibt bis `ok:true`
      erhalten (kein Verlust). Der Absende-Klick blockiert unverändert **nicht**.

**(b) Persistente Host-Queue**
- [ ] `queue_btp_retry`/`clear_btp_retry` schreiben die Queue **synchron + atomar**
      nach `btp-retry.json`.
- [ ] Nach einem Host-App-Neustart mit **gleichem** Turnier wird die Queue
      identisch geladen (inkl. `player_ids`) und weiter geflusht.
- [ ] Nach Neustart mit **anderem** Turnier (`tournament_name`-Mismatch) wird die
      geladene Queue **verworfen** (kein Schreiben in ein fremdes Match).
- [ ] Fehlende **oder** korrupte `btp-retry.json` → leere Queue, kein Fehler/Panic.
- [ ] Ein zwischen Start und erstem Snapshot frisch eingereihter Eintrag überlebt
      das Laden (Merge, nicht Replace).

**Relay**
- [ ] `RESULT_TIMEOUT` ist 8 s; `pending`-Slots werden entsprechend schneller frei.

## Tests

**Rust-Unit-Tests (TDD):**
- (c) `process_result`: die vier Idempotenz-Fälle oben (identisch→ok, abweichend→
  err, neu→normal, TTL-abgelaufen→err) + „Feld gewechselt, identisches
  Alt-Ergebnis→ok".
- (b) state.rs (Vorbild `tempdir`-Tests): Persist→Load-Roundtrip (inkl.
  `player_ids`/`enqueued_ms`); Turnier-Guard verwirft Mismatch; fehlende Datei →
  leer; korrupte JSON → leer + `warn`, kein Panic; `clear_btp_retry` schreibt die
  verkleinerte Datei; Merge behält frisch Eingereihtes.
- Relay: vorhandene Result-Tests grün; ggf. Timeout-Wert-Assertion.

**JS/Build:** `npm run build` grün; Browser-Smoke „Abort → sofortiger Retry,
`pendingResult` bleibt bis `ok`".

`cargo test` grün, `cargo clippy --workspace --all-targets -D warnings` sauber vor
jedem Commit.

## Risiken & Rollback

- **Idempotenz zu breit** (echtes Ergebnis fälschlich als „settled" → stiller
  Verlust): abgesichert durch Feld-Vergleich `(sets, team1_won, score_status)` +
  kurzes TTL; die (c)-Tests sind das Gate. **security-reviewer** prüft den
  Kontrakt.
- **Fremd-Turnier-Replay** nach Neustart < 24 h mit anderem Turnier: durch
  `tournament_name`-Guard verworfen.
- **Doppel-`SENDUPDATE`** bei echt-paralleler In-Flight-Zustellung: identischer
  Payload → BTP-idempotent, datenharmlos (bekanntes Restrisiko, dokumentiert).
- **Platten-I/O im Hot-Path** (`queue_btp_retry`): I/O außerhalb des Daten-Locks
  (eigener Persist-Mutex), best-effort — darf die Ergebnisannahme nie blockieren
  (wie `persist_scores`).
- **Rollback:** alle vier Teile einzeln reversibel; ein App-Downgrade liest
  `btp-retry.json` nicht → Verhalten = Status quo ante (Queue nur im RAM). Kein
  Schema-Zwang. Der Relay-Wert ist ein Einzeiler (eigener Deploy).

## Offene Fragen / Annahmen

- **Timeout-Werte** (Client-Abort ~12 s Backstop, Relay 8 s) sind nachjustierbar —
  keine harte p95-Messung nötig, weil (c) Duplikate harmlos macht. Der Client-
  Abort ist ein Backstop; im Normalfall antwortet der Relay bei ~8 s mit
  `ok:false` und der Client retryt regulär.
- **Annahme:** `direct_btp_write_since` (bestehend) ist der zuverlässige
  „zuletzt-geschrieben"-Merker; er wird vor `clear_court` gesetzt.

## Betroffene Doku-Dateien

- `docs/multi-hall.md` (nur die veraltete „kein Puffer/20-s"-Zeile ~346).
- `docs/tablet.md` (Ergebnis-/Retry-Weg: Client-Abort, Idempotenz-Kontrakt,
  persistente Host-Queue).
- `docs/cloud-relay.md` (`RESULT_TIMEOUT` 20→8, result/ResultAck).
- `docs/adr/0018-ergebnis-weg-verlustsicher.md`; `docs/roadmap.md`-Eintrag; je
  Version `docs/changelog.md`.

## Umsetzungs-Hinweise

(Ergebnis der How-To-Phase — Details:
`docs/features/_intake/ergebnis-puffer/3-how-to.md`.)

Reihenfolge: **(c) Idempotenz zuerst** (macht (a) sicher) → **(a) Client-Abort**
→ **(b) persistente Queue** → **Relay-Timeout**. Muster: (c) nutzt
`direct_btp_write_since` (state.rs:687); (b) spiegelt `persist_scores`
(atomar, I/O außerhalb des Daten-Locks, DTO wie `PersistedScore`,
Turnier-Guard = `tournament_name` wie ADR 0015), laden in `set_snapshot` beim
ersten Aufruf, Merge statt Replace.

- **ADR 0018** vor der Umsetzung finalisieren (4 gekoppelte Entscheidungen:
  Idempotenz-Kontrakt, Persist-Format/-Ort/Guard, DTO vs. Proto, Schreib-Zeitpunkt).
- **Version gemeinsam** bumpen (`src-tauri/Cargo.toml` + `tauri.conf.json` +
  `package.json`); Relay eigener Deploy.
- **Review:** `code-reviewer` (Idempotenz-Korrektheit/R5, Lock-Disziplin,
  Merge-Logik); **`security-reviewer`** (neuer Datei-Persist mit player_ids +
  geänderter `process_result`-Kontrakt = Integritätsrisiko bei falschem `ok()`).
