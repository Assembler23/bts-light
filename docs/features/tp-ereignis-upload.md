# Ereignis-Auslöser für den TP-Upload (Stufe 2b) — Spezifikation

> Status: **Freigegeben 28.10.2026** (Tilo: „Na dann los“, Drossel und
> Abschalter ausdrücklich verlangt). **Umgesetzt in v0.9.305.**
> Baut auf [tp-takt-upload.md](tp-takt-upload.md) (ADR 0061) auf.
> ADR: [0062](../adr/0062-tp-upload-ereignis-ausloeser.md). Nur bts-light.

## Kontext

Der Takt prüft die TP-Datei nach der Uhr (Standard 15 min). Auslosungen und
gefüllte Paarungen erreichen die Turnierseite dadurch spät. Die badhub-Seite
übernimmt Ergebnisse, Zwischenstände und Feld aus dem Liveticker, **Namen,
Paarungen und neue Spiele aber nur aus der Datei** (Messung
`LiveVerschmelzung`, siehe ADR 0062).

## Zielbild & Erfolgskriterien

1. Nach einer Auslosung oder einer gefüllten Paarung in BTP zieht die
   Automatik die nächste Prüfung auf etwa 30 s vor; der Upload läuft über
   den bestehenden Takt-Schritt.
2. Ergebnisse, Feldzuweisungen, Aufrufe lösen **nichts** aus.
3. Höchstens ein vorgezogener Upload je Mindestabstand (Standard 5 min).
4. Der Auslöser lässt sich im Setup abschalten und drosseln; beides wirkt
   ohne Neustart. Abgeschaltet bleibt der reine Takt.

## Nicht-Ziele

Keine badhub-Änderung; kein zweiter Sendeweg; keine Ereignisse aus Vereins-,
Setzplatz- oder Meldelisten-Änderungen (dafür bleibt der Takt); keine
Fernsteuerung der Drossel (badhub bremst über `Retry-After`, ADR 0061).

## Entscheidungen

| Frage | Entscheidung |
|---|---|
| E1 Was löst aus | Kern je Spiel `(draw_id, entry1_id, entry2_id)`; neue/entfernte Match-IDs oder geänderter Kern. Gründe für den Text: „neue Spiele“ > „Spielplan geändert“ > „Paarung“. |
| E2 Wo erkannt | `SyncEngine::run_once` direkt nach dem Leer-Snapshot-Guard über `tp_ereignis::fortschreiben`: erster Abruf, leerer Stand nach gefülltem und Turnierwechsel (keine gemeinsame ID) sind kein Ereignis. Der Sync-Loop holt das Ereignis ab (`tp_ereignis_nehmen`) und ruft `TaktZustand::ereignis`. |
| E3 Wie gewirkt | Echte Entprellung: 30 s nach dem letzten Ereignis, höchstens 2 min nach dem ersten (`MAX_ENTPRELLUNG`), nie später als der ursprüngliche Termin. Untergrenzen: letzter Upload (Takt oder Knopf) + Mindestabstand und `sperre_bis_ms` (Retry-After, Pause, Nachversuch, Knopf läuft). Ereignis während fälligem/laufendem Schritt → `ereignis_offen`, nach dem Schritt angewendet. Datei bei der vorgezogenen Prüfung noch gleich → einmal nach 90 s nachprüfen. Nicht bei Halt oder Takt aus; Halt, Wiederanlauf, Knopf, Aus/An löschen den Grund. |
| E4 Schalter | `ereignis_enabled` (Standard an), `ereignis_minuten` (Standard 5, Klemmung ≥ 5 über `ereignis_abstand()`); aus der geteilten Config je Abruf. |
| E5 Status | `TaktZustand.ereignis_grund` reist ans Dashboard („Paarung erkannt — Prüfung 14:32“), fällt nach dem Schritt weg. Eine Log-Zeile je vorgezogenem Termin. |

## Akzeptanzkriterien

1. `vergleiche`: gleich → kein Ereignis; neue IDs → „neue Spiele“; entfernte →
   „Spielplan geändert“; gefüllter Platz → „Paarung“; neue Spiele vor Paarung.
2. `ereignis`: Takt aus → wirkungslos; zieht auf +30 s vor; zweites Ereignis
   im Fenster verschiebt nicht; Mindestabstand zum letzten Upload (Takt und
   Knopf); nie hinter einen näheren Termin; Halt → wirkungslos; Drossel
   20 min wartet 20 min ab.
3. Config: alte Datei → Auslöser an, 5 min; Klemmung 1 → 5; aus → `None`,
   Takt läuft weiter; Roundtrip.
4. Setup: Häkchen **„Bei neuen Spielen und Paarungen sofort prüfen“** und Feld
   **„höchstens alle … Minuten“** nur bei eingeschalteter Automatik.
5. `cargo test`, Clippy `-D warnings`, `npm run build`, Handbuch-Anker grün.
