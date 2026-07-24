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

- `Settings` → `Setting{ID,Value}` – Turniername = Setting mit `ID == 1001`
- `Events` → `Event`
- `Draws` → `Draw`
- `Matches` → `Match`
- `PlayerMatches` → `PlayerMatch` – **nur im Liga-Modus**; Präsenz dieses
  Containers signalisiert Liga-Modus
- `Players` → `Player`
- `Entries` → `Entry`
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
`Country`.

**Event:** `ID`, `Name`, `GameTypeID` (1 = Einzel, 2 = Doppel),
`GenderID` (1 = Herren, 2 = Damen, 3 = Mixed).
**Draw:** `ID`, `Name`, `EventID`.
**Entry:** `ID`, **`EventID`**, `Player1ID`, `Player2ID` (zweiter Spieler nur
bei Doppel).

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
- **`Duration`** kommt aus dem Aufruf-Zeitstempel (`on_court_since`,
  1. Aufruf des Matches auf dem Feld) bis zum Ergebnis-Eingang, in ganzen
  Minuten; 0, wenn der Startzeitpunkt nicht bekannt ist (z. B. App-Neustart
  mitten im Spiel).
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
