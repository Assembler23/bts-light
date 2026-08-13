# 0018 — Ergebnis-Weg verlustsicher: Idempotenz, persistente Retry-Queue

- **Status:** proposed
- **Datum:** 2026-08-13

Gehört zu [docs/features/ergebnis-puffer.md](../features/ergebnis-puffer.md)
(Cluster-Hebel B).

## Kontext

Der Ergebnis-Weg (Match-Ende → `POST /result` → `process_result` → BTP) hat
bereits zwei Puffer: Tablet-`pendingResult` (localStorage, 5-s-Retry bis
`ok:true`) und die Host-`btp_retry`-Queue (30-s-Flush). Der Grill deckte drei
Lücken auf, die eine ADR mit **vier gekoppelten** Entscheidungen erzwingen. Die
zentrale: Nach einem erfolgreichen Write räumt der Server das Feld
(`clear_court`), sodass ein Wiederholungs-POST „Kein Match auf diesem Court"
bekommt und **nie `ok:true`** — das Tablet retryt endlos, und ein zeitgleicher
zweiter POST kann einen doppelten `SENDUPDATE` auslösen. Ein kürzerer
Client-Timeout (gegen träge Cloud-Retries) verschärft das.

## Entscheidung

**(1) Idempotenz-Kontrakt in `process_result`.** Ein Wiederholungs-POST für ein
Match, dessen **identisches** Ergebnis bereits geschrieben wurde, wird mit
`ResultResponse::ok()` quittiert statt mit Fehler. Erkennung primär über den
bestehenden Merker `direct_btp_write_since(match_id, now-TTL)` (state.rs:687,
gesetzt in `write_result_settled` **vor** `clear_court`), Vergleich der
**entscheidenden Felder** `(sets, team1_won, score_status)`; als Netz zusätzlich
„Snapshot hat schon `winner` für die match_id mit konsistenter Sieger-Seite".
`RESULT_IDEMPOTENCY_TTL` ~60 s. **R5:** nur ein Retry mit identischem Ergebnis
wird quittiert; ein abweichender Payload fällt weiter auf Fehler; die
`derive_result`/`sets_fit_format`-Validierung bleibt für neue Ergebnisse aktiv.

**(2) Persist-Format/-Ort der Host-Queue.** `app_data_dir/btp-retry.json`
(Helper `tablet_btp_retry_path`, Vorbild `tablet_scores_path`). Die Datei trägt
einen **Turnier-Guard** = `snapshot.tournament_name`; beim Laden wird bei
Mismatch verworfen (BTP-`match_id` sind pro Turnier vergeben). Vollständig inkl.
`player_ids`. Atomar (tmp+rename), **kein `fsync`** → Durabilität nur gegen
App-Neustart. Fehlende/korrupte Datei → leere Queue.

**(3) Eigenes Persist-DTO** (`PersistedBtpQueue`/`PersistedMatchUpdate`) statt
`#[derive(Serialize)]` auf den BTP-Wire-Typ `MatchUpdate`. Explizite,
versionierbare Grenze zwischen Platten-Schema und Wire-Typ (Muster
`PersistedScore`).

**(4) Schreib-Zeitpunkt/Locking.** **Synchron** bei jedem `queue_btp_retry`/
`clear_btp_retry` (nicht periodisch), **I/O außerhalb des `btp_retry`-Locks**
über einen separaten Persist-Mutex (Queue unter kurzem `read()` klonen, dann
schreiben) — Muster `persist_scores`. Laden in `set_snapshot` beim ersten Aufruf
(Turnier-Guard verfügbar); **Merge statt Replace**.

## Alternativen

- **(1) Snapshot-only-Idempotenz:** verworfen — der 2-s-Poll-Snapshot zeigt den
  BTP-Sieger im Fast-Retry-Fenster evtl. noch nicht (racy). Der Direkt-Write-
  Merker ist deterministisch. — **Retry hart stoppen statt quittieren:** ein
  „schon verarbeitet"-Signal, das den Client-Retry beendet, ohne `ok` — verworfen,
  weil das Tablet-Kontrakt („löschen nur bei `ok:true`") sonst umgebaut werden
  müsste.
- **(2) `checkin.tournament_uuid` als Guard:** verworfen — nur bei konfiguriertem
  Check-In gesetzt (sonst leer). `tournament_name` ist immer da (ADR 0015). —
  **Reines Max-Alter (24 h) ohne Turnier-Guard:** verworfen — deckt den
  Turnierwechsel < 24 h mit match_id-Kollision nicht.
- **(3) serde direkt auf `MatchUpdate`:** verworfen — koppelt Disk-Schema an den
  Wire-Typ.
- **(4) Periodisches Persistieren:** verworfen — lässt das Crash-Fenster zwischen
  Enqueue und nächstem Flush offen (verfehlt das Ziel).

## Konsequenzen

- Der Endlos-Retry/Doppel-Write-Bug ist behoben; ein kürzerer Client-Timeout wird
  sicher (Duplikate idempotent). Ergebnisse überstehen Host-App-Neustart.
- **Negativ / Restrisiken:** Treffen zwei POSTs ein, während der erste Write noch
  läuft (Merker noch nicht gesetzt), schreiben beide → doppelter `SENDUPDATE` mit
  identischem Payload (BTP-idempotent, datenharmlos) — ein per-Match-In-Flight-
  Guard ist optionales Future-Hardening. Keine Strom-/OS-Crash-Durabilität
  (`write` ohne `fsync`). Ein zu breiter Idempotenz-Zweig wäre stiller
  Ergebnisverlust — durch Feld-Vergleich + TTL + Tests + Security-Review
  abgesichert.
