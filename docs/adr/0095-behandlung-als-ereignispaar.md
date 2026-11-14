# 0095 — Behandlung als Ereignispaar aus der Pause

- **Status:** accepted
- **Datum:** 2026-11-14

Ergänzt [ADR 0038](0038-ereignisse-append-only.md) (Undo-Ausnahme) und
[ADR 0043](0043-zettelblatt-nach-dbv-vorbild.md) (Marke „B", zwei Marken je Zelle).
Spec: [verletzungsunterbrechung](../features/verletzungsunterbrechung.md).

## Kontext

Eine Verletzungsunterbrechung (✚ am Tablet) soll mit Beginn, Ende und Dauer automatisch auf dem
Schiedsrichterzettel stehen, in jedem Modus. Bisher entstehen `InjuryStart`/`InjuryEnd` nur als
Handeintrag im Schiri-Modus. Mehrere Fragen haben je zwei tragfähige Antworten:

1. Wo entsteht die **Dauer** — im Renderer aus zwei Zeitstempeln oder als Feld am Ende-Ereignis?
2. Was macht ein **Undo** mit einer Behandlung? Nach ADR 0038 nimmt der Undo-Schnitt alle
   Ereignisse hinter der neuen Position zurück — auch eine Behandlung, die tatsächlich stattfand.
3. Welche **Zeitbasis** haben Ereignisse? Die Pausenuhr läuft in Server-Zeit, Ereignisse trugen
   Tablet-Zeit; über einen Tablet-Wechsel ergab das falsche, auch negative Dauern.
4. Wie verhindert man, dass eine neue Tablet-Seite (das Relay liefert sie beim Merge sofort aus)
   hinter einem **alten Turnier-PC** automatische Behandlungszeilen erzeugt, die dieser mit
   Spieler „A1" druckt — eine Gesundheitsangabe an der falschen Person?
5. Wie passt eine neue Marke ins Raster, wenn die Zelle nur **ein** Zeichen trägt?

## Entscheidung

1. **✚ und das Ende der Behandlung erzeugen `injury_start`/`injury_end`** in jedem Modus; der
   Abbruch aus der Behandlung und das Spielende „Aufgabe" erzeugen zusätzlich `retired`. Die
   Menü-Handeinträge werden ausgeblendet und später entfernt.
2. **Die Paarung macht der Renderer.** Wirksame Ereignisse in `(set, afterN, seq, id)`; ein Ende
   schließt den **jüngsten offenen** Beginn (`ts_ms` als Hilfsordnung nur bei gleicher Position).
   Beginn ohne Ende → „Dauer offen"; unplausible Dauer → „Dauer —". Keine Dauer auf dem Draht.
3. **Undo-Ausnahme:** `injury_start`/`injury_end` sind Zeit-Tatsachen. Der Schnitt nimmt sie nicht
   mit; liegen sie hinter dem neuen Schnitt, werden sie per Rücknahme + Kopie (gleicher `tsMs`)
   **neu verankert**. Passt das nicht unter den 64er-Deckel, unterbleibt es. Der Renderer zeigt
   eine verschobene Behandlung nur einmal (zurückgenommenes `Injury*` mit wirksamem Zwilling
   gleicher Art und `ts_ms` wird ausgeblendet) — Abweichung von „Zurückgenommenes erscheint
   durchgestrichen".
4. **`tsMs` neuer Ereignisse ist Server-Zeit** (`serverNow()`); die Ordnung bleibt positionsbasiert.
5. **Fähigkeitsmeldung über `MatchBrief.injuryGuideSecs > 0`** (zugleich die Richtzeit, Doppelrolle
   wie `shuttleCounter`). Nur dann erzeugt das Tablet automatische Ereignisse und verankert neu.
6. **Raster (Ergänzung ADR 0043):** Marke „B" nur am Beginn, in der Zelle bei Phase `play` oder
   `break_eleven`, sonst Rand. Eine Zelle trägt bis zu zwei Marken, Rangfolge `D > F > W > R > B`,
   Marke 1 rechts oben, Marke 2 links oben; Maße unverändert.
7. **Spieler-Spalte nach Positivliste** (`traegt_person`): nur Aufschlagfolge, Karten und
   Disqualifikation tragen eine Person; alle anderen Arten, auch künftige, bleiben leer.
8. **Richtzeit** ist eine Turnier-Konvention: Standard 300 s, geklemmt 60–1800 s, kein „aus";
   wirkt ab der nächsten Zuweisung (`CourtPushKey` bleibt unverändert).

## Alternativen

- **Dauer als Feld an `InjuryEnd`:** Wire-Änderung samt Relay-Deploy, zweiter ungeprüfter Wert,
  alte Handpaare blieben ohne Dauer, und die Dauer hinge am Gerät, das das Ende erfasst. Verworfen.
- **Behandlung beim Undo nur vom Schnitt ausnehmen:** Der Anker `afterN = N` bliebe stehen; nach
  dem Undo wird Ballwechsel N neu vergeben, „B" säße am Ballwechsel **nach** der Behandlung.
  Verworfen.
- **Behandlung beim Undo mitnehmen (ADR 0038 unverändert):** löschte eine echte Tatsache vom
  Zettel. Verworfen.
- **Server-Zeit nur für Behandlungen:** zwei Zeitbasen auf einem Blatt. Verworfen.
- **Handeinträge sofort entfernen:** hinter einem alten Turnier-PC gäbe es vorübergehend keinen
  Erfassungsweg. Zeitlich gestreckt.
- **Person nach ✚ abfragen:** zusätzlicher Tipp in der Stresssituation und mehr Gesundheitsdaten.
  Verworfen.
- **Zwei Zeichen rechtsbündig in einem Kasten:** liefe in einen zweistelligen Zellwert. Verworfen.

## Konsequenzen

- Alte Handeinträge erhalten beim Nachdruck automatisch „B", leere Spieler-Spalte und Dauer.
- Jede Behandlung kostet zwei Ereignisse, jede Neuverankerung zwei je Ereignis; der Deckel bleibt
  ohne Vorrang.
- Der Zettel-Renderer muss in einem Release vor der Fähigkeitsmeldung stehen.
- Ein Tablet-Wechsel mit altem lokalem Stand kann eine laufende Behandlung verlieren („Dauer offen").
- Bei drei Marken an einem Ballwechsel steht „B" nur im Protokoll.
