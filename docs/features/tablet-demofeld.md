# Demofeld: Übungsspiel am Zähltablett — Spezifikation

> Status: **freigegeben 2026-11-15** (im Gespräch; der Nutzer hat sich für
> direkte Umsetzung statt `/idee` entschieden), umgesetzt in v0.9.416.

## Kontext / Problem

> Schiedsrichter oder andere unerfahrene Personen wollen gerne das Tablet
> einmal testen. […] Wichtig ist, dass man es gut auch während eines
> laufenden Turniers verwenden kann, ohne echte Daten des Turniers. […] Ich
> will es auch in den normalen Tablets des Turniers einfach in der App
> auswählbar haben.

Bisher konnte man das Tablet nur an einem echten Feld mit einem echten Spiel
ausprobieren. Das ging im Turnier nicht: Es nahm der Turnierleitung ein Spiel
weg, belegte ein Feld und schickte Stände an Monitore und Liveticker.

## Verhalten

### Einstieg (an jedem Turnier-Tablet)

- **Feldwahl** (`/felder`, auch die Startseite der Kiosk-App): Die Kachel
  **Demofeld** steht unter den Feldern, ohne PIN.
- **Zahnrad-Menü → Feld wechseln:** Der letzte Eintrag heißt
  **Demofeld · Übungsspiel**. Das funktioniert auch im Cloud-Modus, wo es
  keine Feldwahl-Seite gibt.
- **Slave-Brücke (ferne Halle):** Die Feld-Auswahlseite der Brücke zeigt den
  Eintrag „Demofeld · Übung". Die Seite liefert die Brücke selbst aus, auch
  ohne Träger.
- Route: `/court/demo` (LAN, Slave-Brücke), `/{ns}/court/demo` (Relay).

### Übungsspiele

Auswahl aus `DEMO_SZENARIEN` in `tablet.html`. Alle Spiele gehen über zwei
Gewinnsätze bis 21 (Deckel 30, Pause bei 11):

| Szenario | Start |
|---|---|
| Einzel | Damen-Einzel, 0:0 |
| Doppel | Herren-Doppel, 0:0 |
| Mixed | Mixed, 0:0 |
| Einzel · Entscheidungssatz | Sätze 21:17, 18:21, dritter Satz ab 0:0 (Seitenwechsel bei 11) |
| Doppel · Verlängerung | erster Satz bei 19:19 |
| Mixed · Spielpunkt | 21:16, zweiter Satz bei 20:18 |

Die Namen sind ausgedacht („Anna Beispiel", „Berta Muster" …). Ein
Zwischenstand läuft über `einstiegMitSaetzen`, also über denselben Weg wie
der Tablet-Wechsel mitten im Spiel: Seitenwahl und Aufstellung werden
abgefragt. **Erweitern** heißt: einen Eintrag ergänzen (`titel`, `text`,
`klasse`, `a`, `b`, optional `saetze` in Team-Koordinaten).

Seitenwahl, Aufschlag, Pausen, Karten, Korrektur, „Match beenden",
„Ergebnis eintragen", Schiri-Modus und Große Schrift verhalten sich wie im
echten Spiel. Das Zahnrad öffnet **ohne PIN** und zeigt zusätzlich **Neues
Übungsspiel wählen**. „Anzeige (nur Spielstand)" ist ausgeblendet. Nach dem
Übermitteln steht dort „✓ Übung beendet — im echten Spiel ginge das Ergebnis
jetzt an die Turnierleitung." mit dem Knopf **Neues Übungsspiel**.

Kennzeichnung: ein schwaches, diagonales **ÜBUNG** über allen Ansichten
(ohne Tipp-Abfang), oben rechts ein violetter Punkt mit „Übung · nichts wird
übertragen", Kopfzeile „Übung · <Klasse>".

### Abkapselung (Pflicht — es läuft parallel zum Turnier)

1. **Kein WebSocket**: `connect()` kehrt sofort zurück. Das Tablet meldet
   sich nie als Feld an. Damit gehen auch Live-Stand, Spiegel, Verlauf,
   Zettel-Ereignisse, Akku und Meldungen (✚/📣) nirgendwohin.
2. **Netzsperre** `demoNetzSperren`: Eine Stelle für alle Wege. `fetch` lässt
   nur `GET …/courts` und `GET …/info/ad/state` durch. `POST …/result`
   antwortet lokal mit `{ok:true}`, alles andere scheitert.
   `WebSocket`/`sendBeacon` sind gesperrt (die `WebSocket.*`-Konstanten
   bleiben).
3. **Eigener Speicher**: `SPEICHER_FELD = 'demo'` in allen feldbezogenen
   `localStorage`-Schlüsseln (Stand, Log, Karten, Ring, geparkte Ergebnisse,
   Schiri-Modus).
4. **Server unberührt**: Die Routen rendern nur die Seite. Das Relay legt
   keinen Namespace an, der Host keinen Feldzustand. CourtID `0` gibt es in
   BTP nicht.

## Bewusst nicht

- Kein Ballzähler (hängt am Turnier-Schalter, den das Demofeld nicht kennt).
- Keine Aufruf-Uhr (es gibt keinen Aufruf).
- Kein Zettel/Punktverlauf beim Turnier-PC.
- Keine Szenario-Verwaltung in der Windows-App; Szenarien leben im Code.

## Tests

- `relay-proto`: `demo_seite_schaltet_uebungsmodus_ein_und_fuellt_alle_platzhalter`
  (echte `tablet.html`, Schalter-Text als Anker).
- LAN: `demofeld_route_liefert_uebungsmodus_und_laesst_echte_felder_normal`
  (statische Route neben `/court/{id}`).
- Relay: `demofeld_seite_im_uebungsmodus_ohne_namespace_anzulegen` (Router
  baut, kein Namespace).
- JS: `scripts/test-demo-netzsperre.mjs` (CI) prüft die echte `demoNetzSperren`
  aus `tablet.html`: nur die zwei Lesewege kommen durch, `/result` wird lokal
  bestätigt, alles andere abgewiesen.
- Browser (manuell, Mock-Server + Playwright): komplettes Doppel und Mixed
  bis zum Übermitteln. Es gab keinen WebSocket-Versuch, nur die zwei
  erlaubten GETs, und `badhub.tablet.state.<echtes Feld>` blieb unverändert.
