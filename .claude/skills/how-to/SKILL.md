---
name: how-to
description: "Übersetzt eine geklärte Anforderung (das Was) in ein durchdachtes Wie: untersucht die bestehende Architektur, identifiziert betroffene Crates/Komponenten, vergleicht Lösungswege und erstellt einen Implementierungsplan — ohne Code zu schreiben. Phase 3 des /idee-Ablaufs, auch einzeln per /how-to nutzbar."
---

# how-to — vom Was zum Wie (Design, kein Code)

Nimmt eine geklärte Anforderung und entwirft die Umsetzung: welche Teile der bestehenden Architektur
betroffen sind, welche Lösungswege es gibt und wie ein Implementierungsplan aussieht. Nutzbar als
Phase 3 von `/idee` oder **einzeln** (`/how-to`).

<HARD-GATE>
Dieser Skill schreibt KEINEN Produktivcode und legt kein Modul/Scaffolding an. Ergebnis ist ein
Umsetzungsplan — nicht die Umsetzung. (Globale Regel 1)
</HARD-GATE>

## Ablauf

1. **Input holen** — die geklärte Anforderung (aus `grill-me`) vom Nutzer oder aus
   `docs/features/_intake/<slug>/2-grill.md`.
2. **Architektur untersuchen** — den eingebauten `Explore`-Agenten nutzen, um Betroffenes zu finden:
   - Zuordnung zu den Crates: `src-tauri/` (App-Kern: BTP-Protokoll, Liveticker-Push,
     Tablet-Server/Relay-Client, Tauri-Commands), `relay/` (WebSocket-Broker), `relay-proto/`
     (geteilte Wire-Typen), `src/` (React-UI).
   - Architekturregeln **R1–R6** aus `CLAUDE.md`: nur Tauri-Commands über die Grenze (R1), BTP ist
     die Wahrheit (R2), LAN vs. Cloud-Relay (R3), ein Host je Namespace / ein aktives Tablet je
     Court (R4), Validierung in `process_result` (R5), `install_id` als Namespace und Log-Zuordnung
     (R6). Übergreifend zuerst lesen: `docs/multi-hall.md`.
   - Zustand & Konfiguration: neue Felder in `config.rs`? Abwärtskompatibel für bestehende
     Installationen (Auto-Update!), Defaults definiert?
   - Abhängigkeiten: BTP-Protokoll (`docs/btp_protocol.md`), badhub-Push, Relay hinter nginx,
     Pi-Kiosk (`pi/`), neue Cargo-/npm-Dependency (globale Regel 5)?
3. **Lösungswege vergleichen** — 2–3 Ansätze mit Trade-offs, Empfehlung zuerst. Für tiefere
   Planung den eingebauten `Plan`-Agenten dispatchen.
4. **Implementierungsplan** — konkrete, kleine, überprüfbare Schritte; welche Dateien/Muster,
   welche **Rust-Unit-Tests** (TDD-Pflicht: `cargo test` grün, `npm run build` fehlerfrei), welche
   Reviews (`code-reviewer` immer, `security-reviewer` bei neuem User-Input/Auth/Datei-/URL-Handling),
   welche `docs/**/*.md` laut Tabelle in `CLAUDE.md` mitgepflegt werden, und ob die Version
   gemeinsam in `src-tauri/Cargo.toml` + `src-tauri/tauri.conf.json` + `package.json` zu bumpen ist.
   ADR-Bedarf markieren (2+ tragfähige Wege → `create-adr`, Verzeichnis `docs/adr/`).
5. **Ergebnis** — im Pipeline-Lauf nach `docs/features/_intake/<slug>/3-how-to.md` (fließt später
   in den Spec-Abschnitt „Umsetzungs-Hinweise"); bei Einzelnutzung im Chat oder an einen vom Nutzer
   genannten Ort.

## Prinzip

Erst verstehen, was schon da ist, dann bestehenden Mustern folgen. Kein unnötiges Refactoring —
fokussiert auf das, was der Anforderung dient. Noch kein Code.
Gesamtkonzept: `docs/spec-pipeline-konzept.md`.
