# 0022 — Officials-Turnierdaten: eigene turniergebundene Datei

- **Status:** accepted
- **Datum:** 2026-08-13

## Kontext

Das [Schiedsrichtermanagement](../features/schiedsrichter-management.md)
braucht persistente Laufzeitdaten, die ein App-Neustart mitten im Turnier
überleben müssen: Rotationsreihenfolge, Pausenstatus, Sperrlisten
(Sperr-Vereine und Sperr-Spieler je Official), manuelle Vereins-Overrides
(die Messung 13.08.2026 zeigt: BTP überträgt **keinen** Verein am
Official — nur `ID`, `Name`, `FirstName`, `Country`), feldweise Schalter
und lokale Zuweisungen. Besonderheiten:

- **Sperrlisten sind Personendaten** (kodieren Beziehungen SR↔Spieler)
  und dürfen weder ins Identitäts-Bündel (`export_identity` nimmt die
  komplette `AppConfig` mit) noch in den Broadcast-TL-State.
- **BTP-IDs sind turnier-spezifisch** — Official-ID 2 ist im nächsten
  Turnier jemand anderes. Über Turniere hinweg sind die Daten wertlos
  bis irreführend.
- `config.json` wird beim Speichern der Einstellungen komplett
  zurückgeschrieben (`keep_host_managed_fields` schützt nur explizit
  gelistete Felder) — im Betrieb wachsender Zustand ist dort fehlplatziert.

## Entscheidung

Alle turnier-spezifischen Officials-Daten liegen in einer **eigenen
JSON-Datei im App-Datenverzeichnis** (Muster `live-scores.json` /
ADR 0015), geschlüsselt am Turnier. Beim Turnierwechsel wird der Stand
verworfen. In der `AppConfig` liegen nur die geräteweiten Schalter
(`officials.enabled`, `rotation_sr`, `rotation_ar`).

## Alternativen

- **Alles in `AppConfig`:** verworfen — wandert ins Identitäts-Bündel
  (Personendaten!), wird vom Einstellungen-Speichern überschrieben und
  kennt keine Turnier-Trennung (gleiche Begründung wie bei
  `CheckinConfig`, config.rs).
- **Nur RAM** (wie die Zähltafelbediener-Queue): verworfen — die Queue
  füllt sich selbst nach, Rotation/Pausen/Sperrlisten nicht; ein Absturz
  mitten im Turnier würde die komplette Einteilung verlieren.
- **Remote bei badhub** (Muster Check-In-State): verworfen — Officials
  sind kein badhub-Zweck, und die Sperrlisten sollen das Gerät gar nicht
  erst verlassen (nur TL-Web-Pflege auf gezielte Anfrage).

## Konsequenzen

- Ein zweiter Persistenz-Pfad neben `live-scores.json` — Schreiben über
  denselben Locking-Ansatz (`Mutex<()>` um Dateizugriffe).
- Identitäts-Export/-Import bleiben unberührt; kein neuer Strip-Code in
  `identity_bundle` nötig.
- Ältere App-Versionen ignorieren die Datei schlicht — Rollback bleibt
  gefahrlos.
- Die Turnier-Erkennung (Schlüssel: Turniername oder GUID) muss ein
  Umbenennen des laufenden Turniers tolerieren — im Zweifel gilt: lieber
  verwerfen als falsch zuordnen (Datenschutz vor Komfort).
