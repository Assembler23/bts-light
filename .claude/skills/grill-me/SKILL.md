---
name: grill-me
description: "Löchert eine Anforderung/einen Brief adversarial, bevor gecodet wird: stellt ≥5 gezielte Rückfragen und deckt Edge Cases, stille Annahmen, Fehlerfälle und Out-of-Scope auf. Das Kern-Gate des /idee-Ablaufs — auch einzeln per /grill-me nutzbar. Lieber 5 gute Rückfragen als loszucoden."
---

# grill-me — Anforderung hart klären, bevor gecodet wird

Das wertvollste Gate: Statt eine Anforderung anzunehmen, wird sie kritisch hinterfragt. Nutzbar als
Phase 2 von `/idee` oder **einzeln** (`/grill-me`), um einen Brief, eine Spec oder eine grobe
Idee zu löchern.

<HARD-GATE>
Dieser Skill schreibt keinen Produktivcode und legt nichts an. Ergebnis sind geklärte Anforderung +
offene Punkte — nicht die Umsetzung.
</HARD-GATE>

## Ablauf

1. **Input holen** — den zu prüfenden Brief/die Idee vom Nutzer (eingefügt oder Datei-Pfad). Liegt
   ein Brief unter `docs/features/_intake/<slug>/1-brief.md`, diesen verwenden.
2. **Grillen** — den adversarialen Subagenten gemäß `grill-prompt.md` (general-purpose) auf den
   Input ansetzen. Er prüft gnadenlos gegen: Problem/Zweck belegt? Erfolgs-/**testbare**
   Akzeptanzkriterien? Nicht-Ziele? Architekturregeln R1–R6 (`CLAUDE.md`) verletzt? Beide
   Verbindungswege (LAN und Cloud-Relay) bedacht? Config-Abwärtskompatibilität für bestehende
   Installationen (Auto-Update)? Datenschutz (kein Geburtsjahr)? Abhängigkeit zu BTP/badhub/Relay?
   Technik-Entscheidung → ADR-Bedarf? Tests/TDD? Risiken/Rollback im laufenden Turnier? stille
   Annahmen?
3. **Rückfragen stellen** — die Befunde als **≥5 konkrete, geschlossene Rückfragen** an den Nutzer
   zurückspielen (fokussiert, nicht alles auf einmal). Lücken schließen.
4. **Ergebnis** — geklärte Anforderung + verbleibende offene Punkte zusammenfassen. Im Pipeline-Lauf
   nach `docs/features/_intake/<slug>/2-grill.md`; bei Einzelnutzung im Chat oder an einen vom
   Nutzer genannten Ort.

## Prinzip

Lieber eine scharfe Rückfrage zu viel als eine Lücke, die später teure Nacharbeit verursacht. Reine
Wortglättung ist kein Finding — echte Lücken, Widersprüche und untestbare Kriterien schon. Details
und das Subagenten-Template stehen in `grill-prompt.md`, das Gesamtkonzept in
`docs/spec-pipeline-konzept.md`.
