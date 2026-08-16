# TP-Network-Protokoll (BTP / BLP)

Clean-Room-Spezifikation des TP-Network-Protokolls von Visual Reality /
tournamentsoftware.com, abgeleitet aus beobachtetem Verhalten und öffentlicher
Doku. Grundlage für die Rust-Implementierung in `src-tauri/src/btp/`.

Kein Code aus phihag/bts wurde übernommen – siehe [NOTICE.md](../NOTICE.md).

## Transport

- **TCP**, Port **9901** (BTP, Einzelturniere) bzw. **9911** (BLP, Liga/Team).
- BTP läuft als Server, der Client verbindet sich.
- **Jeder Request ist eine eigene, kurzlebige TCP-Verbindung.** Der Server
  antwortet mit genau einer Nachricht und schließt dann die Verbindung. Es gibt
  keine persistente, gemultiplexte Session.
- Timeouts (Referenzwerte): Connect 5000 ms, Read/Idle 10000 ms.

## Frame-Format

```
[ 4-Byte Längen-Header ][ Payload ]
```

- Header: 4 Bytes, **signed i32, Big-Endian**.
- Der Header-Wert ist die Länge **nur des Payloads** – er zählt sich selbst
  nicht mit. Gesamtframe = `4 + payload_len`.
- Payload: gzip-komprimiertes UTF-8-XML (siehe unten).
- **Toleranz beim Lesen:** Echte BTP-Server senden gelegentlich einen falschen
  Längenwert. Der Reader soll bei Abweichung dem tatsächlich empfangenen
  Byte-Count vertrauen statt hart zu scheitern.

## Kompression

- **gzip** (mit gzip-Wrapper: Magic `1f 8b`, Header, DEFLATE-Body, CRC32, ISIZE).
  Kein raw deflate, kein bare zlib (`78 9c`).
- **Beide Richtungen** komprimiert – Request- wie Response-Payload.
- Kein Sonderfall für kleine/leere Payloads: jeder Request wird gzip-komprimiert.

## VISUALXML

Der Payload ist ein XML-Dokument:

```xml
<?xml version="1.0" encoding="UTF-8"?><VISUALXML VERSION="1.0">...</VISUALXML>
```

Zwei Strukturelemente unter dem Root, beide mit `ID`-Attribut (logischer
Feldname):

- **`<GROUP ID="...">`** – Container/Objekt. Kinder sind weitere `GROUP`/`ITEM`.
  Auch für **Listen**: N gleichnamige `GROUP`-Elemente = Liste mit N Einträgen.
- **`<ITEM ID="..." TYPE="...">`** – skalarer Leaf-Wert.

| TYPE | Kodierung | Dekodiert als |
|---|---|---|
| `String` | Text-Inhalt | String |
| `Integer` | Text-Inhalt, Basis 10 | Integer |
| `Float` | Text-Inhalt | Float |
| `Bool` | Text-Inhalt `true`/`false` | Boolean |
| `DateTime` | ein Kind-Element `<DATETIME>` (kein Text) | strukturiertes Datum |

**DateTime:** `<ITEM TYPE="DateTime" ID="..."><DATETIME .../></ITEM>` mit
Attributen am `<DATETIME>`:

| Attr | Bedeutung |
|---|---|
| `Y` | Jahr (4-stellig) |
| `MM` | Monat **1–12** |
| `D` | Tag |
| `H` | Stunde (24h) |
| `M` | Minute |
| `S` | Sekunde |
| `MS` | Millisekunde |

Achtung asymmetrisch: Monat = `MM`, Minute = `M`. Beispiel (Timestamp
1652529397790, Europe/Berlin):

```xml
<ITEM TYPE="DateTime" ID="test_date"><DATETIME Y="2022" MM="5" D="14" H="13" M="56" S="37" MS="790"/></ITEM>
```

Das Datum wird in der lokalen Zeitzone des Turniers kodiert.

**Listen/Verschachtelung:** Keine Array-Syntax. Eine Liste von N Matches sind N
`<GROUP ID="Match">`-Geschwister in einem `<GROUP ID="Matches">`. **Eine leere
Liste fehlt komplett** – der Container wird weggelassen, nicht leer gesendet.
Konsequenz: Jedes dekodierte Feld ist faktisch eine Liste (Konsumenten greifen
immer Element `[0]`).

## Nachrichten-Skelett

Objektform vor der XML-Kodierung:

```
Header  { Version { Hi:1, Lo:1 } }
Action  { ID: <action> [, Password: <pw>] [, Unicode: <session-key>] }
Client  { IP: "bts-light" }
```

- `Header.Version` = `Hi:1, Lo:1` – in **jeder** Nachricht, kein separater
  Handshake.
- `Client.IP` = freier Client-Identifier.
- `Action.Password` (`ITEM String`) nur wenn gesetzt, sonst weggelassen.

## Requests

Drei Action-IDs:

- **`LOGIN`** – Authentifizierung, liefert Session-Key.
- **`SENDTOURNAMENTINFO`** – kompletter Turnier-Snapshot.
- **`SENDUPDATE`** – ein Match-Ergebnis zurück nach BTP schreiben (siehe
  Abschnitt „Schreiben: SENDUPDATE").

`SENDUPDATE` benötigt zusätzlich `Action.Unicode` (Session-Key aus LOGIN) und
einen `Update`-Container.

## Login-Flow

1. TCP-Connect zu Port 9901 / 9911.
2. Sofort einen `LOGIN`-Request senden (ein vollständiger Frame).
3. Server antwortet mit einem Frame, dann schließt er die Verbindung.
   Auswertung:
   - `Action.ID` muss `"REPLY"` sein, sonst „ungültige Login-Antwort".
   - `Action.Result` muss Integer `1` sein, sonst „falsches Passwort".
   - Bei Erfolg: `Action.Unicode` = Session-Key, speichern.
4. Für `SENDTOURNAMENTINFO` / `SENDUPDATE` jeweils eine **neue** Verbindung
   öffnen.

## Response: SENDTOURNAMENTINFO

> **Leer-Snapshot-Guard** (`sync.rs`, seit Cluster A): BTP kann vereinzelt
> einen Abruf lang einen leeren Turnier-Stand liefern (Turnier-Befund
> 19.07.2026, u. a. während eines Gruppen-Umbaus in BTP). Ein Snapshot
> **ohne Matches direkt nach gefüllten Daten** wird deshalb verworfen und
> erst übernommen, wenn der Folge-Abruf ihn bestätigt — vorher ändert der
> Zyklus keinerlei Zustand (keine Feld-Freigabe, keine Auto-Vergabe, kein
> Liveticker-Push). Das Dashboard zeigt den verworfenen Abruf als
> orangene Warnung (kein Rot — der Guard heilt sich selbst).
> Bewusste Grenzen: Nach einem App-**Neustart** kennt der Guard noch
> keinen gefüllten Stand — ein Aussetzer exakt im allerersten Poll würde
> durchrutschen (akzeptiertes Restrisiko). BTP-**Verbindungsfehler**
> zwischen zwei leeren Abrufen setzen den Bestätigungs-Zähler nicht
> zurück (BTP hat zweimal „leer" gesagt — der Fehl-Poll dazwischen ändert
> daran nichts).

Struktur: `VISUALXML > Result > Tournament`. Top-Level-Container unter
`Tournament` (jeder ist eine `GROUP`, jeder optional – fehlt wenn leer):

- `Settings` → `Setting{ID,Value}` – Turniername = Setting mit `ID == 1001`.
  **Kein verlässliches Turnier-Startdatum:** Der echte Mitschnitt (Probe
  2026-08-11, Punktverlauf-Feature) enthält als einzigen Datums-Kandidaten
  Setting `1094`, einen OLE-Automation-Zeitstempel (Tage seit 1899-12-30,
  mit Uhrzeit-Bruchteil), der exakt der **Aufnahmezeit** des Mitschnitts
  entspricht — also „zuletzt gespeichert", nicht „Turnierbeginn".
  Setting `1005` trägt nur das Jahr (Integer). Wer ein Turnierdatum
  braucht (z. B. der Dateischlüssel des Punktverlaufs), stempelt deshalb
  das **Erstsichtungs-Datum** selbst (Fallback laut
  [Spec](features/punktverlauf-graph.md)).
- `Events` → `Event`
- `Draws` → `Draw`
- `Matches` → `Match`
- `PlayerMatches` → `PlayerMatch` – **nur im Liga-Modus**; Präsenz dieses
  Containers signalisiert Liga-Modus
- `Players` → `Player`
- `Entries` → `Entry`
- `Stages` → `Stage{ID, EventID, StageType}` – Turnierabschnitte je Event.
  `StageType`: **1 = Hauptfeld, 2 = Qualifikation, 8 = Playoff, 9998 =
  Reserve, 9999 = Ausschließen** (am Mitschnitt + laufendem Turnier
  gemessen). Der Typ ist stabil; der Stage-**Name** ist frei benennbar.
- `StageEntries` → `StageEntry{ID, EntryID, StageID, Status, Seed1?}` – die
  **maßgebliche Zuordnung Meldung → Stage** (Struktur-Probe 12.08.2026,
  `tests/checkin_roster_probe.rs`). Ein Turnier mit 424 Meldungen hat 424
  `StageEntry`-Einträge; darüber steht, ob eine Meldung im Hauptfeld,
  in Reserve oder in Ausschließen liegt. **`Entry.StageID` bleibt in
  echten Daten leer** — der Hauptfeld-Filter der Check-In-Liste liest
  deshalb primär `StageEntries` ([spieler-check-in.md](spieler-check-in.md)).
- `Courts` → `Court`
- `Locations` → `Location` – Standorte/Hallen, von `Court.LocationID` referenziert
- `Officials` → `Official` (Schiedsrichter)
- `Teams` → `Team` (Liga-Modus)

**Match:** `ID`, `DrawID`, `PlanningID`, `MatchNr`, `RoundName`, `IsMatch`
(Bool), `IsPlayable` (Bool), `From1`/`From2` (Feeder-PlanningIDs), `EntryID`,
`Winner` (1 oder 2), `Sets`, `PlannedTime` (DateTime), `CourtID`,
`Official1ID`, `Official2ID`, `Shuttles`, `DisplayOrder`. Liga-`PlayerMatch`
zusätzlich: `TeamMatchID`, `MatchTypeID`, `MatchTypeNo`, `MatchOrder`,
`Team1Player1ID`, `Team1Player2ID`, `Team2Player1ID`, `Team2Player2ID`.

### Wo gespielt wird: `Match.LocationID` — wenn das Turnier sie pflegt

**Ein angesetztes Spiel kann den Spielort tragen.** Das Feld heißt
`Match.LocationID` und verweist auf einen `Locations > Location`-Eintrag.
Am 09.08.2026 an einem laufenden Turnier gemessen: **48 Matches** trugen
eine `LocationID`, die meisten davon **ohne** jede Feldzuweisung — also
genau die Information, wo ein wartendes Spiel stattfinden soll.

bts-light liest sie seither (`BtpMatch::location_id`,
`assign::hall_for_match` mit `HallSource::Btp`).

#### Warum hier zuvor das Gegenteil stand

Der erste Befund (08.08.) lautete: „Es gibt keinen Spielort an der
Ansetzung." Er stützte sich auf zwei Mitschnitte, in denen **kein einziges**
Match eine `LocationID` trug — beide Turniere pflegten die Spalte schlicht
nicht. Der Schluss von „liegt in diesen Daten nicht vor" auf „gibt es nicht"
war zu weit. Die damals notierte Einschränkung („was ein Turnier liefert,
das die Spalte pflegt, ist unbeantwortet") war der Kern der Sache — und ist
jetzt beantwortet.

Für Turniere **ohne** gepflegten Spielort bleibt alles beim Alten; die
folgende Messung beschreibt genau diesen Fall:

- `Match` trägt **keine** `LocationID`. `CourtID` erscheint erst, wenn das
  Spiel auf dem Feld steht (im Zwei-Hallen-Mitschnitt bei 5 von 36
  Paarungen, je zusammen mit `StartTime`).
- `Draw`, `Event` und `Stage` tragen ebenfalls keinen Ortsbezug — ihre
  Felder sind Größe, Typ, Reihenfolge und Namen, sonst nichts.
- Die einzige Ortsangabe im ganzen Protokoll ist **`Court.LocationID`**: Ein
  *Feld* gehört zu einer Halle.

Der Spielplan-Export („Spiele von …") dieser Turniere hat die Spalten
**Feld** und **Spielort** — in allen 540 Zeilen leer. Genau das ist der
Unterschied zum Turnier oben: Wird die Spalte gepflegt, steht sie als
`Match.LocationID` im Mitschnitt; wird sie es nicht, fehlt sie ganz.

Für solche Turniere muss die Halle eines wartenden Spiels abgeleitet werden
— aus der Disziplin/Klasse→Halle-Regel, einer Handzuweisung der
Turnierleitung oder dem Vorbereitungs-Aufruf (`assign::hall_for_match`).
Ein Test in `btp_capture.rs` hält fest, dass es solche Turniere gibt, damit
die abgeleitete Kaskade nicht als überflüssig verschwindet.

**Das Messwerkzeug** dafür liegt in `tests/btp_location_probe.rs`: Es zählt
gegen ein laufendes BTP, welche Felder an Matches vorkommen, und zeigt
alles Ortsverdächtige. So lässt sich für jedes neue Turnier in Sekunden
klären, womit man es zu tun hat.

#### Schreibversuch gemessen (10.08.2026)

Der Lesebefund oben klärt nur, ob BTP eine `LocationID` **liefert** — nicht,
ob sie sich auch **setzen** lässt. Probe dazu: `tests/btp_location_probe.rs`,
Test `does_btp_accept_a_location_write_for_a_scheduled_match`. Sie schickt
gegen ein laufendes Test-BTP einen minimalen `SENDUPDATE`, der an einem
angesetzten Match ausschließlich `ID, DrawID, PlanningID, LocationID`
setzt.

**Befund:** BTP beantwortet den Schreibversuch mit `Result=1` (Erfolg) —
übernimmt den Wert aber nicht. Beleg: `LocationID` VORHER `None`, Ziel `1`
gesendet, NACHHER weiterhin `None`. Alle anderen Felder (`CourtID`, `Sets`,
`Winner`, `Status`) blieben dabei unverändert; der Restore-Schritt lief
ebenfalls erfolgreich und wurde verifiziert.

**Folgerung:** Eine Spielort-Rückschreibung nach BTP ist über diese
`SENDUPDATE`-Form nicht möglich — `Match.LocationID` verhält sich wie ein
serverseitig abgeleitetes, nur lesbares Feld. Allgemeiner gilt: `Result=1`
ist bei einem unbekannten Feld kein verlässliches Erfolgssignal — jeder
künftige Schreibpfad muss den Wert zur Kontrolle zurücklesen.

**Varianten-Matrix gemessen (11.08.2026, Test-BTP „TEST Köpi-Cup"):**
Die Messung oben deckte nur die minimale Form ab; drei weitere Formen
wurden daraufhin geprüft (`which_location_write_variant_sticks` in
`tests/btp_location_probe.rs`, wiederholbar gegen jedes Test-BTP):

| Variante | Ergebnis |
|---|---|
| LocationID **mit gespiegelter `PlannedTime`** (BTPs Ansetzungs-Dialog pflegt Zeit + Ort zusammen) | `Result=1`, still ignoriert |
| **Voller Match-Knoten gespiegelt** (ohne `Status`), nur LocationID ersetzt | `Result=1`, still ignoriert |
| LocationID **als String** (BTP typisiert gemischt) | `Result=1`, still ignoriert |

Alle Restores verifiziert, keine Nebenwirkungen. Zusammen mit der
Minimal-Form sind damit **vier** Schreibformen ausgeschlossen — und das,
obwohl `CourtID` in exakt derselben Knotenform nachweislich ankommt.

**Auch der „Feld-Trick" scheitert (gemessen 11.08.2026):** Die Idee war,
ein Feld zuzuweisen (das kommt an, und am Feld hängt die Halle) und es
danach wieder zu entfernen — in der Hoffnung, die abgeleitete
`Match.LocationID` bliebe stehen. Messung
(`does_assign_then_free_leave_the_location_behind`, ebenda): Die
Zuweisung setzt **gar keine** `Match.LocationID` — selbst während das
Spiel mit `CourtID` und Status `OnCourt` auf dem Feld steht, bleibt sie
leer; nach voller wie halber Freigabe ebenso. `Match.LocationID` wird
also **nicht** vom Feld abgeleitet, sondern kommt ausschließlich aus
BTPs eigener Spielort-Planung (die „Spielort"-Spalte). Es gibt auf dem
Draht nichts, das kleben bleiben könnte.

**Selbst die letilo/bts-Schreibform scheitert (gemessen 11.08.2026).**
Der Vorgänger `letilo/bts` (Branch `feat/multilocation`,
`btp_proto.js` `update_request`) schreibt `Match.LocationID` **zusammen
mit `Status`, `Highlight` und `DisplayOrder`** — der vollen
Planungsraster-Identität, die alle vier Proben oben wegließen. Diese
fünfte Form 1:1 nachgebaut
(`location_write_full_planning_node_like_letilo`): `Result=1`, aber
`LocationID` bleibt `None` — still ignoriert, Status verifiziert
zurückgestellt. **Wichtig:** letilo verlässt sich dort auf `Result=1`
als Erfolg und markiert das Match als synchronisiert — bemerkt also gar
nicht, dass BTP den Ort verwirft. Der Vorgänger „konnte" den
Spielort-Rücksync also nur scheinbar; tatsächlich ist er nie angekommen.

Der Wire-Weg ist damit **vollständig ausgereizt** (fünf Schreibformen +
Feld-Trick, alle still verworfen, während `CourtID` im selben Knoten
ankommt): `Match.LocationID` ist über den Connector **nur lesbar**. Was
bleibt: Spielort-Pflege in BTP selbst (bts-light liest sie bereits
automatisch), eine Messung gegen eine neuere BTP-Version (Proben
wiederverwendbar) oder eine Anfrage an Visual Reality, das Feld im
Connector schreibbar zu machen. Deshalb bleibt die Hallen-Festlegung in
TL-Web (`TlAction::SetHall`) bewusst host-lokal (bts-light + Liveticker)
statt nach BTP zurückzuschreiben.

### Officials: Struktur & Schreibweg (gemessen 13.08.2026)

Messgrundlage für das [Schiedsrichtermanagement](features/schiedsrichter-management.md)
(Probe: `tests/btp_officials_probe.rs`, Test-BTP „TEST Köpi-Cup" mit drei
gepflegten Schiedsrichtern; ADR
[0021](adr/0021-officials-ruecksync-eigenstaendiger-write.md)):

- **Struktur:** `Officials > Official{ID, Name, FirstName, Country}` —
  **kein Verein/ClubID** auf dem Draht, auch nachdem in BTP am
  Schiedsrichter gepflegt wurde (nur `Country` kam hinzu; BTP lässt leere
  Felder generell weg). Der Verein eines Schiedsrichters muss also in
  bts-light gepflegt werden.
- **Semantik:** `Match.Official1ID` = Schiedsrichter, `Match.Official2ID`
  = Aufschlagrichter (am Testturnier gegen die BTP-Maske verifiziert).
  Beide Felder erscheinen nur, wenn gepflegt.
- **Schreibweg — anders als `LocationID` funktioniert er:** Ein
  `SENDUPDATE` mit `Match{ID, DrawID, PlanningID, Official1ID,
  Official2ID}` (bewusst **ohne `Status`**, Check-in-Bits-Falle) wird
  **übernommen** — eigenständig (V1) genauso wie eingebettet in die
  Feldzuweisungs-Form mit `CourtID` + `Courts`-Block (V2). Löschen per
  Wert `0`. Keine Nebenwirkungen an `CourtID`/`Sets`/`Winner`/`Status`;
  Restores verifiziert.
- **Asynchrone Übernahme:** Die Antwort ist sofort `Result=1`, aber ein
  unmittelbar folgender `SENDTOURNAMENTINFO` zeigt noch den alten Stand;
  nach ≤1 s steht der neue Wert. Schreibpfade müssen also tolerant
  zurücklesen (Poll statt Einmal-Check) — die erste Messfassung ohne
  Poll-Schleife hatte deshalb fälschlich „ignoriert" gemeldet.

bts-light liest die Officials seither: `BtpOfficial` +
`BtpSnapshot::officials` (`official_list`, fehlender Container ⇒ leere
Liste) und `BtpMatch::official1_id`/`official2_id` (0 gilt als „nicht
gesetzt", wie bei `LocationID`) in
[`btp/model.rs`](../src-tauri/src/btp/model.rs).

### Die Reihenfolge der angesetzten Spiele

**`PlannedTime` + Auslosung (`DrawID`) ergeben zusammen die gedruckte
Spielliste.** Beides ist nötig, und beides war lange falsch bzw. gar nicht
ausgewertet:

- **`PlannedTime` ist ein `ITEM` vom Typ `DateTime`**, dessen Wert der
  Knoten `<DATETIME Y="2027" MM="2" D="5" H="9" M="0" …/>` ist — Attribute,
  keine Kind-Knoten, und die Kurznamen `D`/`H`/`M` statt `Day`/`Hour`/
  `Minute`. Wer nach Kind-Knoten sucht, findet **nie** etwas und hält jedes
  Turnier für unangesetzt.
- **Alle Spiele eines Zeitfensters tragen dieselbe Zeit** — ein ganzer
  Vormittag steht auf 9:00. Die Reihenfolge *innerhalb* des Fensters gibt
  die Auslosung (`DrawID`) vor, dann die Spielnummer. Ohne einen der beiden
  Schlüssel läuft die reine Spielnummer quer: Aus der gedruckten Liste
  (Nr 2, 6, 2, 6 …) wird „erst alle Nummer 2, dann alle Nummer 6".
- **Bis 09.08.2026 stand hier `DisplayOrder`** statt `DrawID`. Gemessen an
  einem echten Turnier trugen aber nur rund 10 % der Spiele einen Wert — die
  ohne landeten hinter allen anderen, das erste Spiel des Tages stand
  plötzlich an fünfter Stelle. `DrawID` ist an jedem Match gesetzt und
  reproduziert dieselbe Reihenfolge zuverlässig.

Belegt an einem echten Turnier (878 Paarungen, 759 davon angesetzt): Die
Sortierung `PlannedTime → DrawID → MatchNr → ID` reproduziert die aus BTP
exportierte Spielliste **Position für Position**.

Implementierung: `parse_planned_time` in
[model.rs](../src-tauri/src/btp/model.rs), `sort_key`/`sort_key_parts` in
[assign.rs](../src-tauri/src/tablet/assign.rs).

**Diese eine Definition gilt überall**, wo Spiele in eine Reihenfolge
kommen — sonst zeigt jede Ansicht eine andere „nächste Begegnung", und
niemand weiß mehr, welche stimmt:

| Wo | Über |
|---|---|
| Automatische Feldvergabe | `sync.rs` → `assign::resolve_and_sort_key` |
| Turnierleitungs-Oberfläche (Warteliste) | `tablet/tl.rs` → `assign::resolve_and_sort_key` |
| Vorbereitungs-Kandidaten (Desktop) | `commands.rs` → `assign::resolve_and_sort_key` |
| Vorbereitungs-Kandidaten (Tablet/Monitor) | `tablet/server.rs` → `assign::resolve_and_sort_key` |
| **Liveticker „anstehende Spiele"** | `badhub/payload.rs` → `assign::resolve_and_sort_key` |

Der Liveticker sortierte bis 12.08.2026 **allein nach Spielnummer** — die
Ansicht mit den meisten Augen zeigte damit eine Reihenfolge, die im
Turnierplan nirgends stand. Bei nur 15 Einträgen konnten die tatsächlich
nächsten Spiele sogar ganz herausfallen.

**Seit 14.08.2026 (Spec `spielliste-manuelle-reihenfolge`, ADR 0023)**
kennt jede der fünf Stellen zusätzlich einen **manuellen Präfix** — seit
15.08.2026 **hallenübergreifend** statt je Halle getrennt (ADR 0026, löst
die Hallen-Frage von ADR 0023 ab). `assign::resolve_and_sort_key` bündelt
Hallen-Auflösung (`assign::hall_for_match`, weiterhin nötig für die
Anzeige) + globalen Präfix-Rang-Nachschlag
(`tablet/queue_order.rs::QueueOrderStore::rank`) +
`assign::sort_key_with_manual_order` zu **einem** verpflichtenden Helfer —
genau der Punkt, den diese Tabelle seit jeher schützen soll. Ein
Cross-Site-Regressionstest (`tests/queue_order_consistency.rs`, inkl.
Zwei-Hallen-Szenario) vergleicht TL-Web, Desktop und Liveticker
gegeneinander. `DisplayOrder` selbst lässt sich **nicht** nach BTP
zurückschreiben (siehe Abschnitt weiter unten) — die manuelle Reihenfolge
bleibt daher rein host-lokal.

Nicht betroffen: die Liste der **in Vorbereitung gerufenen** Spiele
(`build_prepared_list`). Sie folgt der Reihenfolge der Aufrufe — wer
zuerst gerufen wurde, steht oben, und das ist dort die richtige Ordnung.

### `DisplayOrder` zurückschreiben — geht nicht

Gemessen am 14.08.2026 (`btp_displayorder_probe.rs`,
`does_btp_accept_a_displayorder_write_for_a_scheduled_match`): Ein
`SENDUPDATE`, der an einem angesetzten, noch nicht zugewiesenen Match
ausschließlich `ID, DrawID, PlanningID, DisplayOrder` schreibt, liefert
`Result=1` — aber der nachfolgende Snapshot zeigt weiterhin den alten
Wert. **Stiller No-Op, exakt dasselbe Verhalten wie bei `LocationID`**
(siehe oben). Eine manuelle Spiel-Reihenfolge lässt sich also **nicht**
nach BTP zurückschreiben; sie kann nur lokal in bts-light geführt und den
fünf Sortier-Stellen aus der Tabelle oben vorgeschaltet werden.

**Player:** `ID`, `Firstname`, `Lastname`, `Asianname` (wenn gesetzt → Anzeige
`NACHNAME Vorname`), `Country` (Nationalität), `GenderID` (1 = m, 2 = w),
`MemberID` (Lizenznummer, Format `08-012002`), `ClubID` (→ `Clubs`),
`CheckedIn`/`FirstCheckIn` (Bool), `LastTimeOnCourt` (DateTime).

`MemberID` und `ClubID` sind **optional** und in vielen Turnieren leer (im
Fixture-Mitschnitt fehlen beide) — Auswertungen dürfen sich nicht darauf
verlassen. `CheckedIn`/`FirstCheckIn` hängen **am Spieler und gelten
turnierweit**; sie können „in Klasse A anwesend, in Klasse B noch nicht" nicht
abbilden (siehe [features/spieler-check-in.md](features/spieler-check-in.md)).
Kein Geburtsjahr auslesen oder speichern — Projektregel.

**Court:** `ID`, `Name`, `LocationID` (→ `Location`, ordnet das Feld einer
Halle/einem Standort zu), `MatchID`, `SortOrder` (BTP-Sortierreihenfolge).

**Location:** `ID`, `Name`. Bei Ein-Hallen-Turnieren genau eine („Main
Location"); bei mehreren Hallen je ein Eintrag, `Court.LocationID` zeigt
auf den jeweiligen.

**Official:** `ID`, `Name`, `FirstName` (Schreibweise weicht von Player ab),
`Country`. **Mehr nicht** — insbesondere keine `ClubID` (gemessen
13.08.2026, siehe „Officials: Struktur & Schreibweg"). Am Match:
`Official1ID` = Schiedsrichter, `Official2ID` = Aufschlagrichter.

**Event:** `ID`, `Name`, `GameTypeID` (1 = Einzel, 2 = Doppel),
`GenderID` (1 = Herren, 2 = Damen, 3 = Mixed).
**Stage:** `ID`, `Name`, `EventID`, **`StageType`**, `DisplayOrder`. Der
`StageType` ist der stabile Schlüssel (der Name ist frei benennbar) —
gemessen an beiden Turnier-Mitschnitten: **1 = Hauptfeld,
2 = Qualifikation, 8 = Playoff, 9998 = Reserve, 9999 = Ausschließen**.
Der Hauptfeld-Filter der Check-In-Meldeliste hängt daran
(`non_main_stage_entries()` in [`btp/model.rs`](../src-tauri/src/btp/model.rs),
[docs/spieler-check-in.md](spieler-check-in.md)).
**Draw:** `ID`, `Name`, `EventID`, `StageID`, `DrawTypeID`, `DrawSize`,
`Position`, `DisplayOrder` u. a. — über `StageID` hängt der Draw an seiner
Stage.
**Entry:** `ID`, **`EventID`**, `Player1ID`, `Player2ID` (zweiter Spieler nur
bei Doppel). Eine `StageID` am Entry wurde in den Mitschnitten (nur
Hauptfeld-Meldungen) **nie beobachtet**; da BTP leere Felder generell
weglässt, ist offen, ob sie bei Reserve-/Quali-Meldungen erscheint — der
Parser liest sie opportunistisch (s. o.).

> **`Entry.EventID` ist die Meldeliste.** Ein `Entry` kennt seine Klasse
> **direkt** — unabhängig davon, ob für sie schon eine Auslosung existiert.
> Wer die Teilnehmer einer Klasse **vor** der Auslosung braucht, geht also
> `Entries → Entry.EventID → Event`, **nicht** über die Slot-Kette unten
> (die setzt Matches voraus). Belegt am Mitschnitt
> `tests/fixtures/btp-tournament-2halls.bin`. Der Parser
> ([`btp/model.rs`](../src-tauri/src/btp/model.rs) `entry_map`) wertet heute
> nur `EntryID → PlayerIDs` aus und verwirft die `EventID` — für die
> Teilnehmer-Auflösung eines Matches reicht das, für eine Meldeliste nicht.
**Team:** `ID`, `Name`.

Die **Disziplin** eines Matches ergibt sich aus dem Event seines Draws:
`Match.DrawID → Draw.EventID → Event{GameTypeID, GenderID}`. Der Draw-Name
allein (z. B. „Gruppe A") trägt sie nicht.

## Score

Satz-Ergebnisse hängen am Match unter `Sets`:

```
Match.Sets → GROUP "Sets" mit N × GROUP "Set"
jedes Set  → ITEM "T1" (Integer), ITEM "T2" (Integer)
```

`T1`/`T2` = Punkte von Seite 1/2 in diesem Satz. Reihenfolge = Spielreihenfolge.
Kein Satz-Sieger-Flag – wird aus den Punkten abgeleitet. `Winner` (1/2) ist ein
separates Match-Feld.

## Teilnehmer-Auflösung (From → Slot → Entry → Player)

`Matches` enthält zweierlei Einträge:

- **Teilnehmer-Slots** – tragen `PlanningID` + `EntryID`, aber kein
  `IsMatch=true`. Sie ordnen einer Planungsposition einen `Entry` zu.
- **Echte Paarungen** – tragen `IsMatch=true` und verweisen über
  `From1`/`From2` auf die `PlanningID` zweier Slots. Jede Round-Robin-Paarung
  taucht zusätzlich gespiegelt (ohne `IsMatch`) auf; diese Spiegel werden
  verworfen.

Auflösungskette einer Paarung:

```
Match.From1 → Slot.PlanningID → Slot.EntryID → Entry.Player{1,2}ID → Player
```

**Wichtig – PlanningIDs sind nur pro Draw eindeutig.** BTP vergibt in jedem
Draw dieselben Slot-PlanningIDs (1000, 2000, 3000 …). Der Slot-Lookup muss
daher mit `(DrawID, PlanningID)` geschlüsselt werden; `From1`/`From2` zeigen
immer auf einen Slot im selben Draw wie das Match. Ein globaler, nur über
`PlanningID` geschlüsselter Lookup lässt Slots verschiedener Draws
kollidieren – Folge: Paarungen lösen zu fremden Spielern auf ("Hilde gegen
Hilde"). In einem 116-Draw-Turnier waren so 95 % aller Teilnehmer falsch.

In einem KO-Draw bekommt eine beendete Paarung selbst eine `EntryID` (den
Sieger) zugewiesen und wirkt damit als Feeder-Slot für die nächste Runde –
derselbe `(DrawID, PlanningID)`-Lookup deckt das mit ab.

## Schreiben: SENDUPDATE

`SENDUPDATE` schreibt ein Match-Ergebnis zurück nach BTP – die Grundlage
für den digitalen Spielzettel (Tablet → bts-light → BTP).

Request-Aufbau (zusätzlich zum Nachrichten-Skelett):

```
Action  { ID: "SENDUPDATE", Unicode: <session-key> [, Password: <pw>] }
Update {
  Tournament {
    Courts {                        (nur bei Tablet-Ergebnis: Feldfreigabe)
      Court { ID: <BTP-Court-ID> }  (Court OHNE MatchID = Feld frei)
    }
    Matches {                       (bei Liga stattdessen PlayerMatches)
      Match {
        ID:          <BTP-Match-ID>
        Sets { Set { T1, T2 } ... } (ein Set-Knoten je Satz, Spielreihenfolge)
        Winner:      1 | 2
        ScoreStatus: 0              (0 = regulär; 1/2/3 = Walkover/Aufgabe/Disq.)
        Duration:    <Minuten>      (Spieldauer seit dem 1. Aufruf, ganze Minuten)
        Status:      0
        CourtID:     <BTP-Court-ID> (das ECHTE Feld bleibt am Match — s. u.)
        DrawID:      <Draw des Matches>
        PlanningID:  <Planungsposition im Draw>
        Official1ID: <SR-ID>         (nur bei Schiedsrichter-Betrieb — s. u.)
        Official2ID: <AR-ID>
      }
    }
    Players {                       (nur bei Tablet-Ergebnis: Spielende je Spieler)
      Player {
        ID:              <BTP-Player-ID>
        LastTimeOnCourt: <DateTime, lokale Uhrzeit des Spielendes>
        CheckedIn:       false      (Spieler wieder für die Planung verfügbar)
      }
    }
  }
}
```

- Das Match wird über `ID` + `DrawID` + `PlanningID` adressiert.
- `Sets` enthält je Satz einen `Set`-Knoten mit `T1`/`T2` (Punkte Team 1/2).
- **`CourtID` bleibt das echte Feld** (seit v0.9.147): BTP zeigt so am
  beendeten Spiel, WO es lief. Die Freigabe des Felds übernimmt allein der
  `Courts`-Block (Court ohne MatchID = frei) — genau wie im Original-BTS
  (letilo-bts `btp_proto.js`). `CourtID: 0` zu schreiben (so der frühere
  Stand) löschte die Feld-Info am Match (Tilo-Feedback 18.07.2026).
- **`Duration`** ist die **Bruttozeit** (Spec
  [`features/spielzeiten-prognose.md`](features/spielzeiten-prognose.md)):
  erste Feldzuweisung des Matches bis zum Ergebnis-Eingang, in ganzen
  Minuten. Quelle ist seit ADR 0027 der persistente Zeiten-Store
  (`match-times.json`) — die Dauer überlebt damit App-Neustart und
  Feldwechsel; `on_court_since` (RAM) bleibt nur Fallback, solange der
  erste Sync-Poll noch nicht gestempelt hat. Auch das manuelle
  Backend-Ergebnis und die TL-Web-Wertung senden so eine echte Dauer
  (früher 0). 0 nur noch, wenn wirklich kein Startzeitpunkt bekannt ist —
  und **bewusst immer 0 beim Walkover** (kampflos wurde nicht gespielt).
- **`Players`-Block = Spielende-Uhrzeit:** BTP kennt kein „Spielende" am
  Match — Tilos Mechanismus setzt je Spieler `LastTimeOnCourt` (lokale
  Uhrzeit) und `CheckedIn: false` (wieder einplanbar). Entfällt beim
  Walkover aus der Turnierleitung (niemand stand auf dem Feld) und für
  Spieler ohne bekannte BTP-PlayerID.
- Antwort wie beim Login: `Action.ID = "REPLY"`, Erfolg bei
  `Action.Result == 1`.
- Jeder `SENDUPDATE` läuft über eine eigene, frische TCP-Verbindung.

> ⚠️ **`Status` niemals aus dem Ergebnis-Request entfernen.** Ohne dieses
> Feld schließt BTP das Match **nicht** ab: Die Sätze sind nach Doppelklick
> sichtbar, aber die Turnierleitung muss je Spiel manuell den Sieger wählen
> und speichern (Live-Befund Zwei-Hallen-Turnier 17.07.2026). Das
> Original-BTS schreibt `Status` in jedem Ergebnis-Update mit
> (letilo-bts `btp_proto.js`). Regressionsgeschichte: v0.9.103 entfernte
> `Status` zu Recht aus der **Feldzuweisung** (`court_assign_request`,
> Check-in-Bits der Spieler) — und versehentlich auch hier.
>
> **Ergebnis + Feldfreigabe = EIN Request** (seit dem Fix): Der frühere
> zweite SENDUPDATE mit „nacktem" Match-Knoten (nur `ID`+`CourtID=0`)
> konnte das gerade geschriebene Ergebnis wieder entwerten. Bei Walkover
> aus der Turnierleitung (`free_court_id = None`) entfallen `Courts`-Block
> und `CourtID`.

> ⚠️ **`Official1ID`/`Official2ID` niemals aus dem Ergebnis-Request weglassen,
> sobald Schiedsrichter-Betrieb läuft** (Live-Befund 14.08.2026, dieselbe
> Fehlerklasse wie `Status` oben). Ohne dieses Feld löschte BTP die
> Schiedsrichter-Besetzung eines Matches bei **jedem** Ergebnis-Eintrag —
> egal ob die Zuweisung Sekunden oder über eine Stunde alt war. Fix:
> `MatchUpdate::officials` reassertiert die aktuell bekannte Besetzung
> (BTP-Wert gewinnt, sonst lokale Zuweisung, sonst explizit `(0, 0)`) in
> jedem Ergebnis-Write. `None` nur ohne Schiedsrichter-Betrieb — dann bleibt
> der Request unverändert zum Bestand. Details:
> [schiedsrichter-management.md](schiedsrichter-management.md#ergebnis-write-löschte-die-besetzung-live-befund-14082026-fortsetzung).

### Vorbereitungs-Aufruf-Highlight (P1)

`highlight_request` (proto.rs) schreibt ausschließlich `Match.Highlight`
(1 = aufgerufen, 0 = nicht mehr), Match-Knoten NUR mit Identität
(`ID`/`DrawID`/`PlanningID`) — **kein** `Status` (dieselbe Check-in-Falle wie
oben) und keine Ergebnisfelder. Der Sync-Loop (`sync.rs`,
`reconcile_highlights`) gleicht die Menge gerufener, noch ruf-barer Spiele
gegen den zuletzt geschriebenen Stand ab und schreibt **nur den Diff** — also
gar nichts, solange sich nichts ändert. So sieht die Turnierleitung „in
Vorbereitung"-Aufrufe direkt im BTP-Planer (Vorbild Original-BTS); beim Ruf
aufs Feld / Rücknahme / Spielende fällt das Match aus der gewünschten Menge
und bekommt `Highlight:0`. Wie BTP das Highlight darstellt, ist einmalig am
echten BTP gegenzuprüfen.

**Voraussetzungen / Caveats:**

- BTP muss Netzwerk-Edits zulassen (Einstellung im BTP) – sonst antwortet
  es mit `Result != 1`.
- Kein Konflikt-Check: „last write wins". Ein zwischenzeitlich in BTP
  manuell geändertes Ergebnis wird überschrieben.
- Liga-Matches (`PlayerMatches`, Port 9911) sind noch nicht abgedeckt –
  sie tragen statt `DrawID`/`PlanningID` Felder wie `TeamMatchID`,
  `MatchTypeID`, `Team1Player1ID` usw.
- Implementierung: [src-tauri/src/btp/proto.rs](../src-tauri/src/btp/proto.rs)
  (`update_request`, `parse_update_response`, `MatchUpdate`).

## Offen: Was macht BTP beim Überschreiben einer Wertung?

**Muss an einem Test-BTP beantwortet werden, bevor die Ergebnis-Korrektur in
der Turnierleitungs-Oberfläche freigeschaltet wird** (Schritt 12 der
[TL-Web-Spec](features/turnierleitung-web.md), offener Punkt 1). Bis dahin
lehnt der Host jedes `overwrite` mit „noch nicht freigeschaltet" ab.

**Warum die Frage zählt.** Eine beendete KO-Paarung bekommt selbst eine
`EntryID` — den Sieger — und wirkt damit als Feeder-Slot der nächsten Runde
(siehe oben). Der Sieger steht also **sofort** im nächsten Spiel. Eine
strenge Auslegung („Nachfolger existiert und ist besetzt → nicht
korrigierbar") hieße damit praktisch: nur im Finale und in Gruppen. Deshalb
muss man wissen, ob BTP beim Überschreiben den Baum **selbst neu rechnet**.

**Vorbefund an echten Daten (08.08.2026).** Ein Mitschnitt aus einem
laufenden Turnier („TEST Köpi-Cup", 878 echte Paarungen) zeigt: Von den
9 bereits gewerteten Spielen hatten **alle 9** ein Folgespiel im selben
Draw. Die konservative Regel sperrt die Korrektur dort also in **100 %**
der Fälle — die Sorge aus der Spec ist damit keine Theorie. Ohne das
Experiment bleibt die Ergebnis-Korrektur in der Turnierleitungs-Oberfläche
praktisch wirkungslos, und die Turnierleitung muss weiter in BTP wechseln.

### Aufbau

Ein Test-Turnier mit einem KO-Draw für vier Teilnehmer (zwei Halbfinals, ein
Finale) und einer Gruppe mit drei Teilnehmern. Namen frei erfunden — das
Turnier wird nicht veröffentlicht.

Nach **jedem** Schritt einen Mitschnitt ziehen und durchnummeriert ablegen:

```powershell
.\tools\capture-btp.ps1 -Password "<TP-Network-Passwort>"
# btp-tournament.bin wegkopieren, z. B. nach ov-1-hf1-gewertet.bin
```

### Die Versuche

| # | Handlung | Zu beobachten |
|---|---|---|
| 1 | HF1 werten (A gewinnt) | Bekommt die HF1-Paarung eine `EntryID`? Welche? Steht A im Finale? |
| 2 | HF1 **überschreiben** (B gewinnt), Finale noch **nicht** aufgerufen | Ändert sich die `EntryID` der HF1-Paarung auf B? Steht jetzt B im Finale — oder bleibt A stehen? Antwortet `SENDUPDATE` überhaupt mit `Result=1`? |
| 3 | Finale auf ein Feld legen (läuft), dann HF1 überschreiben | Wird der Baum trotzdem umgerechnet, während das Folgespiel läuft? Bleibt das Finale auf dem Feld? |
| 4 | Finale werten, dann HF1 überschreiben | Was passiert mit der Wertung des Finales? Bleibt sie stehen, wird sie verworfen, wird sie widersprüchlich? |
| 5 | In der **Gruppe** ein bereits gewertetes Spiel überschreiben | Werden `Rank`, `SetRatio`, `GameRatio` der Tabelle neu gerechnet? |
| 6 | Ein Spiel überschreiben, das in BTP von Hand geändert wurde | Bestätigt „last write wins" auch beim Überschreiben? |

Bei jedem Versuch zusätzlich festhalten: Was zeigt die **BTP-Oberfläche**
danach an — und stimmt sie mit dem überein, was über die Schnittstelle kommt?

### Zwischenstand des Experiments (08.08.2026, „TEST Köpi-Cup")

Ausgeführt über `src-tauri/tests/btp_overwrite_experiment.rs` (schreibt
per `SENDUPDATE`, wie bts-light selbst). **Gemessen:**

1. **BTP nimmt Überschreib-Requests an** — `Result=1`, keine Ablehnung.
2. **Wirken tun sie nicht immer.** Bei einem Spiel, das BTP selbst gewertet
   hatte, wechselte `Winner` von 1 auf 2. Bei einem Spiel, das derselbe
   Versuch kurz zuvor gewertet hatte, blieb `Winner` auf 1 — **trotz
   `Result=1`**. Ein Überschreiben kann also stillschweigend wirkungslos
   sein, und der Erfolgscode sagt darüber nichts.
3. **Das Folgespiel wurde nie besetzt** — auch nicht, nachdem *beide*
   Vorgänger gewertet waren: `EntryID` leer, Teilnehmerzahl 0/0. Die
   Annahme aus der Teilnehmer-Auflösung („eine beendete Paarung bekommt
   selbst eine `EntryID` und wird zum Feeder-Slot") trifft hier **nicht**
   zu.
4. Nebenbei: `ScoreStatus` verschwand nach dem Überschreiben aus der
   Antwort (`0` → Feld fehlt).

**Damit ist die Kernfrage offen.** Ob BTP den Baum neu rechnet, ließ sich
nicht messen, weil in diesem Turnier gar nichts umzurechnen war — kein
Folgespiel war je besetzt. Was noch fehlt: ein Draw, in dem BTP die
nächste Runde nachweislich füllt (also ein weiter fortgeschrittenes
Turnier oder ein Draw-Typ, bei dem die Auflösung greift), und darin die
Versuche 2 bis 4.

Befund (2) ist unabhängig davon wichtig: Eine Korrektur darf sich **nicht**
auf `Result=1` verlassen. Sie muss nachlesen, ob der Sieger wirklich
gewechselt hat, und andernfalls sagen, dass nichts geschehen ist.

### Auswertung

1. Die aussagekräftigsten Mitschnitte als Fixtures nach
   `src-tauri/tests/fixtures/` legen und mit einem Test in
   `btp_capture.rs` einfrieren — so ist das Verhalten dokumentiert, auch
   wenn kein BTP zur Hand ist.
2. Ergebnis in ein **eigenes ADR** gießen (welche Fälle die Oberfläche
   freigibt und warum), dann `plan_result_action` entsprechend öffnen.
3. Zeigt sich, dass BTP den Baum **nicht** neu rechnet, bleibt die
   Korrektur auf die Fälle beschränkt, in denen es nichts umzurechnen gibt:
   kein Nachfolger, oder Gruppen-Auslosung.

## Fehlerfälle

- **Falsches Passwort:** LOGIN liefert trotzdem `Action ID="REPLY"`, aber
  `Result != 1`.
- **`Result`** ist der generische Status-Indikator in `Action`-Antworten;
  `1` = Erfolg.
- **Verbindungsabbruch:** Socket-Error / Timeout / vorzeitiges `end`.
- **Malformed Frame:** zu wenige Bytes (`< 4`) oder gunzip-Fehler.
- Es gibt keine In-Band-Fehlertexte über numerische `Result`-Codes hinaus.

### Nachschub-Queue für Ergebnis-Writes (Cluster A5)

Schlägt ein Ergebnis-`SENDUPDATE` fehl (BTP nicht erreichbar oder
`Result != 1`), landet der komplette `MatchUpdate` in einer
Nachschub-Queue (je Match ein Eintrag, neuester Stand gewinnt). Der
Sync-Loop schiebt die Einträge nach, sobald BTP wieder antwortet —
frühestens alle 30 s (Tilos `needsync`-Prinzip, aber **periodisch**
statt nur beim Reconnect; bei Tilo bleiben fachliche Rejects bis zum
nächsten Socket-Fehler liegen). Schutzregeln beim Nachschub:

- **Nie überschreiben:** Kennt BTP für das Match inzwischen ein Ergebnis
  (z. B. von der Turnierleitung manuell nachgetragen), wird der Eintrag
  verworfen.
- **Spieler-Checkout nur binnen 5 min seit Spielende** (Tilos Guard):
  danach geht das Ergebnis OHNE Players-Block raus — späte Replays
  dürfen Spieler nicht erneut auschecken/umstempeln.
- **Feld-Freigabe nur, solange das Feld laut Snapshot noch dieses Match
  trägt** — sonst räumte das Replay einem neu belegten Feld die frische
  Zuweisung weg.
- Einträge älter als 24 h verfallen.

Das Tablet wiederholt seine Übermittlung unabhängig davon selbst; gelingt
sie, wird der Queue-Eintrag entfernt und der Flush prüft das direkt vor
jedem Write erneut. **Race-Selbstheilung:** Geht während eines
(hängenden) Nachschub-Writes eine Korrektur direkt durch, hätte der
ältere Stand sie überschrieben — der Flush erkennt das (Vermerk der
erfolgreichen Direkt-Writes) und schreibt die neuere Korrektur sofort
erneut; schlägt auch das fehl, wird sie wieder eingereiht. Ein doppelter
*identischer* Write ist unschädlich (Players-Block setzt Werte, er
toggelt nichts). Die Queue lebt im Speicher — ein App-Neustart leert sie
(das Tablet hält sein Ergebnis ohnehin bis zum `ok:true`). Bei bestätigt
leerem Turnier-Stand (Leer-Snapshot-Guard) pausiert der Nachschub.

### Officials schreiben (Umsetzung ab v0.9.201, überarbeitet v0.9.202)

Eine Wire-Form für beide Anlässe (`proto.rs::court_assign_request` mit
`MatchCourt`-Knoten): `Match{ID, DrawID, PlanningID, CourtID,
[Official1ID, Official2ID]}`, `0` löscht Dienst bzw. Feldzuordnung. Ohne
`Status` (Check-in-Bitfeld, Regression v0.9.103) und ohne Ergebnisfelder.
BTP übernimmt asynchron (≤ 1 s) — zurückgelesen wird über den nächsten
Snapshot, nicht per Einmal-Check.

- **eigenständig** (`sync.rs::reconcile_officials`) — leerer `Courts`-Block,
  nur `Matches`, `CourtID` immer aus dem aktuellen Snapshot mitgeschrieben
  (`m.court_id.unwrap_or(0)`).
- **eingebettet** beim Ruf aufs Feld — zusätzlich der `Courts`-Block
  (`MatchCourt::officials` additiv).

**Race mit einer frischen Feldzuweisung, behoben (Live-Befund 14.08.2026,
verschärft/bestätigt nach einem verworfenen Zwischenfix).** Ursprünglich
schickte die eigenständige Form **kein** `CourtID`-Feld (Muster
`officials_request`, mittlerweile entfernt). Folgte dieser Write binnen
Sekunden auf ein `court_assign_request` **desselben** Matches, verlor BTP
dabei die eben erst angekommene `CourtID` wieder — zwei `SENDUPDATE`s zum
selben Match in enger Folge brachten BTPs eigene Persistenz durcheinander.

Ein erster Fix (feste Karenzzeit, 10 s) reichte nicht: Am selben Turnier
gemessen lag der tatsächliche Abstand zwischen Feldzuweisung und
eigenständigem Schiedsrichter-Write teils bei 11–18 s. **Der eigentliche
Fix:** Die eigenständige Form schreibt die `CourtID` jetzt immer mit — dann
ist die Reihenfolge zweier Requests zum selben Match folgenlos, egal wie
knapp oder weit sie zeitlich auseinanderliegen. Details:
[schiedsrichter-management.md](schiedsrichter-management.md#courtid-immer-mitschreiben-live-befund-14082026).
