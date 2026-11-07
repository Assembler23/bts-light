# Reihenfolge der gerufenen Partien (Meetingpoint / In Vorbereitung) — Spezifikation

> Status: **freigegeben** (07.11.2026; die Turnierleitung hat die Freigabe
> und alle Restentscheidungen ausdrücklich an die Umsetzung delegiert). Quelle: Rückmeldung der
> Turnierleitung vom 07.11.2026 („Leute stehen lange am Meetingpoint, obwohl
> ihr Spiel laut Warteliste später dran ist"). Durchlaufen: Brief → Grill
> (9 Blocker, alle beantwortet) → How-To. Vorarbeit: Bugfix #447 (v0.9.346,
> Meetingpoint-Seite zeigt die Vergabe-Reihenfolge).
> ADR: [0077 — Aufruf-Reihenfolge der Gerufenen](../adr/0077-aufruf-reihenfolge-der-gerufenen.md).
> Betroffene Crates: `src-tauri/`, `src/`, `relay-proto/` (neue TL-Aktion),
> `relay/` (leitet sie typisiert weiter).

## Kontext / Problem

Gerufene Partien („In Vorbereitung", zum Meetingpoint gerufen) kommen heute
nicht in der Reihenfolge aufs Feld, in der sie gerufen wurden:

- Die **Aufruf-Automatik** (`meetingpoint::auto_aufrufe`) geht die Spielliste
  von oben durch und **überspringt** Spiele, deren Spieler noch spielen, Pause
  haben, ausgenommen sind oder schon anderswo gerufen stehen.
- Die **Feld-Automatik** (`sync.rs auto_assign`) vergibt die Gerufenen nach
  dem Spielliste-Schlüssel (Präfix-Rang, Ansetzung, …), denn der Schlüssel
  kennt nur „gerufen ja/nein".
- Ein übersprungenes, später nachgeholtes Spiel **drängelt sich deshalb vor**
  alle, die schon am Meetingpoint stehen.
- Gerufene Partien sind **nicht umsortierbar** (`assign::ready_queue` lässt
  sie aus).
- Jeder Zug in der Warteliste **löscht** ihre Präfix-Einträge
  (`QueueOrderStore::reorder` baut den Präfix ohne sie neu auf). Ihre
  Reihenfolge springt dann, obwohl ein anderes Spiel gezogen wurde.
- Der **Cloud-Vorbereitungsmonitor** (`build_prepared_list`) folgt schon
  heute der Aufrufreihenfolge, der im LAN dagegen der Spielliste. Beide
  zeigen also verschiedene Folgen.

Der `MeetingpointStore` (`meetingpoint.json`) führt **alle**
Vorbereitungs-Aufrufe, auch bei ausgeschaltetem Meetingpoint, und das
bereits in Aufrufreihenfolge. Diese Reihenfolge fließt nur nirgends in die
Vergabe ein.

## Zielbild & Erfolgskriterien

**Unter den gerufenen Partien gilt eine Warteschlange:** Wer zuerst gerufen
wurde, kommt zuerst aufs Feld. Die Turnierleitung kann die Reihenfolge im
TL-Web und in der Desktop-App ändern. Vergabe und **alle** Anzeigen folgen
derselben Reihenfolge.

- Ein später nachgeholtes Spiel steht hinter den schon Wartenden, auf der
  Meetingpoint-Seite wie auf dem Feld.
- Ein Zug im Block der Gerufenen ändert die nächste Feldvergabe sofort. Die
  Anzeigen ziehen innerhalb ihres Takts nach (TL-Web ≤ 1 s, Meetingpoint ≤ 2 s,
  Liveticker ≤ 60 s).
- Ein Zug in der Warteliste lässt die Reihenfolge der Gerufenen unberührt.

## Regeln

1. **Neuer Aufruf → hinten.** Das gilt für jeden Aufruf, ob von der Automatik
   oder von Hand („In Vorbereitung rufen"), auch ohne eingeschalteten
   Meetingpoint.
2. **Platz bleibt** bei:
   - erneutem Aufruf in **dieselbe** Halle,
   - Parken (Partie geht aufs Feld) und Reaktivieren (Partie wird bestätigt
     wieder vom Feld genommen), so wie es der Store heute schon macht.

   **Hallenwechsel** stellt hinten an. Rücknahme und erneuter Aufruf stellen
   hinten an.
3. **Mehrfachruf** (mehrere Partien in einem Zug): Die Partien reihen sich in
   **Warteliste-Reihenfolge** an, nicht in Klickfolge.
4. **Aufruf löscht den Präfix-Eintrag.** War ein Spiel in der Warteliste nach
   oben gezogen, verliert es beim Aufruf seinen Präfix-Rang. „Nach oben
   ziehen" heißt damit „als Nächstes **gerufen**", nicht „als Nächstes aufs
   Feld". Nach einer Rücknahme steht es wieder auf seinem BTP-Platz.
5. **Aufruf-Automatik** überspringt weiterhin blockierte Spiele (keine
   Änderung daran, *welche* Spiele sie ruft). Holt sie ein Spiel später nach,
   gilt Regel 1.
6. **Feldvergabe:** Die erste **vergebbare** Gerufene in Aufruf-Reihenfolge
   bekommt das Feld. Weiter vorn bleibt eine Gerufene, deren Spieler wieder
   blockiert sind oder die im strikten Modus nicht vollständig eingecheckt
   ist. Sie ist beim nächsten Feld dran, ohne dass ein Feld für sie
   freigehalten wird.
7. **Vorrang vor der Reihenfolge** (unverändert, bewusst):
   - Ein **Wunschfeld** reserviert sein Feld für die Wunschpartie, auch vor
     früher Gerufenen.
   - Die **Hallenbindung** (ADR 0030) gilt weiter.
   - Ist **keine** Gerufene vergebbar, darf eine nicht gerufene Partie aufs
     Feld (Ein-Hallen-Turnier ohne strikten Modus, wie heute).
8. **Eine Liste, Züge nur innerhalb einer Halle.** Im Mehr-Hallen-Turnier ist
   die Reihenfolge eine gemeinsame Liste. Umsortieren geht aber nur zwischen
   Partien derselben Aufruf-Halle; ein Zug über die Hallengrenze wird mit
   Fehlermeldung abgelehnt.
9. **Kurz leere Paarung:** Liefert BTP einen Abruf lang leere Teams, bleibt
   der Aufruf samt Platz erhalten. Gelöscht wird erst nach
   `DEASSIGN_CONFIRM_POLLS` (3) Abrufen in Folge. Bei „beendet" und
   „verschwunden" wird wie heute sofort gelöscht.
10. **Vorläufiger Schiedsrichter** (ADR 0072) bleibt beim Umsortieren an
    seiner Partie.

## Bedienung (Turnierleitung)

**TL-Web.** Gerufene Zeilen der Spielliste bekommen einen eigenen Griff und
die Verschiebe-Knöpfe (an den Anfang, eins hoch, eins runter, ans Ende) für
den Block der Gerufenen. Wer auf eine nicht gerufene Zeile ablegt, schiebt
ans Ende der Gerufenen. Die Knöpfe erscheinen nur, wenn der Host
`can_reorder_called` meldet. Ist im Mehr-Hallen-Turnier der Hallenfilter aus,
wirken die Knöpfe nur auf Nachbarn derselben Halle.

**Desktop-App.** Im Block „In Vorbereitung" der Vorbereitungs-Seite gibt es
Ziehen und Knöpfe nach demselben Muster, gruppiert nach Aufruf-Halle.

**Am Ansage-Slave** gibt es weder Griffe noch Knöpfe. Die Vergabe läuft am
Master, ein Zug am Slave hätte keine Wirkung. Die TL-Aktion wird dort
abgelehnt („bitte am Turnier-PC umsortieren").

**Meetingpoint-Seite:** keine Bedienung (Nicht-Ziel). Sie zeigt die
Reihenfolge an.

Es gibt **keinen** Knopf zum Zurücksetzen der Reihenfolge der Gerufenen.

## Betroffene Komponenten / Architekturregeln / Daten

**Ordnungsquelle (ADR 0077):** Die Position eines aktiven Aufrufs in
`MeetingpointStore.calls` **ist** sein Aufruf-Rang. Es gibt kein zweites
Rangfeld und keine Mitbenutzung des Warteliste-Präfixes.

**Ein Schlüssel (R2-nah, „keine zweite Berechnung"):**
- In `assign.rs` wird `called: bool` in `sort_key_with_manual_order` /
  `resolve_and_sort_key` / `sort_key_for` zu `call_rank: Option<usize>`.
- `ManualOrderSortKey` = `(rank.is_none(), rank.unwrap_or(MAX), manual_rank,
  planned, display, draw, num, id)`.
- Der Compiler erzwingt alle Aufrufer: `sync.rs` (Vorverteilung,
  `auto_assign`), `tl.rs` (Warteliste), `commands.rs` (`preparation_candidates`),
  `server.rs` (LAN-Vorbereitungsmonitor), `meetingpoint.rs` (`bewertungen`).
  Nicht-Gerufene (`ready_queue`, `auto_aufrufe`) übergeben `None`.
- Der neue Helfer `assign::call_ranks(&[PreparationCall])` liefert die Ränge.
- Für den Liveticker gibt es den transienten Stempel
  `BtpMatch.preparation_call_rank`, gesetzt in `apply_preparation_calls`
  (derselbe Moment wie `preparation_call_ts`) und gelesen in `badhub/payload.rs`.
- `build_prepared_list` und `vorab_sr_verteilen` folgen der Vec schon und
  bleiben, wie sie sind.

**Aufruf zentral:**
- `TabletState::rufe_vorbereitung(call, auto)` führt `call_mit` aus, dann
  `queue_order_store().remove(id)` (neu) und `auto_hall_store().remove(id)`.
  `hand_aufruf` und der Automatik-Zweig in `sync.rs` nutzen die Methode.
- `reconcile_queue_order` nimmt aktive Gerufene aus `keep` heraus. Das ist
  das Sicherheitsnetz und räumt zugleich Altbestände.
- `hand_aufruf` sortiert die IDs eines Mehrfachrufs nach
  `sort_key_for(m, None, order)`.

**Umsortieren:**
- `MeetingpointStore::verschiebe(match_id, before: Option<i64>) ->
  Result<bool, VerschiebeFehler>` mit den Fehlern `NichtGerufen` (fehlt oder
  geparkt), `ZielNichtGerufen` und `AndereHalle`.
- Vor sich selbst ergibt `Ok(false)`. `None` heißt ans Ende.
- Verschoben wird nur das Element. `called_at_ms`, Check-ins und `vorab_sr`
  bleiben.
- Darüber liegt `TabletState::called_reorder` für TL-Web und
  Desktop-Command `called_reorder`.
- **Wire:** `relay-proto` `TlAction::CalledReorder { matchId,
  beforeMatchId? }` (JSON `called_reorder`), `TlState.can_reorder_called`
  (= `!slave_mode`), `PreparationView.can_reorder_called`.
- Ein Fehler wird als Fehlermeldung zurückgegeben, **nie** als stilles Ok.

**Migration (Update mitten im Turnier):**
- `MeetingpointFile.order_v` (`serde(default)`).
- Beim ersten Binden einer Altdatei (`order_v < 1`) ordnet
  `ordne_einmalig` die Vec einmal stabil nach dem **bisherigen**
  Vergabe-Schlüssel und setzt `order_v = 1`.
- Das passiert in `TabletState::binde_meetingpoint`, **vor** `auto_assign`
  und **vor** `reconcile_queue_order`, damit der Präfix noch da ist.
- Neue und leere Dateien tragen die Marke sofort. Eine unlesbare Datei
  migriert nicht.

**Persistenz:** Die Reihenfolge liegt in `meetingpoint.json` und übersteht
Neustart, Update und das Speichern der Einstellungen. Es gibt keinen Merker
nur in der SyncEngine.

**Cloud:** Der Relay leitet `CalledReorder` typisiert weiter und bekommt
keine eigene Logik. Deploy-Reihenfolge: Das Relay kennt die Aktion ab dem
Merge von PR 1 (automatischer Relay-Deploy). Die Seite zeigt die Bedienung
nur bei `can_reorder_called`, also erst ab einer App mit PR 1.

## Datenschutz

Keine neuen Daten. Die Reihenfolge ist eine Liste von Match-IDs.

## Akzeptanzkriterien

1. A wird vor B gerufen, B hat den besseren Präfix-Rang bzw. die frühere
   Ansetzung → das Feld bekommt **A**. Meetingpoint-Seite, TL-Web, Desktop,
   LAN- und Cloud-Vorbereitungsmonitor zeigen A vor B.
2. Die Automatik überspringt A (Spieler auf dem Feld), ruft B und C und holt A
   später nach → Reihenfolge B, C, A. Das nächste Feld bekommt B.
3. Hand-Aufruf von D bei schon gerufenen B, C → B, C, D.
4. Mehrfachruf von 12, 10, 11 (Klickfolge) bei Warteliste-Folge 10, 11, 12 →
   Reihenfolge 10, 11, 12.
5. Erneuter Aufruf in dieselbe Halle → Platz unverändert. Parken und
   Reaktivieren → Platz unverändert. Hallenwechsel → hinten. Rücknahme und
   erneuter Aufruf → hinten.
6. Spiel X in der Warteliste an den Anfang gezogen, dann gerufen → X steht
   hinter den schon Gerufenen. Nach Rücknahme steht X auf seinem BTP-Platz
   (kein Präfix-Rest).
7. Ein Zug in der Warteliste ändert die Reihenfolge der Gerufenen **nicht**
   (Regression zum heutigen Springen).
8. `CalledReorder` C vor B → Reihenfolge C, B. Die nächste Vergabe nimmt C.
   Das übersteht einen Neustart.
9. Fehlerfälle von `CalledReorder` liefern Fehlermeldungen und ändern nichts:
   - nicht gerufen,
   - Ziel nicht (mehr) gerufen, etwa weil es inzwischen auf dem Feld ist,
   - andere Halle,
   - am Slave.
10. Gerufene #1 mit wieder blockiertem Spieler bzw. im strikten Modus nicht
    vollständig → #2 bekommt das Feld, #1 bleibt vorn.
11. Wunschfeld: Die Wunschpartie (Rang 3) bekommt ihr reserviertes Feld vor
    Rang 1.
12. Altdatei ohne `order_v`, Vec [B, A], A vorn nach altem Schlüssel → nach
    dem Start [A, B], `order_v = 1`. Ein zweiter Start ordnet nicht erneut.
13. Leere Teams einen Abruf lang → Aufruf und Platz bleiben. Drei Abrufe in
    Folge → gelöscht.
14. Ohne eingeschalteten Meetingpoint gelten 1–13 für die Vorbereitungs-Aufrufe
    genauso.
15. Liveticker: Die nächsten Spiele folgen dem Aufruf-Rang.

## Tests

- **Unit (TDD):** Schlüssel in `assign.rs`; Store (`verschiebe`, Migration,
  Platz-Regeln, Leere-Teams-Zähler) in `meetingpoint.rs`; `auto_assign`-Fälle
  in `sync.rs` (Kriterien 1, 2, 10, 11); `tl.rs` Dispatch samt Fehlerfällen
  und Slave; `payload.rs`; `relay-proto`-Roundtrip; Relay-Weiterleitung.
- **Konsistenz:** `tests/queue_order_consistency.rs` erweitern, sodass
  TL-Web, Desktop, Liveticker, `bewertungen` und `build_prepared_list`
  dieselbe Folge liefern (zwei Hallen, Gerufene in Nicht-Präfix-Reihenfolge).
- **Migration im Abruf:** Integrationstest, dass `binde_meetingpoint` vor
  `auto_assign` und vor `reconcile_queue_order` läuft.
- **TL-Web:** Die Abbildung des Drop-Ziels als reine Funktion (`rufZiel`)
  mit Skript-Test. Drag im Browser-Harness (Fangverlust, ADR 0026).
- **Feldtest:** Turnier mit Meetingpoint und Aufruf-Automatik
  („nach Restzeit").

## Umsetzungs-Hinweise (aus dem How-To)

**PR 1, Kern und Wire, ohne sichtbare Bedienung:**
- Schlüssel und alle Aufrufer umstellen.
- Stempel `preparation_call_rank`, Konsistenztest.
- `rufe_vorbereitung`/Präfix weg, Mehrfachruf, Tests für die Platz-Regeln.
- `verschiebe`, `CalledReorder` samt Flag, Desktop-Command.
- Migration, Leere-Teams-Netz.
- Doku: `btp_protocol.md` Sortier-Definition, `spielliste-manuelle-reihenfolge.md`,
  `meetingpoint.md` (Satz aus #447 umdrehen), `preparation.md`,
  `cloud-relay.md`, CLAUDE.md-Tabelle, ADR 0077, Changelog.

**PR 2, Bedienung:**
- `tl.html` mit Griffen und Knöpfen für Gerufene je Halle.
- `PreparationPanel.tsx` und `api.ts calledReorder`.
- Handbuch-Abschnitte mit `pruef`-Ankern.

**Review:** Nach jedem PR läuft der code-reviewer. Der security-reviewer ist
nicht nötig, denn die einzigen Eingaben sind zwei geprüfte Match-IDs, und es
gibt keinen neuen Auth- oder Dateipfad.

## Risiken & Rollback

- **Breiter, aber mechanischer Diff** an den Sortierstellen. Der Compiler
  deckt sie ab, es droht aber Konfliktgefahr mit parallelen Branches.
- **Ziehen in einer gemischten Liste** (gerufen und wartend in einem
  Container) ist das größte UI-Risiko. Die Knöpfe sind der Rückfallweg.
- **Verhaltensänderung für Turniere ohne Meetingpoint:** Die Reihenfolge der
  „In Vorbereitung" gerufenen Partien folgt jetzt dem Aufruf statt der
  Spielliste. Das ist bewusst so (Brief, Antwort 5) und steht im Changelog.
- **Rollback:** Eine ältere App ignoriert `order_v` und sortiert wieder nach
  der Spielliste. Die Vec-Reihenfolge bleibt dabei unschädlich erhalten.

## Offene Fragen / Annahmen

- **Annahme:** Regel 9 (Leere-Teams-Netz) geht über die Antworten der
  Turnierleitung hinaus. Sie ist nötig, weil ein Flackern sonst den Platz
  kostet. Kann als eigenes Ticket herausgelöst werden.

## Betroffene Doku-Dateien

`docs/meetingpoint.md`, `docs/preparation.md`, `docs/turnierleitung-web.md`
(Handbuch) · `docs/btp_protocol.md`, `docs/features/spielliste-manuelle-reihenfolge.md`,
`docs/cloud-relay.md`, `docs/changelog.md`, `docs/roadmap.md`, CLAUDE.md-Tabelle ·
ADR 0077.
