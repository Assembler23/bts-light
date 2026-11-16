# 0101 — Rückweg mit einem Klick: eine Vorversion, „rückwärts lesbar ab" im signierten Kopf, Abdruck-Wächter

- **Status:** accepted
- **Datum:** 2026-11-16

## Kontext

Wer einen Fix im laufenden Turnier einspielt, braucht einen Weg zurück. Für
Seiten ist das billig (ADR 0099). Für den Kern teilen sich alte und neue
Version `config.json` und alle Stores im App-Datenverzeichnis:

- Es gibt kein `deny_unknown_fields`: Die Vorversion liest neue Dateien,
  wirft unbekannte Felder aber beim nächsten Speichern still weg.
- Eine neue Enum-Variante lässt `load_from` der Vorversion scheitern; die App
  landet dann im Einrichtungs-Assistenten.
- Es gibt Einweg-Migrationen (`migrate_combo_dir_einmalig`, `order_v` im
  Meetingpoint-Store).
- Installationen überspringen Releases; „gegenüber dem Vorgänger lesbar"
  reicht als Angabe nicht.

## Entscheidung

- **Genau ein voriger Stand** bleibt liegen: `bts-light.vorher.exe` und das
  vorige Seiten-Paket. Kein Archiv.
- **`release/datenformat.json`** nennt `rueckwaerts_lesbar_ab`: die älteste
  Version, die die Daten der aktuellen noch lesen kann. Der Release-Job
  kopiert den Wert in den signierten Kern-Kopf (ADR 0100).
- Der Kern-Rückweg ist nur aktiv, wenn die Vorversion ≥ dieser Grenze ist.
  Fehlt die Angabe, gilt „nicht lesbar". Der Knopf bleibt sichtbar und nennt
  den Grund.
- **„Lesbar" heißt:** Die Vorversion startet mit den Daten und arbeitet
  fachlich richtig. Dass sie Einstellungen verliert, die sie nicht kennt, ist
  erlaubt und wird im Handbuch gesagt.
- **Abdruck-Wächter:** Ein Rust-Test bildet einen Abdruck der persistierten
  Formate (Schlüsselbaum von `AppConfig::default()` samt Enum-Varianten,
  Musterdateien der JSON-Stores) und vergleicht ihn mit `abdruck` in
  derselben Datei. Ändert sich ein Format, ist die CI rot, bis Abdruck und
  Grenze gemeinsam angefasst wurden — der Reviewer sieht beides in einem Diff.
- **Sperre:** Eine zurückgenommene Version oder Folge wird nicht erneut
  angeboten; eine neuere schon. Dasselbe gilt nach einem Rückfall durch den
  Start-Wächter.
- **Gültigkeit der Vorversion:** `rueckweg.json` nennt die Version, zu der
  die Vorher-Datei gehört. Passt sie beim Start nicht zur laufenden (etwa
  nach einem Installer-Lauf), wird die Datei verworfen.
- **Kein automatisches Zurückrollen** bei fachlichen Fehlern. Die einzige
  Automatik ist der Start-Wächter für „startet nicht".

## Alternativen

- **Rückweg immer, mit Sicherungskopie von Config und Stores.** Verworfen:
  Beim Zurückspielen ginge alles verloren, was die neue Version seither
  geschrieben hat (Aufrufe, Reihenfolge, Spielzeiten) — mitten im Turnier
  schlimmer als kein Rückweg.
- **Rückweg nur für Seiten.** Verworfen: Gerade beim Kernwechsel ist die
  Hemmschwelle am höchsten.
- **Ja/Nein je Release, Standard „ja".** Verworfen: veraltet still und sagt
  nichts über übersprungene Releases.
- **Standard immer „nein".** Verworfen: Der Knopf wäre fast nie da.
- **Selbstschutz bei fachlichen Fehlern** (z. B. Übertragung startet nicht
  binnen einer Frist). Verworfen: Die Ursache liegt oft außerhalb der App
  (BTP aus, Netz weg); ein Rückfall würde dann nichts heilen und verwirren.

## Konsequenzen

- Jeder PR, der ein persistiertes Format ändert, muss eine Entscheidung über
  die Lesbarkeit treffen; das kostet einen Handgriff und eine Zeile
  Begründung.
- Der Abdruck erkennt Strukturänderungen, nicht jede Bedeutungsänderung
  eines bestehenden Felds. Die bleibt Sache des Reviews.
- Nach einem Release mit hochgesetzter Grenze gibt es für diesen einen
  Schritt keinen Kern-Rückweg in der App. Das Banner sagt das schon im
  Angebot, also vor dem Klick — die Grenze steht im geladenen Kopf.
- Die Sperrliste lebt im App-Datenverzeichnis je PC.
