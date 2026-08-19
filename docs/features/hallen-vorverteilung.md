# Automatische Hallen-Vorverteilung — Spezifikation

> Status: **abgestimmt 2026-08-16** (via /idee: Brief → Grill → How-To → Review;
> Umsetzung vom Nutzer am 2026-08-16 pauschal freigegeben).
> Quelle: Idee (Chat vom 2026-08-16). Betroffene Crates: src-tauri,
> relay-proto (neue TlActions), `assets/tl.html`; relay/ nur über den
> einkompilierten Seiten-/Action-Parser (Deploy nötig).
> ADR: [0029](../adr/0029-hallen-vorverteilung-eigener-store.md) (Store +
> Herkunft), [0030](../adr/0030-halle-bindet-die-feldvergabe.md)
> (Bindungswirkung).

## Kontext / Problem

Bei Mehr-Hallen-Turnieren erfahren Spieler erst mit dem Aufruf bzw. der
Feldzuweisung, in welcher Halle sie spielen — zu spät, um rechtzeitig ins
richtige Gebäude zu gehen. Die Turnierleitung kann Hallen zwar je Spiel von
Hand setzen (`SetHall`), aber niemand pflegt das für Dutzende Spiele
vorausschauend. Zusätzlich war die Hallen-Zuordnung bisher **keine**
Vergabe-Zusage: Die automatische Feldvergabe prüfte Hand-Hallen gar nicht.

## Zielbild & Erfolgskriterien

Die Turnierleitung schaltet die **Vorverteilung** ein → die vordersten
**x** wartenden Spiele tragen verbindlich eine Halle, verteilt im
Verhältnis der entsperrten Spielfelder je Halle und **gemischt** (bei 2:1:
A, A, B, A, A, B — keine Blöcke). Endet ein Spiel oder kommen neue hinzu,
füllt die Automatik nach, sodass immer x Spiele im Voraus feststehen.
Spieler sehen ihre Halle früh auf den Hallen-Monitoren und im
badhub-„nächste Spiele"-Aushang (`display=next&halle=…`); das konkrete
Feld folgt wie bisher erst kurz vorher.

**Erfolg:** Beim nächsten Mehr-Hallen-Turnier steht für die nächsten x
Spiele die Halle sichtbar fest (TL-Web-Badge, Monitor, badhub), die
automatische Feldvergabe vergibt ausschließlich in die zugesagte Halle,
und die Verteilung entspricht dem Feldverhältnis (±1 durch Rundung).

## Fachliche Entscheidungen (Brief B1–B5, Grill E1–E12)

1. **Nur Lücken füllen (B1):** Die Automatik setzt Hallen nur an Spielen
   **ohne** Halle; Regel-, Hand-, BTP- und Aufruf-Hallen bleiben
   unangetastet, zählen aber auf die Verhältnis-Quote an.
2. **Fest = fest (B2):** Einmal verteilte Hallen zieht die Automatik nie
   um. Nur die TL ändert (Hand-Halle ersetzt Auto; Rücknahme der
   Hand-Halle löscht auch den Auto-Eintrag → frische Verteilung).
3. **Bindungswirkung (E1, ADR 0030):** Regel-, Hand- und Auto-Hallen sind
   im Mehr-Hallen-Betrieb ein **hartes Constraint** der automatischen
   Feldvergabe; für **Auto**-vorverteilte Spiele entfällt zusätzlich die
   Aufruf-Pflicht der Vergabe (`require_call`). BTP-Ort und Aufruf-Halle
   binden wie bisher (Aufruf wirkt über den `require_call`-Zweig).
4. **Tages-Halle (E2):** Bei gesetzter `auto_assign.active_hall` ist die
   Vorverteilung deaktiviert (UI ausgegraut, Host lehnt Einschalten ab).
5. **Aufruf schlägt Auto (E3):** Ein Vorbereitungs-Aufruf löscht die
   Auto-Zuordnung des Spiels (beide Aufruf-Pfade + Reconcile-Netz).
6. **Eigener Store (E4, ADR 0029):** Auto-Zuordnungen liegen
   turniergebunden in `auto-halls.json` (ADR-0022-Muster) — nie in
   `spielorte.json`. Neue Herkunft **`hall_source: "auto"`** mit eigenem
   Badge in TL-Web.
7. **Bedienung (B3/E5):** Schalter + Fenstergröße x in TL-Web, persistiert
   in der Config (`hall_prefill`, Default **aus**; x = 0 heißt
   „automatisch = Gesamtzahl der Spielfelder", B4); Klemme 1..=120.
   Massen-Rücknahme-Knopf „Auto-Hallen räumen" (E10; räumt NUR
   Auto-Einträge). Ausschalten lässt Verteiltes stehen (B2).
8. **Algorithmus (E6/B5):** Quoten je Halle nach größtem Rest aus den
   **aktuell entsperrten** Feldern (E11); bereits zugeordnete (auch
   gerufene, E8) Spiele im Fenster werden angerechnet; Überschuss einer
   Halle reduziert nur die Restquoten; Lücken werden in Listenreihenfolge
   per Höchstzahl-Verfahren gefüllt (ergibt das Misch-Muster);
   deterministisch und **idempotent** (gleicher Input ⇒ keine Änderung,
   keine Revisions-/IO-Flut — Insert-only statt Fingerprint).
9. **Fenster (E8/A3):** x = die vordersten x Spiele der globalen
   Warteliste; per Drag ins Fenster gerückte Spiele verteilt der nächste
   Lauf; hinten hinausgerutschte behalten ihre Halle.
10. **Hallen-Wegfall (E12):** Zeigt eine Auto-Zuordnung auf eine Halle
    ohne Felder, wird sie verworfen und im selben Lauf neu verteilt.
    Vorübergehend komplett gesperrte Hallen lassen den Bestand stehen.
11. **Kein Spieler-Bezug (E9):** Reines Feld-Verhältnis; „gleiche Halle
    fürs Folgespiel eines Spielers" ist explizites Nicht-Ziel.
12. **Nur der Master verteilt (A2);** Slave/Slave-Bridge fassen den Store
    nie an.

## Nicht-Ziele

- Kein automatisches Umziehen verteilter Spiele; keine Änderung der
  Spiel-Reihenfolge; kein BTP-Write (unmöglich).
- Kein Übersteuern von Regel-/Hand-/Aufruf-/BTP-Hallen durch die Automatik.
- Keine Spieler-Wegzeit-/Folgespiel-Präferenz (E9).
- Regel-/BTP-Hallen erreichen badhub-„next" weiterhin nicht (heutiges
  Verhalten; nur die Auto-Halle wird dort zusätzlich eingestempelt).
- Kein separater „Hallen binden ja/nein"-Schalter (bewusst: eine
  Betriebsart weniger; Rückweg je Spiel ist die Hand-Halle „–").

## Betroffene Komponenten / Architekturregeln / Daten

- **Kaskade** `assign::hall_for_match`/`resolve_and_sort_key`/`ready_queue`:
  neue letzte Stufe **Auto** (Reihung: Regel → Hand → BTP → Aufruf → Auto →
  keine) + `HallSource::Auto`; alle sechs Aufrufer wachsen um den
  Parameter (`sync.rs`, `tl.rs`, `server.rs`, `commands.rs`,
  `badhub/payload.rs` via `LivetickerContext.auto_halls`, `ready_queue`).
- **Neu** `tablet/hall_assign.rs`: `AutoHallStore` (`auto-halls.json`,
  ADR-0022-Muster, generation-Zähler) + reine Verteil-Funktion
  `distribute(window, halls)`.
- **Sync-Loop:** `reconcile_auto_halls` im Master-Block unmittelbar vor
  `auto_assign` (Aufräumen immer, Verteilen nur bei enabled + multi_hall +
  keiner aktiven Halle); **Vergabe-Umbau** im `pick` von `auto_assign`
  (Constraint + `require_call`-Ersatz, nur `multi_hall`-Guard!).
- **Spieler-Kanäle:** dritter Stempel-Block in
  `state::apply_preparation_calls` (Auto-Hallen → `preparation_hall`, ohne
  `preparation_call_ts`; Vorrang Call > Manual > Auto).
- **Config:** `AppConfig.hall_prefill: HallPrefillConfig { enabled=false,
  window=0 }` mit serde-Default — alte config.json lädt unverändert.
- **relay-proto:** neue TlActions `SetHallPrefill { enabled, window }`
  (atomar) + `ClearAutoHalls`; `every_tl_action`/Fingerprint/Label-Pflege.
  **Relay-Deploy vor App-Release** (Action-Parser + tl.html einkompiliert).
- **TL-State:** `hall_prefill: TlHallPrefill { enabled, window,
  effective_window, blocked_by_active_hall }`; tl.html: Schalter, x-Feld,
  Räumen-Knopf, „Auto"-Badge; Feature-Detection über das State-Feld (alte
  Hosts liefern es nicht → keine toten Bedienelemente).
- **R1–R6:** R2 gewahrt — die Halle ist host-lokale Disposition, keine
  Court→Match-Zuordnung; die Vergabe bleibt der einzige BTP-Schreiber.
  Berechnung host-seitig, LAN = Cloud (R3). R5: `SetHallPrefill` wird
  host-seitig validiert (Klemme, E2-Ablehnung).
- **Datenschutz:** Store enthält nur Match-IDs + Hallennamen.

## Akzeptanzkriterien

**Verteilung**
- [ ] 2 Hallen mit 12:6 entsperrten Feldern, x = 18, Fenster ohne
  Vorbelegung → 12× A und 6× B im Muster A, A, B, … (Höchstzahl-Verfahren);
  bei Vorbelegung (z. B. 10× A per Regel) werden nur die Lücken gefüllt
  und A wird angerechnet.
- [ ] Ein zweiter Lauf mit unverändertem Input ändert nichts (keine
  Persistenz, keine TL-Revision).
- [ ] Spielende/Neuzugang/Drag ins Fenster → der nächste Lauf füllt wieder
  auf x auf; hinten hinausgerutschte Spiele behalten ihre Halle.
- [ ] Gerufene Spiele im Fenster zählen auf die Quote, bekommen aber nie
  einen Auto-Eintrag; Hallen ohne entsperrte Felder bekommen nichts.
- [ ] Auto-Zuordnung auf eine Halle ohne Felder wird verworfen und im
  selben Lauf neu verteilt (E12).

**Bindung & Kaskade**
- [ ] Ein Spiel mit Regel-, Hand- oder Auto-Halle B bekommt von der
  automatischen Vergabe nie ein Feld in Halle A (Mehr-Hallen); im
  Ein-Hallen-Turnier bleibt die Vergabe von Hallennamen unbeeinflusst.
- [ ] Ein Auto-vorverteiltes Spiel wird im `require_call`-Modus ohne
  Vorbereitungs-Aufruf in seiner Halle vergeben; alle anderen Spiele
  brauchen den Aufruf wie bisher.
- [ ] Ein Vorbereitungs-Aufruf (beide Pfade) löscht die Auto-Zuordnung;
  `SetHall` (auch Rücknahme) ebenso; die Kaskade zeigt Aufruf/Hand vor
  Auto, BTP vor Auto.
- [ ] Bei gesetzter aktiver Halle lehnt der Host `SetHallPrefill(enabled)`
  mit verständlicher Meldung ab; die UI ist ausgegraut.

**Bedienung & Persistenz**
- [ ] Schalter + x überleben App-Neustart (config.json); alte config.json
  ohne `hall_prefill` lädt mit Default aus; x = 0 zeigt „automatisch (N)".
- [ ] „Auto-Hallen räumen" entfernt ausschließlich Auto-Einträge; Hand-,
  Regel- und Aufruf-Hallen bleiben.
- [ ] Auto-verteilte Spiele tragen in TL-Web ein eigenes „Auto"-Badge
  (`hall_source: "auto"`); Turnierwechsel verwirft den Store.

**Spieler-Kanäle**
- [ ] Die Auto-Halle erscheint als `upcoming_matches[].hall` im
  badhub-Push (ohne Aufruf-Zeitstempel) und auf dem
  Vorbereitungs-/Hallen-Monitor; ein späterer Aufruf/Hand-Eintrag behält
  Vorrang.

**Kompatibilität**
- [ ] Alte tl.html zeigt bei `hall_source: "auto"` keine Fehler (toleriert
  unbekannte Quelle); neue tl.html an altem Host blendet die
  Bedienelemente aus (kein `state.hall_prefill`).

## Tests

Rust-Unit-Tests (TDD): `distribute`-Szenarien (B5-Anker 2:1, größter Rest
mit 3 Hallen, Anrechnung, Überschuss, called, 0-Felder-Halle, Idempotenz,
Tie-Break); Store-Suite (Roundtrip, Turnierwechsel, Insert-only,
clear_all, retain, unlesbare Datei, generation); Kaskaden-Reihung +
Serde `"auto"`; `auto_assign`-Matrix (Hand bindet, Auto bindet +
Aufruf-Ersatz, Ein-Hallen-Guard, active_hall, Pausen-Prüfungen unberührt);
`reconcile_auto_halls` (Auffüllen, E3, E12, Schalter aus, active_hall,
Slave); Config-Default-Roundtrip; TlAction-Serde + Klemme + E2-Ablehnung;
badhub-Stempel-Vorrang. `cargo test` grün, `npm run build` fehlerfrei.
Manuelle Prüfliste: Zwei-Hallen-Testturnier, LAN + Cloud.

## Risiken & Rollback

- **Verhaltensänderung:** Hand-/Regel-Hallen binden die Vergabe erstmals
  hart — Turniere, die Hand-Hallen nur als Anzeige nutzten, müssen das
  wissen (Changelog + Bedien-Doku prominent). Rückweg je Spiel:
  Hand-Halle auf „–". Der Vergabe-Umbau ist ein eigener, einzeln
  revertierbarer Commit.
- Spiele können ohne Vorbereitungs-Ansage aufs Feld kommen (Aufruf-Ersatz)
  — dokumentiert; der Aufruf bleibt jederzeit möglich (räumt die
  Auto-Halle).
- Rollback im Turnier: Schalter aus → „Auto-Hallen räumen" → Zustand wie
  vorher. App-Downgrade: `auto-halls.json` wird ignoriert, config bleibt
  lesbar.
- `locked_courts` ist RAM-only: nach Neustart rechnet das Verhältnis mit
  allen Feldern; Bestand bleibt (B2) — bewusst akzeptiert.

## Offene Fragen / Annahmen

- A2/A4 bestätigt (nur Master; leere Hallen gehen leer aus). Angenommen:
  Fenster-Obergrenze 120 (Wartelisten-Limit) reicht praktisch; die
  Vergabe-Bindung gilt bewusst NICHT für BTP-Orte (stale Spalten) —
  am Testturnier gegenprüfen.

## Betroffene Doku-Dateien

Diese Spec; `docs/turnierleitung-web.md` (Bedienung + Verhaltensänderung);
`docs/multi-hall.md` (Architektur-Erzählung); `docs/btp_protocol.md`
(Kaskaden-/Vergabe-Definition); `docs/cloud-relay.md` (neue Actions,
`"auto"`-Wire-Wert, Deploy-Reihenfolge); `docs/changelog.md`;
CLAUDE.md-Tabelle; ADR 0029 + 0030.

## Umsetzungs-Hinweise

Drei Etappen (Details: `docs/features/_intake/hallen-vorverteilung/3-how-to.md`):
**A** Fundament (Kaskade + Store + badhub-Stempel, verhaltensneutral) →
**B** Vergabe-Umbau (isoliert revertierbar, ADR 0030) → **C** Verteil-Lauf
+ Config + TlActions + tl.html (ADR 0029; security-reviewer wegen neuer
TL-Eingaben). Version gemeinsam bumpen; code-reviewer je Etappe;
Relay-Deploy vor App-Release.
