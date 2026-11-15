# 0097 — TP-Automatik übernimmt die Kennung nur bei Gleichheit oder leerem Feld

- **Status:** proposed
- **Datum:** 2026-11-15

## Kontext

Die Turnier-Kennung steuert Liveticker, Check-In, Aushang und den TP-Upload.
Bisher galt: bts-light übernimmt sie **nie still**, sondern nur nach
Bestätigung im Setup ([tp-datei-upload.md](../tp-datei-upload.md)). Die neue
Automatik ([ADR 0096](0096-tp-pfad-aus-den-handles-von-btp.md)) findet die in
BTP offene Datei ohne Zutun — auch dann, wenn der Turnierleiter während des
Turniers kurz eine andere Datei öffnet (BTP startet nur einmal, die offene
Datei wechselt also).

## Entscheidung

- Kennung der Datei **gleich** der eingetragenen → Pfad still übernehmen.
- **Keine** Kennung eingetragen → Pfad und Kennung still übernehmen.
- Kennung **weicht ab** oder die Datei trägt keine lesbare Kennung →
  **Rückfrage**, je Fund einmal. Ja übernimmt (bei laufender Übertragung
  startet bts-light sie danach selbst neu, die Frage kündigt das an). Nein
  ändert nichts und **schaltet die Automatik ab**, bis sie im Setup wieder
  eingeschaltet wird.
- Solange die Frage offen ist, ändert sich nichts; der Halt des TP-Uploads
  bei fremder Datei (`GuidFremd`, ADR 0061) bleibt bestehen.

## Alternativen

- **Immer still übernehmen:** Ein kurz geöffnetes Vorjahres- oder
  Folgeturnier würde Liveticker und Upload umlenken, unfertige Auslosungen
  veröffentlichen und gedruckte QR-Codes entwerten.
- **Immer fragen:** auch bei leerem Feld und gleicher Kennung — sicher, aber
  das Ziel „ohne Zutun" wäre verfehlt.
- **Bei Abweichung nur Hinweis, nie übernehmen:** zwingt für jedes neue
  Turnier zurück ins Setup.

## Konsequenzen

- Der Satz „nie still" gilt nicht mehr für das leere Feld; die Doku wird
  angepasst.
- Ein neues Turnier kostet einen Bestätigungsklick statt Dateidialog und
  Abtippen.
- Wer die Frage verneint, hat danach eine abgeschaltete Automatik und muss
  sie bewusst wieder einschalten.
