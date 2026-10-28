# 0061 — Takt-Upload: Lebenszyklus am Liveticker, Schnell-Hash ohne Gate, Anhalte-Regel

- **Status:** accepted
- **Datum:** 2026-10-28

## Kontext

Stufe 1 ([tp-datei-upload](../features/tp-datei-upload.md), ADR 0060) sendet
die offene TP-Datei auf Knopfdruck. Stufe 2
([tp-takt-upload](../features/tp-takt-upload.md)) soll das in einem Takt
von selbst tun. Drei Kräfte ziehen gegeneinander:

- Jede Kopie unter dem Schreib-Gate lässt eigene BTP-Writes typisch 5 s
  warten (ADR 0060). Ein Takt, der das alle 15 Minuten ohne Not tut, stört
  den Turnierbetrieb messbar.
- Die Datei ist die Freigabe (Spec Stufe 1, F3): Was in ihr steht, wird
  sichtbar. Ein Takt, der zu Hause beim Vorbereiten läuft, veröffentlicht
  halbfertige Auslosungen ohne Klick.
- Antworten, die ein neuer Upload nicht heilen kann (Endimport, falscher
  Verband, falsches Passwort), dürfen nicht alle 15 Minuten neu scheitern;
  Antworten, die vorübergehen (BTP beschäftigt, Netz), dürfen den Stand
  nicht 45 Minuten altern lassen.

Zusätzlich: Der Sync-Task wird bei jedem Speichern im Setup gestoppt und
neu gestartet (Lehre „Sync-Neustart bei jedem Speichern“); jeder Zustand,
der nur im Task lebt, geht dabei verloren.

## Entscheidung

1. **Lebenszyklus am Liveticker.** Der Takt-Task entsteht in `start_sync`
   als `TaktWaechter` im Sync-Task und endet mit ihm. Er läuft nur bei
   `!slave_mode` und nur, wenn der Schalter an ist (je Tick aus
   `AppState.config` gelesen). Sein Zustand — nächster Termin, Merker der
   zuletzt gesendeten Bytes, Halt-Grund, Nachversuchszähler — liegt im
   gemeinsamen Status in `AppState`, nicht im Task. Erster Lauf frühestens
   60 s nach dem Start; ein bestehender Termin bleibt über den Neustart
   stehen.
2. **Schnell-Hash ohne Gate.** Je fälligem Takt liest der Task die Datei
   einmal **ohne** Schreibsperre (dieselbe Lesefunktion mit Jet-Kopf- und
   Größendeckel), bildet den SHA-256 (`ring::digest`) und vergleicht ihn mit
   dem Merker der Bytes, die der Server zuletzt angenommen oder als bekannt
   quittiert hat (202, `unchanged`, `rejected`). Nur bei Abweichung folgt
   der Stufe-1-Weg: Gate, stabil lesen, senden. Gemessen: 41 ms für 3 MB.
3. **Anhalte-Regel.** Halt bei 409, 403, 401, 413, `guid_mismatch`,
   `final_imported`, Lesefehlern „keine TP-Datei“/„zu groß“ und verletzten
   Vorbedingungen; „nicht lesbar“ erst beim dritten Mal in Folge
   (vorübergehende Sperren); Pause 30 min bei „badhub kennt den Upload noch
   nicht“; Nachversuch nach 90 s (höchstens zwei in Folge) bei
   vorübergehenden Fehlern, bei 429/503 nach `Retry-After`. `parse_error`
   und `implausible` halten **nicht** an: Der Merker verhindert, dass
   dieselben Bytes erneut gehen, die nächste geänderte Datei bekommt eine
   neue Chance. Wiederanlauf durch Speichern mit geänderter Datei, Kennung,
   Liveticker-Adresse (Quelle des Kürzels), badhub-URL oder badhub-Passwort
   oder durch einen erfolgreichen Knopf-Upload. Schalter aus/an löst den
   Halt nicht.

## Alternativen

- **Eigenständiger Task ab App-Start** (Takt auch ohne Liveticker):
  verworfen — veröffentlicht beim Vorbereiten; die Freigabe läge allein an
  einem Schalter-Text.
- **Tick in `run_once`:** verworfen — streckt den Liveticker-Zyklus
  (Begründung an `CheckinLese` in `sync.rs`), und `run_once` steht bei
  BTP-Timeouts 20 s.
- **mtime + Größe als Vorfilter:** verworfen — Jet schreibt Seiten in place
  (Größe bleibt), das mtime-Verhalten bei dauerhaft offenem Handle ist
  ungemessen; beide Fehlrichtungen (stille Nicht-Uploads, Gate umsonst)
  wären erst im Turnier aufgefallen.
- **Immer Gate, Server entscheidet:** verworfen — 5 s Write-Sperre je Takt
  umsonst, 96 Log-Zeilen am Tag.
- **Byte-Vergleich gegen die letzte Kopie statt SHA:** verworfen — bis zu
  32 MB dauerhaft im Speicher für einen Vergleich, den ein Hash in 32 Bytes
  erledigt. `ring` ist transitiv schon im Baum, `sha2` wäre neu.
- **Halt auch bei `parse_error`/`implausible`** (Grill-Vorschlag):
  verworfen — eine zerrissene oder gerade umgebaute Datei ist beim nächsten
  geänderten Stand wieder gut; ein Halt verlangte dann einen Handgriff für
  einen Zustand, der sich von selbst erledigt.
- **Wiederanlauf nur über Schalter aus/an oder Zeitfenster:** verworfen —
  nach „Datei neu wählen“ ein zweiter Handgriff bzw. ein 409 am Montag, der
  alle zwei Stunden neu scheitert.

## Konsequenzen

- Kein Takt am Vorabend; die Auslosung vor dem Turnier bleibt Knopfsache.
  Das ist gewollt (F3), steht aber im Handbuch, sonst wundert sich jemand.
- Es gibt einen zweiten Leser der Datei **außerhalb** des Gates. Er kann
  eine halb geschriebene Seite erwischen; die Folge ist höchstens ein
  überflüssiger Gate-Lauf, nie ein falscher Upload (der läuft weiter unter
  dem Gate mit zweimal Lesen).
- Der Halt ist zustandsbehaftet und nur über Setup-Änderung oder Knopf
  lösbar. Nach einem App-Neustart ist er weg — ein 409 kommt dann einmal
  neu und hält wieder an; das ist billig (ein Request, kein Extraktor).
- `fuehre_upload_aus` bekommt einen strukturierten Rückgabewert
  (`UploadAusgang` mit dem SHA der gesendeten Bytes); `status_pollen`
  liefert die Server-Antwort statt eines Texts; der Knopf-Pfad übersetzt
  beides wie bisher in Text, der Takt ordnet ein. `Serverfehler` trägt den
  HTTP-Code, weil 413 (Halt) und 415 (Nachversuch) auseinander müssen.
- Der Upload läuft als eigener Task; bricht das Speichern den Takt-Task
  mitten im Upload ab, läuft der Upload zu Ende, nur die Einordnung fehlt —
  der nächste Schritt holt sie sich über `unchanged` (ein Gate-Lauf).
- Die Gate-Konstanten bleiben unberührt; die Jet-Nachlauf-Messung und
  Messung A2 bleiben Pflicht vor dem Feldtest (Roadmap).
