# 0096 — TP-Pfad aus den Datei-Handles von `btp.exe`

- **Status:** accepted
- **Datum:** 2026-11-15

## Kontext

Der Turnierleiter muss die in BTP geöffnete `.TP`-Datei heute im Dateidialog
suchen. BTP nennt den Pfad weder im Protokoll noch im Fenstertitel (der zeigt
den Turniernamen). Gemessen am 15.11.2026 mit `tests/btp_tp_pfad_probe.rs`
(Ergebnisse in [btp_protocol.md](../btp_protocol.md)): BTP hält die Datei
dauerhaft offen, und die offenen Datei-Handles des Prozesses liefern genau
eine `.tp` mit vollem Pfad — in rund 70 ms, ohne Hänger, gleich ob die Datei
per Doppelklick oder über „Datei → Öffnen" geöffnet wurde.

## Entscheidung

bts-light liest den Pfad aus den Handles von `btp.exe`: jeden Handle-Wert
(Vielfache von 4) mit `DuplicateHandle` in den eigenen Prozess holen, mit
`GetFileType` auf Datenträger-Dateien filtern, mit
`GetFinalPathNameByHandleW` den Pfad fragen. Dabei gilt:

- **Read-only:** Die Kopien tragen kein Zugriffsrecht (Zugriff 0 — gemessen:
  für Typ und Pfad genügt das); nie `DUPLICATE_CLOSE_SOURCE`; geschlossen
  werden nur eigene Kopien.
- **Nur lokale Laufwerke:** Eine Datei im Netz öffnet die Suche nicht.
- **Frist und Deckel:** Die Abfrage läuft in einem Arbeiter-Thread mit 3 s
  Frist je Handle; höchstens zwei Überspringer je Suche, ab drei noch
  blockierten Threads startet keine Suche mehr.
- **Nur lokaler BTP:** gesucht wird nur bei Loopback-Adresse und nie am Slave.
- **Der Prozessname ist kein Beweis:** Jeder Fund läuft durch die bestehende
  Dateiprüfung samt Turnier-Kennung, bevor er etwas bewirkt.
- Zugriff über das vorhandene `windows`-Crate; die Sonde bleibt mit rohem FFI
  als unabhängige Gegenmessung bestehen.

## Alternativen

- **Kommandozeile des Prozesses:** trägt den Pfad nur nach Doppelklick-Start,
  nach „Datei → Öffnen" ist sie leer (gemessen).
- **Fenstertitel:** nennt den Turniernamen, nicht die Datei (gemessen).
- **`NtQuerySystemInformation`:** liefert die Handle-Liste direkt, ist aber
  undokumentiert; das Durchprobieren kostet nur Millisekunden.
- **Restart Manager / „Zuletzt verwendet":** bestätigen oder raten nur,
  finden den Pfad nicht selbst.
- **Weiter nur Dateidialog:** kein Risiko, aber das Problem bleibt.

## Konsequenzen

- Läuft BTP als Administrator, verweigert Windows den Zugriff (Fehler 5);
  dann bleibt die Handwahl, und das Setup sagt warum.
- bts-light hält für Millisekunden Kopien fremder Handles; das kann
  Virenscanner aufmerksam machen (ungemessen).
- Netzlaufwerk, USB-Stick und zwei Windows-Benutzer sind ungemessen; die
  Dateiprüfung fängt unlesbare Funde ab.
- Der Windows-Teil ist nur am echten BTP prüfbar; Auswahl, Normalisierung
  und Entscheidung sind davon getrennt und unter Linux getestet.
