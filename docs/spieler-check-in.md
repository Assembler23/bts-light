# Hallen-Check-In

Spieler bestätigen vor Beginn ihrer Spielklasse über eine öffentlich
erreichbare Webseite selbst, dass sie in der Halle und spielbereit sind. Die
Turnierleitung sieht dadurch **vor der Auslosung**, wer da ist und wer fehlt,
und kann Fehlende gezielt ausrufen lassen — statt am Anmeldetisch einen Stau
zu erzeugen.

Spezifikation: [features/spieler-check-in.md](features/spieler-check-in.md) ·
Entscheidung: [ADR 0009](adr/0009-hallen-checkin-persistenz-und-identitaet.md).

> **Nicht zu verwechseln mit BTPs eigenem Check-in.** BTP führt
> `Player.CheckedIn`/`FirstCheckIn` **am Spieler und turnierweit**; der Fall
> „in Herrendoppel B anwesend, in Herreneinzel A noch nicht" ist damit nicht
> abbildbar. Der Hallen-Check-In gilt **je Klasse** und fließt **nicht** nach
> BTP zurück.

## Stand der Umsetzung

Das Feature ist in drei nacheinander lieferbare Schnitte geteilt.

| Schnitt | Inhalt | Stand |
|---|---|---|
| **A** | Meldelisten-Push (bts-light) + Persistenz und Verwaltung (badhub) | bts-light-Teil steht |
| **B** | Öffentliche Check-In-Seite + QR-Aushang (badhub) | offen |
| **C** | Turnierleitungs-Sicht, Zeiten-Pflege + Ansagen (bts-light) | offen |

**Solange der badhub-Teil von Schnitt A nicht ausgerollt ist**, antwortet der
Endpunkt mit 404 — bts-light legt den Meldelisten-Push dann für die laufende
Sitzung still (siehe „Versionsschiefstand" unten). Es entsteht kein Fehler und
keine Warnung im Dashboard.

## Wie es zusammenhängt

```
BTP ──SENDTOURNAMENTINFO──▶ bts-light ──centry_list (HTTPS)──▶ badhub
 (Events + Entries)          (Master)                          (Persistenz)
                                                                    │
                                              Spieler ◀─ Check-In-Seite (QR)
```

### Woher die Meldeliste kommt

Aus dem BTP-Snapshot, über **`Entry.EventID`**. Das ist der Dreh- und
Angelpunkt des Features: Eine Meldung kennt ihre Klasse **direkt** und braucht
dafür weder Draw noch Match. Die Meldeliste steht deshalb schon **vor der
Auslosung** bereit — genau dann, wenn der Check-In sie braucht.

Die sonst übliche Kette `From → Slot → Entry → Player` (siehe
[btp_protocol.md](btp_protocol.md)) taugt dafür **nicht**: sie setzt Matches
voraus, die es vor der Auslosung nicht gibt.

Im Code:

- [`btp/model.rs`](../src-tauri/src/btp/model.rs) — `BtpEvent`, `BtpEntry`,
  `event_list()`, `entry_list()`. Die bestehende `entry_map()` bleibt
  unangetastet; sie bedient weiterhin die Match-Auflösung.
- Meldungen **ohne** `EventID` oder ohne auflösbare Spieler werden verworfen —
  ein namenloser Eintrag wäre auf der Check-In-Seite nicht anklickbar.

### Turnier- und Spieler-Identität

| | Schlüssel | Warum |
|---|---|---|
| Turnier | **turnier.de-Turnier-GUID** (36 Zeichen) | Stabil, vorab bekannt, in badhub bereits als `tournaments.tournament_uuid` geführt. Steht **nicht** im BTP-Snapshot und wird einmalig eingetragen. |
| Authentifizierung | Liveticker-Passwort (Bearer) | Der bestehende, erprobte Kanal — kein zweiter Auth-Weg. |
| Spieler | BTP-`PlayerID` | Innerhalb des Turniers stabil und **immer** vorhanden. |
| Spieler (optional) | `MemberID` (Lizenznummer) | Brücke zu badhubs `players.dbv_licence_nr` fürs Anonymisierungs-Gate. **Nie Pflicht** — ein Turnier ohne gepflegte Lizenznummern funktioniert vollständig. |

Begründung und verworfene Alternativen stehen in
[ADR 0009](adr/0009-hallen-checkin-persistenz-und-identitaet.md).

### Was gesendet wird

Nachrichtentyp **`centry_list`** an denselben Endpunkt wie der Liveticker
([`badhub/payload.rs`](../src-tauri/src/badhub/payload.rs)):
Turnier-GUID, Turniername, Klassen (EventID, Name, Disziplin) und Meldungen
(EntryID, EventID, Spieler).

**Gesendet wird nur bei echter Änderung** — Nachmeldung, Abmeldung,
korrigierter Name, umbenannte Klasse, nachgepflegte Lizenznummer,
Turnierwechsel. Ohne diesen Filter gingen mehrere hundert Namen im
5-Sekunden-Poll-Takt über die Leitung.

Verglichen wird die **Nachricht selbst** (`same_content_as`, ohne `rid`), nicht
ein eigener Fingerabdruck: Ein zweites Feldschema würde beim nächsten
zusätzlichen Payload-Feld stillschweigend auseinanderlaufen, und die
Meldeliste wäre dann veraltet, ohne dass es jemand merkt.

Anders als beim `tset` gibt es **keinen Heartbeat** — die Meldeliste sind
Stammdaten, badhub hält sie dauerhaft.

## Einrichtung durch die Turnierleitung

Im Einrichtungs-Assistenten, Abschnitt **Hallen-Check-In**:

1. Häkchen setzen.
2. Das Turnier bei turnier.de öffnen und **die Adresse aus dem Browser
   einfügen** — die Kennung wird automatisch herausgelesen
   ([`tournamentGuid.ts`](../src/tournamentGuid.ts)). Die GUID direkt geht
   auch.
3. Optional: bis zu wie vielen fehlenden Spielern die Ansage Namen nennt
   (Standard 8, darüber nur die Anzahl).

Ohne gültige Kennung bleibt der Check-In **aus**, auch wenn das Häkchen
gesetzt ist — sonst stünde er als „aktiv" im Dashboard, ohne dass badhub je
etwas erhielte.

## Wo die Zeiten gepflegt werden

Anfangszeit und Anmeldeschluss sind **an beiden Stellen bedienbar**: vorab in
badhub vom Schreibtisch (bevor BTP oder bts-light laufen) und am Turniertag in
bts-light, wenn ein Turnier in Verzug gerät.

**badhub speichert, bts-light schreibt durch.** Es gibt genau einen
gespeicherten Wert und zwei Eingabemasken — bts-light hält **keine** eigene
Kopie. Ein lokaler Zwischenspeicher würde die zweite Wahrheit erzeugen, die
dieses Modell gerade vermeidet; ohne Verbindung sind die Zeiten in bts-light
deshalb nur lesbar.

Der **Rückfrage-Status** bleibt bewusst nur in badhub: er entsteht beim
Zahlungsabgleich Tage vor dem Turnier.

*(Die bts-light-Seite kommt mit Schnitt C.)*

## Grenzen und Randfälle

- **Genau ein Master schreibt.** Der Push steht hinter dem
  `slave_mode`-Return in [`sync.rs`](../src-tauri/src/sync.rs); Slaves und
  Zweit-Master senden nie (siehe [multi-hall.md](multi-hall.md)).
- **Braucht Internet.** Im reinen LAN-Betrieb ohne Internet ist der Check-In
  nicht verfügbar. Das Turnier läuft unverändert weiter — das Feature ist
  **additiv**, es hängt weder an der Feldvergabe noch an Ergebnissen.
- **Reihenfolge im Sync-Zyklus.** Der Roster-Push läuft **nach** dem
  Liveticker-Push. Der Liveticker ist die zeitkritische Funktion, der Check-In
  die additive: stünde er davor, könnte ein hängender Check-In-Endpunkt die
  Ergebnis-Übertragung um seinen ganzen Timeout verzögern — bei einem
  5-Sekunden-Poll-Takt ein spürbarer Aussetzer.
- **Versionsschiefstand.** bts-light kommt per Auto-Update auf alle
  Installationen, badhub wird unabhängig deployt. Antwortet badhub mit
  **404/400**, kennt es den Check-In noch nicht → der Push pausiert 30 Minuten
  und wird dann erneut versucht. Bewusst **keine** dauerhafte Stilllegung:
  derselbe Status kann von einem kurzen Aussetzer während eines
  badhub-Deploys stammen, und ein Turnier läuft über mehrere Tage. Ein **5xx**
  pausiert gar nicht — der nächste Zyklus sendet die vollständige Liste erneut.
- **Unvollständige Doppel-Meldung.** Nennt BTP zwei Spieler, ist aber einer
  nicht auflösbar, bleibt die Meldung erhalten (der anwesende Partner soll
  einchecken können) und erscheint als Einzel-Meldung. Das ist ein Datenfehler
  in BTP und wird protokolliert, statt still zu bleiben.
- **Ein Spieler in mehreren Klassen** checkt je Klasse einzeln ein. Der
  Check-In gilt je Klasse, nicht je Person.
- **Kein Rückfluss nach BTP** und **keine Kopplung an die Feldvergabe** —
  Letzteres bewusst: sonst hinge die Feldvergabe an einer ungeprüften
  Selbstauskunft vom Handy.

## Datenschutz

- **Kein Geburtsjahr** — weder gespeichert noch gesendet noch geloggt. Ein
  Test im Payload-Modul prüft das ausdrücklich.
- Gesendet werden Vor- und Nachname sowie, **falls BTP sie führt**, Verein und
  Nationalität. Beide Felder sind in BTP optional (im Testmitschnitt leer) und
  dienen der Unterscheidung Gleichnamiger; fehlen sie, werden sie weggelassen
  statt leer gesendet.
- Der Status „Rückfrage an Turnierleitung" (Schnitt B) wird auf der
  öffentlichen Seite **nie als Zustand ausgeliefert** — er hat typischerweise
  einen finanziellen Hintergrund und darf nicht neben einem Klarnamen stehen.
- Namen laufen badhub-seitig durch das Anonymisierungs-Gate (Art. 17).

## Tests

- [`btp/model.rs`](../src-tauri/src/btp/model.rs) — Meldeliste je Klasse ohne
  jede Auslosung, Spieler ohne Lizenz/Verein, Verwerfen kaputter Meldungen.
- [`tests/btp_capture.rs`](../src-tauri/tests/btp_capture.rs) — gegen den
  echten Zwei-Hallen-Mitschnitt: 2 Klassen, 10 Meldungen, 5 Spieler in
  **beiden** Klassen (zugleich der Beleg für den Mehrklassen-Fall).
- [`badhub/payload.rs`](../src-tauri/src/badhub/payload.rs) — Wire-Form,
  Doppelpartner, kein Geburtsjahr, Klassen ohne Meldung entfallen.
- [`badhub/diff.rs`](../src-tauri/src/badhub/diff.rs) — was einen Push auslöst
  und was nicht.
- [`sync.rs`](../src-tauri/src/sync.rs) — gegen einen HTTP-Mock: einmal statt
  zweimal senden, nichts ohne Kennung, nichts wenn ausgeschaltet, 404 legt
  still, 500 wird wiederholt.
- [`config.rs`](../src-tauri/src/config.rs) — alte `config.json` lädt mit
  Defaults (Auto-Update-Pfad), Kennungs-Format.
