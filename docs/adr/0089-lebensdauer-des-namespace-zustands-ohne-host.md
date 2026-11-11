# 0089 — Lebensdauer des Namespace-Zustands ohne Host

- **Status:** accepted
- **Datum:** 2026-11-11
- Spec: [`relay-haerten`](../features/relay-haerten.md), Abschnitt f).
- Zusammen mit [ADR 0088](0088-anzeigen-bei-host-verlust-karenz-dann-503.md) (dieselbe Uhr).
  Berührt [ADR 0003](0003-azure-tts-vererbung-relay.md) (Sprachdienst-Schlüssel am Relay)
  und [ADR 0083](0083-stand-autoritaet-nach-fortschritt.md) (gesicherter Tablet-Stand).

## Kontext

Der Relay hält allen Zustand im RAM und räumt nur ereignisgetrieben: Ein Namespace
verschwindet, wenn Host, Tablets, offene Ergebnisse und TL-Abos fort sind. Solange ein
Tablet oder ein TL-Tab hängt, bleiben nach dem Abgang des Turnier-PCs Namen und Vereine,
Listen, Freitext, Werbebilder, Logo und der Sprachdienst-Schlüssel unbegrenzt im Speicher
und abrufbar. Einen Zeit-Verfall gibt es nicht.

Dagegen steht: Der gesicherte Tablet-Stand ist das, was ein Ersatz-Tablet bekommt und was
ein Cloud-Tablet zum Weiterzählen braucht. Ein Update oder Neustart des Turnier-PCs dauert
Minuten; wird in dieser Zeit geleert, beginnt ein Ersatzgerät wieder bei 0:0. Und ohne
Eintrag in `court_matches` verwirft der Stale-Filter jeden weiteren Punkt des Tablets.

Entschieden vom Nutzer: Turnierdaten bleiben 60 min, der Sprachdienst-Schlüssel wird sofort
verworfen, der gesicherte Tablet-Stand bleibt, solange ein Tablet am Feld hängt.

## Entscheidung

Gestaffelter Verfall, gemessen an `host_weg_seit`, ausgeführt von einem Sweeper (alle 5 s,
reine Entscheidungsfunktion):

1. **Sofort beim Host-Abgang:** `azure_tts`; wie bisher TL-Zugänge, TL-Zustand und wartende
   Abrufe; die TL-Abos werden geschlossen.
2. **Nach 60 min:** Spiel-Briefe der Felder ohne Tablet, Vorbereitungsliste, Meetingpoint,
   Feldliste, Labels, Hallen, Freitext, Bilder, Logo, Monitor-Konfiguration und -Steuerung,
   Geräte-Lebenszeichen, Slave-Liste, Übersichts-Cache; für Felder ohne Tablet auch Sätze
   und gesicherter Stand.
3. **Solange ein Tablet am Feld hängt:** Halter, Gerätekennung, gesicherter Stand, Sätze und
   ein **anonymisierter Brief** (nur `match_id` und `finalized`).
4. **Danach** wird der Namespace entfernt, sobald kein Tablet und kein offenes Ergebnis mehr
   hängt.
5. **Verdrängung am Limit:** Ist `MAX_NAMESPACES` erreicht, weicht der am längsten hostlose
   Namespace ohne Tablet einem neuen Host.
6. Kommt der Host vor Ablauf zurück, wird die Uhr gelöscht und nichts geleert.

## Verworfene Alternativen

- **Ereignisgetrieben wie heute:** kein neuer Task, aber die Daten bleiben, solange
  irgendein Tab offen ist — im Zweifel Tage.
- **Eine einzige Karenz für alles:** einfach, aber entweder verschwindet der Tablet-Stand
  während eines PC-Neustarts (zu kurz), oder Schlüssel und Namen bleiben lange abrufbar (zu
  lang).
- **15 min statt 60 min:** kürzer abrufbar, aber ein Rechnertausch oder ein längeres Update
  fiele heraus und kostete die Bilder und den Stand der Felder ohne Tablet.
- **Feld mit Tablet ganz leeren:** Der Stale-Filter verwürfe danach jeden Punkt; bei der
  Rückkehr des Hosts gälte „anderes Spiel", Sätze und Zustand würden geräumt.
- **Zustand auf Platte:** überstünde auch einen Relay-Neustart, bringt aber einen
  Datenbestand mit Namen auf den Server und ist für diesen Zweck nicht nötig.

## Folgen

- Nach einem Host-Abgang ist der Sprachdienst-Schlüssel sofort, alles Personenbezogene der
  Felder ohne Tablet nach 60 min aus dem Speicher.
- Solange der Host verbunden ist, bleibt der Schlüssel abrufbar (ADR 0003, unverändert).
- Die Rückkehr binnen 60 min ist der heutige Reconnect-Pfad; danach verhält sich ein Feld
  ohne Tablet wie nach einem Relay-Neustart (Host-Sätze, Rückspiegel).
- Ein Feld mit Tablet behält seinen Stand über das Leeren hinweg; bei der Rückkehr des
  Hosts gilt „gleiches Spiel" — kein 0:0.
- Enthält der gespiegelte Tablet-Zustand Spielernamen, bleibt für Felder mit verbundenem
  Tablet ein Rest im Speicher, bis das Tablet geht. Zu prüfen bei der Umsetzung und in
  `docs/cloud-relay.md` zu benennen.
- Gehaltene Daten können keinen neuen Host aussperren.
- Der Werbe-Fingerabdruck des Hosts wird nicht falsch: Geleert wird nur ohne Host, und jede
  neue Sitzung beginnt ohne Fingerabdruck.
