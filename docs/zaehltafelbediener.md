# Zähltafelbediener (Tabletoperator)

Verwaltung der Zähltafelbediener nach dem Vorbild des Original-BTS
(letilo/bts). Grundlage: [ADR 0007](adr/0007-zaehltafelbediener.md). Wird
**in zwei Phasen** gebaut; hier ist **Phase 1** (rein bts-light-seitig, ohne
neuen BTP-Schreibpfad) beschrieben.

Opt-in: Einstellungen → **„Zähltafelbediener"** → „Warteschlange führen"
(`config.scorekeeper.enabled`, Default aus). Ohne den Schalter ändert sich
nichts.

## Phase 1 — Warteschlange (v0.9.163)

**Idee (wie im Original-BTS):** Der **Verlierer** eines regulär beendeten
Spiels ist als nächster Zähltafelbediener dran. Die Reihenfolge ist eine
globale **FIFO-Warteschlange**.

- **Einreihen:** Der Sync-Loop erkennt beim Feldwechsel ein regulär beendetes
  Spiel (`track_scorekeepers` in `sync.rs`) und reiht bei aktivierter
  Verwaltung den Verlierer ein (`TabletState::enqueue_scorekeeper`).
  **Walkover/Aufgabe/Disqualifikation erzeugen keinen Eintrag** (nur
  `MatchResult::Normal`). Idempotent je Match (Dedup über `enqueued_finishes`),
  Doppel = **ein** Eintrag (das ganze Team).
- **Anzeige & Pflege:** In der **Spielübersicht** listet der Abschnitt
  „Nächste Zähltafelbediener" die Warteschlange (FIFO). Pflege: **vorziehen**
  (`advance_scorekeeper`), **entfernen** (`remove_scorekeeper`), **manuell
  hinzufügen** (`add_scorekeeper`). Die Warteschlange lebt im Arbeitsspeicher
  (nicht persistiert) — ein App-Neustart leert sie.
- **Datenmodell:** `ScorekeeperEntry { key, names, from_court_id, enqueued_ms }`
  in `tablet/state.rs`. `from_court_id` (zuletzt gespieltes Feld) ist für die
  spätere „bevorzugt aufs eigene Feld"-Zuweisung vorgesehen.

Commands: `scorekeeper_queue`, `remove_scorekeeper`, `advance_scorekeeper`,
`add_scorekeeper` (`commands.rs`). Konfiguration:
`config::ScorekeeperConfig { enabled, break_seconds }` (break_seconds Default
300 s, wirkt erst mit der Zuweisung in einer späteren Scheibe).

## Zuweisung beim Feld-Aufruf (Scheibe 2, v0.9.164)

Sobald ein Feld belegt wird, zieht der Sync-Loop einen Bediener aus der
Warteschlange (`assign_scorekeeper_for_court`): **bevorzugt jemanden mit
`from_court_id == court` (spielte zuletzt auf genau diesem Feld — der Verlierer
des Vorspiels), sonst den ältesten** Wartenden. Idempotent je (Feld, Match);
ist die Schlange leer, bleibt das Feld ohne Bediener. Wird das Feld frei oder
wechselt das Spiel, räumt `retain_scorekeeper_assignments` die Zuweisung.

Der zugewiesene Bediener ersetzt in `CourtOverview.scorekeeper` den pro-Feld-
Hinweis (wenn die Verwaltung aktiv ist) und erscheint so in der Spielübersicht
je Feld („Bediener: …"). Wird die Verwaltung mitten im Turnier **abgeschaltet**,
löscht der Sync-Loop alle Zuweisungen (`clear_scorekeeper_assignments`) — es
bleibt kein veralteter Name in der Anzeige hängen; angezeigt wird dann wieder
der pro-Feld-Hinweis. Bei mehreren gleichzeitig neu belegten Feldern wird
nach CourtID sortiert zugewiesen (deterministisch/fair).

**Felder ohne Bediener-Vergabe.** Je Feld lässt sich die Vergabe abschalten
(`CourtSwitches::operator`, turniergebunden gespeichert — siehe
[schiedsrichter-management.md](schiedsrichter-management.md)). Auf so einem
Feld bedient der Schiedsrichter das Tablet selbst: `assign_scorekeeper_for_court`
kehrt sofort zurück, das Feld bekommt keinen Bediener und **verbraucht auch
keinen Eintrag** aus der Warteschlange — der Wartende bleibt für ein anderes
Feld übrig. Ohne Eintrag gilt „Vergabe aktiv"; für bestehende Installationen
ändert sich nichts. Der Schalter wirkt nur, solange der
Schiedsrichter-Betrieb eingeschaltet ist — sonst wäre er nach dem Abschalten
nirgends mehr zurückzunehmen.

## Ansage (Scheibe 3, v0.9.165)

Steht am Feld eine Bedienung, hängt die Feld-Ansage am Ende
„**Tabletbedienung: {Name}.**" an (EN: „Scoreboard operator: …"). Umgesetzt in
`announcer.ts` (`buildAnnouncementSegments` + `buildAnnouncementSsml`, Feld
`scorekeeperNames`). Gilt für Standard- und Azure-Stimme.

**Seit v0.9.246 entscheidet der Schalter `announce.announce_scorekeeper`**
(Default an, Ansage-Einstellungen: „Zähltafelbedienung mit ansagen"), nicht
mehr die Herkunft des Namens — [ADR 0040](adr/0040-ansage-besetzung-einstellbar.md)
löst die Regel aus ADR 0007 ab. Ist der Schalter an, wird angesagt, was am
Feld steht: der zugewiesene Bediener **oder** der pro-Feld-Hinweis. Vorher gab
es für Turniere, die nur mit dem pro-Feld-Hinweis arbeiten, überhaupt keine
Bedienungs-Ansage — der Name stand auf dem Bildschirm und blieb stumm.

`scorekeeper_assigned` bleibt für die **Anzeige** erhalten und unterscheidet
weiterhin echte Zuweisung von Hinweis. Wer den Hinweis nicht ausgerufen haben
will — etwa auf einem reinen LAN-Ansage-PC ohne Bediener-Verwaltung, wo der
Sync die Zuweisungen räumt und den Hinweis weiterfüllt —, schaltet die
Bedienungs-Ansage aus.

## Cloud-Ansage der fernen Halle (v0.9.166)

Warteschlange und Zuweisung leben auf dem Master (Sync-Loop). Damit die ferne
Halle den Bediener trotzdem ansagen kann, schickt der Master ihn je Feld über
den Relay mit: `MatchBrief` trägt `scorekeeper` + `scorekeeper_assigned`
(gesetzt aus `scorekeeper_display`), der Command reicht sie als `CloudPrepared`/
`CloudAnnounceCourt` durch, und `CloudAnnounceSlave` sagt „Tabletbedienung: …"
an, sofern `announce_scorekeeper` gesetzt ist (ADR 0040; vorher: nur bei
`scorekeeper_assigned`). Reicht die Cloud (relay-deploy für
MatchBrief) + die App.

## Bedienung aus der Turnierleitungs-Seite (TL-Web)

Die Warteschlange ist auch in der browserbasierten [Turnierleitungs-Seite
(TL-Web)](turnierleitung-web.md) sichtbar und bedienbar — nicht nur in der
Spielübersicht am Turnier-PC. Ein eigener aufklappbarer Abschnitt zeigt die
Wartenden samt Wartezeit; von dort lässt sich **vorziehen**, **entfernen**
und **manuell hinzufügen** — dieselben host-seitigen Commands wie am
Turnier-PC (`advance_scorekeeper`, `remove_scorekeeper`, `add_scorekeeper`,
auf die Leitung als `TlAction::ScorekeeperAdvance`/`ScorekeeperRemove`/
`ScorekeeperAdd`). Der Abschnitt erscheint nur bei eingeschalteter
Verwaltung (`config.scorekeeper.enabled`) — sonst gäbe es dort nichts zu
bedienen. Umgesetzt in `src-tauri/assets/tl.html`.

## Noch offen

- **Mindestpause** (`break_seconds`): in Phase 1 **ohne Wirkung** — ein
  Bediener verlässt beim Zuweisen die Warteschlange und wird nicht automatisch
  wieder eingereiht, eine „Pause nach dem Dienst" hat hier also keinen Effekt.
  Die Pause greift erst mit **Phase 2** (BTP-Auscheck mit künstlich
  verschobenem `last_time_on_court`), damit BTP den Bediener nicht zu früh für
  ein eigenes Spiel einplant. Das Config-Feld ist bereits vorhanden.
- ~~Optional: ein Zweitaufruf-Knopf „… bitte als Tabletbedienung melden".~~
  **Umgesetzt in v0.9.232** — siehe „Nachruf" unten.

## Nachruf an die Bedienung (v0.9.232)

Der Bediener wird beim Feld-Aufruf genannt („Tabletbedienung: {Name}"),
aber kommt niemand, blieb der Turnierleitung bisher nur, die **Spieler**
erneut zu rufen. Jetzt gibt es im ⋯-Menü der Feld-Kachel in TL-Web den Knopf
**„Bedienung nachrufen"**:

> 🔔 „**Feld 3. Meier / Kraus, bitte als Tabletbedienung melden.**"
> (englisch: „Court 3. Meier / Kraus, please report as scoreboard operator.")

Ab dem zweiten Mal steht das Stufenwort davor („Zweiter Aufruf.", dann
„Dritter und letzter Aufruf."), höchstens bis Stufe 3.

**Das ist kein Spieler-Aufruf.** Der Turnier-PC führt dafür einen **eigenen**
Zähler (`scorekeeper_call_stages`): `call_stages` und `prep_call_stages`
bleiben unberührt, das Aufruf-Abzeichen an der Kachel steht still. Ohne diese
Trennung zöge ein Nachruf an die Bedienung die angezeigte Aufruf-Zahl der
**Spieler** hoch — und an der dritten Stufe hängt die kampflose Wertung. Ein
Spielwechsel auf dem Feld setzt den Bediener-Zähler zurück.

Der Zählerstand wird **nicht** ausgeliefert: Er bestimmt allein die
Ansage-Stufe. Anders als beim Spieler-Aufruf steht hinter dem letzten Nachruf
keine Rechtsfolge, die man anzeigen müsste.

**Der Knopf erscheint nur bei zugewiesenem Bediener** — das eine Flag
`scorekeeper_assigned` deckt alle drei Fälle ab, in denen es niemanden zu
rufen gibt: leere Warteschlange, Feld mit abgeschalteter Bediener-Vergabe
(`CourtSwitches::operator`) und global ausgeschaltete Verwaltung. In allen
dreien weist der Sync-Lauf gar nicht erst zu.

Eine Feinheit: Wird der Feld-Schalter **mitten im laufenden Spiel**
abgeschaltet, bleibt die bereits erteilte Zuweisung (und damit der Knopf) bis
zum Spielwechsel bestehen — die Person bedient ja tatsächlich gerade.

**Bewusst nicht gebaut:** ein Bediener beim *Vorbereitungs*-Aufruf. Der
Bediener hängt am **Feld**, nicht am Match — zugewiesen wird er erst, wenn
das Feld belegt wird, und ein Vorbereitungs-Aufruf kennt nur die Halle. Eine
Reservierung je Match hätte die erprobte Regel „bevorzugt der Verlierer des
Vorspiels auf genau diesem Feld" abgelöst; das war den Nachruf nicht wert
(Spec [`features/tl-sicht-feinschliff.md`](features/tl-sicht-feinschliff.md),
Punkt 2).

Wortlaut: [`src/io/scorekeeperCallText.mjs`](../src/io/scorekeeperCallText.mjs),
in der CI geprüft; beide Synthese-Pfade (Web Speech und Azure-SSML) nutzen
dieselben Bausteine. Wie jede TL-Web-Ansage erklingt der Nachruf nur in der
Halle des Felds.

## Phase 2 (später, eigene Freigabe)

Auscheck des Bedieners in **BTP** (`CheckedIn=false`, Tilos „Schreibweg 2"),
damit BTP ihn nicht parallel für ein eigenes Spiel einplant. Erst nach
echtem-BTP-Gegencheck (Check-in-Bit-Regression v0.9.103), siehe ADR 0007.

## Verwandtes

Der ältere **pro-Feld-Hinweis** (`scorekeeper_by_court`, „Verlierer des
Vorspiels auf diesem Feld" am Tablet, `MatchBrief.scorekeeper`) bleibt
unverändert bestehen; die globale Warteschlange kommt additiv daneben.
