# Handbuch-Website (`/download/bts-light/handbuch/`)

> Umgesetzt 05.09.2026, Stufe 1. Generator `scripts/build-handbuch.mjs`,
> Manifest [`docs/handbuch.json`](../handbuch.json), Deploy
> `.github/workflows/handbuch.yml`. Betriebsbeschreibung in
> [release.md](../release.md#handbuch-downloadbts-lighthandbuch).
>
> **Kein `/idee`-Durchlauf.** Die Pipeline gilt laut `CLAUDE.md` für neue
> Features der Software. Hier entsteht kein Produktivcode: ein Build-Skript,
> ein Test und ein Deploy-Workflow für Dokumentation, die bereits im Repo
> liegt. Diese Datei hält die Entscheidungen fest, damit sie nicht nur im
> Commit stehen.

## Wozu

Für bts-light gab es öffentlich nur die Release-Seite: Version, Changelog,
Download. Eine Anleitung für Turnierleitung, Schiedsrichter,
Zähltafelbediener und Hallenaufbau war nirgends im Web erreichbar — obwohl
rund 20 Dateien unter `docs/` bereits im Anleitungston geschrieben sind
(`pi-setup.md`, `turnierleitung-web.md`, `aushang.md`, `walkover.md` …).
Wer in der Halle steht, hat das Repo nicht.

Die Download-Seite ist aus Sicht des Handbuchs **ein Kapitel**
(„Downloads & Versionshinweise"), nicht der Ort, an den man ein Handbuch hängt.
Beide verlinken sich gegenseitig.

## Entscheidungen

**Quelle bleibt `docs/`, keine zweite Textfassung.** Eine umgeschriebene
Endnutzer-Kopie neben der Entwicklerdoku wäre sprachlich schöner geworden und
innerhalb eines Jahres veraltet. Der Preis dieser Entscheidung: halb-interne
Dateien müssen beim Rendern beschnitten werden (siehe unten), und die Prosa
enthält weiterhin Entwickler-Spuren („seit v0.9.247", „ADR 0018"). Das
aufzulösen ist Schreibarbeit an den Quelldateien — Stufe 2, nicht Technik.

**Whitelist statt Blacklist.** `docs/handbuch.json` listet auf, was
veröffentlicht wird. Eine Blacklist kann zu viel durchlassen, und genau das
fällt niemandem auf; eine Whitelist kann nur zu wenig zeigen, und das fällt
beim Lesen auf. ADRs, Specs, Roadmaps und die Server-Einrichtung des Relays
(Benutzeranlage, sudoers, SSH-Schlüssel) bleiben draußen.

**Abschnitts-Ausschluss im Manifest, nicht nur im Markdown.** `tablet.md`,
`court-monitor.md`, `multi-hall.md` und `cloud-relay.md` sind je zur Hälfte
Bedienung und Architektur. Sie zu zerschneiden hätte vier Dateien in acht
verwandelt. Stattdessen nennt `"aus"` die Überschriften, die samt
Unterabschnitten wegfallen; für Stellen mitten in einem Abschnitt gibt es
zusätzlich `<!-- handbuch:aus -->` … `<!-- handbuch:an -->` im Markdown.

**Ein `aus`-Titel, den es nicht gibt, lässt den Bau fehlschlagen.** Das ist
die wichtigste Regel des Ganzen. Ohne sie macht eine umbenannte Überschrift
einen internen Abschnitt still wieder öffentlich, und die Seite sieht dabei
völlig in Ordnung aus. Beim allerersten Lauf hat genau das zugeschlagen (drei
README-Abschnitte hießen anders als angenommen).

**Deploy an `main`-Pushes, nicht an Tag-Releases.** Der `publish`-Job aus
`release.yml` läuft nur bei `refs/tags/*`. Doku ändert sich in diesem Repo
deutlich häufiger als die Versionsnummer; eine Korrektur wäre bis zum nächsten
Release unsichtbar geblieben.

**`marked` als einzige Abhängigkeit.** `build-release-page.mjs` ist bewusst
dependency-frei, kann aber nur Changelog-Bullets. Handbuch-Seiten brauchen
Überschriften, verschachtelte Listen, GFM-Tabellen, Codeblöcke, Querlinks.
Der `dependency-auditor` (05.09.2026) hat `marked` gegenüber `markdown-it`
empfohlen: gleiche Fähigkeiten, **null** transitive Pakete statt sechs, MIT,
keine offenen Schwachstellen. Reine devDependency, läuft nur in Node/CI und
wird nicht in die App gebündelt.

**URL vorerst unter `/download/bts-light/handbuch/`.** Der Deploy-Benutzer
`bts-deploy` hat Schreibrecht auf genau zwei Verzeichnisse; ein Ordner direkt
unter `badhub.de/bts-light/` wäre eine Rechte- und nginx-Änderung am Server
mit eigener Freigabe gewesen. Nachgemessen am 05.09.2026: der vHost löst
Verzeichnis-URLs auf `index.html` auf (`/download/bts-light/` → 200), die
Adresse funktioniert also ohne ausgeschriebenen Dateinamen. Eine schönere URL
kann später ein nginx-`alias` ohne Dateiumzug nachreichen.

## Was der Test hält (`scripts/test-handbuch.mjs`)

- Ausgeschlossene Abschnitte fehlen in der Ausgabe — samt Unterabschnitten,
  auch bei Setext-Überschriften als Grenze.
- Ein umbenannter oder ein nie geschlossener Ausschluss lässt den Bau scheitern.
- **Jede** `docs/**/*.md` ist entweder veröffentlicht oder als intern
  eingetragen — rekursiv, 137 Dateien. Der Test prüft dabei sich selbst
  darauf, dass er wirklich in Unterverzeichnisse absteigt: die erste Fassung
  sah nur 32 Dateien und war trotzdem grün (Review 05.09.2026).
- Kein toter Querverweis, kein überlebender `.md`-Link, kein Link auf eine
  nicht veröffentlichte Datei (auch nicht per `../`-Ausbruch).
- Keine Server-/Deploy-Details in der Ausgabe.
- Die Release-Seite verlinkt das Handbuch (in `test-release-notes.mjs`).

## Bewusst nicht umgesetzt

- **Screenshots.** Es gibt keinen einzigen im Repo; für Vollabdeckung wären
  rund 45–60 nötig. Die Web-Oberflächen (Tablet, TL-Web, Monitor, Aushang,
  Werbung) sind reine HTML-Seiten des eingebetteten Servers und damit gegen
  einen Mock-Stand automatisierbar; nur die Tauri-Fenster bleiben Handarbeit.
  Erst sinnvoll, wenn die Textfassung steht — sonst veralten sie mit jedem
  UI-Umbau.
- **Fehlende Kapitel.** Für Installation/Erste Schritte, Setup-Wizard,
  Einstellungs-Referenz, Wartung und Siegerehrung existiert bislang gar keine
  Doku. Sie entstehen als normale `docs/*.md` und werden ins Manifest
  eingetragen — es bleibt bei einem Doku-Ort.
- **Suche, Versionsauswahl, Mehrsprachigkeit.** Für ein Handbuch dieser Größe
  keine Notwendigkeit.
