# Mehrere Liveticker je Verband — Umsetzungsplan (bts-light-Seite)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** bts-light macht die turnier.de-GUID zum Pflichtfeld, schickt sie in jeder badhub-Nachricht mit und verlinkt Aushang und Dashboard direkt auf das eigene Turnier — damit badhub (Kind-Turniere, ADR 0054) zwei Installationen desselben Verbands auseinanderhält.

**Architecture:** Die GUID wandert von `checkin.tournament_uuid` in ein neues Wurzelfeld `AppConfig.tournament_uuid`; der Lader übernimmt den alten Wert einmalig und **spiegelt** das Wurzelfeld danach immer in den Check-In-Block, sodass die sechs bestehenden Leser dort unverändert bleiben und ein Rückrollen funktioniert. Die vier Push-Builder lesen die GUID aus der Config (`tset`/`sched` über `LivetickerContext`, `tupdate` als Parameter, Branding beim Bau). Der Start verweigert ohne gültige GUID. Links bekommen `&g=<GUID>` über eine reine Hilfsfunktion in `aushang.rs`.

**Tech Stack:** Rust (Tauri 2, serde), React 19 + TypeScript (Setup-Wizard), Tests mit `cargo test`, `npm run build`.

**Spec:** `docs/features/liveticker-mehrere-turniere-je-verband.md` (Abschnitte „bts-light", „Verhalten im Detail › bts-light", Akzeptanzkriterien 11–12). ADR 0054. Der badhub-Teil ist ein eigener Plan im badhub-Repo (`docs/superpowers/plans/2026-09-04-liveticker-kind-turniere.md`) und muss **vorher deployt** sein — ein neuer Client gegen altes badhub schadet nicht, aber die Direktlinks liefen erst nach dem Deploy.

## Global Constraints

- **GUID kanonisch:** Großschreibung, ohne `{}`/Leerraum, Form `8-4-4-4-12` (`config::is_tournament_uuid` prüft, `tournamentGuid.ts::extractTournamentGuid` normalisiert im Frontend). In den Push geht **immer** die kanonische Form.
- **Feldname im Wire:** `tournament_uuid` (wie `centry_list` heute). Bei `tset`/`sched` im `event`-Block, bei `tupdate_match`/`checkin-branding` auf oberster Ebene.
- **Pflicht:** kein Sync-Start ohne gültige GUID, **außer** im Ansage-Slave-Modus (`slave_mode`, pusht nie nach badhub). Fehlertext: `Die Turnier-Kennung von turnier.de fehlt — im Setup unter „1 · Liveticker-Ziel" die Adresse deines Turniers einfügen.`
- **Spiegel:** Nach `load_from` gilt immer `checkin.tournament_uuid == tournament_uuid`. Das Frontend schreibt beide Felder aus einem Eingabefeld.
- **`install_id` bleibt draußen** aus Push und URLs (R6, ADR 0006).
- **Version:** `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `package.json` gemeinsam auf **0.9.274** (erst beim Merge festlegen — siehe Memory „Versionsnummer erst beim Merge"; im PR-Titel keine Version nennen).
- Rust-Kommentare Deutsch (was + warum). `cargo test --workspace` und `cargo clippy --workspace --all-targets -- -D warnings` grün; `cargo fmt`; `npm run build` fehlerfrei. Nach jeder Code-Änderung `code-reviewer`-Subagent (CLAUDE.md).

---

### Task 1: `AppConfig.tournament_uuid` + Lader-Migration und Spiegel

**Files:**
- Modify: `src-tauri/src/config.rs` — `AppConfig` (Feld hinter `install_id`), `load_from` (Zeile ~1257), Testmodul (hinter `config_without_announce_key_loads_with_defaults`)

**Interfaces:**
- Produces: `AppConfig.tournament_uuid: String` (serde default, kanonisch nach Laden), `AppConfig::tournament_uuid_kanonisch(&self) -> Option<String>` (getrimmt, ohne Klammern, groß; `None` wenn ungültig), `AppConfig::spiegele_turnier_guid(&mut self)` (Migration + Spiegel, wird von `load_from` gerufen).

- [ ] **Step 1: Failing Tests schreiben** (im `mod tests` von `config.rs`)

```rust
    #[test]
    fn turnier_guid_wird_aus_dem_checkin_block_uebernommen() {
        // Konfiguration einer Version vor diesem Feature: die GUID steht nur
        // im Check-In-Block. Sie muss beim Laden ins Wurzelfeld wandern —
        // sonst stünde eine Installation nach dem Auto-Update ohne GUID da
        // und dürfte nicht mehr starten.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"btp":{"host":"127.0.0.1","port":9901,"password":null},
                "badhub":{"url":"u","password":"p","live_url":""},
                "checkin":{"enabled":true,"tournament_uuid":"{0ea5fd86-a64f-4445-a8de-bae3dbf762ba}","missing_names_max":8}}"#,
        )
        .unwrap();
        let loaded = AppConfig::load_from(&path).unwrap();
        assert_eq!(loaded.tournament_uuid, "0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA");
        // Spiegel: der Check-In-Block trägt danach dieselbe kanonische Form.
        assert_eq!(loaded.checkin.tournament_uuid, "0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA");
        assert!(loaded.checkin.is_ready());
    }

    #[test]
    fn wurzelfeld_gewinnt_und_spiegelt_in_den_checkin_block() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"btp":{"host":"127.0.0.1","port":9901,"password":null},
                "badhub":{"url":"u","password":"p","live_url":""},
                "tournament_uuid":"11111111-2222-3333-4444-555555555555",
                "checkin":{"enabled":false,"tournament_uuid":"0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA","missing_names_max":8}}"#,
        )
        .unwrap();
        let loaded = AppConfig::load_from(&path).unwrap();
        assert_eq!(loaded.tournament_uuid, "11111111-2222-3333-4444-555555555555");
        assert_eq!(loaded.checkin.tournament_uuid, "11111111-2222-3333-4444-555555555555");
    }

    #[test]
    fn ohne_guid_bleibt_alles_leer_und_kanonisch_ist_none() {
        let cfg = AppConfig::default();
        assert!(cfg.tournament_uuid.is_empty());
        assert_eq!(cfg.tournament_uuid_kanonisch(), None);

        let mut kaputt = AppConfig::default();
        kaputt.tournament_uuid = "nicht-gueltig".to_string();
        assert_eq!(kaputt.tournament_uuid_kanonisch(), None);

        let mut ok = AppConfig::default();
        ok.tournament_uuid = "  {0ea5fd86-a64f-4445-a8de-bae3dbf762ba} ".to_string();
        assert_eq!(
            ok.tournament_uuid_kanonisch().as_deref(),
            Some("0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA")
        );
    }

    #[test]
    fn roundtrip_behaelt_das_wurzelfeld() {
        let mut cfg = AppConfig::default();
        cfg.tournament_uuid = "0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA".to_string();
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains(r#""tournament_uuid":"0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA""#));
        let back: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tournament_uuid, cfg.tournament_uuid);
    }
```

- [ ] **Step 2: Tests laufen lassen, müssen rot sein**

Run: `cd src-tauri && cargo test --lib config::tests::turnier_guid -- --nocapture` (und die drei anderen Namen)
Expected: Compile-Fehler `no field tournament_uuid on type AppConfig`.

- [ ] **Step 3: Feld + Methoden einbauen**

In `AppConfig` direkt hinter `install_id`:

```rust
    /// turnier.de-Turnier-GUID (kanonisch: `8-4-4-4-12`, Großschreibung).
    /// **Pflicht** für den Liveticker-Push: badhub führt jedes Turnier eines
    /// Verbandszugangs als eigenes Kind-Turnier unter dieser GUID (ADR 0054),
    /// sonst überschrieben sich zwei parallele Turniere gegenseitig.
    ///
    /// Ursprünglich lebte der Wert nur im Check-In-Block. Der Lader übernimmt
    /// ihn von dort einmalig und **spiegelt** dieses Feld danach immer nach
    /// `checkin.tournament_uuid` zurück — so bleiben die Check-In-Leser
    /// unverändert, und eine ältere Version nach einem Rückrollen findet ihre
    /// GUID an der gewohnten Stelle. `#[serde(default)]` hält ältere
    /// Konfigurationsdateien lesbar.
    #[serde(default)]
    pub tournament_uuid: String,
```

Als `impl AppConfig`-Methoden (neben `load_from`):

```rust
    /// Die Turnier-GUID in kanonischer Form — oder `None`, wenn keine
    /// gültige eingetragen ist. Einzige Quelle für Push und Links.
    pub fn tournament_uuid_kanonisch(&self) -> Option<String> {
        kanonische_guid(&self.tournament_uuid)
    }

    /// Migration + Spiegel der Turnier-GUID (siehe Feld-Kommentar). Wird
    /// beim Laden gerufen; idempotent.
    pub fn spiegele_turnier_guid(&mut self) {
        if kanonische_guid(&self.tournament_uuid).is_none() {
            if let Some(alt) = kanonische_guid(&self.checkin.tournament_uuid) {
                self.tournament_uuid = alt;
            }
        }
        if let Some(k) = kanonische_guid(&self.tournament_uuid) {
            self.tournament_uuid = k.clone();
            self.checkin.tournament_uuid = k;
        }
    }
```

Freie Funktion neben `is_tournament_uuid`:

```rust
/// Kanonische Form einer Turnier-GUID: getrimmt, ohne BTPs `{…}`,
/// Großschreibung. `None`, wenn [`is_tournament_uuid`] sie ablehnt. Muss zur
/// Normalisierung in badhub (`liveticker_guid_normalisieren`) passen — beide
/// Seiten leiten daraus denselben Kindschlüssel ab.
pub fn kanonische_guid(value: &str) -> Option<String> {
    if !is_tournament_uuid(value) {
        return None;
    }
    Some(
        value
            .trim()
            .trim_start_matches('{')
            .trim_end_matches('}')
            .to_ascii_uppercase(),
    )
}
```

In `load_from`, direkt vor `crate::badhub_host::set_aus_push_url(&cfg.badhub.url);`:

```rust
                // Turnier-GUID: altes Check-In-Feld übernehmen, Wurzelfeld
                // spiegeln (ADR 0054). Vor dem Push-URL-Schalter, damit ein
                // Fehler hier nicht den Testsystem-Schalter überspringt.
                cfg.spiegele_turnier_guid();
```

- [ ] **Step 4: Tests laufen lassen, müssen grün sein**

Run: `cd src-tauri && cargo test --lib config::` — Expected: alle grün, auch die bestehenden Default-Tests (`loaded.checkin.tournament_uuid.is_empty()` bleibt wahr, weil nichts zu spiegeln ist).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/config.rs
git commit -m "feat(config): Turnier-GUID als Wurzelfeld mit Migration aus dem Check-In-Block (ADR 0054)"
```

---

### Task 2: GUID in `tset`, `sched`, `tupdate_match`, `checkin-branding`

**Files:**
- Modify: `src-tauri/src/badhub/payload.rs` — `TsetEvent` (~Zeile 100), `SchedEvent` (~364), `TupdateMessage` (~611), `build_tupdate` (~627), `CheckinBrandingMessage` (~675), `build_tset` (~561), `build_sched` (~446), Tests
- Modify: `src-tauri/src/badhub/diff.rs:68` (Aufruf `build_tupdate`)
- Modify: `src-tauri/src/tablet/server.rs:4088` und `:4724` (Aufrufe `build_tupdate`)
- Modify: `src-tauri/src/commands.rs` — `spawn_branding_push`-Aufrufer `push_bar_sponsors_to_badhub` / `push_logo_to_badhub` (~4150–4200)

**Interfaces:**
- Consumes: `AppConfig::tournament_uuid_kanonisch()` (Task 1); `LivetickerContext.config`.
- Produces: `TsetEvent.tournament_uuid: Option<String>`, `SchedEvent.tournament_uuid: Option<String>`, `TupdateMessage.tournament_uuid: Option<String>`, `CheckinBrandingMessage.tournament_uuid: Option<String>` — alle `#[serde(skip_serializing_if = "Option::is_none")]`; `build_tupdate(m: &BtpMatch, rid: u64, tournament_uuid: Option<String>) -> TupdateMessage`.

- [ ] **Step 1: Failing Tests** (im `mod tests` von `payload.rs`, neben `tset_matches_and_courts_cover_on_court_matches`)

```rust
    fn config_mit_guid() -> AppConfig {
        let mut cfg = AppConfig::default();
        cfg.tournament_uuid = "0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA".to_string();
        cfg
    }

    fn leerer_snapshot() -> BtpSnapshot {
        BtpSnapshot {
            tournament_name: "Test-Turnier".to_string(),
            rest_minutes: None,
            courts: Vec::new(),
            locations: Vec::new(),
            court_infos: Vec::new(),
            events: Vec::new(),
            entries: Vec::new(),
            officials: Vec::new(),
            matches: vec![sample_match(1, MatchStatus::OnCourt, Some("Feld 9"))],
        }
    }

    #[test]
    fn tset_und_sched_tragen_die_turnier_guid_im_event_block() {
        let cfg = config_mit_guid();
        let snapshot = leerer_snapshot();
        let tset = build_tset(&snapshot, 1, &LivetickerContext::bare(&cfg));
        let json = serde_json::to_string(&tset).unwrap();
        assert!(json.contains(r#""tournament_uuid":"0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA""#));
        assert_eq!(
            tset.event.tournament_uuid.as_deref(),
            Some("0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA")
        );

        let sched = build_sched(&snapshot, &LivetickerContext::bare(&cfg), &HashMap::new(), 2);
        assert_eq!(
            sched.event.tournament_uuid.as_deref(),
            Some("0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA")
        );
    }

    #[test]
    fn ohne_guid_fehlt_das_feld_im_wire() {
        // Kein leeres Feld senden: badhub behandelt „fehlt" und „ungültig"
        // gleich (Elternschlüssel), aber ein leerer String im Log verwirrt.
        let tset = build_tset(&leerer_snapshot(), 1, &LivetickerContext::bare(&AppConfig::default()));
        let json = serde_json::to_string(&tset).unwrap();
        assert!(!json.contains("tournament_uuid"));

        let up = build_tupdate(&sample_match(1, MatchStatus::OnCourt, Some("Feld 9")), 3, None);
        assert!(!serde_json::to_string(&up).unwrap().contains("tournament_uuid"));
    }

    #[test]
    fn tupdate_traegt_die_guid_auf_oberster_ebene() {
        let up = build_tupdate(
            &sample_match(1, MatchStatus::OnCourt, Some("Feld 9")),
            3,
            Some("0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA".to_string()),
        );
        let json = serde_json::to_string(&up).unwrap();
        assert!(json.contains(r#""type":"tupdate_match""#));
        assert!(json.contains(r#""tournament_uuid":"0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA""#));
        // Auf oberster Ebene, nicht im match-Block.
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("tournament_uuid").is_some());
        assert!(v["match"].get("tournament_uuid").is_none());
    }

    #[test]
    fn branding_nachricht_traegt_die_guid() {
        let msg = CheckinBrandingMessage {
            sponsors: None,
            logo: Some(String::new()),
            tournament_uuid: Some("0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""tournament_uuid":"0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA""#));
        assert!(!json.contains("sponsors"));
    }
```

- [ ] **Step 2: Tests laufen lassen, müssen rot sein**

Run: `cd src-tauri && cargo test --lib badhub::payload::tests` — Expected: Compile-Fehler (fehlende Felder / falsche Argumentzahl).

- [ ] **Step 3: Strukturen und Builder ändern**

`TsetEvent` (hinter `tournament_name`):

```rust
    /// turnier.de-GUID des Turniers (kanonisch). badhub ordnet den Push
    /// damit dem Kind-Turnier unter dem Verbandszugang zu (ADR 0054).
    /// Fehlt bei alter Konfiguration ohne GUID — dann landet der Push wie
    /// früher beim Verbandsschlüssel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tournament_uuid: Option<String>,
```

Dasselbe Feld mit demselben Kommentar in `SchedEvent` (hinter `tournament_name`), in `TupdateMessage` (hinter `rid`) und in `CheckinBrandingMessage` (hinter `logo`).

In `build_tset` beim Bau des `TsetEvent`: `tournament_uuid: ctx.config.tournament_uuid_kanonisch(),` — **an allen Stellen**, an denen `TsetEvent { … }` gebaut wird (auch die Default-/Test-Konstruktion um Zeile 592–602: dort `tournament_uuid: None,`).

In `build_sched` beim Bau des `SchedEvent`: `tournament_uuid: ctx.config.tournament_uuid_kanonisch(),`.

`build_tupdate`:

```rust
/// Baut eine `tupdate_match`-Nachricht für ein Match mit geändertem Score.
/// `tournament_uuid` ist die kanonische Turnier-GUID aus der Config
/// (`AppConfig::tournament_uuid_kanonisch`) — jedes Punkt-Update trägt sie,
/// weil badhub jede Nachricht einzeln dem Kind-Turnier zuordnet.
pub fn build_tupdate(m: &BtpMatch, rid: u64, tournament_uuid: Option<String>) -> TupdateMessage {
    TupdateMessage {
        kind: "tupdate_match",
        match_update: TupdateMatch {
            id: match_id(m.id),
            s: m.sets.iter().map(|&(a, b)| [a, b]).collect(),
        },
        rid,
        tournament_uuid,
    }
}
```

- [ ] **Step 4: Aufrufer anpassen**

`diff.rs:68`: `[m] => Update::Single(build_tupdate(m, rid, ctx.config.tournament_uuid_kanonisch())),`

`tablet/server.rs:4088`: `let update = Update::Single(build_tupdate(&live, ctx.next_rid(), ctx.config.tournament_uuid_kanonisch()));` (`ctx.config` ist die `AppConfig` des Tablet-Server-Kontexts, Zeile ~251; sie wird beim Start gesetzt — und der Sync startet nach jedem Speichern neu, siehe Memory „Sync-Neustart bei jedem Speichern").

`tablet/server.rs:4724` (Test): `build_tupdate(&match_on_court(), n as u64, None)`.

`commands.rs` `push_bar_sponsors_to_badhub` und `push_logo_to_badhub`: beim Lesen aus `cfg` zusätzlich `cfg.tournament_uuid_kanonisch()` holen und in der `CheckinBrandingMessage` setzen:

```rust
        crate::badhub::payload::CheckinBrandingMessage {
            sponsors: Some(sponsors),
            logo: None,
            tournament_uuid,
        },
```

(analog beim Logo mit `sponsors: None, logo: Some(logo)`). Andere `CheckinBrandingMessage { … }`-Konstruktionen (Tests in `push.rs`) bekommen `tournament_uuid: None`.

- [ ] **Step 5: Tests laufen lassen, müssen grün sein**

Run: `cd src-tauri && cargo test --workspace` — Expected: grün. `cargo clippy --workspace --all-targets -- -D warnings` — Expected: keine Warnung.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/badhub/payload.rs src-tauri/src/badhub/diff.rs src-tauri/src/tablet/server.rs src-tauri/src/commands.rs src-tauri/src/badhub/push.rs
git commit -m "feat(badhub): Turnier-GUID in tset, sched, tupdate_match und Branding-Nachricht"
```

---

### Task 3: Startsperre ohne GUID

**Files:**
- Modify: `src-tauri/src/commands.rs` — `start_sync` (~Zeile 935–955) + neue reine Funktion `pruefe_startbedingungen`, Testmodul von `commands.rs` (falls keines existiert: `#[cfg(test)] mod tests` am Dateiende anlegen)

**Interfaces:**
- Consumes: `AppConfig::tournament_uuid_kanonisch()`.
- Produces: `pub(crate) fn pruefe_startbedingungen(config: &AppConfig) -> Result<(), String>` — bündelt Badhub-Passwort, Cloud-`install_id` und GUID-Pflicht.

- [ ] **Step 1: Failing Tests**

```rust
#[cfg(test)]
mod startbedingungen_tests {
    use super::pruefe_startbedingungen;
    use crate::config::{AppConfig, ConnectionMode};

    fn basis() -> AppConfig {
        let mut cfg = AppConfig::default();
        cfg.badhub.password = "pw".to_string();
        cfg.install_id = "inst".to_string();
        cfg.connection_mode = ConnectionMode::Lan;
        cfg.tournament_uuid = "0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA".to_string();
        cfg
    }

    #[test]
    fn vollstaendige_config_darf_starten() {
        assert_eq!(pruefe_startbedingungen(&basis()), Ok(()));
    }

    #[test]
    fn ohne_guid_kein_start() {
        let mut cfg = basis();
        cfg.tournament_uuid = String::new();
        let err = pruefe_startbedingungen(&cfg).unwrap_err();
        assert!(err.contains("Turnier-Kennung"), "{err}");
        assert!(err.contains("1 · Liveticker-Ziel"), "{err}");
    }

    #[test]
    fn kaputte_guid_kein_start() {
        let mut cfg = basis();
        cfg.tournament_uuid = "0EA5FD86-A64F".to_string();
        assert!(pruefe_startbedingungen(&cfg).is_err());
    }

    #[test]
    fn slave_braucht_keine_guid_und_kein_passwort() {
        let mut cfg = AppConfig::default();
        cfg.slave_mode = true;
        assert_eq!(pruefe_startbedingungen(&cfg), Ok(()));
    }

    #[test]
    fn passwort_fehlt_wird_vor_der_guid_gemeldet() {
        // Reihenfolge wie bisher: erst der Badhub-Zugang, dann alles Weitere —
        // wer kein Passwort hat, soll nicht zuerst nach der GUID suchen.
        let mut cfg = basis();
        cfg.badhub.password = String::new();
        cfg.tournament_uuid = String::new();
        assert!(pruefe_startbedingungen(&cfg).unwrap_err().contains("Badhub-Passwort"));
    }
}
```

- [ ] **Step 2: Tests laufen lassen, müssen rot sein**

Run: `cd src-tauri && cargo test --lib commands::startbedingungen_tests` — Expected: `cannot find function pruefe_startbedingungen`.

- [ ] **Step 3: Funktion einführen und in `start_sync` verwenden**

Direkt über `pub fn start_sync`:

```rust
/// Was vor dem Start der Übertragung stimmen muss. Reine Funktion, damit die
/// Regeln testbar sind — `start_sync` selbst hängt an Tauri.
///
/// Ein Ansage-Slave pusht nie nach badhub und braucht nichts davon. Sonst:
/// Badhub-Passwort, im Cloud-Modus die Installations-ID, und seit ADR 0054
/// die turnier.de-GUID — ohne sie könnte badhub das Turnier nicht von einem
/// parallel laufenden desselben Verbands unterscheiden.
pub(crate) fn pruefe_startbedingungen(config: &AppConfig) -> Result<(), String> {
    if config.slave_mode {
        return Ok(());
    }
    if config.badhub.password.is_empty() {
        return Err("Es ist kein Badhub-Passwort konfiguriert.".to_string());
    }
    if config.connection_mode.cloud_enabled() && config.install_id.is_empty() {
        return Err("Für den Cloud-Modus fehlt die Installations-ID.".to_string());
    }
    if config.tournament_uuid_kanonisch().is_none() {
        return Err(
            "Die Turnier-Kennung von turnier.de fehlt — im Setup unter „1 · Liveticker-Ziel“ \
             die Adresse deines Turniers einfügen."
                .to_string(),
        );
    }
    Ok(())
}
```

In `start_sync` den Block `if !config.slave_mode { … }` (Passwort + install_id) ersetzen durch `pruefe_startbedingungen(&config)?;`.

- [ ] **Step 4: Tests laufen lassen, müssen grün sein**

Run: `cd src-tauri && cargo test --lib commands::` — Expected: grün.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat(sync): kein Start ohne Turnier-GUID (ADR 0054), Startregeln als reine Funktion"
```

---

### Task 4: Direktlinks — Aushang-QR und Dashboard mit `&g=<GUID>`

**Files:**
- Modify: `src-tauri/src/aushang.rs` — neue Funktion `link_mit_guid`, `daten_aus` (Signatur + `url_ticker`), Tests
- Modify: `src-tauri/src/commands.rs` — `open_live_view` (~1703–1717), `aushang_html` (~2612–2625)

**Interfaces:**
- Produces: `pub fn link_mit_guid(url: &str, guid: Option<&str>) -> String` — hängt `g=<GUID>` mit `?` oder `&` an; ohne GUID unverändert.
- Changes: `pub fn daten_aus(live_url: &str, turnier: &str, logo: Option<String>, guid: Option<&str>) -> Option<AushangDaten>`.

- [ ] **Step 1: Failing Tests** (im `mod tests` von `aushang.rs`)

```rust
    #[test]
    fn link_mit_guid_haengt_g_korrekt_an() {
        let g = Some("0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA");
        assert_eq!(
            link_mit_guid("https://badhub.de/live?t=bvbb", g),
            "https://badhub.de/live?t=bvbb&g=0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA"
        );
        assert_eq!(
            link_mit_guid("https://badhub.de/live", g),
            "https://badhub.de/live?g=0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA"
        );
        assert_eq!(link_mit_guid("https://badhub.de/live?t=bvbb", None), "https://badhub.de/live?t=bvbb");
        // Leere Adresse bleibt leer — der Aufrufer meldet „keine Live-Seite".
        assert_eq!(link_mit_guid("", g), "");
    }

    #[test]
    fn aushang_ticker_zeigt_direkt_aufs_turnier() {
        let g = Some("0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA");
        let d = daten_aus("https://badhub.de/live?t=bvbb&display=monitor", "Test", None, g).unwrap();
        assert_eq!(
            d.url_ticker,
            "https://badhub.de/live?t=bvbb&g=0EA5FD86-A64F-4445-A8DE-BAE3DBF762BA"
        );
        // Die Teilnehmerliste hängt am Verbandsschlüssel — badhub löst dort
        // über das zuletzt bepushte Turnier auf; unverändert.
        assert_eq!(d.url_teilnehmer, "https://badhub.de/live/bvbb/teilnehmer");
        // Ohne GUID wie bisher.
        let ohne = daten_aus("https://badhub.de/live?t=bvbb", "Test", None, None).unwrap();
        assert_eq!(ohne.url_ticker, "https://badhub.de/live?t=bvbb");
    }
```

- [ ] **Step 2: Tests laufen lassen, müssen rot sein**

Run: `cd src-tauri && cargo test --lib aushang::` — Expected: Compile-Fehler.

- [ ] **Step 3: Implementieren**

In `aushang.rs` (vor `daten_aus`):

```rust
/// Hängt die Turnier-GUID als `g=` an eine Live-Adresse (ADR 0054): Der
/// Verbandsschlüssel `t` führt bei mehreren laufenden Turnieren auf eine
/// Auswahl, `g` direkt auf dieses Turnier. Ohne GUID bleibt die Adresse,
/// wie sie ist — badhub zeigt dann wie früher den Verbandsschlüssel.
pub fn link_mit_guid(url: &str, guid: Option<&str>) -> String {
    let url = url.trim();
    match guid {
        Some(g) if !url.is_empty() => {
            let trenner = if url.contains('?') { '&' } else { '?' };
            format!("{url}{trenner}g={g}")
        }
        _ => url.to_string(),
    }
}
```

`daten_aus`:

```rust
pub fn daten_aus(
    live_url: &str,
    turnier: &str,
    logo: Option<String>,
    guid: Option<&str>,
) -> Option<AushangDaten> {
    let kuerzel = kuerzel_aus_live_url(live_url)?;
    let basis = basis_aus_live_url(live_url)?;
    // … (bestehender Kommentar) …
    Some(AushangDaten {
        turnier: turnier.trim().to_string(),
        logo,
        url_ticker: link_mit_guid(&format!("{basis}/live?t={kuerzel}"), guid),
        url_teilnehmer: format!("{basis}/live/{kuerzel}/teilnehmer"),
    })
}
```

Alle bestehenden `daten_aus(…)`-Aufrufe in den Tests um `, None` ergänzen (`beide_adressen_werden_neu_gebaut`, `teilnehmerliste_bleibt_auf_derselben_installation`, `blatt_traegt_beide_adressen_und_zwei_qr_codes`, `turniername_und_logo_landen_escaped_im_kopf`, `ohne_turniername_und_logo_bleibt_der_kopf_schlicht` — je nachdem, welche `daten_aus` rufen; `cargo test` nennt sie).

`commands.rs` `aushang_html`: `let guid = config.tournament_uuid_kanonisch();` vor dem Aufruf, dann `crate::aushang::daten_aus(&config.badhub.live_url, &turnier, logo, guid.as_deref())`.

`commands.rs` `open_live_view`: nach dem Klonen von `live_url` auch `let guid = cfg.tournament_uuid_kanonisch();` aus derselben Sperre holen (den `state.config.lock()`-Block dafür in eine `let (live_url, guid) = { let cfg = …; (cfg.badhub.live_url.clone(), cfg.tournament_uuid_kanonisch()) };`-Form bringen), dann:

```rust
    // Erst die GUID (Direktlink aufs Turnier, ADR 0054), dann die Ansicht.
    let mit_guid = crate::aushang::link_mit_guid(&live_url, guid.as_deref());
    let url = match display {
        Some(view) => format!("{mit_guid}&display={view}"),
        None => mit_guid,
    };
```

- [ ] **Step 4: Tests laufen lassen, müssen grün sein**

Run: `cd src-tauri && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings` — Expected: grün.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/aushang.rs src-tauri/src/commands.rs
git commit -m "feat(links): Aushang-QR und Dashboard-Link zeigen mit &g=<GUID> direkt aufs Turnier"
```

---

### Task 5: Setup-Wizard — GUID-Pflichtfeld im Abschnitt „1 · Liveticker-Ziel"

**Files:**
- Modify: `src/types.ts` (~Zeile 615, `AppConfig`)
- Modify: `src/pages/SetupWizard.tsx` — State (~262), `buildConfig` (~528–645), `canSave` (~745), Abschnitt 1 (~991–1068), Check-In-Abschnitt (~1375–1425)

**Interfaces:**
- Consumes: `AppConfig.tournament_uuid` (Task 1), `isTournamentGuid`/`extractTournamentGuid` aus `src/tournamentGuid.ts`.

- [ ] **Step 1: Typ ergänzen** — in `src/types.ts` `AppConfig` hinter `install_id`:

```ts
  /** turnier.de-Turnier-GUID (kanonisch, Pflicht für den Liveticker-Push;
   *  Rust: AppConfig.tournament_uuid, ADR 0054). Wird in `checkin.tournament_uuid`
   *  gespiegelt — beide aus EINEM Eingabefeld. */
  tournament_uuid: string;
```

- [ ] **Step 2: State im Wizard** — hinter `badhubLiveUrl`:

```tsx
  // Turnier-GUID von turnier.de (Pflicht, ADR 0054). Vor diesem Feature lebte
  // sie nur im Check-In-Block; der Rust-Lader hat sie bereits nach oben
  // gespiegelt, hier gilt das Wurzelfeld mit Rückfall auf den alten Platz.
  const [tournamentGuid, setTournamentGuid] = useState(
    initialConfig.tournament_uuid || initialConfig.checkin?.tournament_uuid || "",
  );
```

Die Zeile `const [ciUuid, setCiUuid] = useState(ci?.tournament_uuid ?? "");` **entfernen**; jede Verwendung von `ciUuid` durch `tournamentGuid` ersetzen (Zeilen ~640–641, ~1399, ~1417).

- [ ] **Step 3: `buildConfig`** — hinter `install_id: initialConfig.install_id,`:

```tsx
      tournament_uuid: extractTournamentGuid(tournamentGuid),
```

und im `checkin`-Block: `enabled: ciEnabled && isTournamentGuid(tournamentGuid), tournament_uuid: extractTournamentGuid(tournamentGuid),`.

- [ ] **Step 4: `canSave`** erweitern:

```tsx
  const canSave =
    host.trim() !== "" &&
    (!isManual || (badhubUrl.trim() !== "" && badhubPassword.trim() !== "")) &&
    // Ohne Turnier-GUID kann badhub das Turnier nicht von einem parallelen
    // desselben Verbands unterscheiden (ADR 0054). Ein Ansage-Slave pusht nie.
    (slaveMode || isTournamentGuid(tournamentGuid)) &&
    (lanEnabled || cloudEnabled);
```

- [ ] **Step 5: Feld im Abschnitt 1 einfügen** — direkt nach der `PRESETS.map(...)`-Liste und vor dem manuellen Block (`isManual && …`), als eigenes `<label>` im Stil des bisherigen Check-In-Feldes:

```tsx
        <label className="mt-1 flex flex-col gap-1 text-sm text-slate-600">
          <span>
            Turnier bei turnier.de <span className="text-rose-600">*</span>
          </span>
          <input
            className="rounded-lg border border-slate-300 px-3 py-2 font-mono text-xs"
            placeholder="turnier.de-Adresse einfügen oder Turnier-GUID"
            value={tournamentGuid}
            onChange={(e) => {
              // Aus einer eingefügten Adresse die GUID herausziehen — kopiert
              // wird fast immer die ganze URL aus dem Browser. Nur dann
              // ersetzen: sonst würde jedes Tippen mitten in einer schon
              // gültigen Kennung den Feldinhalt neu setzen.
              const raw = e.currentTarget.value;
              const looksLikeUrl = /[/:]/.test(raw);
              const found = looksLikeUrl ? extractTournamentGuid(raw) : "";
              setTournamentGuid(found || raw);
            }}
          />
          <span className="text-xs text-slate-500">
            Öffne dein Turnier auf turnier.de und füge die Adresse hier ein —
            die Kennung wird automatisch herausgelesen. badhub hält damit dein
            Turnier von anderen des Verbands auseinander, die am selben Tag
            laufen; Aushang und Liveticker-Link zeigen direkt auf dein Turnier.
          </span>
          {tournamentGuid.trim() !== "" && !isTournamentGuid(tournamentGuid) && (
            <span className="text-xs font-medium text-amber-600">
              Das sieht noch nicht nach einer Turnier-Kennung aus. Erwartet
              wird die Adresse deines Turniers bei turnier.de.
            </span>
          )}
          {tournamentGuid.trim() === "" && !slaveMode && (
            <span className="text-xs font-medium text-amber-600">
              Pflichtfeld — ohne Kennung lässt sich die Übertragung nicht
              starten.
            </span>
          )}
        </label>
```

- [ ] **Step 6: Check-In-Abschnitt entschlacken** — das `<label>` „Turnier bei turnier.de" samt Hinweisen im `ciEnabled &&`-Block entfernen; stattdessen ein Satz: `<p className="text-xs text-slate-500">Die Turnier-Kennung kommt aus Abschnitt „1 · Liveticker-Ziel".</p>`. Das Feld „Namen in der ‚Es fehlen noch'-Ansage" bleibt.

- [ ] **Step 7: Bauen und manuell prüfen**

Run: `npm run build` — Expected: fehlerfrei (`tsc` meldet jede vergessene `ciUuid`-Stelle).
Manuell (`npx tauri build --debug --no-bundle`, siehe Memory „App lokal starten"): Setup ohne GUID → Speichern gesperrt mit Hinweis; turnier.de-URL einfügen → GUID erscheint, Speichern frei; Check-In-Abschnitt zeigt kein GUID-Feld mehr; gespeicherte `config.json` trägt `tournament_uuid` **und** `checkin.tournament_uuid` identisch.

- [ ] **Step 8: Commit**

```bash
git add src/types.ts src/pages/SetupWizard.tsx
git commit -m "feat(setup): Turnier-GUID als Pflichtfeld im Liveticker-Ziel, aus dem Check-In-Block gezogen"
```

---

### Task 6: Doku, Changelog, Version

**Files:**
- Modify: `docs/aushang.md` (Abschnitt „Grenzen", Zeile ~103)
- Modify: `docs/spieler-check-in.md` (Tabelle ~Zeile 110, Einrichtung ~172–187, Push-Schema ~149–155)
- Modify: `docs/changelog.md` (neuer Abschnitt oben)
- Modify: `docs/roadmap.md` (Eintrag von „Spezifiziert" nach „Umgesetzt, aber noch nicht abgenommen")
- Modify: `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `package.json` (Version, **erst beim Merge**)

- [ ] **Step 1: `docs/aushang.md`** — den Punkt „Das Blatt zeigt auf den Verband, nicht auf ein einzelnes Turnier" ersetzen durch:

```markdown
- **Der Liveticker-Code zeigt direkt auf dieses Turnier** (`?t=<verband>&g=<GUID>`,
  seit ADR 0054): Laufen bei einem Verband mehrere Turniere parallel, landet
  die Halle trotzdem beim richtigen. Der Teilnehmerlisten-Code hängt weiter am
  Verbandskürzel — badhub zeigt dort das zuletzt bepushte Turnier des Verbands.
```

- [ ] **Step 2: `docs/spieler-check-in.md`** — in der Tabelle „Turnier | turnier.de-Turnier-GUID" ergänzen: „Seit ADR 0054 ein **Pflichtfeld** der ganzen App (`AppConfig.tournament_uuid`, Setup-Abschnitt „1 · Liveticker-Ziel"); der Check-In-Block spiegelt es (`checkin.tournament_uuid`), damit die Leser hier unverändert bleiben." Im Abschnitt „Einrichtung durch die Turnierleitung" den Schritt „GUID im Check-In-Abschnitt eintragen" auf „steht bereits im Liveticker-Ziel" umschreiben. Beim Branding-Push (Tabelle „Auth … keine GUID im Body") den Satz ändern: „Body trägt seit ADR 0054 zusätzlich `tournament_uuid`; badhub nutzt sie über den Kindschlüssel." Neuer kurzer Absatz „GUID in allen Push-Nachrichten": `tset.event.tournament_uuid`, `sched.event.tournament_uuid`, `tupdate_match.tournament_uuid`, `centry_list.tournament_uuid`, `checkin-branding.tournament_uuid` — kanonisch, weggelassen wenn keine gültige GUID konfiguriert ist.

- [ ] **Step 3: `docs/changelog.md`** — neuer Abschnitt über `## v0.9.273`:

```markdown
## v0.9.274

- **Neu: Zwei Turniere desselben Verbands am selben Tag stören sich nicht mehr
  im Liveticker.** Bisher schrieben zwei Installationen mit dem Preset „BVBB"
  abwechselnd denselben Live-Stand — auf badhub flackerte mal das eine, mal das
  andere Turnier. badhub führt jetzt jedes Turnier unter seiner turnier.de-
  Kennung getrennt (ADR 0054); bts-light schickt die Kennung in jeder
  Nachricht mit.

  Dafür ist die **Turnier-Kennung von turnier.de jetzt ein Pflichtfeld** im
  Setup unter „1 · Liveticker-Ziel" (die Adresse des Turniers einfügen genügt).
  Wer sie schon für den Hallen-Check-In eingetragen hatte, muss nichts tun —
  der Wert wird übernommen. Ohne Kennung startet die Übertragung nicht.

- **Aushang und Liveticker-Link zeigen direkt aufs eigene Turnier**
  (`…&g=<Kennung>`), statt auf die Verbandsseite mit Auswahl.

- Voraussetzung badhub-seitig: Migration 198 (Kind-Turniere). Gegen ein
  älteres badhub verhält sich die App wie bisher.
```

- [ ] **Step 4: `docs/roadmap.md`** — den Eintrag „Mehrere Liveticker je Verband" aus „Spezifiziert" nach „Umgesetzt, aber noch nicht abgenommen" verschieben, mit Zusatz: „bts-light-Seite umgesetzt (v0.9.274); badhub-Seite: PR im badhub-Repo (Migration 198) — Feldtest am nächsten Doppel-Wochenende offen."

- [ ] **Step 5: Version** — `0.9.274` in `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `package.json`. Danach `cargo build` einmal, damit `Cargo.lock` nachzieht, und `git add src-tauri/Cargo.lock`.

- [ ] **Step 6: Gesamtprüfung**

```bash
cd src-tauri && cargo fmt --all -- --check && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings
cd .. && npm run build
```

Expected: alles grün.

- [ ] **Step 7: Commit + PR**

```bash
git add docs/aushang.md docs/spieler-check-in.md docs/changelog.md docs/roadmap.md src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json package.json
git commit -m "docs: Turnier-GUID Pflicht, Direktlinks, Changelog v0.9.274"
```

PR gegen `main`: „Mehrere Liveticker je Verband — Turnier-GUID Pflicht, in jeder Nachricht, Direktlinks (ADR 0054)". Im Body: Abhängigkeit auf den badhub-Deploy (Migration 198) nennen; Tag erst nach dem badhub-Deploy (Admin, siehe Memory „Release-Tags nur durch Admin").

---

## Reihenfolge und Abnahme

Task 1 → 2 → 3 → 4 sequenziell (alle hängen an `tournament_uuid_kanonisch`). Task 5 (Frontend) kann parallel zu 2–4 laufen, sobald Task 1 gemergt ist. Task 6 zum Schluss.

Abnahme gegen die Spec: Kriterium 11 (Lader-Migration, Start ohne GUID) in Task 1 + 3; Kriterium 12 (GUID in allen Nachrichten, Aushang-URL) in Task 2 + 4. Feldtest laut Spec: zwei Installationen mit Preset „BVBB" und zwei GUIDs gegen das deployte badhub — beide Stände getrennt sichtbar, `?t=bvbb` zeigt die Auswahl, der Aushang-QR führt direkt aufs Turnier.
