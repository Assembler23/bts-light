# 0059 — Setzplatz als fertiger Text, aufgelöst im Parser

- **Status:** proposed
- **Datum:** 2026-10-12

## Kontext

BTP liefert die Setzung je Meldung **und je Stage** an
`StageEntries/StageEntry` (`Seed1`, optional `Seed2`, gemessen am
12.10.2026). Viele Stellen sollen sie anzeigen:

- fünf Monitor-Seiten (LAN und Cloud über das Relay)
- das TL-Web
- die Desktop-Check-In-Seite
- später badhub (PHP)

Zu entscheiden waren drei Fragen: Wo wird die Setzung einer Spielseite
aufgelöst, in welcher Form reist sie über die Leitungen, und wo wirkt der
Anzeige-Schalter?

## Entscheidung

- **Auflösung im Parser.** `btp/model.rs` baut einmal je Snapshot einen
  Index und schreibt fertige Felder an `BtpMatch` (`team1_seed`,
  `team2_seed`) und `BtpEntry` (`seed`).
  - Regel: Setzung der Stage des Spiels (`Draw.StageID`), sonst die der
    Stage mit `StageType 1`, sonst keine.
  - `BtpEntry.seed` kommt nur aus `StageType 1`.
- **Wire-Form als Text ohne Klammern:** `"1"`, `"3/4"`. Formatiert wird an
  genau einer Stelle (`seed_text`). Jeder Abnehmer setzt nur die Klammern
  `[…]`.
- **Schalter:**
  - TL-Web: Der Schalter wirkt **im Browser** (`hide_seeds`, invertiert,
    fehlendes Feld = sichtbar). Die Setzung reist immer mit.
  - Monitore: haben keinen Schalter.

## Alternativen

- **Auflösung lazy in den Ansichten** (Index am `BtpSnapshot`): verworfen.
  - Sie betrifft 66 Snapshot-Literale.
  - `MonitorCourt`, `match_brief` und `winners::placement` kennen den
    Snapshot nicht und bräuchten neue Parameter.
  - Am Parser reist das Feld dagegen von allein über alle Wege mit.
- **Strukturiert `{seed1, seed2}` über die Leitungen:** verworfen. Fünf
  Abnehmer in drei Sprachen müssten dieselbe Regel (Normalisierung,
  Ausreißer nur `Seed2`) nachbauen und würden auseinanderlaufen. Sortieren
  oder Filtern nach Setzung ist kein Ziel.
- **Schalter am Host** (Feld wird gestrichen): verworfen. Ein Umschalten
  erreicht die Cloud nur über einen Push, und der Push-Schlüssel kennt den
  Schalter nicht. Das Muster `hide_open_matches` wirkt ebenfalls im
  Browser.
- **Kein Rückfall aufs Hauptfeld** (streng): vom Nutzer verworfen. Ein
  Playoff-Eintrag ohne eigene Setzung soll die Hauptfeld-Setzung zeigen.

## Konsequenzen

- **Literale:** 31 `BtpMatch`- und 11 `BtpEntry`-Literale müssen die neuen
  Felder tragen.
- **Keine Zahl als Wert:** Wer später nach Setzung sortieren will, braucht
  ein zusätzliches numerisches Feld.
- **Cloud-Push:** Die Setzung gehört in den Push-Schlüssel von `push_court`
  (`CourtPushKey`), sonst bleibt eine Neusetzung in der Cloud liegen.
- **Teil-Patch:** `istPatchbar` der Übersicht muss die Setzung
  mitvergleichen.
- **Neuer Datenschutz-Fall:** Die Setzung ist eine weitere Angabe im
  TL-Zustand. Sie ist öffentlich und steht im Wächter-Test.
