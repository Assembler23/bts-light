# Spielplan an badhub (`sched`)

> Seit v0.9.209. Gegenstück in badhub: `features/spieler_live_turniertag.md`,
> Spezifikation: `docs/superpowers/specs/2026-08-16-spieler-live-vollstaendiger-spielplan-design.md`
> (badhub-Repo).

## Wozu

badhubs Spielerseite `/spieler/{lizenz}/live` zeigt einem Teilnehmer seinen
Turniertag: laufendes Spiel, **alle** kommenden mit Warteschlangen-Position,
angesetzter und prognostizierter Zeit, **alle** bereits gespielten.

Aus dem `tset` ging das nicht. Der kappt bei `UPCOMING_LIMIT = 15` kommenden
Spielen des **gesamten** Turniers — wessen Spiel weiter hinten in der
Reihenfolge liegt, taucht dort gar nicht auf. Am 16.08.2026 gegen ein echtes
Turnier gemessen: für eine Lizenz mit laufendem Spiel lieferte der Snapshot
null kommende und null vergangene Spiele.

## Warum ein zweiter Kanal

Den `tset` zu erweitern wäre einfacher gewesen, aber teuer: Er geht bei
**jeder** Liveticker-Änderung raus und trägt bereits das Base64-Turnierlogo.
Mehrere hundert Spiele darin würden den Liveticker für alle langsamer machen,
damit eine Spielerseite vollständig ist.

`sched` geht deshalb getrennt und **höchstens minütlich** (`SchedTakt` in
`sync.rs`). Der `tset` bleibt unverändert.

## Was gesendet wird

`build_sched()` in `badhub/payload.rs` — **alle** Spiele mit Teilnehmern, ohne
Kappung. Je Spiel:

| Feld | Quelle | Anmerkung |
|---|---|---|
| `_id`, `n`, `status` | wie im `tset` | `scheduled` · `oncourt` · `finished` |
| `p0`/`p1` + `*_member_ids` | `BtpMatch::team1/2` | Gäste kommen als **`null`**, nicht als `""` |
| `planned_ts` | BTP `PlannedTime` | als **Unix-ms**, nicht `YYYYMMDDHHMM` |
| `predicted_start_ts` | `tablet::predict` | **nur** bei `scheduled` |
| `queue_pos` | `resolve_and_sort_key` | 0-basiert, **innerhalb der Halle** |
| `hall` | `resolve_and_sort_key` | leer bei Ein-Hallen-Turnieren |
| `court` | `BtpMatch::court` | bei `scheduled` immer `null` |
| `hall_color` | `hall_colors::farbe_fuer` | dieselbe Quelle wie im `tset` |
| `discipline` | `BtpMatch::discipline` | `mens_singles`, `womens_doubles`, … |
| `class_label` | `BtpMatch::class_label` | „A", „B", „U15"; `null`, wenn keins erkennbar |
| `sets`, `team1_won`, `end_ts`, `outcome` | wie im `tset` | |

**Warum `discipline` und `class_label` mitmüssen (seit v0.9.211):** `n` ist
`draw_name + round_name` — und `draw_name` ist bei Gruppenturnieren die
AUSLOSUNGSGRUPPE („Gruppe 1"), nicht die Klasse. badhub zeigte deshalb nur
„Gruppe 1 G1" und konnte die Disziplin nicht ableiten, egal was es tat. Beide
Felder lagen hier längst vor und wurden nur nicht gesendet. Zusammen ergeben
sie „HE A".

**Die Reihenfolge ist die aus ADR 0023** — dieselbe, die die Turnierleitung
sieht. Eine eigene Sortierung für badhub würde Zuschauer ans falsche Feld
schicken; das ist derselbe Grund, aus dem `upcoming()` sie schon nutzt.

**`planned_ts` ist lokale Wandzeit → Unix-ms.** BTP liefert `YYYYMMDDHHMM` in
der Zeit des Rechners, der in der Halle steht (dasselbe `Local`-Muster wie
`btp/proto.rs`). Ohne Zonen-Zuordnung läge im Sommer jede Anwurfzeit zwei
Stunden daneben. Bei mehrdeutiger Wandzeit (Zeitumstellung) wird **keine**
Zeit gesendet statt einer falschen — badhub zeigt das Feld dann nicht an.

**`predicted_start_ts` nur für wartende Spiele.** Bei einem laufenden oder
beendeten Spiel ist „wann bin ich dran" sinnlos, und ein stehengebliebener
Wert wäre schlimmer als keiner.

## Takt

`SchedTakt::faellig(jetzt_ms)` gibt höchstens alle 60 Sekunden `true`.

**Warum ein Zeittakt und keine Änderungserkennung** (anders als beim
Check-In-Roster): Der Spielplan selbst ändert sich selten, die enthaltene
Prognose aber mit jedem beendeten Spiel. Eine Änderungserkennung löste damit
faktisch bei jedem Poll aus. 60 Sekunden liegen unterhalb der Prognosegüte von
±10 Minuten (`spielzeiten-prognose`, E12) und sind nicht wahrnehmbar.

Bewusst **nicht** gebaut: ein Sofort-Versand bei Strukturänderungen. Er bringt
maximal 60 Sekunden und kostet einen zweiten gespeicherten Vergleichsstand.

## Fehlerverhalten

Wie der Check-In-Roster **additiv mit eigenem Fehlerpfad**: Der Spielplan ist
eine Zusatzinformation — geht er schief, muss der Liveticker trotzdem laufen.
Fehler ändern den `SyncOutcome` nicht.

Antwortet badhub mit **400/404**, kennt es den Nachrichtentyp noch nicht
(ältere Version). Dann wird für `CHECKIN_UNSUPPORTED_RETRY` pausiert statt
jeden Zyklus anzuklopfen — aber nicht für immer aufgegeben: derselbe Status
kann von einem kurzen Aussetzer während eines badhub-Deploys stammen, und ein
Turnier läuft über Tage.

## Der Vertrag ist getestet, nicht behauptet

`build_sched_haelt_den_feldvertrag_mit_badhub` prüft die serialisierten
Feldnamen gegen genau die Liste, die badhub liest — inklusive „kein
unbekanntes Feld". Die Gegenstelle hält dieselbe Liste als Fixture
(`tests/fixtures/sched_golden.json` im badhub-Repo). Ohne diesen Abgleich
bräche der Spielplan dort lautlos, weil badhub unbekannte Felder ignoriert.

`build_sched_kappt_nicht` fährt 20 geplante Spiele durch und verlangt 20 im
Payload — der Test würde rot, sobald jemand `sched` ein Limit gäbe und damit
den Zweck des Kanals aufhöbe.
