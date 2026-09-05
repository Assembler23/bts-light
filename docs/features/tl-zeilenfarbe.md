# Zeilenfarbe aus BTP in der Turnierleitungs-Sicht — Spezifikation

> Status: **abgestimmt 2026-09-05** (Brief → Messung → Entwurf, Freigabe im
> Gespräch). Quelle: Nutzer-Anforderung vom 05.09.2026. Betroffene Crates:
> `src-tauri`, `relay-proto`, `relay` (nur Neubau).
> ADR: [0056](../adr/0056-zeilenfarbe-btp-fuehrt-aufrufmarke-weicht.md).

## Kontext / Problem

> Im BTP Programm kann ich Zeilen auch mit einer Farbe markieren, biete diese
> Funktion auch in der Turnierleitungssicht (Web) an, im … Button.

BTP hat im Kontextmenü eines Spiels den Eintrag **„Hervorheben"** mit sechs
Farben plus „keine". Die Turnierleitung nutzt das als freie Notiz am Spiel:
„Spieler angesprochen", „wartet auf Schiri", „Presse will das sehen" — was
die Farbe bedeutet, vereinbart das Team am Turniertag. Wer die
Turnierleitungs-Seite auf dem Tablet bedient, sieht diese Farben bisher
nicht und kann sie nicht setzen; er muss dafür an den BTP-PC.

## Messung (05.09.2026, laufendes BTP, `tests/btp_highlight_probe.rs`)

- Jedes `Match` trägt ein Feld **`Highlight`** (Integer). In den beiden
  Archiv-Mitschnitten steht überall `0`.
- Der Wert ist ein **Index**, kein Farbwert. Zuordnung am echten Planer per
  Screenshot-Pixelmessung:

  | Wert | Farbe im BTP-Planer | Hex |
  |---|---|---|
  | 0 | keine | – |
  | 1 | Gelb | `#FFD838` |
  | 2 | Pink | `#FF79AB` |
  | 3 | Orange | `#FBAD4B` |
  | 4 | Blau | `#1DACE6` |
  | 5 | Grün | `#5CE1B8` |
  | 6 | Lila | `#C56BFF` |

  Mehr als sechs Farben bietet das Menü nicht.
- **BTP nimmt den Wert per `SENDUPDATE` an** (Match-Knoten nur mit
  `ID`/`DrawID`/`PlanningID`/`Highlight`, wie der bestehende Aufruf-Marker
  P1): Setzen, Löschen und Zurücksetzen halten beim Nachlesen; keine
  Nebenwirkung am Check-in-`Status`. Der Planer **zeichnet die Farbe sofort**
  und behält sie auch, wenn danach im BTP an anderen Zeilen gearbeitet wird
  (Pink-Probe an Gruppe 1 Spiel 10, nach Bedienschritten im BTP nachgelesen).
- **Ungeklärt bleibt ein Einzelfall:** Ein Spiel, das bts-light um 09:01 Uhr
  automatisch auf ein Feld gelegt hatte, stand beim ersten Lesen auf `1`,
  wurde vom Planer weiß gezeichnet und stand eine halbe Stunde später auf
  `0` — ohne dass bts-light laut Log etwas geschrieben hätte. Die Spec geht
  deshalb nirgends davon aus, dass eine gesetzte Farbe ewig hält: **BTP ist
  die Wahrheit** (R2), die Seite zeigt, was der nächste Abruf liefert.

## Der bestehende Aufruf-Marker (P1) — die Falle

bts-light schreibt `Highlight` **schon heute**: Ein Vorbereitungs-Aufruf
setzt `1` (Gelb), das Ende des Aufrufs (Ruf aufs Feld, Rücknahme,
Spielende) setzt `0`. Der Abgleich (`sync.rs::reconcile_highlights`) kennt
nur „gerufen ja/nein" und würde eine von Hand gesetzte Farbe

1. beim Aufruf mit Gelb **überschreiben** und
2. am Ende des Aufrufs auf „keine" **löschen**.

Beides sind stille Datenverluste an der Stelle, die die Turnierleitung
gerade bewusst gepflegt hat. Entscheidung dazu: [ADR 0056](../adr/0056-zeilenfarbe-btp-fuehrt-aufrufmarke-weicht.md)
— **die Aufrufmarke weicht der Handfarbe.**

## Zielbild & Erfolgskriterien

1. Jede Zeile der TL-Spielliste (Warteliste **und** offene Paarungen) zeigt
   die BTP-Farbe als Zeilenhintergrund — dieselben sechs Töne wie im Planer,
   damit „das Orange" in beiden Programmen dasselbe Spiel ist.
2. Laufende Spiele zeigen die Farbe als Marke in der Feldkachel (die Kachel
   selbst trägt schon Hallenfarbe und Zustandsfarben; ein zweiter Vollton
   erschlüge das).
3. Im ⋮-Menü einer Zeile steht die Gruppe **„Hervorheben"** mit sieben
   Feldern: „keine" + sechs Farben. Ein Tipp schreibt die Farbe nach BTP.
   **Zwei Tipps** vom Öffnen bis zur gesetzten Farbe.
4. Die Farbe erscheint **sofort** in der eigenen Liste (nicht erst nach dem
   nächsten BTP-Abruf) — und verschwindet wieder, falls BTP sie nicht
   übernimmt.
5. Eine Handfarbe überlebt Aufruf und Aufruf-Ende (P1) unverändert.
6. Der Aufruf-Marker funktioniert an Spielen **ohne** Handfarbe wie bisher.
7. Ein Turnier-PC ohne diese Version zeigt die Gruppe gar nicht erst
   (Fähigkeitsmerkmal `can_set_highlight`, Muster `can_set_wish_court`).

## Verhalten

### Lesen

`BtpMatch::highlight: u8` aus `Match.Highlight`; Werte außerhalb 0–6 gelten
als 0 (unbekannte Farbe = keine — besser als ein falscher Ton). Der
TL-Zustand trägt `highlight` an `TlMatch`, `TlOpenMatch` und `TlCourt`
(`0` wird auf dem Draht weggelassen — kein Rev-Churn für die 1 700 Zeilen
ohne Farbe).

### Schreiben

`TlAction::SetHighlight { match_id, highlight }` (`highlight` 0–6). Der
Turnier-PC prüft: Spiel im aktuellen Stand bekannt, Wert im Bereich. Dann
ein `SENDUPDATE` über den bestehenden `write_highlight_to_btp` mit dem
freien Wert (die `HighlightEntry` trägt statt `on: bool` den Wert). BTP
nicht erreichbar → `BtpError`, nichts gemerkt, die Seite meldet es.

### Sofort sichtbar: das lokale Echo

Nach erfolgreichem Write merkt sich der Turnier-PC `(match_id, wert,
zeitpunkt)` — der Zeitpunkt ist der des **geglückten Writes**, nicht des
Aktionseingangs — und trägt den Wert sofort in den liegenden Stand ein.
Jeden folgenden BTP-Abruf legt `run_once` **einmal** durch das Echo
(`apply_highlight_echo`), bevor der Stand an Anzeige (`set_snapshot`) und
P1-Abgleich geht: Der Wert wird über den Abruf gelegt, solange das Echo
jünger als 20 s ist **und** BTP noch den alten Wert liefert; sobald BTP
denselben Wert zeigt — oder das Echo abgelaufen ist — fällt das Echo weg.
Eine rückwärts springende Uhr beendet das Echo (Muster der
Reservierungen). So sieht die Turnierleitung ihre Farbe sofort, und wenn
BTP sie wider Erwarten verwirft, ist sie nach spätestens 20 s ehrlich
wieder weg. Anzeige und Abgleich sehen immer denselben Wert.

Zugleich merkt sich der Turnier-PC das Spiel als **von Hand gefärbt**
(auch bei „keine"), solange es angesetzt ist — nur im Speicher.

### P1-Abgleich mit Handfarben (ADR 0056)

Ein gerufenes, rufbares Spiel gehört zur gewünschten Gelb-Menge, wenn

- es `0` trägt und **nicht** von Hand gefärbt wurde (Normalfall), oder
- es `1` (Gelb) trägt — Gelb an einem gerufenen Spiel gilt immer als
  Aufrufmarke, auch nach einem Neustart des Syncs mit leerem Merkbestand
  (sonst bliebe eigenes Gelb für immer stehen), oder
- der Merkbestand es schon kennt (dann bleibt es gewünscht, auch wenn die
  Turnierleitung es umgefärbt oder auf „keine" gestellt hat → kein
  erneutes Gelb).

Daraus folgt:

- **Aufruf:** Ein Spiel mit anderer Handfarbe behält sie. Ein bereits
  gelbes Spiel bekommt keinen Write, nur den Merkbestand.
- **Aufruf-Ende:** `0` wird nur geschrieben, wenn BTP für das Spiel noch
  `1` liefert. Steht dort inzwischen eine andere Farbe (jemand hat das
  gerufene Spiel umgefärbt), bleibt sie stehen — das Spiel fällt nur aus
  dem Merkbestand.
- Setzt die Turnierleitung an einem gerufenen Spiel bewusst „keine", bleibt
  es bei „keine" — über den Merkbestand (unser Gelb) oder die Hand-Marke
  (fremde Farbe, dann „keine").
- Hand-**Gelb** an einem gerufenen Spiel verschwindet mit dem Aufruf-Ende
  (bewusst, ADR 0056).

### Was **nicht** dazugehört (YAGNI)

- Keine Anzeige auf Hallenmonitoren, Zähl-Tablets oder im Liveticker: Die
  Farbe ist eine interne Notiz der Turnierleitung.
- Keine Farbwahl in der Desktop-App (Vorbereitungs-Panel): dort steht der
  BTP-PC daneben.
- Keine Bedeutungs-Legende („Orange = Presse"): Das vereinbart das Team,
  BTP hat sie auch nicht.

## Wire-Ebene

```json
{ "action": "set_highlight", "matchId": 4711, "highlight": 3 }
```

`TlMatch`/`TlOpenMatch`/`TlCourt` bekommen `"highlight": 3` (fehlt bei 0).
`TlState.can_set_highlight: true` vom neuen Host, `false` (Default) vom
alten. Der Relay trägt die Aktion nur durch (kompiliert gegen das neue
`relay-proto`; Deploy per `relay-deploy.yml` beim Merge).

## Tests

- `btp/model.rs`: `Highlight` wird gelesen, Bereichsgrenze (7 → 0).
- `btp/proto.rs`: `highlight_request` trägt den freien Wert.
- `sync.rs`: Handfarbe wird beim Aufruf nicht überschrieben; am Aufruf-Ende
  nur `1` gelöscht; Hand-„keine" am gerufenen Spiel bleibt „keine"; Gelb
  gilt auch mit leerem Merkbestand als unseres (kein Write, später gelöscht).
- `tablet/state.rs`: Echo überlagert den Snapshot, fällt bei Gleichstand,
  nach Ablauf oder bei rückwärts springender Uhr weg; Hand-Marke fällt mit
  dem Ansetzungs-Status.
- `tablet/tl.rs`: `highlight_entry_fuer` lehnt Wert 7 und unbekanntes Spiel
  ab, trägt die BTP-Identität; Vorgangskennung trennt „Orange" von „keine";
  Wächter-Test kennt `highlight`/`can_set_highlight`.
- `relay-proto`: Serde-Roundtrip der Aktion.
- Messwerkzeug `tests/btp_highlight_probe.rs` (ignoriert, braucht BTP).

## Doku-Pflicht

`docs/turnierleitung-web.md` (Bedienung), `docs/btp_protocol.md`
(Highlight-Werte, Messung), `docs/cloud-relay.md` (Wire), `docs/preparation.md`
(P1-Nachtrag), `docs/changelog.md`, CLAUDE.md-Tabelle.
