# Warnung bei scheinbar fertigem Spiel — Spezifikation

> Status: **abgestimmt 2026-08-23** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Nutzer-Anforderung vom 23.08.2026. Betroffene Crates: `src-tauri`.
> ADR: [0045](../adr/0045-fertig-warnung-serverseitig-gestempelt.md).

## Kontext / Problem

Am Feldtest 22.08.2026 standen Tablets, die weiterzählten, ihr Ergebnis aber
nicht loswurden (Felder 7 und 19). Die Ursache ist mit v0.9.254 behoben — das
Tablet unterscheidet jetzt dauerhafte von vorübergehenden Ablehnungen und zeigt
den Grund.

**Diese Warnung ist die andere Seite davon:** Sie greift unabhängig von der
Ursache und sieht auch die Fälle, die am Tablet gar nicht auffallen — der
Schiedsrichter hat vergessen zu übermitteln, das Tablet ist ausgegangen,
jemand ist einfach gegangen. Heute bemerkt die Turnierleitung so einen Fall
nur, wenn sie zufällig auf die Kachel schaut und den Satzstand selbst deutet.

## Zielbild & Erfolgskriterien

Eine Minute nachdem ein Spiel nach seinen Sätzen entschieden ist und das Feld
trotzdem belegt bleibt, erscheint in der Turnierleitungs-Sicht eine rote Marke
an der Kachel und eine Zeile im Störungsband.

1. Die Turnierleitung erfährt von einem hängenden Ergebnis **ohne** hinsehen zu
   müssen, spätestens 60 Sekunden nach dem letzten Ballwechsel.
2. **Kein Fehlalarm** in den bekannten Normalfällen (Liste unten) — eine
   Warnung, der man nicht glaubt, ist schlimmer als keine.
3. Die Warnung lässt sich **im laufenden Turnier abschalten**, ohne eine neue
   Version einzuspielen.

## Nicht-Ziele

- Warnen, weil **lange kein Punkt** mehr kam.
- Warnen, weil das **Tablet offline** ist (eigener Fall, eigene Anzeige).
- Eine **Ansage** oder ein Ton.
- Automatisch eingreifen (Ergebnis schreiben, Feld räumen).
- Die Warnung auf dem **Wandmonitor**, im Liveticker oder in der
  **Desktop**-Feldübersicht.

## Verhalten

### Wann gilt ein Spiel als „fertig"?

`server.rs::spiel_ist_entschieden(sets, scoring)` — eine reine Funktion neben
`set_is_complete`/`sets_fit_format`, damit dieselbe Zählweise gilt wie bei der
Ergebnisprüfung:

1. **Ohne aufgelöste Zählweise** (`target_score <= 0`) wird **nie** gewarnt.
   `set_is_complete` fällt still auf 21/30 zurück — in einem 15er-Turnier ohne
   Format hieße das reihenweise falsche Alarme.
2. **Leere Sätze (0:0) werden übersprungen.** Das Tablet setzt den laufenden
   Satz beim Match-Ende auf 0:0 und schickt ihn mit; ohne diese Regel gälte
   jedes fertige Spiel als unvollständig und die Warnung käme nie.
3. **Ein angefangener Satz blockiert.** Steht ein unvollständiger Satz in der
   Liste, wird gespielt — auch wenn die Sätze davor eine Mehrheit ergäben.
   Dieser Fall entsteht bei einer Korrektur oder falsch aufgelöstem `best_of`;
   in beiden Lagen wäre eine Warnung falsch.
4. Sonst: Satzmehrheit `best_of / 2 + 1` (mindestens 1) aus vollständigen
   Sätzen.

Die vorhandene private Funktion `match_decided` wird **nicht** benutzt: Sie
prüft die Sätze nicht auf Vollständigkeit und hielte bei `best_of = 1` schon
ein 3:1 im laufenden Satz für entschieden.

### Wann wird gewarnt?

Der Sync-Lauf stempelt je Poll `decided_seen_ms` in den **persistenten**
Zeitspeicher (`match-times.json`), sobald ein Spiel entschieden aussieht und
sein Feld belegt ist. Die Anzeige rechnet gegen
`config.finished_warning_seconds` (Default 60, `0` = aus).

**Kein Alarm in diesen Lagen:**

| Lage | Warum |
|---|---|
| Ergebnis liegt in der **BTP-Nachschub-Queue** | Der Host hat es angenommen und reicht es nach; das Feld bleibt trotzdem belegt. Dorthin muss niemand laufen — die Ursache liegt bei BTP. Geprüft über `btp_retry_pending`. |
| Satzpause (1:1) | Keine Mehrheit. |
| Laufender Satz | Regel 3 oben. |
| Aufgabe/Kampflos/DQ | Satzstand unvollständig ⇒ nie „entschieden". |
| Feld frei oder `clearing` | Kein belegtes Feld, keine Sätze. |
| Zählweise unbekannt | Regel 1 oben. |
| Korrektur nimmt den Stand zurück | Stempel wird verworfen, die Frist läuft beim nächsten Ende neu. |

**Bewusst in Kauf genommen:** Ein Schiedsrichter, der im Karten- oder
Aufgabe-Dialog hängt, sieht für den Host aus wie ein echter Hänger. Das ist von
außen nicht unterscheidbar — und die Turnierleitung will es in diesem Fall
ohnehin wissen.

### Wo erscheint sie?

- **Marke an der Feld-Kachel** („Ergebnis fehlt") in der Alarm-Sprache, die es
  dort schon gibt (gesperrt, Verletzung, TL gerufen).
- **Zeile im Störungsband.** Bewusst dort und nicht in einem eigenen Panel:
  Ein Panel lässt sich per Profil ausblenden und einklappen — dann fehlte die
  Meldung genau in der Lage, für die sie gedacht ist. Das Band ist immer da.
  - **Vorrang:** Kein Verbindungsband und Warnung gleichzeitig. Ohne
    Verbindung ist jede andere Meldung wertlos, weil der gezeigte Stand alt
    sein kann.
  - **Der Hallenfilter wird ignoriert**, die Halle steht stattdessen im Text.
    Sonst verschwiege die Zeile genau den Fall, für den sie gebaut wurde (das
    Gerät ist auf die andere Halle gestellt).
  - Mehrere Fälle: **ältester zuerst**, als Aufzählung der Feldnamen.

## Betroffene Komponenten / Architekturregeln / Daten

- `src-tauri/src/tablet/server.rs` (`spiel_ist_entschieden`) ·
  `src-tauri/src/sync.rs` (`stempel_entschiedene_spiele`) ·
  `src-tauri/src/tablet/match_times.rs` (`decided_seen_ms`) ·
  `src-tauri/src/tablet/tl.rs` (`TlCourt::decided_since_ms`,
  `TlState::finished_warning_seconds`) · `src-tauri/src/config.rs` ·
  `src-tauri/assets/tl.html`.
- **R1/R2** — reine Anzeige aus vorhandenen Daten; **nichts** wird nach BTP
  geschrieben.
- **R3** — LAN und Cloud zeigen dasselbe; der Wert reist im `TlState`.
- **Abwärtskompatibilität:** `finished_warning_seconds` mit
  `#[serde(default = …)]`, `decided_seen_ms`/`decided_since_ms` mit
  `#[serde(default)]`. Alte Configs bleiben lesbar; ein alter Host liefert das
  Feld nicht, die neue Seite warnt dann schlicht nicht (stumm, akzeptiert).
- **Datenschutz:** Ein Zeitstempel und eine Sekundenzahl. Beide sind im
  Wächter `every_published_field_is_deliberately_allowed` eingetragen.
- **Keine neuen Abhängigkeiten.**

## Akzeptanzkriterien

- [ ] **E1** Ein Spiel mit entschiedenem Satzstand auf belegtem Feld wird
      gestempelt; ein zweiter Sync-Lauf verschiebt den Stempel nicht.
- [ ] **E2** Ein laufendes Spiel wird nicht gestempelt.
- [ ] **E3** Eine Korrektur, nach der der Stand nicht mehr entschieden ist,
      nimmt den Stempel zurück.
- [ ] **E4** Ein Ergebnis in der BTP-Nachschub-Queue verhindert den Stempel.
- [ ] **E5** Die Erkennung stimmt in den Formaten 3×21/30, 3×15/21, 1×21,
      5×11/15 — inklusive Deckel-Patt (30:29) und über dem Deckel (31:29 ⇒
      **nicht** entschieden).
- [ ] **E6** Ein 0:0-Satz am Ende der Liste stört die Erkennung nicht.
- [ ] **E7** Ohne aufgelöste Zählweise wird nicht gewarnt.
- [ ] **E8** Die Anzeige warnt erst ab der eingestellten Frist, nicht davor.
- [ ] **E9** `finished_warning_seconds = 0` schaltet die Warnung vollständig
      ab.
- [ ] **E10** Ein Host ohne dieses Feature (Feld fehlt) führt zu keiner
      Warnung und keinem Fehler.
- [ ] **E11** Bei Verbindungsverlust zeigt das Band die Verbindungsmeldung,
      nicht die Warnung.
- [ ] **E12** Mehrere betroffene Felder erscheinen in einer Zeile, ältester
      Fall zuerst.

## Tests

| Test | Ort | Sichert |
|---|---|---|
| Zählweisen-Matrix (4 Tests) | `server.rs` | E5, E6, E7 |
| Stempel gesetzt / nicht verschoben | `sync.rs` | E1 |
| laufendes Spiel | `sync.rs` | E2 |
| Korrektur nimmt zurück | `sync.rs` | E3 |
| Nachschub-Queue | `sync.rs` | E4 |
| Warn-Regel der Oberfläche (8 Fälle) | Node-Prüfung | E8, E9, E10 |

## Risiken & Rollback

- **Fehlalarm-Sturm** ist das Hauptrisiko einer reinen Anzeige. Rückzugsweg:
  `finished_warning_seconds = 0` in der Config — wirkt ohne neue Version.
- **Rollback:** Ältere Versionen ignorieren die neuen Felder; die Config bleibt
  lesbar. Es gibt keinen Zustand, der ohne dieses Feature falsch wäre.
- Der Stempel liegt im ohnehin bestehenden `match-times.json` und wächst nicht
  über dessen Lebensdauer hinaus.

## Offene Fragen / Annahmen

- **A1 (im Grill korrigiert):** Der Stempel liegt **persistent** im
  Zeitspeicher, nicht im Arbeitsspeicher. Beim App-Start lädt der Host die
  Live-Stände aus `scores.json` zurück; ein RAM-Merker hätte die Frist neu
  starten lassen — die Warnung wäre ausgerechnet nach einem Neustart eine
  Minute lang verschwunden.
- **A2** Eine Korrektur setzt die Frist zurück (statt sie weiterlaufen zu
  lassen). Bewusst: Nach einer Korrektur wird meist weitergespielt.
- **A3 (im Grill präzisiert):** Aufgabe/DQ/Walkover sind nicht ausgeschlossen,
  weil sie ausgeschlossen *werden*, sondern weil ihr Satzstand nie vollständig
  ist. Der Schiedsrichter im Aufgabe-Dialog erzeugt daher einen bewussten,
  hingenommenen Fehlalarm.
- **A4** Die Warnung gilt je **Feld** — sie beschreibt einen Zustand der
  Halle, und die Turnierleitung geht zum Feld.
- **Keine offenen Fragen.** Alle acht Grill-Blocker sind entschieden.

## Doku-Pflicht

`docs/turnierleitung-web.md` (Bedienung) · `docs/changelog.md` ·
Versions-Trippel. Die Wire-Ebene wächst nur additiv im `TlState`, deshalb ein
kurzer Vermerk in `docs/cloud-relay.md`.
