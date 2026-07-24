---
name: idee
description: "Aus einer Idee oder einem Meeting-Transkript entsteht über Brief → Grill → How-To → Spec+Review eine belastbare, review-geprüfte Feature-Spezifikation für bts-light (docs/features/) plus ADR. Bewusst vom Nutzer per /idee zu starten. Die Kern-Stufen /grill-me und /how-to sind auch einzeln nutzbar."
---

# idee — von der Idee zur umsetzbaren Spec (schlank, 3 Gates)

Orchestriert vier Phasen von der Rohidee bis zur freigegebenen Spec. Bündelt die einfachen Phasen
(Brief, Spec+Review) selbst und ruft die zwei wertvollsten als eigene Skills auf (`grill-me`,
`how-to`). Dupliziert nichts — nutzt die eingebauten `Plan`/`Explore`-Agenten und `create-adr`.
Konzept und Hintergrund: `docs/spec-pipeline-konzept.md`.

<HARD-GATE>
Bis die finale Spec die Review-Checkliste besteht UND der Nutzer sie ausdrücklich freigibt, wird KEIN
Produktivcode geschrieben, KEIN Modul/Scaffolding angelegt und kein Umsetzungs-Skill/Plan-Agent zur
Implementierung gestartet. Endprodukt dieses Skills ist die Spec — nicht die Implementierung.
(Globale Regel 1 „keine Umsetzung ohne klare Anforderung".)
</HARD-GATE>

## Wann dieser Skill greift

Bewusst vom Nutzer gestartet (`/idee`), wenn eine Idee/ein Transkript zu einem echten Feature von
bts-light werden soll. Opt-in: für triviale Änderungen ohne fachliche Anforderung überspringen.

## Ablage & Datenschutz

- Roh-Intake und Zwischenstände: **gitignoriert** unter `docs/features/_intake/<slug>/`
  (`<slug>` = kurzer kebab-case-Titel). Kann Namen aus Transkripten enthalten → **nie committen**.
  Stufen-Dateien dort: `1-brief.md`, `2-grill.md`, `3-how-to.md`.
- Committfähig, eine Datei je Feature: die finale Spec `docs/features/<slug>.md` (Vorlage
  `spec-template.md` in diesem Ordner). Verweis aus `docs/roadmap.md`.

## Ablauf — 4 Phasen (je eine Task, mit kurzer Freigabe je Übergang)

### Phase 1 — Brief
- Nutzer nach Rohidee ODER Transkript fragen (eingefügt oder als Datei-Pfad). Rohtext nach
  `docs/features/_intake/<slug>/_intake.md` (gitignoriert).
- Strukturierten Kurz-Brief erarbeiten: **Ziel, Kontext, Scope, Constraints, gewünschtes Ergebnis**.
  Fragetechnik: **eine Frage pro Nachricht**, Multiple-Choice bevorzugt, YAGNI, Annahmen sichtbar
  machen. Nach `_intake/<slug>/1-brief.md`. Kurz bestätigen lassen.

### Phase 2 — Grill (Kern-Gate)
- Die `grill-me`-Stufe auf den Brief ansetzen (ruft den adversarialen Subagenten, ≥5 gezielte
  Rückfragen, Edge Cases, Annahmen, Fehlerfälle, Out-of-Scope). Lücken mit dem Nutzer schließen.
- Geklärte Anforderung + offene Punkte nach `_intake/<slug>/2-grill.md`.

### Phase 3 — How-To (Design, kein Code)
- Die `how-to`-Stufe aufrufen: `Explore`/`Plan`-Agent, betroffene Crates/Komponenten
  (`src-tauri/src/*`, `relay/`, `relay-proto/`, `src/`), Architekturregeln **R1–R6** aus `CLAUDE.md`,
  Lösungswege, Implementierungsplan.
- Ergebnis nach `_intake/<slug>/3-how-to.md`. **Noch kein Code.**

### Phase 4 — Spec + Review (in einem Schritt)
- Aus Brief + Grill + How-To die finale Spec in der Vorlage `spec-template.md` verdichten
  (How-To fließt in den Abschnitt „Umsetzungs-Hinweise"). **Nichts Neues erfinden** — nur, was in
  Phase 1–3 geklärt wurde.
- Im selben Schritt gegen die **Review-Checkliste** prüfen und inline fixen:
  - Akzeptanzkriterien konkret **und testbar** (Positiv- und Fehlerfälle)?
  - Widersprüche zwischen Abschnitten?
  - Fehlerfälle/Edge Cases beschrieben?
  - Scope eindeutig, Nicht-Ziele explizit?
  - Kann ein Entwickler/Agent damit arbeiten, **ohne wesentliche Annahmen** treffen zu müssen?
  - Betroffene Doku-Datei(en) aus der Tabelle in `CLAUDE.md` benannt?
  - Platzhalter/TODO/„TBD" entfernt, personenbezogene Daten bereinigt?
- Finale, bereinigte Spec nach `docs/features/<slug>.md`; Verweis in `docs/roadmap.md` eintragen.
  Bei echter Technik-Entscheidung mit 2+ Wegen `create-adr` (Verzeichnis `docs/adr/`) und in der
  Spec verlinken.
- Nutzer bitten, die Spec zu reviewen:
  > „Spec liegt unter `docs/features/<slug>.md` und hat die Review-Checkliste bestanden. Bitte
  > prüfen und freigeben, bevor wir an die Umsetzung gehen."
- **Erst nach Freigabe** optional Hand-off an den `Plan`-Agenten / Claude Code — in kleinen,
  überprüfbaren Schritten. Vorher gilt das Hard-Gate.

## Resume & Einzelnutzung

- Liegen unter `_intake/<slug>/` schon Stufen-Dateien, ab der nächsten offenen Phase fortsetzen.
- Die Kern-Stufen sind auch ohne Orchestrator nutzbar: `/grill-me` (eine Anforderung löchern),
  `/how-to` (eine geklärte Anforderung in einen Umsetzungsplan übersetzen).

## Leitprinzipien

- Eine Frage pro Nachricht · Multiple-Choice bevorzugt · YAGNI · Annahmen sichtbar machen.
- Zwischenstände/Transkripte bleiben gitignoriert; committet wird nur die bereinigte Spec.
- TDD ist Pflicht: die Spec benennt, welche Rust-Unit-Tests das Feature absichern.
- Bei neuem User-Input, Auth, Datei-/URL-Handling `security-reviewer` für die spätere Umsetzung in
  der Spec vermerken; `code-reviewer` gilt ohnehin nach jeder Code-Änderung.
