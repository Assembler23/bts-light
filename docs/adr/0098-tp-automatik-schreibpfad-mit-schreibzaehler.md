# 0098 — TP-Automatik schreibt die Config außerhalb des Setups, geschützt durch einen Schreibzähler

- **Status:** proposed
- **Datum:** 2026-11-15

## Kontext

Die TP-Automatik (ADR 0096/0097) ändert Pfad, Bindung und gegebenenfalls die
Turnier-Kennung, ohne dass jemand im Setup „Speichern" drückt. Zwei bekannte
Fallen: Ein offenes Setup-Formular schreibt beim Speichern seinen alten Stand
zurück, und jedes Speichern über `save_config` zieht einen Neustart der
Übertragung nach sich, der Merker der SyncEngine zurücksetzt.

## Entscheidung

- Die Automatik schreibt über `mutate_config` und erhöht dabei den Zähler
  `tp_upload.stand`.
- `keep_host_managed_fields` vergleicht den Zähler: Weicht der des
  eingehenden Formulars vom gespeicherten ab, ist das Formular veraltet —
  dann gelten Pfad, gebundene Kennung, Automatik-Schalter und
  `tournament_uuid` aus dem gespeicherten Stand. Das Setup zieht bei
  geändertem Zähler nur seine TP-Felder nach.
- Ein reiner Pfadwechsel startet die Übertragung **nicht** neu (der TP-Takt
  liest die Config live). Nur ein bestätigter Kennungs-Wechsel bei laufender
  Übertragung löst den vorhandenen Weg Stop → Start aus.
- Die Merker „Frage offen" und „schon gefragt" leben im `AppState`, nicht in
  der SyncEngine.
- Der Pfad ist je PC: Das Identitäts-Bündel trägt ihn nicht, ein Import
  behält Pfad, Bindung und Schalter des Ziel-PCs.

## Alternativen

- **Setup neu aufbauen (Remount):** verliert ungespeicherte Eingaben.
- **„Setup offen"-Sperre:** bleibt hängen, wenn das Fenster abstürzt oder
  offen stehen bleibt.
- **Eigene Datei neben der Config:** zwei Wahrheiten für denselben Pfad.

## Konsequenzen

- Ein Zähler mehr in der `config.json`; eine handeditierte Datei kann ihn
  verstellen, dann greift der Schutz einmal zu viel (harmlos).
- Der Schutz gilt nur für die TP-Felder und die Kennung, nicht für das
  übrige Formular.
