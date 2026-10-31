# 0064 — Erst-Ansage im Kern als Auftrag `court_assigned`; `MatchAnnouncer` nur am Slave

- **Status:** accepted
- **Datum:** 2026-10-30

## Kontext

Die automatische Ansage „neues Spiel auf Feld X" entstand bis v0.9.313 in
der Desktop-Oberfläche: `MatchAnnouncer.tsx` pollte die Feld-Übersicht und
verglich sie mit `announceBaseline.mjs`. Damit erreichte sie nur den
Lautsprecher am PC — die Ansage-Seiten im Browser (ADR 0063) bekamen sie
nicht, obwohl gerade sie die Hallen ohne eigenen PC versorgen sollen (Spec
[ansage-im-browser](../features/ansage-im-browser.md), Etappe E2).

Die Regeln dieses Vergleichs sind hart erkämpft: Ein leerer erster Stand
darf keine Bestandsaufnahme sein (Befund 14.08.2026), ein Hallenwechsel holt
nichts nach (23.08.2026), dasselbe Spiel nicht zweimal binnen 5 s. Merker in
der `SyncEngine` haben zweimal Feldtest-Bugs erzeugt, weil sie beim
Speichern der Einstellungen neu gebaut wird.

## Entscheidung

1. **Der Kern erkennt neue Feldbelegungen** im Sync-Lauf (`sync.rs`,
   `erst_ansage`) — **nur am Master**, hinter dem Slave-Rücksprung und nach
   Schiedsrichter-Rotation und Zettel-Autodruck, damit die Besetzung des
   frisch belegten Felds feststeht. Je neuem Spiel ein Ansage-Auftrag
   `court_assigned` (Feld, Spiel, Halle des Felds).
2. **Der Stand liegt im `TabletState`** (`tablet/erst_ansage.rs`,
   `ErstAnsageStand`), nicht in der `SyncEngine`. Regeln 1:1 aus
   `announceBaseline.mjs`, als Rust-Tests festgehalten; zusätzlich setzt ein
   **Turnierwechsel** die Bestandsaufnahme zurück. Gefiltert nach Halle wird
   erst bei der Zustellung.
3. **Ein Text-Builder für alle:** Der Auftrag trägt keine Worte. Der
   `AnnounceJobPlayer` (PC) und der Vertoner der Ansage-Seiten übersetzen ihn
   mit `jobZuAnsage` → `courtAnsage` — dieselben Angaben (Klasse, Runde,
   SR/AR und Bedienung gemäß Schaltern) wie bisher der `MatchAnnouncer`.
4. **LAN-Slave bleibt bei sich:** `MatchAnnouncer` erkennt nur noch bei
   `slave_mode` (bleibt aber eingehängt — Audio-Freischaltung und Abbruch
   beim Abschalten). `/info/announce/jobs` liefert `court_assigned` nur mit
   `?auto=1`; der Slave fragt ohne, ältere Slaves überspringen die
   unbekannte Art ohnehin (`announce_jobs_aus_json`). Der Cloud-Slave ist
   unberührt.
5. **Auftragsdeckel 50 → 128** für den Morgen-Burst der Feldvergabe.

## Konsequenzen

- Die Ansage-Seiten sagen neue Spiele automatisch an, ohne eigene Logik.
- Die Erst-Ansage am PC kommt jetzt aus einem Auftrag: Sie ist an den
  Sync-Takt plus den 2-s-Abruf des `AnnounceJobPlayer` gebunden statt an den
  2-s-Abruf des `MatchAnnouncer` — gleiche Größenordnung.
- Der `AnnounceJobPlayer` führt seinen Abhol-Stand modulweit und beginnt
  nach dem Abschalten der Ansagen und nach einem Hallenwechsel mit einer
  neuen Bestandsaufnahme, damit nichts aus der letzten Minute nachklingt.
- **Rollback nur per Versions-Rückgang** — es gibt keinen Schalter zurück
  zum `MatchAnnouncer` am Master. Deshalb E2 mit eigenem Feldtest.
- Bewusst nicht geändert: Wird die Übertragung lange gestoppt, gilt der
  Stand davor weiter; in der Pause neu belegte Felder werden nach dem Start
  einmal angesagt — wie zuvor beim `MatchAnnouncer`.
