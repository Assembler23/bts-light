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
| **C** | Turnierleitungs-Sicht, Zeiten-Pflege + Ansagen (bts-light) | Sicht und Zeiten stehen, Ansagen offen |

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

### Nur Hauptfeld-Meldungen (seit 2026-08-12)

BTP ordnet jede Meldung einer **Stage** zu — Hauptfeld, Qualifikation,
Reserve, Ausschließen. Auf der Check-In-Liste stehen nur
**Hauptfeld-Meldungen**: Reservisten und Ausgeschlossene sollen sich nicht
einchecken, reine Quali-Teilnehmer erst, wenn sie sich qualifiziert haben.
Gefiltert wird in `entry_list()` über `non_main_stage_entries()`
([`btp/model.rs`](../src-tauri/src/btp/model.rs)); die Anzahl gefilterter
Meldungen wird geloggt (sonst fiele nie auf, warum jemand auf der Seite
fehlt).

**Filterschlüssel ist der numerische `StageType`** (1 = Hauptfeld,
2 = Qualifikation, 8 = Playoff, 9998 = Reserve, 9999 = Ausschließen —
gemessen an den Mitschnitten **und einem laufenden Turnier**), **nie** der
frei benennbare Stage-Name. Drei Quellen, defensiv kombiniert:

1. **`StageEntries`** (`EntryID → StageID`) — in echten Turnieren die
   **maßgebliche** Zuordnung. Gemessen am laufenden Turnier (Struktur-Probe
   `tests/checkin_roster_probe.rs`, 12.08.2026): HE C = 26 Hauptfeld,
   1 Reserve, 4 Ausschließen — **ausschließlich hierüber**; `Entry.StageID`
   war leer. Eine Zuordnung auf Reserve/Ausschließen/Quali schließt aus,
   eine auf Hauptfeld/Playoff hält.
2. **Direkt am Entry** (`Entry.StageID`), falls BTP es doch mitschickt —
   in keinem beobachteten Turnier belegt, als Rückfall belassen.
3. **Über die Platzierung**: eine Meldung, die ausschließlich in Draws von
   Qualifikations-Stages platziert ist, gehört (noch) nicht aufs Hauptfeld.
   Wer sich qualifiziert, bekommt einen Slot in einem Hauptfeld-Draw und
   erscheint damit wieder auf der Liste. Playoff zählt zur Hauptfeld-Seite
   (Turnierverlauf, keine Vorqualifikation).

> **Wichtig — behoben 12.08.2026:** Die erste Fassung (v0.9.185) las nur
> Quelle 2 und 3 und filterte an echten Daten **gar nichts** (HE C blieb
> 31 statt 26), weil BTP die Zuordnung in `StageEntries` führt, nicht am
> `Entry.StageID`. Erst mit Quelle 1 greift der Filter.

Drei Grenzen mit Absicht: **Unplatzierte bleiben immer drin** — vor der
Auslosung gibt es keine Slots, und genau dann muss die Liste vollständig
sein (die Kern-Eigenschaft oben). **Unbekannte Stage-Verweise kosten die
Meldung nie** — im Zweifel steht jemand zu viel auf der Liste, nie jemand zu
wenig; die Turnierleitung kann Überzählige ignorieren, aber ein zu Unrecht
Fehlender kann sich nicht einchecken. Und **badhub braucht dafür nichts zu
wissen** — die Filterung passiert vor dem Push, der `centry_list`-Vertrag
bleibt unverändert (gefilterte Meldungen verschwinden dort wie eine
BTP-Abmeldung).

### Turnier- und Spieler-Identität

| | Schlüssel | Warum |
|---|---|---|
| Turnier | **turnier.de-Turnier-GUID** (36 Zeichen) | Stabil, vorab bekannt, in badhub bereits als `tournaments.tournament_uuid` geführt. Steht **nicht** im BTP-Snapshot und wird einmalig eingetragen. Seit ADR 0054 ein **Pflichtfeld** der ganzen App (`AppConfig.tournament_uuid`, Setup-Abschnitt „1 · Liveticker-Ziel"); der Check-In-Block spiegelt es (`checkin.tournament_uuid`), damit die Leser hier unverändert bleiben. |
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

### Sponsor-Werbebilder der Check-In-Seite (seit v0.9.194)

Die als „Leiste" markierten Court-Werbebilder (Feature *Sponsor-Leiste*,
Spec [`features/werbung-leisten.md`](features/werbung-leisten.md)) erscheinen
zusätzlich klein auf den badhub-Check-In-/Zeitplan-Seiten. Beim Umschalten des
„Leiste"-Häkchens sendet bts-light sie an einen **eigenen** badhub-Endpunkt
(nicht `centry_list`):

| Feld | Wert |
|---|---|
| Endpunkt | `POST /api/checkin-branding` (abgeleitet aus `config.badhub.url`, letztes Pfadsegment ersetzt) |
| Auth | Liveticker-Passwort als Bearer — **derselbe** Kanal, Body trägt seit ADR 0054 zusätzlich `tournament_uuid`; badhub nutzt sie über den Kindschlüssel |
| Body | `{"sponsors": ["<roh-Base64>", …], "logo": "<roh-Base64>", "tournament_uuid": "<GUID>"}` — Sponsoren/Logo optional, GUID weggelassen ohne gültige Kennung |
| Nachrichtentyp | `CheckinBrandingMessage` ([`badhub/payload.rs`](../src-tauri/src/badhub/payload.rs)) |

Die Turnier-Zuordnung passiert **primär über das Bearer-Passwort** (badhub:
`liveUpdateAuth` → `tournament_key` → Check-In-UUID); die mitgesendete GUID
löst seit ADR 0054 zusätzlich das richtige Kind-Turnier auf, wenn mehrere
Turniere denselben Verbandszugang teilen. **Additiv und feuer-und-vergiss**:
ohne konfiguriertes badhub-Passwort passiert nichts; HTTP 404/400 bedeutet
„badhub kennt den Endpunkt noch nicht" (ältere Version) und wird — wie beim
Roster-Push — nur geloggt, nicht als Fehler gezeigt. Datenschutzlich
unkritisch: übertragen werden nur die vom Operator selbst hochgeladenen
Werbebilder und das Turnierlogo, keine Personendaten.

**Feld-unabhängig (seit v0.9.195):** Sponsoren (jpg/png/gif, max 4) und
Turnierlogo (png/jpg/webp) reisen über **dieselbe** Nachricht, aber getrennt
auslösbar: das „Leiste"-Häkchen sendet nur `sponsors`, das Speichern der
Einstellungen nur das **geänderte** `logo`. Ein weggelassenes Feld lässt badhub
den jeweils anderen Bestand unberührt; ein leeres `logo` löscht es. Damit reist
das (bis 2 MB große) Logo **einmalig beim Speichern** statt wie bisher alle 60 s
im Liveticker-`tset`. badhub legt es als `checkin_{uuid}.{ext}` ab und bevorzugt
diese Datei vor dem tset-Snapshot; die tatsächliche Entfernung aus dem `tset`
folgt in einem separaten Schritt nach dem badhub-Deploy.

### GUID in allen Push-Nachrichten (seit ADR 0054)

Die turnier.de-Turnier-GUID reist kanonisch (`AppConfig.tournament_uuid`) in
jeder Nachricht an badhub mit: `tset.event.tournament_uuid`,
`sched.event.tournament_uuid`, `tupdate_match.tournament_uuid`,
`centry_list.tournament_uuid`, `checkin-branding.tournament_uuid`. Sie wird
weggelassen, wenn keine gültige GUID konfiguriert ist — dann verhält sich
badhub wie vor ADR 0054 (ein Stand je Verbandszugang).

## Einrichtung durch die Turnierleitung

Im Einrichtungs-Assistenten, Abschnitt **Hallen-Check-In**:

1. Häkchen setzen.
2. Die Turnier-Kennung steht bereits im Abschnitt „1 · Liveticker-Ziel" (dort
   einmalig die turnier.de-Adresse einfügen,
   [`tournamentGuid.ts`](../src/tournamentGuid.ts) liest die Kennung heraus)
   — im Check-In-Abschnitt selbst ist nichts mehr einzutragen.
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

## Die Sicht der Turnierleitung

Seitenleisten-Punkt **Check-In**
([`pages/CheckinPanel.tsx`](../src/pages/CheckinPanel.tsx)). Je Klasse stehen
dort Zustand, Zeiten und die Zählung „x von y da"; aufgeklappt die Namen mit
ihrem Zustand.

**TL-Web-Panel „Anfangszeiten"** (seit 17.08.2026): Der heutige Zeitplan
aus demselben `GET …/tl/stand` steht auch der Turnierleitungs-Seite zur
Verfügung — je Klasse Anfangszeit, Anmeldeschluss (ohne eigenen gilt die
Anfangszeit, wie beim Ansage-Countdown) und die Zähler, **ohne
Spielerlisten**: `checkin_state::tl_ablage` streift sie vor dem Ablegen
ab und rechnet dabei — wie die Desktop-Seite — die Abgemeldeten aus
`gemeldet` heraus; Namen bleiben der Desktop-Seite vorbehalten. Dafür
ruft der **Turnier-PC-Kern** den Stand höchstens minütlich ab — als
eigener, vom Liveticker-Zyklus entkoppelter Tick (`sync.rs`
`CheckinLese`/`checkin_lese_tick`, gespawnt in `commands.rs`; er kann
den Liveticker nie verzögern und läuft auch bei BTP-Ausfall).
RAM-Zwischenstand in `tablet/state.rs` `checkin_classes` (die
„kein Cache"-Regel AK-C13 betrifft Persistenz) — bislang fragte nur die
offene Desktop-Seite. Nur aktiv, wenn **TL-Web eingeschaltet** ist.
Ablehnung durch badhub (401/403/400/404) räumt den Stand und pausiert
30 Minuten (wie Roster/Spielplan); ein Offline-Aussetzer lässt den
letzten Stand stehen, ab fünf Minuten markiert `TlState.checkin_stale`
ihn als möglicherweise veraltet. Bedienung:
[turnierleitung-web.md](turnierleitung-web.md).

**Doppel stehen als eine Zeile je Meldung** („A / B"): Zwei Spieler mit
derselben `entry_id` werden zusammengefasst
([`io/checkinPairs.mjs`](../src/io/checkinPairs.mjs)), jede Hälfte behält
Zustand und Knöpfe — ein- und ausgecheckt wird weiterhin **einzeln**, alle
Zählungen bleiben spielerbasiert. Ein Paar entsteht nur bei `entry_id > 0`
und **genau zwei** Trägern: Ein badhub vor der `entry_id`-Auslieferung
schickt überall 0 (nichts klumpt), unvollständige Doppel und Datenfehler
(3+ Träger) bleiben ehrliche Einzelzeilen.

Im Kopf der Seite stehen drei Knöpfe zur **öffentlichen badhub-Seite**:
„Check-In-Seite öffnen" (Standard-Browser), „Link kopieren" (Zwischenablage,
fürs Weitergeben per Messenger) und „Aushang (QR)" (die druckbare
Aushang-Seite für die Halle). Die Adressen baut das **Backend**
(`public_url`/`poster_url` in der `checkin_state`-Antwort, Helfer in
[`badhub/checkin_state.rs`](../src-tauri/src/badhub/checkin_state.rs)) —
das Frontend setzt keine URLs zusammen, Basis und GUID kennt nur die Config.

Ein in badhub **abgemeldeter** Spieler (`state = withdrawn`, dort über die
Verwaltung gesetzt) erscheint grau und durchgestrichen als „abgemeldet" —
weder als da noch als fehlend. Die Zählungen „x von y da" rechnen Abgemeldete
heraus, sowohl je Klasse als auch in der Gesamtsumme oben; sie tauchen nur in
einer eigenen, gesonderten Zahl auf.

Der Punkt ist ausgegraut, solange kein Häkchen **oder** keine Turnier-Kennung
gesetzt ist — ohne Kennung erreicht der Check-In badhub nie, und eine Seite,
die nur „nicht eingerichtet" sagt, führte in die Irre. Der Klick springt dann
in den passenden Abschnitt der Einstellungen.

### Woher die Daten kommen

Über den **Turnierleitungs-Kanal** von badhub — drei Bearer-authentifizierte
Endpunkte unter `/checkin/<GUID>/tl/`, bedient von
[`badhub/checkin_state.rs`](../src-tauri/src/badhub/checkin_state.rs) und
freigegeben über die Commands `checkin_state`, `checkin_set_player` und
`checkin_set_times`. Der Browser spricht badhub **nie** direkt an (R1); das
Liveticker-Passwort bleibt dadurch im Backend.

Der Kanal liefert mehr als die öffentliche Seite: Rückfrage-Status, Sperre und
Herkunft des Check-Ins (`selbst` · `durch Partner` · `Turnierleitung`). Nach
außen verlassen diese Felder badhub nie.

### Vier Zustände statt eines Fehlers

`checkin_state` liefert **nie** `Err`. Stattdessen trägt jede Antwort, ob der
Check-In gerade benutzbar ist:

| | Bedeutung | Was die Seite zeigt |
|---|---|---|
| `ready` | badhub hat geantwortet | den Stand |
| `offline` | keine Verbindung | „Der Check-In braucht Internet" — plus den Hinweis, dass das Turnier unverändert weiterläuft |
| `unsupported` | badhub kennt den Kanal noch nicht (404/400) oder der Check-In ist nicht eingerichtet | denselben ruhigen Hinweis |
| `rejected` | Passwort oder Kennung passen nicht (401/403) | die Aufforderung, beides zu prüfen |

`rejected` ist der einzige Fall, den die Turnierleitung selbst beheben kann —
deshalb ist er als einziger von „offline" unterschieden. Ein 5xx wird wie
offline behandelt: der nächste Abruf versucht es erneut.

### Eingreifen und Zeiten pflegen

- **Setzen** trägt den Spieler mit Herkunft `Turnierleitung` ein.
- **Zurücksetzen** räumt den Check-In ab **und sperrt den Selbst-Check-In**.
  Ohne die Sperre klickt der Spieler auf seinem Handy einfach erneut und die
  Korrektur wäre folgenlos. **Entsperren** hebt nur die Sperre auf.
- **Zeiten** gehen direkt an badhub; die Plausibilität (Anmeldeschluss nicht
  vor der Anfangszeit) prüft **badhub**. Eine zweite Regel hier wäre eine
  zweite Wahrheit, die auseinanderlaufen kann.
- **Abgemeldete lassen sich trotzdem einchecken.** Der „da"-Knopf bleibt für
  sie verfügbar, sein Tooltip weist eigens darauf hin („Trotz Abmeldung als
  anwesend eintragen") — falls jemand entgegen der Abmeldung doch antritt.
  Ein reines **Wieder-Anmelden** ohne gleichzeitiges Einchecken gibt es in
  der TL-Sicht bewusst nicht: die Abmeldung selbst kommt aus badhubs
  Verwaltung und wird nur dort rückgängig gemacht.

Nach jeder Änderung wird neu abgerufen, statt den Stand lokal fortzuschreiben —
sonst stünde nach einer abgelehnten Änderung etwas anderes auf dem Bildschirm
als in der Datenbank. Ohne Verbindung wird der Versuch **gemeldet**, nicht
zwischengespeichert.

**Ein Slave greift nicht ein.** `slave_mode` lehnt beide Schreib-Commands ab;
die Sicht bleibt lesbar (Mehr-Hallen-Regel: genau ein Master schreibt).

Gepollt wird alle **15 Sekunden** — träger als die 4 s der
Vorbereitungs-Seite. badhub läuft auf Shared Hosting und bedient zur
Fensteröffnung gleichzeitig die halbe Halle; der Check-In-Stand ändert sich in
Minuten, nicht in Sekunden.

### Ansagen

Zwei Knöpfe je Klasse, beide **nur auf Klick** — die App sagt nie von selbst
etwas an:

- **Anmeldeschluss** → „Noch 25 Minuten bis Anmeldeschluss Herrendoppel B."
  Fehlt der eigene Anmeldeschluss, gilt die Anfangszeit. Ist er vorbei,
  entfällt die Ansage.
- **Fehlende** → bis `missing_names_max` Namen werden genannt, **darüber nur
  die Anzahl** („In Herrendoppel B fehlen noch 23 Anmeldungen"). Sonst liefe
  die Ansage kurz nach Fensteröffnung minutenlang. Fehlt niemand, erscheint
  der Knopf nicht.

Der Text entsteht in `checkin_announcement` aus einem **frisch geholten**
Stand, nicht aus dem, was gerade auf dem Bildschirm steht: zwischen Poll und
Klick können 15 Sekunden liegen, und eine Ansage, die einen bereits
Eingecheckten ausruft, schickt jemanden umsonst zur Turnierleitung.

Gerufen wird **ohne Hallen-Filter** — eine Klasse startet zwar in einer Halle,
der Check-In gilt aber turnierweit. Abgespielt wird über den bestehenden
Freitext-Weg (`publish_freetext`); ohne eingerichtete Ansagen erscheinen die
Knöpfe nicht, weil dann niemand spräche.

Wer eine **Rückfrage** hat, zählt als fehlend: er soll ohnehin zur
Turnierleitung.

Wer **abgemeldet** ist, zählt umgekehrt nicht als fehlend und wird nie
ausgerufen: er soll gerade nicht zur Turnierleitung.

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
- [`badhub/checkin_state.rs`](../src-tauri/src/badhub/checkin_state.rs) — gegen
  einen HTTP-Mock: Stand mit Sperre und Herkunft, fehlende Spieler (inklusive
  `query`), 404 → `unsupported`, 403 → `rejected`, 5xx und kein Netz →
  `offline`, Turnier ohne Push → leer aber `ready`, abgelehnter Schreibvorgang
  meldet den Text von badhub statt ihn zu verschlucken.
- Ansagetexte als **reine** Funktionen (ebenda): Einzahl/Mehrzahl der Minuten,
  Ansage entfällt nach Anmeldeschluss, Anfangszeit als Rückfall, unter/über
  `missing_names_max`, leere Fehlt-Liste, `query` zählt als fehlend.
- [`scripts/test-checkin-pairs.mjs`](../scripts/test-checkin-pairs.mjs)
  (Node, im CI) — Doppel-Gruppierung der Anzeige: Paar nur bei `entry_id > 0`
  und genau zwei Trägern, `entry_id` 0 klumpt nie, Reihenfolge bleibt.
