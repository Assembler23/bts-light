# 0052 — Offene Plätze: Kandidaten aus einer Ebene, neutraler Rückfall

- **Status:** accepted
- **Datum:** 2026-08-30

## Kontext

Zeigt die Turnierleitungs-Oberfläche Spiele mit noch offenen Plätzen (Spec
[`tl-offene-paarungen`](../features/tl-offene-paarungen.md)), muss dort etwas
stehen. Der Wunsch war „Viertelfinale: Spieler A vs. Spieler B/Spieler C".

Was BTP dafür hergibt, ist weniger, als es zunächst aussieht:

- `Match.From1`/`From2` zeigen auf eine **Slot-`PlanningID`**, nicht auf ein
  Spiel. Das Vorspiel findet man erst über `planning_id == from1` **im selben
  `draw_id`** (Muster: `tl.rs::correction_blocker`).
- Findet sich dort nichts, ist der Slot ein Setzplatz, ein Freilos oder eine
  Speisung über Draw-Grenzen (`StageEntries`) — dann gibt es weder Kandidaten
  noch eine Spielnummer.
- Bei Platzierungsspielen („3/4") speist der **Verlierer** des Vorspiels den
  Slot. Welche Seite es ist, sagt BTP nicht direkt.
- `match_num` ist `Option` — auch ein gefundenes Vorspiel hat nicht immer eine
  Nummer.
- Kandidatennamen sind Spielernamen von Personen, die dieses Spiel womöglich
  nie bestreiten, und reisen wegen [ADR 0051](0051-offene-spiele-eigene-gedeckelte-liste.md)
  unabhängig vom Anzeige-Schalter mit.

## Entscheidung

Ein offener Platz wird über eine dreistufige Kaskade beschriftet:

1. **Kandidaten** — die Teilnehmer des direkten Vorspiels, getrennt durch
   **„ oder "**: `Müller oder Schmidt`. Der Schrägstrich bleibt dem Doppelpaar
   vorbehalten (`Müller/Meier oder Schmidt/Klein`).
2. **`aus Spiel 42`** — wenn das Vorspiel gefunden wird, aber selbst noch offen
   ist und eine `match_num` trägt. Bewusst **neutral**, nie „Sieger aus".
3. **`noch offen`** — wenn kein Vorspiel gefunden wird oder die Spielnummer
   fehlt.

**Genau eine Auflösungsebene**, keine Rekursion. Die Labels tragen **keine
Lizenznummern, keine Spieler-IDs, keinen Verein und kein Geburtsjahr**, und sie
heißen im Zustand spezifisch (`open_slot1_label`), damit die flache
Feldnamen-Whitelist des Wächter-Tests greift.

## Alternativen

- **„Sieger aus 42"** — der naheliegende Wortlaut, in der Mehrheit der Fälle
  richtig, an Platzierungsspielen aber nachweislich falsch. Verworfen: Eine
  Turnierleitung, die einmal eine falsche Behauptung liest, misstraut der
  Anzeige danach dauerhaft — genau das Problem, das dieses Feature lösen soll.
- **Sieger/Verlierer aus dem Draw-Aufbau erschließen** — präziser, verlangt
  aber eine Herleitung, die BTP nicht liefert und die bei exotischen Draws
  still falsch liegt. Verworfen: stille Fehler sind teurer als ein knapper
  neutraler Text.
- **Rein neutrale Herkunft ohne Namen** („aus Spiel 42" / „noch offen") —
  löste Datenschutz, Rekursionstiefe und Trennzeichen in einem Zug. Verworfen,
  weil der eigentliche Nutzen für die Turnierleitung darin liegt zu sehen, WER
  kommen könnte.
- **Rekursive Auflösung über mehrere Ebenen** — bei vier möglichen Kandidaten
  wird die Zeile unlesbar und die Aussage wertlos. Verworfen.

## Konsequenzen

- Die Anzeige behauptet nie etwas, das BTP nicht hergibt.
- **Negativ:** In frühen Turnierphasen steht an vielen Zeilen nur „noch offen"
  — der Informationsgewinn wächst erst mit dem Turnierverlauf.
- **Negativ:** Kandidatennamen erreichen auch Geräte mit ausgeschaltetem
  Schalter. Abgefedert dadurch, dass sie hinter dem Gerätezugang stehen, keine
  IDs tragen und demselben Zweck dienen wie die übrigen Wartelisten-Namen.
- Die Slot→Match-Auflösung wird in `docs/btp_protocol.md` festgehalten, damit
  die nächste Stelle sie nicht erneut herleiten muss.
