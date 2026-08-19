# TL-Web-Ausbau — Implementierungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Die Turnierleitungs-Weboberfläche (TL-Web) um fünf Punkte ausbauen: Zähltafel-Warteschlange bedienen, beendete Spiele anzeigen, Matchball-Einfärbung, echter Ergebnis-Dialog (auch für Spiele ohne Feld und Korrekturen), Feld-Raster nach Hallen-Anordnung (TL-Web + App-Felderübersicht).

**Architektur:** Kein neues Wire-Protokoll nötig — alle benötigten `TlAction`-Varianten (`ScorekeeperAdvance/Remove/Add`, `EnterResult`) existieren bereits in `relay-proto` und sind host-seitig implementiert. Die Arbeit besteht aus: (a) neue Felder im `TlState` (tl.rs) mit Allowlist-/Privacy-Pflege, (b) Bedienung/Anzeige in `src-tauri/assets/tl.html`, (c) für das Raster eine neue Host-Konfiguration `hall_layouts` + Desktop-Editor + geteilte JS-Mapping-Funktion.

**Tech Stack:** Rust (Tauri 2, `src-tauri`), Vanilla-JS in `tl.html` (kein Framework, kein Import), React 19 + TS (`src/`), Node-Skript-Tests (`scripts/test-*.mjs`), `cargo test --workspace`.

## Global Constraints

- **Regressions-Suite grün vor jedem Merge** (`cargo test --workspace`, `npm run build`, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, Node-Tests aus `ci.yml`). Kein Feature-Merge bei roter Suite (docs/regression-suite.md).
- **Jedes Arbeitspaket = eigener Branch + eigener PR** („Features/Fixes einzeln, klein, review’t“, roadmap.md). Nach jeder Code-Änderung den **code-reviewer**-Subagent laufen lassen (CLAUDE.md).
- **Kommentare Deutsch** (was + warum), Test-Namen im Stil der bestehenden (`the_scorekeeper_queue_can_be_tended`).
- **Neue `TlState`-Felder MÜSSEN** in die Allowlist `ERLAUBT` des Tests `every_published_field_is_deliberately_allowed` (tl.rs:3724) **mit Begründung** eingetragen werden; der Privacy-Test `the_state_never_carries_personal_data_beyond_its_purpose` (tl.rs:3838) muss grün bleiben (kein `member/birth/club/battery/serving/…`).
- **Kein Geburtsjahr, keine Member-IDs** in TL-State oder Logs (CLAUDE.md Datenschutz).
- **Größenbudget Relay:** `state_for_relay` (tl.rs:475) kürzt in Stufen 40/20/10/5 gegen `MAX_TL_STATE_LEN` = 64 KiB. Neue Listen (Beendet) müssen mitgekürzt werden.
- **`tl.html` liegt doppelt aus:** eingebettet in der App **und** im Relay. Jede `tl.html`-Änderung braucht am Ende App-Release **und** Relay-Deploy (docs/cloud-relay.md).
- **Doku im selben Commit** (CLAUDE.md-Tabelle): `docs/turnierleitung-web.md` (Bedienung), `docs/features/turnierleitung-web.md` (Spec), ggf. `docs/zaehltafelbediener.md`, `docs/changelog.md` je Release.
- **Versionen gemeinsam bumpen:** `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `package.json` — erst beim Release, nicht je PR.
- **Internet-Erreichbarkeit: bereits vorhanden** (commands.rs:2458 `tl_entrances` — Cloud-QR bei `cloud_enabled()`). Kein Code; nur Doku-Hinweis in WP1 (Schritt „Doku“) ergänzen: „Der Internet-Weg setzt Verbindungsmodus Cloud oder LAN+Cloud voraus.“

## Ist-Stand (Kurzreferenz für alle Tasks)

- `TlAction`-Enum: `relay-proto/src/lib.rs:885` — enthält bereits `ScorekeeperAdvance{key}`, `ScorekeeperRemove{key}`, `ScorekeeperAdd{names}`, `EnterResult{matchId, sets, retired, winner?, overwrite}`.
- Host-Ausführung: `src-tauri/src/tablet/tl.rs` — `execute` (:827), `apply_state_action` (:255, Scorekeeper fertig), `plan_result_action` (:660) + `execute_result_action` (:1028, EnterResult fertig).
- `TlState` (tl.rs:1348) wird in `build_state_limited` (tl.rs:1608) gebaut; `TlCourt` (tl.rs:1416) hat schon `best_of`, `target_score`, `cap_score`.
- Browser: `src-tauri/assets/tl.html` — `send(opId, action, onOkText)` (:812), Polling `poll()` (:1035), Kacheln `courtCard(c)` (:1267), Aktionsleiste (:497), Anzeige-Menü (:488).
- Zähltafel-Queue im Host: `tablet/state.rs:227` `ScorekeeperEntry{key, names, from_court_id, enqueued_ms}`; Methoden `scorekeeper_queue()` (:618), `add_scorekeeper_manual` (:598), `remove_scorekeeper` (:623), `advance_scorekeeper` (:631). Desktop-UI-Vorbild: `src/pages/FieldOverviewPage.tsx:742-828`.
- Beendete Spiele Desktop: `commands.rs:1852` `FinishedMatchRow` + `finished_matches` (:1880) — Filter `status == Finished && winner.is_some()`, neueste zuerst, `finished_at == None` ans Ende.
- Matchball-Regel kanonisch: `src/io/gamePoint.mjs` (`gamePointKind({sets, best_of, target_score, cap_score, match_id})` → `"match" | "set" | null`), Test `scripts/test-gamepoint.mjs`, Nutzung `FieldOverviewPage.tsx:405/456/539`.
- Config: `src-tauri/src/config.rs:437` `AppConfig`; `keep_host_managed_fields` (commands.rs:182) schützt host-verwaltete Felder vor dem SetupWizard-Rückschreiber; `mutate_config` (commands.rs:2435) ist das Muster für host-seitige Config-Änderungen.

---

## Arbeitspaket 1 — Zähltafel-Warteschlange in TL-Web bedienen

Branch: `feat/tl-scorekeeper-queue`. Die drei Aktionen existieren host-seitig samt Tests (tl.rs:3356) — es fehlt die Queue im `TlState` und die Bedienung in `tl.html`.

### Task 1: Warteschlange in den TlState

**Files:**
- Modify: `src-tauri/src/tablet/tl.rs` (Struct :1348, `build_state_limited` :1608, Allowlist :3724)

**Interfaces:**
- Produces: `TlState.scorekeeper_managed: bool`, `TlState.scorekeepers: Vec<TlScorekeeper>` mit `TlScorekeeper{key: String, names: Vec<String>, enqueued_ms: u64}` — Task 2 (tl.html) verlässt sich auf genau diese JSON-Feldnamen (snake_case).

- [ ] **Step 1: Fehlschlagenden Test schreiben** (in `#[cfg(test)] mod tests` von tl.rs, neben `the_scorekeeper_queue_can_be_tended`):

```rust
#[test]
fn the_state_shows_the_scorekeeper_queue_only_when_managed() {
    // Aufbau wie in `the_scorekeeper_queue_can_be_tended`: TabletState mit
    // Snapshot, ein manueller Eintrag in der Warteschlange.
    let tablet = tablet_with_snapshot(); // vorhandenen Test-Helfer verwenden
    tablet.add_scorekeeper_manual(vec!["Anna Alt".into()]);

    let mut config = test_config(); // vorhandenen Helfer verwenden
    config.scorekeeper.enabled = true;
    let state = build_state(&tablet, &config, 1_000, 1);
    assert!(state.scorekeeper_managed);
    assert_eq!(state.scorekeepers.len(), 1);
    assert_eq!(state.scorekeepers[0].names, vec!["Anna Alt".to_string()]);
    assert!(!state.scorekeepers[0].key.is_empty());

    // Verwaltung aus: Die Liste bleibt leer, das Gerät blendet den
    // Abschnitt aus — niemand bedient eine Warteschlange, die es nicht gibt.
    config.scorekeeper.enabled = false;
    let state = build_state(&tablet, &config, 1_000, 2);
    assert!(!state.scorekeeper_managed);
    assert!(state.scorekeepers.is_empty());
}
```

Hinweis an den Umsetzer: Die exakten Namen der Test-Helfer (`tablet_with_snapshot`, `test_config`) aus den Nachbar-Tests übernehmen — nicht neu erfinden.

- [ ] **Step 2: Test laufen lassen — er muss scheitern** (Felder existieren nicht):

Run: `cargo test -p bts-light the_state_shows_the_scorekeeper_queue`
Expected: Kompilierfehler „no field `scorekeeper_managed`“.

- [ ] **Step 3: Implementieren.** In `TlState` (tl.rs:1348) ergänzen:

```rust
    /// Verwaltet dieser Turnier-PC Zähltafelbediener? Nur dann zeigt die
    /// Seite den Warteschlangen-Abschnitt.
    pub scorekeeper_managed: bool,
    /// Die Warteschlange, in Reihenfolge. Der `key` ist die stabile
    /// Kennung für Vorziehen/Entfernen — dieselbe wie am Turnier-PC.
    pub scorekeepers: Vec<TlScorekeeper>,
```

Neues Struct daneben (bei `TlHall`/`TlWalkover`):

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlScorekeeper {
    pub key: String,
    pub names: Vec<String>,
    pub enqueued_ms: u64,
}
```

In `build_state_limited` (beide Zweige — auch der Leer-Zustand ohne Snapshot!) befüllen:

```rust
    let scorekeeper_managed = config.scorekeeper.enabled;
    let scorekeepers = if scorekeeper_managed {
        tablet
            .scorekeeper_queue()
            .into_iter()
            .map(|e| TlScorekeeper {
                key: e.key,
                names: e.names,
                enqueued_ms: e.enqueued_ms,
            })
            .collect()
    } else {
        Vec::new()
    };
```

Im Leer-Zustand (tl.rs:1618): `scorekeeper_managed: config.scorekeeper.enabled, scorekeepers: Vec::new(),`.

- [ ] **Step 4: Allowlist pflegen.** Im Test `every_published_field_is_deliberately_allowed` (tl.rs:3724) die neuen Felder eintragen, mit Begründung im Stil der Nachbarn:

```rust
    // Warteschlange der Zähltafelbediener: Namen stehen ohnehin je Feld im
    // Zustand (`scorekeeper`); der `key` ist eine zufällige Kennung ohne
    // Personenbezug, `enqueued_ms` eine Uhrzeit.
    "scorekeeper_managed",
    "scorekeepers",
    "key",
    "names",
    "enqueued_ms",
```

- [ ] **Step 5: Alle tl-Tests laufen lassen:**

Run: `cargo test -p bts-light tablet::tl`
Expected: PASS, inkl. `every_published_field_is_deliberately_allowed`, `the_state_never_carries_personal_data_beyond_its_purpose` und `the_revision_only_moves_when_the_board_really_changed` (die Revision springt automatisch mit, weil `state_fingerprint` den ganzen Zustand serialisiert).

- [ ] **Step 6: Commit** — `git add src-tauri/src/tablet/tl.rs && git commit -m "TL-Web: Zähltafel-Warteschlange im Anzeige-Zustand"`.

### Task 2: Bedienung in tl.html

**Files:**
- Modify: `src-tauri/assets/tl.html` (Markup bei :509 vor `<h2>Als Nächstes`, Render-/Wire-Funktionen neben `renderWalkovers`, CSS im Stil der Karten)

**Interfaces:**
- Consumes: `state.scorekeeper_managed`, `state.scorekeepers[]` aus Task 1; bestehendes `send(opId, action, onOkText)` (tl.html:812).
- Produces: nichts für spätere Tasks.

- [ ] **Step 1: Markup einfügen** (in `<section>` der rechten Spalte, zwischen `#walkovers` und der Als-Nächstes-Überschrift):

```html
      <details class="sk" id="sk-box" hidden>
        <summary>Zähltafel-Warteschlange <span class="count" id="sk-count"></span></summary>
        <ol id="sk-list"></ol>
        <form id="sk-add">
          <input type="text" id="sk-name" placeholder="Name (Doppel: A / B)" maxlength="80" />
          <button type="submit">Hinzufügen</button>
        </form>
      </details>
```

- [ ] **Step 2: Render + Verdrahtung.** Neue Funktion neben den bestehenden Render-Funktionen; Aufruf im zentralen `render()` dort, wo auch Walkover gerendert werden:

```js
// ── Zähltafel-Warteschlange ───────────────────────────────────────────
// Die Aktionen laufen über dieselben Vorgangskennungen wie am Turnier-PC:
// `key` ist stabil je Eintrag — ein Doppeltipp ist damit dieselbe Kennung
// und wird als Wiederholung erkannt (kein doppeltes Vorziehen).
function renderScorekeepers() {
  const box = $("sk-box");
  box.hidden = !state.scorekeeper_managed;
  if (box.hidden) return;
  const list = state.scorekeepers || [];
  $("sk-count").textContent = list.length ? `(${list.length})` : "";
  $("sk-list").innerHTML = list.map((e, i) => `<li>
      <span class="sk-names">${esc(e.names.join(" / "))}</span>
      <span class="sk-wait num" title="wartet seit">${e.enqueued_ms ? elapsed(e.enqueued_ms) : ""}</span>
      ${i > 0 ? `<button type="button" data-sk-adv="${esc(e.key)}" title="an den Anfang">▲</button>` : ""}
      <button type="button" data-sk-del="${esc(e.key)}" title="aus der Warteschlange nehmen">×</button>
    </li>`).join("");
  $("sk-list").querySelectorAll("[data-sk-adv]").forEach((b) =>
    b.addEventListener("click", () =>
      send(`sk-adv-${b.dataset.skAdv}`, { action: "scorekeeper_advance", key: b.dataset.skAdv }, "vorgezogen")));
  $("sk-list").querySelectorAll("[data-sk-del]").forEach((b) =>
    b.addEventListener("click", () =>
      send(`sk-del-${b.dataset.skDel}`, { action: "scorekeeper_remove", key: b.dataset.skDel }, "entfernt")));
}
$("sk-add").addEventListener("submit", (ev) => {
  ev.preventDefault();
  const roh = $("sk-name").value.trim();
  if (!roh) return;
  const names = roh.split("/").map((s) => s.trim()).filter(Boolean);
  send(`sk-add-${roh}-${opWindow()}`, { action: "scorekeeper_add", names }, "eingetragen");
  $("sk-name").value = "";
});
```

Hinweise an den Umsetzer: (a) Die exakte `send`-Signatur und das Aktions-JSON-Format an den bestehenden Aufrufen ablesen (z. B. der Nachruf-Knopf, tl.html:1360 ff.) — die Aktion muss als `{action: "scorekeeper_advance", key}` auf die Leitung, genau wie `TlAction` es per serde erwartet. (b) `opWindow()` ist das bestehende 20-s-Zeitfenster-Muster (`OP_WINDOW_MS`, tl.html:779) — Funktionsnamen im Code nachschlagen und übernehmen. (c) CSS für `.sk li` kompakt im Stil der Walkover-Zeilen ergänzen.

- [ ] **Step 3: Manuell prüfen** (LAN reicht): App starten (`npm run tauri dev` oder Testlauf), TL-Web öffnen, mit aktivierter Zähltafel-Verwaltung: Eintrag hinzufügen → erscheint; Vorziehen ab Position 2; Entfernen; bei ausgeschalteter Verwaltung ist der Abschnitt unsichtbar.

- [ ] **Step 4: Doku im selben Commit:**
  - `docs/turnierleitung-web.md`: Grenze „Warteschlange lässt sich hier nur ansehen“ (Z. 137-138) **entfernen**, im Betrieb-Abschnitt drei Sätze zur Bedienung ergänzen. Zusätzlich unter „Einrichten“ den Satz: „Der Internet-Weg (zweiter QR-Code) setzt den Verbindungsmodus Cloud oder LAN+Cloud voraus.“
  - `docs/features/turnierleitung-web.md`: offenen Punkt 2 als erledigt markieren.
  - `docs/zaehltafelbediener.md`: Abschnitt „Bedienung aus TL-Web“.

- [ ] **Step 5: Suite + Commit:**

Run: `cargo test --workspace && npm run build`
Expected: grün. Dann `git add -A && git commit -m "TL-Web: Zähltafel-Warteschlange bedienen (vorziehen, entfernen, hinzufügen)"`, PR öffnen, code-reviewer laufen lassen.

---

## Arbeitspaket 2 — Beendete Spiele in TL-Web

Branch: `feat/tl-finished-list`. Anzeige-only; die Korrektur-Bedienung kommt in Arbeitspaket 4.

### Task 3: Beendete Spiele in den TlState

**Files:**
- Modify: `src-tauri/src/tablet/tl.rs` (Struct, `build_state_limited`, Allowlist)

**Interfaces:**
- Produces: `TlState.finished: Vec<TlFinished>` mit `TlFinished{match_id: i64, match_num: i64, draw_name: String, round_name: String, class_label: String, discipline: String, team1: Vec<String>, team2: Vec<String>, winner: u8, sets: Vec<(i64,i64)>, result: String, court: String, finished_at_ms: Option<u64>}`. `result` ∈ `"normal" | "walkover" | "retired" | "disqualified"`. Task 4 und Task 8 (Korrektur) verlassen sich auf diese Feldnamen.

- [ ] **Step 1: Fehlschlagenden Test schreiben:**

```rust
#[test]
fn finished_matches_appear_newest_first_and_are_capped() {
    // Snapshot mit drei beendeten Spielen: zwei mit finished_at
    // (200 und 100), eines ohne (vor App-Start beendet).
    // Aufbau über die vorhandenen Snapshot-Test-Helfer; `winner`,
    // `status = Finished`, `result = MatchResult::Retired` beim ältesten.
    let state = build_state_limited(&tablet, &config, 1_000, 1, 40);
    let ids: Vec<i64> = state.finished.iter().map(|f| f.match_id).collect();
    // Neueste zuerst, „ohne Zeitstempel" ans Ende — wie die Desktop-Liste.
    assert_eq!(ids, vec![id_200, id_100, id_ohne]);
    assert_eq!(state.finished[2].result, "retired");

    // Der Relay-Weg kürzt: Limit 5 heißt höchstens 5 Beendete.
    let eng = build_state_limited(&tablet, &config, 1_000, 2, 5);
    assert!(eng.finished.len() <= 5);
}
```

- [ ] **Step 2: Test laufen lassen** — Kompilierfehler „no field `finished`“ erwartet.

- [ ] **Step 3: Implementieren.** Struct + Konstante:

```rust
/// Höchstzahl beendeter Spiele im Zustand. Die Seite ist ein
/// Arbeits-Werkzeug, kein Archiv — wer mehr braucht, schaut in BTP.
const FINISHED_LIMIT: usize = 30;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TlFinished {
    pub match_id: i64,
    pub match_num: i64,
    pub draw_name: String,
    pub round_name: String,
    pub class_label: String,
    pub discipline: String,
    pub team1: Vec<String>,
    pub team2: Vec<String>,
    /// 1 oder 2 — wer gewonnen hat.
    pub winner: u8,
    pub sets: Vec<(i64, i64)>,
    /// `normal` | `walkover` | `retired` | `disqualified` — die Seite
    /// kennzeichnet alles außer `normal` mit einem Abzeichen, sonst sähe
    /// ein Teil-Spielstand (14:16, 15:10) wie ein Fehler aus.
    pub result: String,
    /// Feld, auf dem es lief; leer, wenn direkt in BTP gewertet.
    pub court: String,
    /// Nur zur Laufzeit gestempelt — Spiele, die vor dem App-Start
    /// beendet waren, haben keinen Zeitstempel und stehen am Ende.
    pub finished_at_ms: Option<u64>,
}
```

Befüllung in `build_state_limited` (Vorbild: `commands.rs:1880 finished_matches` — Filter, Sortierung und `result`-Abbildung dort **ablesen und übernehmen**, nicht neu erfinden):

```rust
    let finished_limit = FINISHED_LIMIT.min(queue_limit.max(5));
    let mut finished: Vec<&crate::btp::model::BtpMatch> = snap
        .matches
        .iter()
        .filter(|m| m.status == crate::btp::model::MatchStatus::Finished && m.winner.is_some())
        .collect();
    // Neueste zuerst; ohne Zeitstempel ans Ende (wie die Desktop-Liste).
    finished.sort_by_key(|m| std::cmp::Reverse(m.finished_at.map(|t| (1u8, t)).unwrap_or((0, 0))));
    let finished: Vec<TlFinished> = finished
        .into_iter()
        .take(finished_limit)
        .map(|m| TlFinished { /* Felder wie oben, court über die
            Court-Auflösung des Snapshots wie in FinishedMatchRow */ })
        .collect();
```

Im Leer-Zustand: `finished: Vec::new(),`.

- [ ] **Step 4: Allowlist** (`ERLAUBT`, tl.rs:3724) ergänzen: `finished`, `winner`, `result`, `court`, `finished_at_ms`, `match_num` … (nur die, die noch nicht drinstehen — Namen wie `draw_name`, `sets`, `team1` sind durch Queue/Courts schon erlaubt). Begründung: „Ergebnis-Übersicht; keine Personendaten über die ohnehin gezeigten Namen hinaus.“

- [ ] **Step 5: Tests laufen lassen:**

Run: `cargo test -p bts-light tablet::tl`
Expected: PASS (auch Privacy- und Allowlist-Wächter).

- [ ] **Step 6: Commit** — `git commit -m "TL-Web: beendete Spiele im Anzeige-Zustand"`.

### Task 4: Beendet-Liste in tl.html

**Files:**
- Modify: `src-tauri/assets/tl.html`

**Interfaces:**
- Consumes: `state.finished[]` aus Task 3.
- Produces: je Zeile einen (noch inaktiven) Platz für den Korrektur-Knopf aus Task 8 — Zeilen tragen `data-fin="<match_id>"`.

- [ ] **Step 1: Markup** (rechte Spalte, unter `#queue`):

```html
      <details class="fin" id="fin-box" hidden>
        <summary>Beendet <span class="count" id="fin-count"></span></summary>
        <div id="fin-list"></div>
      </details>
```

- [ ] **Step 2: Render-Funktion** (Aufruf im zentralen `render()`):

```js
// ── Beendete Spiele ───────────────────────────────────────────────────
const ERGEBNIS_BADGE = { walkover: "kampflos", retired: "Aufgabe", disqualified: "disqualifiziert" };
function renderFinished() {
  const list = state.finished || [];
  $("fin-box").hidden = list.length === 0;
  $("fin-count").textContent = list.length ? `(${list.length})` : "";
  $("fin-list").innerHTML = list.map((f) => {
    const sets = (f.sets || []).map(([a, b]) => `${a}:${b}`).join(" · ");
    const badge = ERGEBNIS_BADGE[f.result]
      ? `<span class="badge warn">${ERGEBNIS_BADGE[f.result]}</span>` : "";
    // Der Sieger zuerst genannt — so liest man ein Ergebnis.
    const team1 = f.team1.join(" / "), team2 = f.team2.join(" / ");
    return `<div class="fin-row" data-fin="${f.match_id}">
      <span class="fin-pair">${esc(f.winner === 2 ? `${team2} – ${team1}` : `${team1} – ${team2}`)}</span>
      <span class="fin-meta">${esc([klasse(f.discipline, f.class_label), f.round_name, f.court].filter(Boolean).join(" · "))}</span>
      <span class="fin-score num">${esc(sets)}${badge}</span>
    </div>`;
  }).join("");
}
```

Hinweis: `klasse(...)` und `esc(...)` sind vorhandene Helfer der Seite. CSS: `.fin-row` als schmale Zeile (Flex, kleine Schrift), zugeklappter `<details>`-Standard.

- [ ] **Step 3: Manuell prüfen:** Turnier mit beendeten Spielen laden → Abschnitt erscheint zugeklappt, Reihenfolge neueste zuerst, Aufgabe-Badge sichtbar.

- [ ] **Step 4: Doku:** `docs/turnierleitung-web.md` Grenze „Beendete Spiele zeigt die Seite nicht“ (Z. 139) entfernen + kurzer Absatz; Spec-Punkt 3 als erledigt markieren.

- [ ] **Step 5: Suite + Commit + PR** (wie WP1 Schritt 5). Commit: `"TL-Web: Beendet-Liste mit Aufgabe-/kampflos-Kennzeichnung"`.

---

## Arbeitspaket 3 — Matchball-Einfärbung in TL-Web

Branch: `feat/tl-gamepoint`. Die Regel ist kanonisch in `src/io/gamePoint.mjs` (getestet); die App-Felderübersicht nutzt sie bereits. `TlCourt` liefert `best_of`/`target_score`/`cap_score` schon (tl.rs:1470-1472) — reine `tl.html`-Änderung.

### Task 5: Satz-/Matchball-Abzeichen in tl.html

**Files:**
- Modify: `src-tauri/assets/tl.html` (Inline-Kopie der Regel + `courtCard`)

**Interfaces:**
- Consumes: `c.sets`, `c.best_of`, `c.target_score`, `c.cap_score`, `c.match_id` aus `TlCourt`.

- [ ] **Step 1: Regel inlinen.** Direkt über `courtCard` eine Kopie von `gamePointKind` aus `src/io/gamePoint.mjs` einfügen, mit Herkunfts-Kommentar:

```js
// ── Satz-/Matchball ───────────────────────────────────────────────────
// INLINE-KOPIE von src/io/gamePoint.mjs (tl.html kann keine Module laden).
// Die kanonische Fassung samt Test (scripts/test-gamepoint.mjs) liegt dort —
// Änderungen IMMER zuerst dort machen und hierher spiegeln.
function gamePointKind(c) { /* Funktionskörper 1:1 aus gamePoint.mjs */ }
```

Der Umsetzer kopiert den Funktionskörper wörtlich aus `src/io/gamePoint.mjs` (samt `setDecided`/`setsToWin`-Helfern, falls dort separat).

- [ ] **Step 2: Kachel einfärben.** In `courtCard(c)` (tl.html:1267) nach der `badges`-Sammlung:

```js
  // Satz-/Matchball: nur für die Turnierleitung, nie auf den Hallen-TVs
  // (Scope-Entscheidung 20.07.2026, Plan 16). Die Streifenfarben bleiben
  // unangetastet — Rot heißt dort weiterhin „überfällig".
  const ball = c.match_id && !c.locked ? gamePointKind(c) : null;
  if (ball === "match") { classes.push("gp-match"); badges.push('<span class="badge gp">Matchball</span>'); }
  else if (ball === "set") { classes.push("gp-set"); badges.push('<span class="badge gp-s">Satzball</span>'); }
```

CSS (Farbwerte an `FieldOverviewPage.tsx:456-460/539-549` angleichen — der Umsetzer liest die dortigen Klassen ab, damit App und TL-Web dieselbe Sprache sprechen):

```css
.card.gp-match { box-shadow: 0 0 0 2px #dc2626 inset; animation: gpPuls 1.2s ease-in-out infinite; }
.card.gp-set   { box-shadow: 0 0 0 2px #d97706 inset; }
.badge.gp   { background: #dc2626; color: #fff; }
.badge.gp-s { background: #d97706; color: #fff; }
@keyframes gpPuls { 50% { box-shadow: 0 0 0 4px #dc2626 inset; } }
```

- [ ] **Step 3: Manuell prüfen:** Testspiel auf 20:x bringen (Tablet-Simulation oder echtes Tablet) → Kachel bekommt Abzeichen; bei Satzball im 1. Satz „Satzball“, beim entscheidenden Satz „Matchball“; 29:29-Cap zeigt beidseitig korrekt (Fall aus dem gamePoint-Test).

- [ ] **Step 4: Doku:** `docs/turnierleitung-web.md` — im Abschnitt „Die Farbe eines Feldes“ einen Absatz zu den zwei Abzeichen; roadmap.md-Punkt „Matchball-Einfärbung“ als erledigt markieren (Plan 16).

- [ ] **Step 5: Suite + Commit + PR.** Run: `cargo test --workspace && npm run build && node scripts/test-gamepoint.mjs`. Commit: `"TL-Web: Satz- und Matchball an der Feldkachel"`.

---

## Arbeitspaket 4 — Ergebnis-Dialog: Sätze eintippen, Spiele ohne Feld, Korrektur

Branch: `feat/tl-result-dialog`. Der Host-Pfad (`EnterResult` → `plan_result_action` → `execute_result_action`) existiert komplett, inkl. `overwrite`-Korrektur mit `correction_blocker`. Heute hängt davor nur ein `prompt()` und der Knopf erscheint ausschließlich für Spiele **auf dem Feld** (tl.html:762). Drei Lücken: ordentlicher Dialog, Eingabe für Spiele aus der Warteliste, Korrektur aus der Beendet-Liste.

### Task 6: Host-Absicherung — Ergebnis für ein Spiel, das nie auf einem Feld stand

**Files:**
- Modify (nur falls der Test scheitert): `src-tauri/src/tablet/tl.rs` (`plan_result_action`/`execute_result_action`)
- Test: `src-tauri/src/tablet/tl.rs` `#[cfg(test)]`

**Interfaces:**
- Consumes: bestehende `TlAction::EnterResult`.

- [ ] **Step 1: Test schreiben** (neben `entering_a_result_builds_the_same_write_as_the_desktop_path`, tl.rs:2404):

```rust
#[test]
fn a_result_can_be_entered_for_a_match_that_never_saw_a_court() {
    // Spiel im Status Scheduled, keinem Feld zugewiesen — die
    // Turnierleitung trägt den Endstand ein (niemand hat gezählt).
    // Erwartung: gleiche Schreib-Nutzlast wie am Desktop-Pfad
    // (enter_result, commands.rs:1349), Status-Feld gesetzt, KEINE
    // Feldfreigabe (es gibt kein Feld freizugeben).
    // Aufbau + Assertions nach dem Muster des Nachbar-Tests.
}
```

- [ ] **Step 2: Test laufen lassen.** Erwartung offen: Läuft er **grün**, ist host-seitig nichts zu tun (die Plan-Recherche legt das nahe — `build_manual_result_update_opt` arbeitet am Match, nicht am Feld). Scheitert er, die Ursache in `plan_result_action`/`execute_result_action` beheben (z. B. `on_court_since_ms`-Annahme), bis er grün ist — ohne den Feld-Pfad zu verändern (Nachbar-Tests bleiben grün).

- [ ] **Step 3: Commit** — `"TL-Web: Ergebnis-Eingabe für nie aufgerufene Spiele abgesichert"`.

### Task 7: Der Dialog in tl.html

**Files:**
- Modify: `src-tauri/assets/tl.html` (Modal-Markup, `enterResult`-Ersatz :912, `renderActionBar` :762)

**Interfaces:**
- Consumes: `TlCourt.best_of/target_score/cap_score` (Vorbelegung/Validierung, wenn bekannt), bestehendes `send()`.
- Produces: `openResultDialog({matchId, label, sets, bestOf, overwrite})` — Task 8 ruft dieselbe Funktion aus der Beendet-Liste.

- [ ] **Step 1: Modal-Markup** (vor dem Toast):

```html
<dialog id="res-dlg">
  <form method="dialog" id="res-form">
    <h3 id="res-title">Ergebnis eintragen</h3>
    <p class="meta" id="res-pair"></p>
    <div id="res-sets"></div>
    <button type="button" id="res-add-set">+ Satz</button>
    <p class="fehler" id="res-error" hidden></p>
    <div class="dlg-foot">
      <button type="button" id="res-cancel">Abbrechen</button>
      <button type="submit" id="res-save">Speichern</button>
    </div>
  </form>
</dialog>
```

- [ ] **Step 2: Dialog-Logik.** `enterResult(court)` (tl.html:912) durch einen Aufruf von `openResultDialog` ersetzen; die neue Funktion:

```js
// ── Ergebnis-Dialog ───────────────────────────────────────────────────
// Ein Formular statt prompt(): je Satz zwei Zahlenfelder. Geprüft wird
// hier nur, was das Gerät sicher weiß (Zahlen, nicht leer) — die
// verbindliche Satzplausibilität hat der Turnier-PC (R5), seine Antwort
// erscheint im Dialog statt als Toast, damit nichts verloren geht.
let resCtx = null;
function openResultDialog(ctx) {
  resCtx = ctx; // {matchId, label, sets, bestOf, overwrite}
  $("res-title").textContent = ctx.overwrite ? "Ergebnis korrigieren" : "Ergebnis eintragen";
  $("res-pair").textContent = ctx.label;
  $("res-error").hidden = true;
  const rows = Math.max(ctx.sets.length, Math.min(ctx.bestOf || 3, 2));
  $("res-sets").innerHTML = Array.from({ length: rows }, (_, i) => resSetRow(i, ctx.sets[i])).join("");
  $("res-dlg").showModal();
}
const resSetRow = (i, [a, b] = ["", ""]) => `<div class="res-row">
    <label>Satz ${i + 1}</label>
    <input type="number" min="0" max="99" inputmode="numeric" value="${a}" data-res-a="${i}" />
    <span>:</span>
    <input type="number" min="0" max="99" inputmode="numeric" value="${b}" data-res-b="${i}" />
  </div>`;
$("res-add-set").addEventListener("click", () => {
  const n = $("res-sets").children.length;
  if (n < (resCtx.bestOf || 3)) $("res-sets").insertAdjacentHTML("beforeend", resSetRow(n));
});
$("res-cancel").addEventListener("click", () => $("res-dlg").close());
$("res-form").addEventListener("submit", (ev) => {
  ev.preventDefault();
  const sets = [];
  for (const row of $("res-sets").children) {
    const a = row.querySelector("[data-res-a]").value, b = row.querySelector("[data-res-b]").value;
    if (a === "" && b === "") continue; // leere Zeile = weggelassener Satz
    sets.push([Number(a || 0), Number(b || 0)]);
  }
  if (!sets.length) { showResError("Mindestens ein Satz."); return; }
  sendResult(resCtx.matchId, sets, resCtx.overwrite);
});
```

`sendResult` baut **dieselbe** Aktions-Nutzlast wie der bisherige `enterResult`-Code (tl.html:932) — Form der `sets` und Feldnamen dort wörtlich übernehmen, nur `overwrite` aus dem Kontext. Die Fehlerantwort des Turnier-PCs (`send`-Fehlerpfad) landet über `showResError(text)` im Dialog; bei Erfolg `$("res-dlg").close()`.

- [ ] **Step 3: Knopf auch für Wartelisten-Spiele.** In `renderActionBar` (tl.html:762) die Bedingung von `hidden = !picked.fromCourtId` auf „ein Spiel ist gewählt“ erweitern; beim Klick für Feld-Spiele `{sets: c.sets, bestOf: c.best_of}` mitgeben (Vorbelegung mit dem Live-Stand), für Wartelisten-Spiele `{sets: [], bestOf: 3}`.

- [ ] **Step 4: Manuell prüfen:** (a) Feld-Spiel: Dialog zeigt Live-Stand vorbelegt, Speichern beendet das Spiel (BTP-Testumgebung oder Mitschnitt-Replay); (b) Wartelisten-Spiel: leerer Dialog, Speichern wertet; (c) Unplausible Sätze (21:5, 2:0 fehlend) → Fehlertext des Turnier-PCs erscheint **im Dialog**.

- [ ] **Step 5: Doku:** `docs/turnierleitung-web.md` Betrieb-Abschnitt („Ergebnis eintragen“ neu beschreiben: auch aus der Warteliste); Spec aktualisieren.

- [ ] **Step 6: Suite + Commit** — `"TL-Web: Ergebnis-Dialog mit Satzfeldern, auch für Spiele ohne Feld"`.

### Task 8: Korrektur aus der Beendet-Liste

**Files:**
- Modify: `src-tauri/assets/tl.html` (`renderFinished` aus Task 4)

**Interfaces:**
- Consumes: `openResultDialog` (Task 7), `state.finished[]` (Task 3), Host-Pfad `overwrite:true` → `correction_blocker` (tl.rs:~620, lehnt `Running`/`Decided`/`Untested` ab — genau die dokumentierte Grenze „nur solange nichts daran hängt“).

- [ ] **Step 1: Knopf je Beendet-Zeile.** In `renderFinished` an jede `fin-row` anhängen:

```js
      <button type="button" data-fin-fix="${f.match_id}" title="Ergebnis korrigieren">✎</button>
```

Verdrahtung nach dem Render:

```js
  $("fin-list").querySelectorAll("[data-fin-fix]").forEach((b) =>
    b.addEventListener("click", () => {
      const f = (state.finished || []).find((x) => String(x.match_id) === b.dataset.finFix);
      if (!f) return;
      openResultDialog({
        matchId: f.match_id,
        label: `${f.team1.join(" / ")} – ${f.team2.join(" / ")}`,
        sets: f.sets, bestOf: 3, overwrite: true,
      });
    }));
```

- [ ] **Step 2: Manuell prüfen:** Beendetes Spiel ohne Folgespiel korrigieren → geht durch; mit Folgespiel → Ablehnungstext des Turnier-PCs im Dialog („Sobald der Sieger im nächsten Spiel steht …“).

- [ ] **Step 3: Doku:** `docs/turnierleitung-web.md` — Korrektur-Absatz in den Grenzen um den neuen Einstieg (Stift an der Beendet-Zeile) ergänzen.

- [ ] **Step 4: Suite + Commit + PR** — `"TL-Web: Ergebniskorrektur aus der Beendet-Liste"`. code-reviewer + **security-reviewer** über das ganze Paket (neuer User-Input im Dialog).

---

## Arbeitspaket 5 — Feld-Raster nach Hallen-Anordnung

Branch: `feat/hall-grid`. **Mini-Spec:** Die Felder erscheinen in TL-Web **und** der App-Felderübersicht so angeordnet wie in der Halle. Das Layout ist eine **Host-Einstellung je Halle** (alle Geräte sehen dasselbe): Spaltenzahl, Start-Ecke der Nummerierung (unten-links / unten-rechts / oben-links / oben-rechts), optional Schlangen-Nummerierung (Richtungswechsel je Reihe). Die Feld-**Reihenfolge** bleibt die BTP-Reihenfolge — das Layout bestimmt nur, in welcher Zelle Feld Nr. n liegt. Ohne Layout für eine Halle: heutige Fließ-Darstellung. Zusätzlich, **je Gerät** in TL-Web: Spielliste rechts neben oder unter den Feldern. Drag-&-Drop-Anordnung ist ausdrücklich **nicht** Teil dieses Pakets (Roadmap-Notiz).

### Task 9: Konfiguration `hall_layouts` + Commands

**Files:**
- Modify: `src-tauri/src/config.rs` (AppConfig :437), `src-tauri/src/commands.rs` (`keep_host_managed_fields` :182, neue Commands), `src-tauri/src/main.rs` bzw. `lib.rs` (`generate_handler`-Liste), `src/api.ts`, `src/types.ts`
- Test: `src-tauri/src/config.rs` und `src-tauri/src/commands.rs` `#[cfg(test)]`

**Interfaces:**
- Produces: `HallLayoutConfig{hall: String, columns: u8, origin: LayoutOrigin, serpentine: bool}` mit `LayoutOrigin ∈ {BottomLeft, BottomRight, TopLeft, TopRight}` (serde snake_case: `bottom_left` …); `AppConfig.hall_layouts: Vec<HallLayoutConfig>`; Tauri-Commands `set_hall_layout(layout) -> AppConfig` (Upsert je Halle) und `remove_hall_layout(hall) -> AppConfig`. Tasks 10-12 verlassen sich auf genau diese Namen.

- [ ] **Step 1: Fehlschlagenden Test schreiben** (config.rs-Tests):

```rust
#[test]
fn hall_layouts_survive_a_config_roundtrip_and_default_empty() {
    // Alte Configs ohne das Feld laden weiter (serde default) …
    let cfg: AppConfig = serde_json::from_str("{}").expect("Minimal-Config lädt");
    assert!(cfg.hall_layouts.is_empty());
    // … und ein gesetztes Layout überlebt Speichern + Laden.
    let mut cfg = cfg;
    cfg.hall_layouts.push(HallLayoutConfig {
        hall: "Halle 1".into(), columns: 3,
        origin: LayoutOrigin::BottomRight, serpentine: true,
    });
    let json = serde_json::to_string(&cfg).expect("serialisiert");
    let zurueck: AppConfig = serde_json::from_str(&json).expect("lädt");
    assert_eq!(zurueck.hall_layouts, cfg.hall_layouts);
}
```

(Falls `from_str("{}")` bei AppConfig nicht trägt, das Muster der bestehenden Config-Default-Tests in config.rs übernehmen.)

- [ ] **Step 2: Test laufen lassen** — Kompilierfehler erwartet.

- [ ] **Step 3: Implementieren** (config.rs):

```rust
/// Ecke, in der die Feld-Nummerierung beginnt — aus Sicht der
/// Turnierleitung auf die Halle geschaut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutOrigin { BottomLeft, BottomRight, TopLeft, TopRight }

/// Anordnung der Felder einer Halle als Raster. Host-Einstellung: Alle
/// Geräte zeigen dasselbe Raster — sonst meinte „das Feld links unten"
/// auf jedem Tablet ein anderes.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HallLayoutConfig {
    pub hall: String,
    pub columns: u8,
    pub origin: LayoutOrigin,
    /// Richtungswechsel je Reihe (Schlangen-Nummerierung), wie Hallen
    /// mit 1-2-3 / 6-5-4 zählen.
    pub serpentine: bool,
}
```

In `AppConfig`: `#[serde(default)] pub hall_layouts: Vec<HallLayoutConfig>,`.

- [ ] **Step 4: `keep_host_managed_fields` erweitern** (commands.rs:182) — der SetupWizard kennt das Feld nicht und würde es sonst beim Speichern leeren (dieselbe Falle wie `locked_courts`, roadmap.md):

```rust
    // Die Hallen-Anordnung wird auf der Felderübersicht gepflegt, nicht im
    // Assistenten — dessen Speichern darf sie nicht zurücksetzen.
    incoming.hall_layouts = current.hall_layouts.clone();
```

Test dazu (Muster der bestehenden `keep_host_managed_fields`-Tests):

```rust
#[test]
fn the_wizard_cannot_wipe_the_hall_layouts() {
    let mut current = AppConfig::default();
    current.hall_layouts.push(HallLayoutConfig {
        hall: "H1".into(), columns: 2, origin: LayoutOrigin::BottomLeft, serpentine: false,
    });
    let incoming = AppConfig::default(); // Wizard-Stand ohne Layouts
    let ergebnis = keep_host_managed_fields(incoming, &current);
    assert_eq!(ergebnis.hall_layouts, current.hall_layouts);
}
```

- [ ] **Step 5: Commands** (commands.rs, Muster `tl_device_add`/`mutate_config`):

```rust
/// Legt die Raster-Anordnung einer Halle fest (oder ersetzt sie).
#[tauri::command]
pub fn set_hall_layout(
    app: AppHandle,
    state: State<'_, AppState>,
    layout: crate::config::HallLayoutConfig,
) -> Result<AppConfig, String> {
    if layout.columns == 0 || layout.columns > 12 {
        return Err("Spaltenzahl muss zwischen 1 und 12 liegen.".into());
    }
    mutate_config(&app, &state, move |cfg| {
        cfg.hall_layouts.retain(|l| l.hall != layout.hall);
        cfg.hall_layouts.push(layout);
        Ok(())
    })
}

/// Entfernt die Anordnung einer Halle — zurück zur Fließ-Darstellung.
#[tauri::command]
pub fn remove_hall_layout(
    app: AppHandle,
    state: State<'_, AppState>,
    hall: String,
) -> Result<AppConfig, String> {
    mutate_config(&app, &state, move |cfg| {
        cfg.hall_layouts.retain(|l| l.hall != hall);
        Ok(())
    })
}
```

Beide in die `generate_handler![…]`-Liste eintragen (dort, wo `tl_device_add` steht). In `src/types.ts`: `LayoutOrigin`, `HallLayoutConfig`, `hall_layouts: HallLayoutConfig[]` an `AppConfig`; in `src/api.ts`: `setHallLayout`, `removeHallLayout` nach dem Muster der `tl*`-Wrapper.

- [ ] **Step 6: Tests + Commit:**

Run: `cargo test --workspace && npm run build`
Expected: PASS. Commit: `"Config: Hallen-Raster (hall_layouts) mit Wizard-Schutz und Commands"`.

### Task 10: Geteilte Mapping-Funktion `hallGrid.mjs` + Node-Test

**Files:**
- Create: `src/io/hallGrid.mjs`, `src/io/hallGrid.d.mts`, `scripts/test-hallgrid.mjs`
- Modify: `.github/workflows/ci.yml` (Node-Test-Schritt nach dem Muster „Satz-/Matchball (JS)“)

**Interfaces:**
- Produces: `gridPositions(count, {columns, origin, serpentine})` → `Array<{col, row}>` — **Bildschirm**-Koordinaten, 0-basiert, `row 0` = oberste Zeile, `col 0` = linke Spalte. Index i der Eingabe = i-tes Feld der Halle in BTP-Reihenfolge. Tasks 11+12 nutzen genau diese Semantik.

- [ ] **Step 1: Fehlschlagenden Test schreiben** (`scripts/test-hallgrid.mjs`, Muster `test-gamepoint.mjs` — nacktes `node:assert`):

```js
import assert from "node:assert/strict";
import { gridPositions } from "../src/io/hallGrid.mjs";

// 6 Felder, 3 Spalten, Start unten-links, ohne Schlange:
// Feld 1-3 unten (links→rechts), Feld 4-6 darüber.
assert.deepEqual(gridPositions(6, { columns: 3, origin: "bottom_left", serpentine: false }), [
  { col: 0, row: 1 }, { col: 1, row: 1 }, { col: 2, row: 1 },
  { col: 0, row: 0 }, { col: 1, row: 0 }, { col: 2, row: 0 },
]);

// Schlange: zweite Reihe läuft rückwärts (1-2-3 / 6-5-4 an der Wand).
assert.deepEqual(gridPositions(6, { columns: 3, origin: "bottom_left", serpentine: true }), [
  { col: 0, row: 1 }, { col: 1, row: 1 }, { col: 2, row: 1 },
  { col: 2, row: 0 }, { col: 1, row: 0 }, { col: 0, row: 0 },
]);

// Start unten-rechts: Feld 1 liegt rechts.
assert.deepEqual(gridPositions(4, { columns: 2, origin: "bottom_right", serpentine: false }), [
  { col: 1, row: 1 }, { col: 0, row: 1 },
  { col: 1, row: 0 }, { col: 0, row: 0 },
]);

// Teilreihe: 5 Felder in 3 Spalten — die angebrochene Reihe liegt oben,
// denn gezählt wird von der Start-Ecke aus.
assert.deepEqual(gridPositions(5, { columns: 3, origin: "top_left", serpentine: false }), [
  { col: 0, row: 0 }, { col: 1, row: 0 }, { col: 2, row: 0 },
  { col: 0, row: 1 }, { col: 1, row: 1 },
]);

console.log("hallGrid: alle Fälle bestanden");
```

- [ ] **Step 2: Laufen lassen** — `node scripts/test-hallgrid.mjs` → Fehler „Cannot find module“.

- [ ] **Step 3: Implementieren** (`src/io/hallGrid.mjs`):

```js
// Bildet die Feld-Reihenfolge einer Halle auf Raster-Zellen ab.
// Bildschirm-Koordinaten: row 0 = oben, col 0 = links. Die Start-Ecke
// beschreibt, wo Feld 1 aus Sicht der Turnierleitung liegt.
export function gridPositions(count, { columns, origin, serpentine }) {
  const cols = Math.max(1, columns | 0);
  const rows = Math.max(1, Math.ceil(count / cols));
  const fromBottom = origin === "bottom_left" || origin === "bottom_right";
  const fromRight = origin === "bottom_right" || origin === "top_right";
  const out = [];
  for (let i = 0; i < count; i++) {
    const r = Math.floor(i / cols);
    let c = i % cols;
    // Schlange: jede zweite Reihe läuft rückwärts — gezählt in
    // Nummerierungs-Reihen, nicht in Bildschirm-Reihen.
    if (serpentine && r % 2 === 1) c = cols - 1 - c;
    if (fromRight) c = cols - 1 - c;
    out.push({ col: c, row: fromBottom ? rows - 1 - r : r });
  }
  return out;
}
```

Typen in `src/io/hallGrid.d.mts` (Muster `gamePoint.d.mts`).

- [ ] **Step 4: Test grün** — `node scripts/test-hallgrid.mjs`. CI-Schritt in `ci.yml` ergänzen: `- name: Hallen-Raster (JS)` / `run: node scripts/test-hallgrid.mjs`.

- [ ] **Step 5: Commit** — `"Feld-Raster: geteilte Zellen-Abbildung mit Node-Test"`.

### Task 11: App-Felderübersicht — Raster + Layout-Editor

**Files:**
- Modify: `src/pages/FieldOverviewPage.tsx` (Hallen-Gruppen-Render; kleiner Editor je Hallen-Kopf), `src/api.ts`/`src/types.ts` (falls in Task 9 noch nicht geschehen)

**Interfaces:**
- Consumes: `gridPositions` (Task 10), `config.hall_layouts` + `setHallLayout`/`removeHallLayout` (Task 9). `FieldOverviewPage` erhält die Config bereits (vgl. `manageScorekeepers`, App.tsx:369) — den `AppConfig`-Zugang dort ablesen und `hall_layouts` auf demselben Weg hereinreichen.

- [ ] **Step 1: Raster rendern.** In der Hallen-Gruppe (dort, wo die Court-Karten je Halle gerendert werden — der Umsetzer sucht die `location`-Gruppierung in FieldOverviewPage.tsx):

```tsx
const layout = hallLayouts.find((l) => l.hall === hallName);
const pos = layout ? gridPositions(courtsOfHall.length, layout) : null;
// Mit Layout: CSS-Grid mit expliziten Zellen; ohne: bisherige Fließ-Liste.
<div
  className={pos ? "court-grid" : "court-flow"}
  style={pos ? { display: "grid", gridTemplateColumns: `repeat(${layout.columns}, minmax(0, 1fr))` } : undefined}
>
  {courtsOfHall.map((c, i) => (
    <div key={c.court_id}
         style={pos ? { gridColumn: pos[i].col + 1, gridRow: pos[i].row + 1 } : undefined}>
      {/* bestehende Court-Karte unverändert */}
    </div>
  ))}
</div>
```

- [ ] **Step 2: Editor je Hallen-Kopf.** Zahnrad-Knopf neben dem Hallen-Namen öffnet ein kleines Popover (Muster: bestehende Popover/Details der Seite):

```tsx
// Formularfelder: Spalten (number, 1–12), Start-Ecke (Select mit den vier
// Ecken, beschriftet „unten links" …), Schlangen-Nummerierung (Checkbox),
// Knöpfe „Übernehmen" (setHallLayout) und „Anordnung entfernen"
// (removeHallLayout). Beide Antworten sind die neue AppConfig — mit dem
// bestehenden onConfigSaved-/setConfig-Weg zurückspielen.
```

- [ ] **Step 3: Manuell prüfen:** Zwei-Hallen-Testdaten: Halle A 3×2 unten-links, Halle B ohne Layout → A als Raster (Feld 1 unten links), B wie bisher; App-Neustart behält das Layout (Config), Wizard-Speichern löscht es **nicht** (Task 9 Schutz).

- [ ] **Step 4: Suite + Commit** — `npm run build && cargo test --workspace`; Commit: `"Felderübersicht: Felder im Hallen-Raster, Editor je Halle"`.

### Task 12: TL-Web — Raster + Listen-Position je Gerät

**Files:**
- Modify: `src-tauri/src/tablet/tl.rs` (Layouts in den TlState), `src-tauri/assets/tl.html`

**Interfaces:**
- Consumes: `AppConfig.hall_layouts` (Task 9), Inline-Kopie von `gridPositions` (Task 10).
- Produces: `TlState.layouts: Vec<TlHallLayout{hall: String, columns: u8, origin: String, serpentine: bool}>` (`origin` als snake_case-String, exakt die vier Werte aus Task 9).

- [ ] **Step 1: Rust-Test** (tl.rs):

```rust
#[test]
fn the_state_carries_the_hall_layouts() {
    let mut config = test_config();
    config.hall_layouts.push(crate::config::HallLayoutConfig {
        hall: "Halle 1".into(), columns: 3,
        origin: crate::config::LayoutOrigin::BottomLeft, serpentine: false,
    });
    let state = build_state(&tablet_with_snapshot(), &config, 1_000, 1);
    assert_eq!(state.layouts.len(), 1);
    assert_eq!(state.layouts[0].origin, "bottom_left");
}
```

- [ ] **Step 2: Implementieren.** `TlHallLayout`-Struct in tl.rs (origin per `serde_json`-Serialisierung des Enums oder simplem `match` in den String); Befüllung in `build_state_limited` aus `config.hall_layouts` (auch im Leer-Zustand: leere Liste). Allowlist: `layouts`, `columns`, `origin`, `serpentine`, `hall` — Begründung „Raster-Anordnung, keine Personendaten“.

- [ ] **Step 3: tl.html — Raster.** Inline-Kopie von `gridPositions` (Herkunfts-Kommentar wie bei gamePoint, Task 5). Im Courts-Render (die Stelle, die `courtCard` je Halle aneinanderreiht): pro Halle das Layout aus `state.layouts` suchen; mit Layout die Kacheln in ein `display:grid`-Element mit `grid-column`/`grid-row` aus `gridPositions` setzen, ohne Layout wie bisher.

- [ ] **Step 4: tl.html — Listen-Position je Gerät.** Im Anzeige-Menü (tl.html:490):

```html
        <label><input type="radio" name="opt-liste" value="rechts" checked /> Spielliste rechts</label>
        <label><input type="radio" name="opt-liste" value="unten" /> Spielliste darunter</label>
```

JS nach dem Muster der bestehenden Optionen (`opt-nummern`): Wahl in `localStorage` (`bts-tl-liste`), Body-Klasse `liste-unten` schalten; CSS: `main` ist heute zweispaltig — mit `body.liste-unten main { grid-template-columns: 1fr; }` (bzw. Flex-Äquivalent, an der bestehenden `main`-Regel ablesen) stapeln sich Felder und Liste.

- [ ] **Step 5: Manuell prüfen:** TL-Web mit Layout aus Task 11 → gleiche Anordnung wie die App; Gerät A „rechts“, Gerät B „darunter“ — unabhängig voneinander, überlebt Neuladen.

- [ ] **Step 6: Doku + Suite + Commit + PR:**
  - `docs/turnierleitung-web.md`: Abschnitt „Anordnung wie in der Halle“ (Host-Einstellung, je Halle) + Anzeige-Menü-Punkt Listen-Position.
  - `docs/features/feld-raster.md`: die Mini-Spec dieses Pakets (Scope, Datenmodell, bewusst verschobenes Drag-&-Drop).
  - `docs/multi-hall.md`: Querverweis (Hallen-Gruppierung → Raster).
  - `docs/roadmap.md`: Drag-&-Drop-Anordnung als neuen offenen Punkt notieren.

Run: `cargo test --workspace && npm run build && node scripts/test-hallgrid.mjs`
Commit: `"TL-Web: Felder im Hallen-Raster, Spielliste wahlweise darunter"`.

---

## Nach allen Paketen (Release)

- [ ] Version gemeinsam bumpen (`src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `package.json`), `docs/changelog.md` je Feature-Zeile ergänzen.
- [ ] Tag `vX.Y.Z` pushen (Release-Workflow) **und** Relay-Deploy anstoßen (tl.html liegt doppelt — ohne Relay-Deploy sehen Cloud-Geräte die alte Seite). Achtung: badhub-/Relay-Deploy läuft nicht von diesem Rechner (Memory `badhub-deploy-zugang`) — den Kollegen einplanen.
- [ ] `docs/roadmap.md`: die vier TL-Web-Punkte als erledigt markieren; „Umgesetzt, aber noch nicht abgenommen“-Liste aktualisieren (manuelle Geräte-Abnahme bleibt offen).

## Self-Review-Notizen

- Alle vier gewählten Roadmap-Punkte + Raster-Feature sind durch Tasks abgedeckt; Internet-Erreichbarkeit ist bewusst nur Doku (WP1 Task 2 Step 4).
- Typkonsistenz geprüft: `TlScorekeeper`/`TlFinished`/`TlHallLayout`-Feldnamen stimmen zwischen Rust-Tasks und tl.html-Konsumenten überein; `gridPositions`-Signatur identisch in Task 10/11/12.
- Bewusste Abhängigkeiten: Task 8 braucht Task 4 (Beendet-Liste) und Task 7 (Dialog); Task 11/12 brauchen Task 9+10. WP1–WP3 sind untereinander unabhängig.
