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
- **`SENDUPDATE`** – Ergebnis zurückschreiben (für bts-light Phase 1 nicht nötig).

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

Struktur: `VISUALXML > Result > Tournament`. Top-Level-Container unter
`Tournament` (jeder ist eine `GROUP`, jeder optional – fehlt wenn leer):

- `Settings` → `Setting{ID,Value}` – Turniername = Setting mit `ID == 1001`
- `Events` → `Event`
- `Draws` → `Draw`
- `Matches` → `Match`
- `PlayerMatches` → `PlayerMatch` – **nur im Liga-Modus**; Präsenz dieses
  Containers signalisiert Liga-Modus
- `Players` → `Player`
- `Entries` → `Entry`
- `Courts` → `Court`
- `Officials` → `Official` (Schiedsrichter)
- `Teams` → `Team` (Liga-Modus)

**Match:** `ID`, `DrawID`, `PlanningID`, `MatchNr`, `RoundName`, `IsMatch`
(Bool), `IsPlayable` (Bool), `From1`/`From2` (Feeder-PlanningIDs), `EntryID`,
`Winner` (1 oder 2), `Sets`, `PlannedTime` (DateTime), `CourtID`,
`Official1ID`, `Official2ID`, `Shuttles`, `DisplayOrder`. Liga-`PlayerMatch`
zusätzlich: `TeamMatchID`, `MatchTypeID`, `MatchTypeNo`, `MatchOrder`,
`Team1Player1ID`, `Team1Player2ID`, `Team2Player1ID`, `Team2Player2ID`.

**Player:** `ID`, `Firstname`, `Lastname`, `Asianname` (wenn gesetzt → Anzeige
`NACHNAME Vorname`), `Country` (Nationalität).

**Court:** `ID`, `Name`, `MatchID`, `SubMatchID`.

**Official:** `ID`, `Name`, `FirstName` (Schreibweise weicht von Player ab),
`Country`.

**Event:** `ID`, `Name`, `GameTypeID` (1 = Einzel, 2 = Doppel).
**Draw:** `ID`, `Name`, `EventID`. **Entry:** `ID`, `Player1ID`, `Player2ID`.
**Team:** `ID`, `Name`.

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

## Fehlerfälle

- **Falsches Passwort:** LOGIN liefert trotzdem `Action ID="REPLY"`, aber
  `Result != 1`.
- **`Result`** ist der generische Status-Indikator in `Action`-Antworten;
  `1` = Erfolg.
- **Verbindungsabbruch:** Socket-Error / Timeout / vorzeitiges `end`.
- **Malformed Frame:** zu wenige Bytes (`< 4`) oder gunzip-Fehler.
- Es gibt keine In-Band-Fehlertexte über numerische `Result`-Codes hinaus.
