# 0071 — Meetingpoint-Zustand: Aufrufe und Check-ins in einem turniergebundenen Store

- **Status:** accepted
- **Datum:** 2026-11-02

## Kontext

Vorbereitungs-Aufrufe (`PreparationCall`) lebten nur im Arbeitsspeicher und
wurden verworfen, sobald ein Spiel aufs Feld kam. Der Meetingpoint
([Spec](../features/meetingpoint-spielbereit.md)) hängt Check-ins je Person an
den Aufruf; sie müssen Neustart und Update überleben, und „Vom Feld nehmen" soll
Aufruf und Check-in zurückbringen.

## Entscheidung

Ein Store `tablet/meetingpoint.rs` (`meetingpoint.json`, Muster ADR 0022)
übernimmt die Aufrufe. Je Partie: Halle, erster Aufrufzeitpunkt, Phase
aktiv/geparkt, Check-ins je Platz (Spieler Seite/Position oder SR) mit
Personen-Fingerabdruck, vorläufiger SR, Ansage-Stufen; daneben Feld-Bestätigungen.
Auf dem Feld wird der Aufruf geparkt, nach drei bestätigten Abrufen „geplant ohne
Feld" reaktiviert. `preparation_calls()` bleibt Projektion auf aktive Aufrufe.
Kennung nach außen ist (Partie, Platz), nie `player_key`. Nichts geht nach BTP.

## Alternativen

- Nur RAM — verliert alles beim Update, P1-Gelb verwaist.
- Zwei Stores (Aufrufe + Check-ins) — jede Lebenszyklus-Regel wird zum Join,
  verwaiste Check-ins nach Neustart.
- BTP `Player.CheckedIn` — turnierweit statt je Partie, verletzt R2-Sinn.
- `player_key` als Kennung — trüge die Lizenznummer auf die öffentliche Seite.

## Konsequenzen

Aufrufe überleben Neustarts (auch ohne Meetingpoint). Aufrufe nur über BTP-Gelb
erzeugen keinen Eintrag. Der Store braucht Frühbindung vor `auto_assign`.
