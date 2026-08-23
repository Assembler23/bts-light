# 0045 — „Fertig, aber kein Ergebnis": Host stempelt, Seite rechnet

- **Status:** accepted
- **Datum:** 2026-08-23

## Kontext

Die Turnierleitungs-Sicht soll melden, wenn ein Spiel nach seinen Sätzen
entschieden ist, das Feld aber belegt bleibt (Spec
[tl-warnung-fertiges-spiel](../features/tl-warnung-fertiges-spiel.md)). Dafür
sind zwei Fragen unabhängig voneinander zu entscheiden.

**Frage A — wer beurteilt „entschieden"?** Die Seite hätte alles, was sie
bräuchte: `sets`, `best_of`, `target_score` und `cap_score` stehen im
TL-Zustand, und sie rechnet damit bereits Satz- und Matchball. Der Host hat
dieselben Daten und teilt sie mit dem Ergebnis-Pfad.

**Frage B — woher kommt „seit wann"?** Für eine Frist braucht es einen
Zeitpunkt. Er kann im Arbeitsspeicher liegen oder im persistenten
Zeitspeicher, den es für die Spielzeiten ohnehin gibt.

Erschwerend: Es existieren bereits **drei** Fassungen der Frage „ist der Satz
zu Ende?" — `set_is_complete` (Rust), `match_decided` (Rust, privat) und
`setDecided` (JS). Sie liefern nachweislich verschiedene Antworten. Bei
Zählweise 15/Deckel 21 hält die JS-Fassung 22:20 für gültig, die Rust-Fassung
nicht.

## Entscheidung

**A: Der Host beurteilt.** Eine neue reine Funktion `spiel_ist_entschieden`
neben `set_is_complete`, die dieselbe Satzprüfung benutzt wie der
Ergebnis-Pfad. Die Seite bekommt das Urteil fertig geliefert.

**B: Der Zeitpunkt liegt persistent** im `MatchTimesStore`
(`decided_seen_ms`), und der `TlState` trägt ihn als **Zeitstempel** — nicht
als ausgewerteten Schalter. Die Frist rechnet die Seite.

## Alternativen

**A2 — die Seite rechnet.** Spart ein Zustandsfeld, den Wächter-Eintrag und
die Versions-Schere zwischen Seite und Host. Verworfen, weil damit eine
**vierte** Definition von „fertig" entstünde, die von der des Ergebnis-Pfads
abweicht: Eine Warnung, die anschlägt, während `process_result` dasselbe
Ergebnis ablehnen würde, verwirrt mehr, als sie hilft. Außerdem ließe sich die
Regel dort nicht mit `cargo test` gegen die Zählweisen-Matrix absichern.

**B2 — Bool statt Zeitstempel, aus einem RAM-Merker.** Einfacher, aber zweimal
falsch:

1. Beim App-Start lädt der Host die Live-Stände aus `scores.json` zurück. Ein
   RAM-Merker sähe den entschiedenen Stand sofort wieder als „gerade zuerst
   gesehen" — die Warnung verschwände für eine Minute, ausgerechnet nach einem
   Neustart, wenn die Turnierleitung am dringendsten hinschaut.
2. `state_fingerprint` nullt bewusst zeitabgeleitete Felder, damit die
   Revision nicht im Sekundentakt hochzählt. Ein Schalter, der nach einer
   Minute umspringt, wäre genau so ein Feld — die Warnung käme per Push **nie**
   an, sondern erst beim nächsten Sicherheits-Poll aus anderem Anlass. Ein
   Zeitstempel ist dagegen stabil und darf in den Fingerabdruck.

## Konsequenzen

- **Gut:** Eine Quelle für „fertig", geteilt mit der Ergebnisprüfung, gegen
  die Zählweisen-Matrix unittestbar (3×21/30, 3×15/21, 1×21, 5×11/15,
  Deckel-Patt, über dem Deckel).
- **Gut:** Die Warnung übersteht Host-Neustart und Feldwechsel.
- **Preis:** Zwei neue Felder im `TlState` samt Eintrag im
  Datenschutz-Wächter, und eine Versions-Schere: Eine neue Seite an einem
  alten Host bekommt das Feld nicht und warnt **stumm gar nicht**. Das ist
  hingenommen — anders als beim Sperren (ADR 0044, Feature-Detection) fehlt
  hier nur eine Zusatzinformation, kein Knopf, der ins Leere führt.
- **Preis:** Die Seite muss die Frist selbst rechnen. Sie hat ohnehin einen
  Sekundentakt für ihre Uhren, also kostet das nichts.
- **Abgrenzung:** `match_decided` (privat, prüft die Vollständigkeit der Sätze
  nicht) bleibt unangetastet für den Ghost-Satz-Pfad in `handle_score`. Wer
  beide zusammenführen will, braucht dort einen Regressionstest.
