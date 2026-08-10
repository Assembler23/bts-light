# bts-light: badhub-Zustand `withdrawn` in der TL-Sicht — Implementierungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Abgemeldete Spieler (`state = withdrawn` aus badhub) werden in der Turnierleitungs-Sicht korrekt angezeigt, nicht mehr ausgerufen und aus den Zählern herausgerechnet; Einchecken bleibt möglich und benennt das Überschreiben.

**Architecture:** Reine Konsumenten-Änderung — kein neuer Endpunkt, kein Protokoll-Feld. Rust (`checkin_state.rs`) lernt den Zustand in `is_missing()`/`is_withdrawn()`; das React-Panel zeigt „abgemeldet" an und korrigiert die Zähler clientseitig aus der Spielerliste.

**Tech Stack:** Rust (Tauri 2, `cargo test`), React 19 + TypeScript (tsc via `npm run build`), Tailwind 4.

**Spec:** `docs/superpowers/specs/2026-08-10-checkin-withdrawn-tl-design.md`

## Global Constraints

- Branch: `feat/checkin-withdrawn` (existiert, Spec liegt drauf). Main ist PR-geschützt.
- Repo: `C:\Users\thieronymus\repos\bts-light`. cargo und node sind lokal installiert (kein Docker nötig).
- R1: Frontend spricht den Rust-Kern nur über Tauri-Commands an — dieser Plan ändert daran nichts, kein neuer Command.
- `cargo test` muss vor jedem Commit grün sein (Repo-Regel aus CLAUDE.md).
- Rust-Tests laufen fokussiert mit `cargo test --manifest-path src-tauri/Cargo.toml checkin` (das Testmodul liegt in `src-tauri/src/badhub/checkin_state.rs`).
- Kommentare Deutsch (Umlaute sind in diesem Repo üblich — Bestand beibehalten).
- Kein `readmit`/`withdraw` über den `/tl/`-Kanal; keine badhub-Änderung.
- Commit-Trailer: `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`

---

### Task 1: Rust — `is_missing()` kennt `withdrawn`

**Files:**
- Modify: `src-tauri/src/badhub/checkin_state.rs` (Docblock `state` ~Z. 73, `is_missing()` ~Z. 89–96, neuer Helfer `is_withdrawn()`, Tests im bestehenden `#[cfg(test)]`-Block)

**Interfaces:**
- Produces: `CheckinPlayer::is_missing(&self) -> bool` (withdrawn → false), `CheckinPlayer::is_withdrawn(&self) -> bool`. Task 2 nutzt nur den JSON-`state`-String direkt (Frontend), verlässt sich aber auf die Ansage-Semantik aus diesem Task.

- [ ] **Step 1: Failing Tests schreiben** — im `#[cfg(test)]`-Block von `checkin_state.rs`, neben `rueckfrage_zaehlt_als_fehlend` (Helfer `klasse_mit(n, da)` wiederverwenden):

```rust
#[test]
fn abgemeldete_zaehlen_nicht_als_fehlend() {
    // AK-C16/C17: ein in badhub Abgemeldeter wird nicht gesucht — er
    // kommt nicht wieder, und ein Ausruf ueber die Hallen-Lautsprecher
    // waere nur Verwirrung.
    let mut k = klasse_mit(3, 1);
    k.players[1].state = "withdrawn".into();
    assert_eq!(
        missing_text(&k, 8).unwrap(),
        "In Herrendoppel B fehlt noch Vor2 Nach2."
    );
}

#[test]
fn nur_abgemeldete_uebrig_gibt_keine_ansage() {
    // AK-C8 greift auch dann, wenn der letzte Offene abgemeldet ist.
    let mut k = klasse_mit(2, 1);
    k.players[1].state = "withdrawn".into();
    assert!(missing_text(&k, 8).is_none());
}

#[test]
fn withdrawn_wird_geparst_und_erkannt() {
    let mut k = klasse_mit(2, 1);
    k.players[1].state = "withdrawn".into();
    assert!(!k.players[1].is_missing());
    assert!(k.players[1].is_withdrawn());
    assert!(!k.players[0].is_withdrawn());
    // Die drei Altzustaende verhalten sich unveraendert:
    assert!(!k.players[0].is_missing()); // checked_in
    k.players[1].state = "query".into();
    assert!(k.players[1].is_missing());
}
```

- [ ] **Step 2: Tests laufen lassen — müssen fehlschlagen**

Run: `cargo test --manifest-path src-tauri/Cargo.toml checkin`
Expected: FAIL — `abgemeldete_zaehlen_nicht_als_fehlend` (Ansage nennt beide) und `withdrawn_wird_geparst_und_erkannt` (kein `is_withdrawn`, Compile-Fehler). Compile-Fehler zählt als RED.

- [ ] **Step 3: Implementieren** — in `impl CheckinPlayer`:

Docblock des Feldes `state` ändern zu:

```rust
    /// `open` · `checked_in` · `query` · `withdrawn` (seit badhub-Migration
    /// 157; gesetzt wird `withdrawn` nur ueber die badhub-Verwaltung).
```

`is_missing()` ersetzen:

```rust
    /// Fehlt dieser Spieler noch? Grundlage der Fehlt-Ansage (AK-C7).
    ///
    /// `query` zählt als fehlend: die betreffende Person soll zur
    /// Turnierleitung kommen, ist also gerade nicht abgehakt.
    ///
    /// `withdrawn` zählt NICHT als fehlend (AK-C17): ein Abgemeldeter kommt
    /// nicht wieder — ihn über die Hallen-Lautsprecher zu suchen wäre nur
    /// Verwirrung. Er ist auch nicht „da": beides trifft nicht zu, deshalb
    /// führen Anzeige und Zähler ihn gesondert.
    pub fn is_missing(&self) -> bool {
        self.state != "checked_in" && self.state != "withdrawn"
    }

    /// In badhub abgemeldet (AK-C16). Als Helfer, damit UI-nahe Stellen
    /// nicht gegen String-Literale vergleichen.
    pub fn is_withdrawn(&self) -> bool {
        self.state == "withdrawn"
    }
```

- [ ] **Step 4: Tests laufen lassen — müssen grün sein**

Run: `cargo test --manifest-path src-tauri/Cargo.toml checkin`
Expected: PASS, alle bestehenden Checkin-Tests weiter grün.

- [ ] **Step 5: Voller Testlauf + Commit**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: alles grün.

```bash
git add src-tauri/src/badhub/checkin_state.rs
git commit -m "feat(checkin): Abgemeldete (withdrawn) fallen aus Fehlt-Ansage und Fehlt-Liste"
```

---

### Task 2: Frontend — Anzeige „abgemeldet" und korrigierte Zähler

**Files:**
- Modify: `src/types.ts` (~Z. 155, Kommentar `state`)
- Modify: `src/pages/CheckinPanel.tsx` (`spielerZustand()` ~Z. 64–82, `gesamt`-useMemo ~Z. 132–142, Kopfzeile ~Z. 235–239, Klassen-Kopf ~Z. 263–292, Einchecken-Knopf-`title` ~Z. 385–393)

**Interfaces:**
- Consumes: `CheckinPlayer.state` kann `"withdrawn"` sein (Task 1 dokumentiert die Semantik; der Payload liefert den Wert seit badhub-Migration 157).

- [ ] **Step 1: `types.ts`-Kommentar anpassen**

```ts
  /** `open` · `checked_in` · `query` · `withdrawn` (abgemeldet, seit badhub-Migration 157) */
  state: string;
```

- [ ] **Step 2: `spielerZustand()` erweitern** — neuer Zweig VOR der `locked`-Prüfung (nach dem `query`-Block):

```ts
  if (p.state === "withdrawn") {
    // In badhub abgemeldet: weder da noch gesucht. Durchgestrichen, damit
    // der Blick beim Durchgehen der Liste nicht an der Zeile hängen bleibt.
    return { text: "abgemeldet", klasse: "text-slate-400 line-through" };
  }
```

- [ ] **Step 3: Zähler korrigieren**

`gesamt`-useMemo ersetzen (Abgemeldete aus der Spielerliste zählen):

```ts
  const gesamt = useMemo(
    () =>
      klassen.reduce(
        (acc, k) => {
          const abgemeldet = k.players.filter(
            (p) => p.state === "withdrawn",
          ).length;
          return {
            gemeldet: acc.gemeldet + k.gemeldet - abgemeldet,
            eingecheckt: acc.eingecheckt + k.eingecheckt,
            abgemeldet: acc.abgemeldet + abgemeldet,
          };
        },
        { gemeldet: 0, eingecheckt: 0, abgemeldet: 0 },
      ),
    [klassen],
  );
```

Kopfzeile ersetzen:

```tsx
        {gesamt.gemeldet > 0 && (
          <span className="text-sm text-slate-500">
            {gesamt.eingecheckt} von {gesamt.gemeldet} da
            {gesamt.abgemeldet > 0 ? ` · ${gesamt.abgemeldet} abgemeldet` : ""}
          </span>
        )}
```

Im Klassen-Kopf (`klassen.map`) die Rechnung ersetzen — aus

```ts
        const fehlend = k.gemeldet - k.eingecheckt;
```

wird:

```ts
        // badhubs TL-Zaehlung schliesst Abgemeldete bewusst ein (dortige
        // Entscheidung) — hier werden sie herausgerechnet, denn fuer die
        // Turnierleitung sind sie weder da noch fehlend.
        const abgemeldet = k.players.filter(
          (p) => p.state === "withdrawn",
        ).length;
        const gemeldet = k.gemeldet - abgemeldet;
        const fehlend = gemeldet - k.eingecheckt;
```

und die Zähl-Anzeige der Klasse:

```tsx
              <span className="text-xs text-slate-500">
                {k.eingecheckt} von {gemeldet} da
                {fehlend > 0 ? ` · ${fehlend} fehlen` : ""}
                {abgemeldet > 0 ? ` · ${abgemeldet} abgemeldet` : ""}
              </span>
```

- [ ] **Step 4: Einchecken-Knopf-Tooltip** — im Spieler-`map` (der `!da`-Knopf mit `title="Als anwesend eintragen"`):

```tsx
                            title={
                              p.state === "withdrawn"
                                ? "Trotz Abmeldung als anwesend eintragen"
                                : "Als anwesend eintragen"
                            }
```

(Der Knopf selbst bleibt für Abgemeldete sichtbar — Betreiber-Entscheidung; der Zurücksetzen-Knopf bleibt unverändert an `da` gebunden.)

- [ ] **Step 5: Compiler-Check**

Run: `npm run build`
Expected: `tsc` und `vite build` ohne Fehler. (Kein Komponenten-Test-Runner im Repo — der Rust-Teil trägt die Verhaltenstests, die TS-Seite sichert der Compiler.)

- [ ] **Step 6: Commit**

```bash
git add src/types.ts src/pages/CheckinPanel.tsx
git commit -m "feat(checkin): Abgemeldete anzeigen und aus den Zaehlern herausrechnen"
```

---

### Task 3: Spec-Doku — AK-C16 bis C18

**Files:**
- Modify: `docs/features/spieler-check-in.md` (Schnitt-C-Abschnitt, nach C15)

**Interfaces:**
- Consumes: Verhalten aus Tasks 1–2.

- [ ] **Step 1: Akzeptanzkriterien anfügen** — nach C15, Stil der Liste:

```markdown
- [x] **C16** Ein in badhub abgemeldeter Spieler (`state = withdrawn`, dort
      über die Verwaltung gesetzt, badhub-Migration 157) erscheint in der
      TL-Sicht als „abgemeldet" (grau, durchgestrichen) und zählt weder als
      eingecheckt noch als fehlend; die Zähler rechnen ihn heraus.
- [x] **C17** Die Fehlt-Ansage nennt Abgemeldete nicht — weder namentlich
      noch in der Anzahl. Sind alle übrigen eingecheckt, gibt es keine
      Ansage (C8 greift).
- [x] **C18** Die Turnierleitung kann einen Abgemeldeten über den
      bestehenden `check_in`-Eingriff trotzdem einchecken; der Knopf benennt
      das Überschreiben („Trotz Abmeldung als anwesend eintragen").
      Abmelden und Wiederanmelden selbst gibt es nur in der
      badhub-Verwaltung.
```

(Die bestehenden C-Kriterien-Häkchen nicht anfassen; nur die drei neuen mit `[x]` anfügen, sie werden mit diesem Schnitt erfüllt.)

- [ ] **Step 2: Test-Abschnitt der Doku ergänzen** — im Tests-Abschnitt unter dem `checkin_state.rs`-Absatz die neuen Testnamen nennen (`abgemeldete_zaehlen_nicht_als_fehlend`, `nur_abgemeldete_uebrig_gibt_keine_ansage`, `withdrawn_wird_geparst_und_erkannt`).

- [ ] **Step 3: Gesamtlauf**

Run: `cargo test --manifest-path src-tauri/Cargo.toml && npm run build`
Expected: beides grün.

- [ ] **Step 4: Commit**

```bash
git add docs/features/spieler-check-in.md
git commit -m "docs(checkin): AK-C16 bis C18 - Abgemeldete in der TL-Sicht"
```

---

## Abschluss

`superpowers:finishing-a-development-branch` — PR gegen main im Repo `Assembler23/bts-light`. Kein Deploy-Schritt: bts-light ist eine Desktop-App, die Änderung kommt mit dem nächsten Release zu den Turnierleitungen; bis dahin zeigen alte Versionen Abgemeldete als „fehlt" (verkraftbar, dokumentiert).
