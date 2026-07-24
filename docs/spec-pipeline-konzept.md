# Spec-Pipeline `/idee` — Konzept und Funktionsweise

Diese Datei beschreibt die Methode **stack-agnostisch**: wie aus einer Rohidee oder einem
Meeting-Transkript in vier Phasen eine belastbare, review-geprüfte Feature-Spezifikation wird.
Sie ist in `bvbb-hub` entstanden und liegt inhaltsgleich in beiden Repos, damit die Arbeitsweise
überall dieselbe ist. Die konkrete Umsetzung steckt in den Skills unter
`.claude/skills/{idee,grill-me,how-to}/` des jeweiligen Repos.

---

## 1. Warum

Globale Grundregel: **keine Umsetzung ohne klare Anforderung** (`~/.claude/CLAUDE.md`, Regel 1).
KI kann in Minuten Code produzieren — und damit auch in Minuten das Falsche bauen. Der teure Fehler
ist nie der Tippfehler, sondern die stillschweigende Annahme, die erst nach der Umsetzung auffällt.

Leitsatz der Pipeline: **lieber fünf scharfe Rückfragen zu viel als eine Lücke, die später
Nacharbeit kostet.**

Die Pipeline ist bewusst **opt-in**: sie wird vom Menschen mit `/idee` gestartet. Für triviale
Änderungen ohne fachliche Anforderung wird sie übersprungen.

---

## 2. Die vier Phasen

```
Rohidee / Transkript
        │
        ▼
  ┌───────────┐   Was will ich eigentlich?
  │ 1 Brief   │   → Ziel, Kontext, Scope, Constraints, gewünschtes Ergebnis
  └─────┬─────┘   Technik: eine Frage pro Nachricht, Multiple-Choice bevorzugt
        ▼
  ┌───────────┐   Hält das stand?            ◀── KERN-GATE
  │ 2 Grill   │   → adversarialer Subagent, ≥5 geschlossene Rückfragen
  └─────┬─────┘   → Blocker, Advisory, ADR-Bedarf
        ▼
  ┌───────────┐   Wie bauen wir das?
  │ 3 How-To  │   → Explore/Plan, betroffene Komponenten, 2–3 Wege mit Trade-offs
  └─────┬─────┘   → Implementierungsplan, noch KEIN Code
        ▼
  ┌───────────┐   Ist es vollständig und testbar?
  │ 4 Spec    │   → Verdichtung in die Vorlage + Review-Checkliste inline
  │  + Review │   → committete Spec + ggf. ADR
  └─────┬─────┘
        ▼
   Freigabe durch den Menschen  ──▶  erst danach Umsetzung
```

Jede Phase ist eine eigene Task mit einer kurzen Freigabe beim Übergang. Der Mensch bleibt in
jedem Schritt in der Schleife.

### Phase 1 — Brief

| | |
|---|---|
| **Zweck** | Die Rohidee in eine strukturierte Kurzfassung bringen. |
| **Input** | Idee im Chat oder ein Meeting-Transkript (eingefügt oder als Dateipfad). |
| **Technik** | Eine Frage pro Nachricht, Multiple-Choice bevorzugt, YAGNI, Annahmen sichtbar machen. |
| **Output** | `_intake/<slug>/1-brief.md` — Ziel, Kontext, Scope, Constraints, gewünschtes Ergebnis. |
| **Gate** | Nutzer bestätigt den Brief kurz. |

### Phase 2 — Grill (das Kern-Gate)

| | |
|---|---|
| **Zweck** | Den Brief adversarial löchern, bevor irgendetwas entworfen wird. |
| **Input** | `1-brief.md`. |
| **Technik** | Ein Subagent mit dem Prompt aus `grill-me/grill-prompt.md`. Er soll **nicht loben**, sondern jede Schwäche finden, die zu falscher Umsetzung, Nacharbeit oder Governance-Verstoß führen würde. |
| **Output** | `_intake/<slug>/2-grill.md` — geklärte Anforderung + verbleibende offene Punkte. |
| **Gate** | Rückfragen fokussiert an den Nutzer, bis **Status: Approved** — oder Reste bewusst als „Offene Fragen" dokumentiert. |

### Phase 3 — How-To (Design, kein Code)

| | |
|---|---|
| **Zweck** | Vom *Was* zum *Wie*: erst verstehen, was schon da ist, dann bestehenden Mustern folgen. |
| **Input** | `2-grill.md`. |
| **Technik** | `Explore`-Agent für die Architektur-Recherche, `Plan`-Agent für tiefere Planung. 2–3 Lösungswege mit Trade-offs, Empfehlung zuerst. |
| **Output** | `_intake/<slug>/3-how-to.md` — betroffene Komponenten, Lösungswege, kleiner überprüfbarer Implementierungsplan, ADR-Bedarf, Review-Bedarf. |
| **Gate** | Kein Code, kein Scaffolding. |

### Phase 4 — Spec + Review (ein Schritt)

| | |
|---|---|
| **Zweck** | Verdichten und sofort gegenprüfen. |
| **Input** | Brief + Grill + How-To. |
| **Technik** | Nur verdichten, **nichts Neues erfinden**. Das How-To fließt in den Abschnitt „Umsetzungs-Hinweise". Danach im selben Schritt gegen die Review-Checkliste prüfen und inline fixen. |
| **Output** | Die committete, PII-bereinigte Spec im Spec-Verzeichnis des Repos; bei echter Technik-Entscheidung zusätzlich ein ADR (`create-adr`), in der Spec verlinkt. |
| **Gate** | Der Nutzer reviewt und gibt frei. **Erst danach** Hand-off an die Umsetzung. |

**Review-Checkliste (6 Punkte):**

1. Akzeptanzkriterien konkret **und testbar** — Positiv- *und* Fehlerfälle?
2. Widersprüche zwischen Abschnitten?
3. Fehlerfälle / Edge Cases beschrieben?
4. Scope eindeutig, Nicht-Ziele explizit?
5. Kann ein Entwickler oder Agent damit arbeiten, **ohne wesentliche Annahmen** treffen zu müssen?
6. Platzhalter / TODO / „TBD" entfernt, PII bereinigt?

---

## 3. Das Hard-Gate

Jeder der drei Skills trägt ein `<HARD-GATE>` im Kopf. Das des Orchestrators lautet sinngemäß:

> Bis die finale Spec die Review-Checkliste besteht **und** der Nutzer sie ausdrücklich freigibt,
> wird KEIN Produktivcode geschrieben, KEIN Modul/Scaffolding angelegt und kein Umsetzungs-Skill
> oder Plan-Agent zur Implementierung gestartet. Endprodukt dieses Skills ist die **Spec** — nicht
> die Implementierung.

Das ist der entscheidende Trick: ohne diese Klausel driftet ein Agent nach der dritten Phase
zuverlässig ins Coden ab, weil der Plan „ja schon fertig" wirkt. `grill-me` und `how-to` haben
jeweils ein eigenes, engeres Hard-Gate für die Einzelnutzung.

---

## 4. Artefakte, Ablage und PII

| Was | Wo | Committed? |
|---|---|---|
| Roh-Intake (Transkript, Rohidee) | `_intake/<slug>/_intake.md` | **nein — gitignoriert** |
| Brief | `_intake/<slug>/1-brief.md` | nein |
| Grill-Ergebnis | `_intake/<slug>/2-grill.md` | nein |
| How-To | `_intake/<slug>/3-how-to.md` | nein |
| Finale Spec | Spec-Verzeichnis des Repos, eine Datei je Feature | **ja** |
| ADR (falls nötig) | `docs/adr/NNNN-….md` | ja |

`<slug>` = kurzer kebab-case-Titel des Features.

**Warum gitignoriert:** Meeting-Transkripte enthalten Namen, Meinungen und oft personenbezogene
Details. Committet wird ausschließlich die bereinigte Spec (globale Regel 6: keine sensiblen Daten
in Logs, Prompts oder externen APIs).

**Resume:** Liegen unter `_intake/<slug>/` bereits Stufen-Dateien, setzt der Orchestrator ab der
nächsten offenen Phase fort. Eine unterbrochene Session geht also nicht verloren.

---

## 5. Der Grill im Detail

Das Herzstück. Ein Subagent bekommt den Brief und eine Liste von Dimensionen, die er **gnadenlos**
durchgeht. Die Dimensionen sind teils universell, teils repo-spezifisch (kursiv = pro Repo anpassen):

| Dimension | Was geprüft wird |
|---|---|
| Problem & Zweck | Ist das echte Problem benannt und belegt? Wer hat den Schmerz — oder ist es eine Lösung auf der Suche nach einem Problem? |
| Erfolgskriterien | Messbar? Woran erkennt man in drei Monaten, dass es funktioniert hat? |
| Akzeptanzkriterien | Vollständig UND testbar? Jedes als prüfbarer Satz? Fehlende Negativ-/Fehlerfälle? |
| Nicht-Ziele | Explizit? Was wird bewusst NICHT gebaut — sonst ufert der Scope aus. |
| *Rollen & Rechte* | *Welches Rechte-/Rollenmodell ist betroffen? Freigaben nötig?* |
| *Datenmodell & Persistenz* | *Neue Felder/Tabellen? Wie läuft die Migration? Abwärtskompatibilität?* |
| *Datenschutz / sensible Daten* | *Welche Sonderregeln des Repos greifen?* |
| *Abhängigkeiten* | *Externe Systeme, Protokolle, andere Repos — Invarianten verletzt?* |
| Technik-Entscheidung | Gibt es 2+ tragfähige Wege? Dann ist ein ADR Pflicht (globale Regel 2). |
| Neue Abhängigkeit | Neues Paket nötig? Notwendigkeit, Pflegezustand, Lizenz geprüft (Regel 5)? |
| Risiken & Rollback | Was kann schiefgehen? Ist die Änderung zurückrollbar? |
| Annahmen | Welche stillschweigenden Annahmen trägt die Spec? Benennen, auch wenn sie verschwiegen werden. |
| Scope | Fokussiert genug für EINEN Umsetzungsplan — oder muss zerlegt werden? |

**Kalibrierung (wichtig):** Gemeldet wird alles, was zu falscher Umsetzung, Governance-Verstoß oder
Nacharbeit führen würde. **Wortglättung, Stilfragen und „Abschnitt X ist kürzer als Y" sind KEINE
Findings.** Ohne diese Kalibrierung produziert der Grill Rauschen statt Erkenntnis.

**Ausgabeformat des Grills:**

```
## Grill-Ergebnis

**Status:** Approved | Issues Found

**Blocker (müssen vor Finalisierung geklärt werden):**
- [Abschnitt]: [konkrete Lücke/Widerspruch] — [warum es die Umsetzung gefährdet]
  → Rückfrage: [eine geschlossene, direkt beantwortbare Frage]

**Sollte-geklärt-werden (advisory, blockt nicht):**
- [Abschnitt]: [Hinweis] → [optionale Rückfrage]

**ADR-Bedarf:** Ja/Nein — [wenn ja: welche Entscheidung mit welchen Alternativen]
```

Geschlossene Rückfragen sind Absicht: „Soll X bei Fehler abbrechen oder überspringen?" lässt sich in
zehn Sekunden beantworten, „Wie soll die Fehlerbehandlung aussehen?" erzeugt eine neue Denkaufgabe.

---

## 6. Einzelnutzung

Die beiden wertvollen Stufen laufen auch ohne Orchestrator:

- **`/grill-me`** — eine bestehende Anforderung, Spec oder grobe Idee löchern. Nützlich auch für
  fremde Dokumente oder ältere Pläne.
- **`/how-to`** — eine bereits geklärte Anforderung in einen Umsetzungsplan übersetzen, ohne den
  vollen Pipeline-Overhead.

Ergebnis landet dann im Chat oder an einem vom Nutzer genannten Ort statt in `_intake/`.

---

## 7. Was die Pipeline nicht dupliziert

Bewusst schlank gehalten: sie ruft vorhandene Bausteine auf, statt sie nachzubauen.

- **`create-adr`** (globaler Skill) für die Entscheidungsdokumentation.
- **`Explore`** / **`Plan`** — die eingebauten Agenten für Recherche und Planung.
- **`code-reviewer`** / **`security-reviewer`** — werden in der Spec für die *spätere* Umsetzung
  vermerkt, nicht in der Pipeline selbst ausgeführt.

---

## 8. Portierung in ein weiteres Repo

Struktur, Phasenlogik, Hard-Gates und Ausgabeformate bleiben unverändert. Anzupassen ist nur der
**Repo-Kontext** — konsistent in allen fünf Dateien:

| Anzupassende Stelle | Frage, die das Repo beantworten muss |
|---|---|
| Code-Landkarte | Wo liegen die Bausteine? Welche Datei beschreibt die Architektur übergreifend? |
| Rollen & Rechte | Gibt es ein Rechtemodell? Wenn nein: welche Rollen/Invarianten treten an seine Stelle? |
| Persistenz & Migration | Wo liegt der Zustand, wie wird migriert, was muss abwärtskompatibel bleiben? |
| Datenschutz | Welche Sonderregeln gelten? Was ist ausdrücklich **kein** Finding? |
| Abhängigkeiten | Welche externen Systeme/Protokolle, welche Invarianten dürfen nicht brechen? |
| Dependency-Manager | Cargo, npm, Composer …? |
| Test-/Build-Kommandos | Was muss vor dem Commit grün sein? |
| Doku-Pflichten | Welche Doku-Datei muss eine Änderung mitpflegen? |
| Spec-Ablageort | Wohin die finale Spec, wo wird sie indiziert? |
| Intake-Pfad | Welcher Pfad muss in `.gitignore`? |

Faustregel für den Check nach der Portierung: nach Begriffen des Quell-Repos greppen — bleibt einer
stehen, halluziniert der Grill später Kontext, den es hier nicht gibt.

---

## 9. Kurzfassung

- **Opt-in**, vom Menschen gestartet: `/idee`.
- **Vier Phasen** mit Freigabe an jedem Übergang: Brief → Grill → How-To → Spec+Review.
- **Hard-Gate**: Endprodukt ist die Spec, nicht der Code.
- **Zwischenstände gitignoriert**, nur die bereinigte Spec wird committet.
- **Der Grill ist der Wert** — geschlossene Rückfragen, klare Kalibrierung, kein Stil-Rauschen.
- **Einzeln nutzbar**: `/grill-me`, `/how-to`.
