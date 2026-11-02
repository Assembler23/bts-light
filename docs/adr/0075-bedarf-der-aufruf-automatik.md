# 0075 — Bedarfsrechnung der Aufruf-Automatik

- **Status:** accepted
- **Datum:** 2026-11-02

## Kontext

Die Aufruf-Automatik soll genug Partien zum Meetingpoint rufen, dass beim
Freiwerden von Feldern spielbereite warten, aber nicht zu viele. Die
Freiwerde-Rechnung je Feld (Live-Restzeit, Etappe D) existiert nur im
TL-Zustandsbau, der nur läuft, wenn ein TL-Gerät zusieht.

## Entscheidung

Freiwerde-Rechnung als Helfer nach `predict.rs` (Refactor ohne
Verhaltensänderung), geteilt von TL-Zustand und Sync-Loop. Schritt `auto_call`
im Sync-Loop (nur Master) nach `auto_assign`. Je Halle: Vorrat = gerufene
vergebbare Partien (nicht nur voll eingecheckte); Ziel fest N oder min(Deckel,
Felder frei jetzt/innerhalb X Min + Puffer); freies Feld = jetzt frei. Läuft
unabhängig von der Feld-Automatik, hält bei TL-Pause, nimmt nie zurück.

## Alternativen

- Automatik im TL-Zustandsbau — läuft ohne offenes TL-Gerät nicht, Seiteneffekt
  im Lesepfad.
- Eigener Takt — zweiter Schreiber, Reihenfolgeprobleme mit `auto_assign`.
- Vorrat nur aus voll eingecheckten Partien — ruft nach, während die Leute noch
  unterwegs sind.

## Konsequenzen

Zu Turnierbeginn wird bis zum Deckel gerufen. Prognose-Fehler wirken direkt auf
die Aufrufmenge; der Deckel begrenzt den Schaden.
