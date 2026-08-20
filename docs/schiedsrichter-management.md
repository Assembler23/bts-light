# Schiedsrichtermanagement

BTS Light übernimmt die Schiedsrichterliste aus BTP (`Officials`-Container),
zeigt sie in Client und TL-Web, erlaubt SR/AR-Zuweisungen je Spiel (mit
Konflikt-Warnung und automatischer Rotation) und schreibt Zuweisungen nach
BTP zurück. Spec: [features/schiedsrichter-management.md](features/schiedsrichter-management.md) ·
ADRs: [0021 (Rücksync)](adr/0021-officials-ruecksync-eigenstaendiger-write.md),
[0022 (Ablage Turnierdaten)](adr/0022-officials-turnierdaten-eigene-datei.md) ·
BTP-Draht: [btp_protocol.md](btp_protocol.md) („Officials: Struktur & Schreibweg").

> **Stand: vollständig umgesetzt** (v0.9.201, Schritte 1–12 des Spec-Plans).
> Offen ist nur der Feldtest am Turnier — und im Cloud-Betrieb der
> Relay-Deploy, ohne den TL-Web die neuen Aktionen nicht absetzen kann.

## Konfiguration

`AppConfig.officials` (`config.rs::OfficialsConfig`, Spiegel in
`src/types.ts`, Schalter im SetupWizard-Abschnitt „Schiedsrichter"):

| Feld | Default | Bedeutung |
|---|---|---|
| `enabled` | `false` | Mit Schiedsrichtern spielen. Aus ⇒ keine SR/AR-Elemente in Client, TL-Web, Tablet und Ansagen (Bestandsverhalten). |
| `rotation_sr` | `false` | Automatische Rotation für Schiedsrichter (Official1). |
| `rotation_ar` | `false` | Automatische Rotation für Aufschlagrichter (Official2). |

Alle Felder tragen `#[serde(default)]` — ältere `config.json` bleiben lesbar,
Bestandsinstallationen verhalten sich nach dem Auto-Update unverändert
(Tests `officials_default_off_and_old_config_stays_readable`,
`officials_block_with_missing_keys_falls_back_to_defaults`,
Roundtrip in `save_then_load_roundtrip`).

**Bewusste Aufteilung (ADR 0022):** In der `config.json` liegen nur diese
**geräteweiten** Schalter. Alles Turnier-Spezifische — feldweise Schalter,
Rotationsreihenfolge, Pausen, Sperrlisten, Vereins-Overrides, lokale
Zuweisungen — liegt in einer **turniergebundenen Datei** im
App-Datenverzeichnis (siehe unten). Sperrlisten sind Personendaten: Sie dürfen
weder ins Identitäts-Export-Bündel noch in den Broadcast-TL-State wandern;
die Datei wird bei Turnierwechsel verworfen.

## Gelesene BTP-Daten (Schritt 2)

- `BtpSnapshot::officials` — Liste aus dem `Officials`-Container
  (`BtpOfficial { id, name, first, nationality }`, `display_name()`);
  fehlender Container ⇒ leere Liste, kein Fehler.
- `BtpMatch::official1_id` / `official2_id` — SR bzw. AR am Spiel
  (`0` gilt als nicht gesetzt; Semantik an der BTP-Maske verifiziert,
  Messung 13.08.2026).
- BTP liefert **keinen Verein** am Official — der Stammverein wird in
  BTS Light gepflegt (Basis der Vereins-Konflikt-Warnung, siehe Roster).

## Roster-Speicher (Schritt 4)

`tablet/officials.rs::OfficialsStore` hält alles, was BTP **nicht** kennt.
Er hängt im `TabletState` (`officials_store()`), damit LAN-Server,
Relay-Client und Tauri-Commands denselben Stand sehen. Die Stammliste selbst
wird nie gespiegelt — sie bleibt BTPs (R2), der Speicher kennt nur
Official-IDs.

| Inhalt | Form | Zweck |
|---|---|---|
| Rotationsreihenfolge | `order: Vec<i64>` | Reihenfolge der Auto-Rotation, manuell umsortierbar (`set_order`) |
| Zusatzdaten je Official | `OfficialExtra { paused, club, blocked_clubs, blocked_players }` | Pause, Stammverein (BTP liefert keinen), Sperrlisten |
| Lokale Zuweisungen | `assignments: Match-ID → MatchOfficials { sr, ar }` | Overlay für Spiele, an denen BTP nichts stehen hat |
| Feldweise Schalter | `courts: CourtID → CourtSwitches { sr, ar, operator }` | SR-Rotation, AR-Rotation, Tabletbediener-Vergabe je Feld |

**Zuweisungen hängen am Match, nicht am Feld.** Nach Spielende bleiben sie
liegen — sie sind die Grundlage der Einsatz-Ableitung (Spec Nr. 11: keine
eigene Historien-Datenhaltung). Geräumt wird nur beim Abschalten von
`officials.enabled` (`clear_assignments`) und beim Turnierwechsel.

**Feldweise Schalter: Default alles aktiv.** Ohne Eintrag gilt
`CourtSwitches::default()` (alle drei `true`); ein auf Default gesetztes Feld
verliert seinen Eintrag wieder. So bleibt das Bestandsverhalten der
Zähltafelbediener-Vergabe unverändert.

### Ablage (ADR 0022)

`<app_data>/officials-state.json`, im Kopf der BTP-Turniername. Geschrieben
wird bei jeder Änderung, atomar (Temp-Datei + Umbenennen) und best effort:
Ein Schreibfehler kostet höchstens die Einteilung, nie ein Ergebnis.

Der Pfad wird beim App-Start gesetzt (`commands.rs::tablet_officials_path`),
das Turnier kommt mit dem ersten Snapshot (`TabletState::set_snapshot` →
`set_tournament` + `sync_roster`). Dabei gilt:

- **Erststart:** Nur ein Datei-Stand **desselben** Turniers wird übernommen —
  ein App-Neustart mitten im Turnier verliert also nichts.
- **Turnierwechsel oder fremder Datei-Stand:** verworfen und sofort
  überschrieben. BTP-IDs gelten nur innerhalb eines Turniers, und Sperrlisten
  sollen kein Turnier überleben. Ein **Umbenennen** des laufenden Turniers
  wirkt deshalb wie ein Wechsel (ADR 0022: lieber verwerfen als falsch
  zuordnen).
- **Leerer Turniername** (Startphase) ändert nichts.
- **Datei vorhanden, aber nicht lesbar** (Virenscanner, hängendes Handle,
  Rechte): Der Stand wird **nicht** verworfen und **nicht** überschrieben —
  der Betrieb läuft im RAM weiter, der nächste Snapshot versucht das Laden
  erneut und holt den Stand nach, sobald die Datei wieder freigegeben ist.
  Nur ein *kaputter Inhalt* (ungültiges JSON) beginnt leer, denn dort ist
  nichts zu retten. Beides wird geloggt.
- **Roster-Abgleich:** Neue BTP-Officials kommen hinten an die Reihenfolge;
  wer aus BTP verschwindet, behält Position und Zusatzdaten (inert) — kehrt
  er zurück, gelten sie wieder.

## Konflikt-Erkennung (Schritt 5)

`officials.rs::official_conflict(extra, players) -> Option<ConflictKind>` —
eine reine Funktion, geprüft in der Reihenfolge der Spec:

1. **Verein** — der gepflegte Stammverein des Officials ist am Spiel beteiligt.
2. **Verein** — ein an ihm gepflegter Sperr-Verein ist beteiligt.
3. **Person** — ein an ihm gepflegter Sperr-Spieler (BTP-`PlayerID`) spielt mit.

Vereinsnamen werden ohne Rücksicht auf Groß-/Kleinschreibung und
Randleerzeichen verglichen: Der Stammverein ist Handeingabe (BTP liefert
ihn nicht), da darf ein Tippfehler in der Schreibweise nicht über die
Warnung entscheiden. Spieler ohne Vereinszuordnung lösen nie einen
Vereins-Konflikt aus.

Nach außen geht **nur die Kategorie** („Verein"/„Person", `label()`), nie
der Grund — welcher Verein oder welcher Spieler dahintersteckt, bleibt auf
dem Turnier-PC.

## Rotation (Schritt 5)

`officials.rs::rotate_court(RotationInput)` besetzt die freien Dienste eines
Felds; orchestriert wird sie vom Master-Hook `sync.rs::track_officials`
(nach `set_snapshot`, damit der Roster gebunden und aktuell ist).

Ein Official wird genommen, wenn er **alle** Bedingungen erfüllt: BTP kennt
ihn noch, er ist nicht pausiert, er tut nirgends sonst Dienst (SR *oder* AR),
er hat keinen Konflikt mit diesem Spiel und er ist nicht schon der andere
Dienst desselben Spiels. Konflikte werden dabei **still** übersprungen —
gewarnt wird nur bei manueller Zuweisung (Spec Nr. 4).

- **Nur beim Neu-Belegen:** Bestückt wird ein Feld in dem Zyklus, in dem ein
  anderes Spiel darauf kommt (Merker `officials_oncourt_prev`). Wer eine
  Zuweisung von Hand löscht, bekommt sie deshalb **nicht** im nächsten Poll
  zurück.
- **BTP gewinnt:** Trägt das Match `Official1ID`/`Official2ID`, gilt dieser
  Wert (`store.effective(...)`); die Rotation füllt dort nichts nach.
- **Nach Spielende** rücken die Officials des beendeten Spiels ans Ende der
  Reihenfolge; ihre Zuweisung bleibt am Match stehen (Einsatz-Ableitung).
  Werden mehrere Felder im selben Zyklus fertig, rücken ihre Officials **nach
  CourtID sortiert** ans Ende — dieselbe Deterministik wie bei der Zuteilung
  unten (bis v0.9.201 hing die Reihenfolge von der zufälligen
  HashMap-Iteration ab).
- **Mehrere Felder gleichzeitig:** nach CourtID sortiert, damit die
  Verteilung deterministisch ist; frisch Zugewiesene gelten sofort als im
  Dienst.
- **Globaler Schalter aus** ⇒ `clear_assignments()` (Spec Nr. 1).
- Niemand frei ⇒ das Feld bleibt ohne diesen Dienst, ohne Fehler.

## Feldweise Schalter (Schritt 6)

Drei unabhängige Schalter je CourtID (`CourtSwitches`), Default alle aktiv:
`sr` und `ar` schalten die Rotation dieses Felds, `operator` die
Zähltafelbediener-Vergabe. Der Operator-Schalter wirkt in
`state.rs::assign_scorekeeper_for_court`: Ein ausgenommenes Feld bekommt
keinen Bediener und **verbraucht keinen Eintrag** aus der Warteschlange
(siehe [zaehltafelbediener.md](zaehltafelbediener.md)). Er greift **nur bei
eingeschaltetem Schiedsrichter-Betrieb**: Seine einzige Bedienstelle liegt in
der Schiedsrichter-Oberfläche, und die ist ohne das Feature nicht erreichbar
— ein einmal ausgenommenes Feld bliebe sonst nach dem Abschalten für immer
ohne Bediener, ohne dass es irgendwo zurückzunehmen wäre.

Aus den Kombinationen ergeben sich die drei Betriebsformen: SR bedient
selbst (`operator` aus), SR mit Papierzettel (alle drei an), kein SR
(`sr`/`ar` aus).

Tests: `tablet/officials.rs` (Roundtrip über Neustart, Turnierwechsel
verwirft — auch auf der Platte, fremder Datei-Stand, unlesbare Datei wird
nicht überschrieben und später nachgeladen, Reihenfolge-Abgleich inkl.
Doppel-Eintrag, später gesetzter Pfad, Zuweisungs-CRUD,
Feldschalter-Defaults, Konflikt-Kategorien, Rotations-Zweige) sowie
`state.rs::snapshot_bindet_das_officials_roster_ans_turnier`,
`state.rs::ein_feld_ohne_bedienervergabe_verbraucht_keinen_eintrag` und die
`sync.rs::track_officials_*`-Tests (Neu-Belegung, kein Nachfüllen nach
manuellem Löschen, Spielende, Doppel-Dienst, Feldschalter, globales
Abschalten).

## Bedienung am Turnier-PC (Schritt 7)

Eigener Menüpunkt **„Schiedsrichter"** (`pages/OfficialsPanel.tsx`) — in der
Seitenleiste ausgegraut, solange ohne Schiedsrichter gespielt wird; ein Klick
springt dann in den Einstellungs-Abschnitt mit dem Häkchen.

Die Seite hat drei Abschnitte:

1. **Rotationsreihenfolge** — Liste in Zuteilungsreihenfolge mit Dienst-Marke,
   Pause-Knopf, Zieh-Griff zum Umsortieren (Drag & Drop, seit 14.08.2026 —
   ersetzt die frühere Pfeil-Bedienung, siehe unten), Vereinsfeld,
   Sperren-Dialog und Einsatz-Zähler (öffnet die Einsatz-Liste).
2. **Einteilung der laufenden Spiele** — je belegtem Feld ein Auswahlfeld für
   SR und AR. Eine Zuweisung mit Konflikt wird **ausgeführt** und daneben als
   „Konflikt: Verein/Person" gemeldet (Spec Nr. 2).
3. **Felder** — die drei Schalter je Feld (SR-Rotation, AR-Rotation,
   Bediener-Vergabe).

Zusätzlich zeigt die **Spielübersicht** an jeder belegten Feld-Kachel
„SR: … · AR: …" samt Warn-Marke.

### Commands (R1)

| Command | Zweck |
|---|---|
| `officials_roster` | Liste in Rotationsreihenfolge: Name, Position, Pause, Verein, **Anzahl** Sperren, aktueller Dienst, Einsatz-Zähler |
| `official_assign` / `official_clear` | Zuweisung setzen/lösen; `assign` liefert die Konflikt-Kategorie zurück |
| `official_pause`, `official_reorder`, `official_set_club` | Pause, Reihenfolge, Stammverein |
| `official_blocklists` / `official_set_blocklists` | Sperrlisten **gezielt** lesen/schreiben |
| `official_appearances` | Einsatz-Liste eines Officials (Spiel, Rolle, Feld, Endezeit) |
| `officials_court_switches` / `officials_set_court_switches` | feldweise Schalter |

**Sperrlisten reisen nie mit der Liste.** `officials_roster` trägt nur die
*Anzahl* der Einträge; die Inhalte kommen ausschließlich über
`official_blocklists` für genau einen Official — dasselbe Muster, das später
TL-Web über die authentifizierte Leseroute nutzt.

### Globale Schalter zur Laufzeit

`officials.enabled`, `rotation_sr` und `rotation_ar` stehen zwar in der
`config.json`, werden aber zusätzlich im Roster-Speicher gespiegelt
(`set_enabled`/`set_rotation`, gesetzt beim App-Start und beim Speichern der
Einstellungen). Grund: Der Sync-Lauf liest seine Konfiguration nur beim Start
— ohne diesen Spiegel bliebe ein frisch gesetztes Häkchen bis zum nächsten
Stoppen/Starten der Übertragung wirkungslos.

## TL-Web (Schritt 8)

Wire-Ebene und Bedienung sind in [cloud-relay.md](cloud-relay.md) bzw.
[turnierleitung-web.md](turnierleitung-web.md) beschrieben. Der Kern:

- **Broadcast-`TlState`** trägt `officials_managed`, die reduzierte Liste
  `officials` (`id`, `name`, `paused`, `on_duty_court_id`, `appearances`)
  und je Feld `sr`/`ar`/`official_warn` plus die drei Feld-Schalter.
- **Sperrlisten, Verein und Einsatz-Liste** kommen ausschließlich über
  `GET /tl/api/officials/{id}` (Geräte-Token; im Cloud-Modus durchgereicht
  per `OfficialDetailRequest`/`OfficialDetail`). Beide Wächter-Tests in
  `tl.rs` prüfen das mit einem Officials-Fixture: Der **Name** ist bewusst
  freigegeben (Gegenprobe im Test), Sperrlisten und Stammverein stehen auf
  der Verbotsliste.
- **Aktionen** (geschlossener Satz): `official_assign`, `official_clear`,
  `official_pause`, `official_reorder`, `official_set_club`,
  `official_blocklist_set`, `officials_court_toggle`, `announce_officials`.
  Fingerabdruck und Protokoll-Etikett nennen **nie** Vereinsnamen oder
  Sperrlisten — die Protokolle werden zur Fehlersuche hochgeladen.

**Relay-Deploy vor dem Client-Release:** Ein alter Relay weist unbekannte
Aktionen ab; im Hallennetz (LAN) ist nichts betroffen.

## Tablet-Anzeige (Schritt 9)

`MatchBrief` bekommt `srNames`/`arNames` — **Namen statt IDs**, damit das
Tablet nichts auflösen muss und keine Officials-Liste braucht. Beide Felder
tragen `#[serde(default)]`: Ältere Frames bleiben lesbar (leere Listen).

Gefüllt werden sie an den beiden Push-Pfaden (`server.rs` für LAN,
`relay_client.rs` für Cloud) über `TabletState::match_officials` — dieselbe
Auflösung wie in der Feldübersicht, inklusive „ohne Schiedsrichter-Betrieb
bleibt alles leer". Damit sieht auch die **ferne Halle** (Cloud-Slave) die
Besetzung, weil der Brief mit `MatchAssigned` mitreist.

`tablet.html` zeigt sie in der Einrichtung neben dem Zähltafel-Hinweis
(„⚖️ Schiedsrichter / Umpire: …"); während des Zählens zählt der Bildschirm,
nicht die Besetzung.

## Ansagen (Schritt 10)

`announcer.ts` bekommt `umpireNames`/`serviceJudgeNames` und den Schalter
`officialsOnly`. Die Segmente stehen **nach** der Tabletbedienung:

| | Deutsch | Englisch |
|---|---|---|
| SR | „Schiedsrichter: {Name}." | „Umpire: {Name}." |
| AR | „Aufschlagrichter: {Name}." | „Service judge: {Name}." |

Beide Ansage-Wege bauen sie aus **derselben** Funktion
(`officialSegments`) — der Web-Speech-Pfad als eigene Utterances, der
Azure-Pfad XML-escaped im SSML. Damit sagen sie Wort für Wort dasselbe.

- **Feld-Ansage:** `announceCourt` gibt `court.sr`/`court.ar` mit; ohne
  Zuweisung entfällt das Segment ersatzlos. Seit v0.9.246 gilt das auch für
  die **automatische** Ansage neu belegter Felder (`MatchAnnouncer.tsx`) — sie
  baute ihr `AnnounceMatchInput` selbst zusammen und ließ SR/AR bis dahin weg,
  sodass dieselbe Belegung je nach Auslöser anders klang (Nutzer-Befund
  20.08.2026). Der Schalter `announce.announce_umpire` (Default an,
  [ADR 0040](adr/0040-ansage-besetzung-einstellbar.md)) schaltet die Nennung
  bei Bedarf ab. Seit v0.9.248 gilt das auch für den
  **Cloud-Ansage-Slave**: Die Namen reisten im `MatchBrief` längst bis dorthin,
  fielen aber bei der Umwandlung in `CloudAnnounceCourt` heraus.
- **Manueller Knopf** (`announceOfficials`, `officialsOnly`): sagt nur Feld
  und Besetzung an — eine nachträgliche Zuweisung soll nicht die ganze
  Paarung erneut aufrufen, das Spiel läuft ja schon. Der Knopf sitzt im
  Client neben der Einteilung und in TL-Web an der Feld-Kachel; TL-Web löst
  ihn über `announce_officials` aus, gesprochen wird in der Zielhalle
  (`AnnounceJobKind::Officials`).

## Rücksync nach BTP (Schritt 11, ADR 0021)

Jede Änderung der Besetzung geht nach BTP — beide Wege laufen über
**dieselbe** Wire-Form, `proto.rs::court_assign_request` mit
`MatchCourt`-Knoten (Identität + `CourtID` + optional `Official1ID`/
`Official2ID`), **kein** `Status`-Feld (Check-in-Bitfeld, Regression
v0.9.103). Löschen eines Dienstes ist die `0`.

1. **Eigenständig**, aus `sync.rs::reconcile_officials`/`officials_entries`:
   leerer `Courts`-Block, nur der `Matches`-Block. Der Sync-Loop ruft
   `reconcile_officials` direkt nach den Highlights: Es geht nur der
   **Unterschied** raus, der Stand wird nur bei `Ok` übernommen, ein
   Fehlschlag wird im nächsten Zyklus wiederholt. Weil BTP asynchron
   übernimmt (≤ 1 s, Messung 13.08.2026), merkt sich die Engine das
   Geschriebene, bis der Snapshot nachzieht. **Trägt immer die aktuell
   bekannte `CourtID` mit** (`m.court_id.unwrap_or(0)` aus dem Snapshot) —
   dazu mehr im Abschnitt „CourtID immer mitschreiben" unten.
2. **Eingebettet** beim Ruf aufs Feld: derselbe `court_assign_request` trägt
   dann zusätzlich den `Courts`-Block, die Officials additiv
   (`MatchCourt::officials`), verdrahtet an allen drei Einstiegen
   (`commands.rs::assign_court`, `sync.rs::auto_assign`, TL-Web-Pfad in
   `tl.rs`). Ohne Schiedsrichter-Betrieb bleibt der Request **exakt** wie im
   Bestand — dann steht dort kein zusätzliches Feld.

### Anzeigen und Schreiben folgen verschiedenen Regeln

Das ist Absicht und der einzige Punkt, an dem die Spec-Regel „BTP gewinnt"
nicht wörtlich gilt:

- **Anzeige** (`effective`): Trägt das BTP-Match einen Wert, gilt dieser.
- **Schreiben** (`officials_for_write`): Hier schlägt die lokale Absicht den
  BTP-Stand. Sonst ließe sich eine einmal nach BTP geschriebene Besetzung
  nie wieder ändern — der Rücksync fände keinen Unterschied mehr. Ein
  Dienst, den BTS Light nie angefasst hat, wird unverändert mitgeschrieben
  statt gelöscht.
- **Lösen** merkt sich als `Some(0)` („ausdrücklich keiner"), nicht als
  „nie angefasst". Nur so geht die `0` nach BTP und der Schiedsrichter
  verschwindet auch dort. Ein so gelöster Dienst gilt für die Rotation als
  **erledigt**, nicht als offen — sonst bekäme ein bewusst ohne
  Schiedsrichter spielendes Feld nach einem App-Neustart wieder einen
  zugeteilt (dort sieht jedes belegte Feld wie neu belegt aus).
- **Loslassen nach Bestätigung** (`OfficialsStore::confirm`, im Sync-Loop vor
  dem Diff): Sobald der Snapshot für einen Dienst einen Wert zeigt — oder das
  ausdrückliche „keiner" bestätigt —, wird der lokale Eintrag entfernt und
  BTP ist wieder allein die Wahrheit (R2). **Ohne diesen Schritt** würde eine
  spätere Änderung *in BTP* bei jedem Sync-Zyklus wieder überschrieben, für
  den Rest des Turniers. Die Einsatz-Ableitung verliert dabei nichts: Sie
  liest denselben Wert dann aus dem BTP-Match.

### CourtID immer mitschreiben (Live-Befund 14.08.2026)

Ein eigenständiges Schiedsrichter-`SENDUPDATE`, das **kein** `CourtID`-Feld
trug, ließ BTP an einem laufenden Zwei-Hallen-Turnier beobachtbar die eben
erst angekommene Feldzuweisung wieder verlieren, wenn es kurz nach einem
`court_assign_request` desselben Matches folgte (Match 1216, Feld 8:
zugewiesen, im nächsten Poll bestätigt, wenige Sekunden später wieder
leer — ohne dass irgendein Gerät das Feld freigegeben hätte). Zwei
`SENDUPDATE`s zum selben Match in enger Folge bringen BTPs eigene
Persistenz durcheinander.

**Erster Versuch (verworfen): eine feste Karenzzeit.** `officials_entries`
ließ ein frisch zugewiesenes Match zunächst `OFFICIALS_COURT_SETTLE_MS`
(10 s) lang in Ruhe, bevor der eigenständige Abgleich nachkorrigiert. Am
selben Turnier gemessen: Der tatsächliche Abstand zwischen Feldzuweisung
und eigenständigem Schiedsrichter-Write lag teils bei 11–18 s — außerhalb
des Fensters. Eine feste Wartezeit rät nur, wie lange BTP zum Verarbeiten
braucht, und die Antwort war „länger als vermutet, und variabel".

**Fix: `court_id` reist immer mit.** `officials_entries` (`sync.rs`)
schreibt jetzt bei **jedem** eigenständigen Write die aktuell aus dem
Snapshot bekannte `CourtID` mit (`MatchCourt::court_id`, `0` nur bei einem
Match, das nie auf einem Feld stand — nie zum Freigeben eines belegten
Felds, das bleibt Aufgabe des `Courts`-Blocks). Reasserted der Write
denselben Wert, den der zeitlich nähere `court_assign_request` gerade
geschrieben hat, ist die Reihenfolge der beiden Requests folgenlos — eine
Wartezeit erübrigt sich, unabhängig davon, wie lange BTP tatsächlich
braucht. `officials_request`/`OfficialsEntry` (die CourtID-lose Wire-Form)
sind damit entfallen.

**Bewusst akzeptierter Rest-Trade-off** (Code-Review-Fund 14.08.2026): Die
Reassertion nimmt den `court_id`-Wert aus dem **zuletzt gepollten**
Snapshot — ändert jemand die Feldzuordnung eines Matches über einen ganz
anderen Weg (z. B. von Hand direkt in BTP) exakt im Fenster zwischen
diesem Poll und dem Schiedsrichter-Write, würde der Write diese Änderung
zurücksetzen. Das Fenster ist auf einen Poll-Zyklus begrenzt und tritt nur
ein, wenn zufällig zeitgleich ein Schiedsrichter-Unterschied ansteht —
deutlich enger als das behobene Problem (das bei **jeder** frischen
Feldzuweisung mit Schiedsrichter-Rotation auftrat). Keine weitere
Mitigation vorgesehen, solange kein Turnier-Befund das Gegenteil zeigt.

### Ergebnis-Write löschte die Besetzung (Live-Befund 14.08.2026, Fortsetzung)

Der `CourtID`-Fix oben deckte nur die **Feldzuweisung** ab. Am selben
Turnier zeigte sich unabhängig davon ein **zweiter** Fall derselben
Fehlerklasse: Das Ergebnis-`SENDUPDATE` (`update_request`, gebaut in
`server::build_manual_result_update_opt`/`build_manual_dq_update` sowie
direkt in `process_result`) trug `Official1ID`/`Official2ID` gar nicht —
und BTP hat die Schiedsrichter-Besetzung eines Matches dadurch bei
**jedem** Spielabschluss gelöscht, unabhängig davon, wie lange sie vorher
stand (beobachtet sowohl bei einer Zuweisung von 74 Sekunden als auch bei
über einer Stunde). Das erklärte gleich drei Symptome auf einmal:

- Die **Rotation** rückte niemanden ans Ende (`move_to_end`) — es gab ja
  nichts, was das gerade beendete Match noch als Besetzung kannte.
- Der **Einsatz-Zähler** (`appearances`) zählte nicht hoch — er liest
  `Match.official1_id`/`official2_id` der beendeten Spiele, und die waren
  leer.
- Die **Liste der beendeten Spiele** zeigte keine Schiedsrichter.

**Fix:** `MatchUpdate` trägt jetzt ein `officials: Option<(i64, i64)>` —
gefüllt über `TabletState::officials_for_result(match_id)` (BTP-Wert
gewinnt, sonst lokale Zuweisung, sonst explizit `(0, 0)` — nie `None`,
solange der Schiedsrichter-Betrieb läuft) an **jedem** Ergebnis-Schreibweg:
Tablet (`process_result`), Turnierleitung (Desktop + TL-Web, regulär und
Disqualifikation), Walkover und die Nachschub-Queue (persistiert das Feld
mit, `#[serde(default)]` für ältere Queue-Dateien). `officials_for_result`
liefert `None` nur, wenn ohne Schiedsrichter-Betrieb gespielt wird — dann
bleibt der Request unverändert zum Bestand.

### Was das Tablet erreicht

Der Push-Schlüssel beider Wege (LAN und Cloud) enthält neben Match-ID und
`finalized` einen Fingerabdruck der Besetzung. Ohne ihn erreichte eine
Zuweisung, die **nach** dem Ruf aufs Feld erfolgt, das Tablet nie — die
Match-ID ändert sich dabei ja nicht.

## Sperrlisten pflegen: auswählen statt tippen

Die Sperr-Spieler stehen als BTP-`PlayerID` in der Turnierdatei — gepflegt
werden sie aber **nie** als Zahl: Niemand kennt die Kennung von Anna Müller
auswendig, und eine vertippte ID warnt einfach nie (still falsch ist
schlimmer als gar nicht).

`officials.rs::pick_lists(entries)` sammelt dafür aus der **Meldeliste**
(`BtpSnapshot::entries`, vollständiger als die Paarungen — auch Klassen ohne
Auslosung sind dabei) zwei Listen: alle Spieler (`id`, `name`, `club`) und
alle Vereine, jeweils einmalig und alphabetisch. Der Verein steht dabei, um
Namensgleiche zu unterscheiden.

Beide Listen reisen mit der **gezielten** Antwort des Pflege-Dialogs
(`official_blocklists` bzw. `/tl/api/officials/{id}`), nicht im
Broadcast-Zustand: Die Meldeliste ist deutlich größer als alles, was die
Seite sonst bekommt, und wird nur beim bewussten Öffnen gebraucht.

In beiden Oberflächen gilt dieselbe Bedienung: **Spieler** ausschließlich
über Suche (ab zwei Zeichen, max. 25 Treffer) und Klick; **Vereine** über
Vorschlagsliste, aber mit Freitext — ein Verein, der (noch) nicht gemeldet
ist, muss sich trotzdem sperren lassen. Gewählte Sperren stehen als
entfernbare Marken. Wer inzwischen aus der Meldeliste verschwunden ist,
behält seine Sperre (dann ohne Namen), statt still herauszufallen.

## Warum die CI die Asset-Syntax prüft

`tl.html` und `tablet.html` durchlaufen **keinen** Build — sie gehen so an
Browser und Tablets, wie sie im Repo stehen. Ein Syntaxfehler lässt das
komplette Modul unausgeführt und die Seite leer; genau das ist beim Bau
dieses Features passiert (ein Zeilenumbruch mitten in einem String-Literal
im Sperrlisten-Dialog). Seitdem prüft `scripts/check-asset-syntax.mjs` alle
Inline-Skripte der Assets, eingehängt in `ci.yml`.

## Offen: Verfügbarkeit je Tag (Messfrage, 13.08.2026)

BTP führt in der Schiedsrichterliste die **Tage**, an denen jemand da ist —
ein Schiedsrichter, der nur freitags kommt, soll samstags gar nicht erst in
der Rotation stehen. Ob diese Angabe über den Draht kommt, ist **ungeklärt**:

- Die Messung vom 13.08.2026 sah nur `Official{ID, Name, FirstName,
  Country}`. Das ist **kein** Beweis für „gibt es nicht": Am Testturnier
  waren keine Tage gepflegt, und **BTP lässt leere Felder generell weg** —
  genau daran ist auch der Verein nicht aufgefallen, bis er gepflegt wurde
  (und dann trotzdem nicht kam).
- Die Probe `tests/btp_officials_probe.rs` beantwortet die Frage jetzt mit:
  Sie listet alle Official-Feldnamen, sucht gezielt nach tages- und
  datumsverdächtigen Namen und schaut zusätzlich nach einem **eigenen**
  Container (`OfficialDays`, `Availability`, …) — BTP könnte die Tage auch
  getrennt führen, so wie `Entries` neben `Players` stehen.

**So wird gemessen:** In BTP bei *einem* Schiedsrichter die Tage pflegen, bei
einem anderen nicht, dann

```
cargo test -p bts-light --test btp_officials_probe -- --ignored --nocapture
```

Erst der Vergleich beider Zeilen zeigt, ob das Feld existiert und wie es heißt.

**Wenn die Tage kommen:** Die Rotation blendet Officials aus, die heute nicht
da sind — technisch dieselbe Stelle wie „pausiert" (`next_free` überspringt
sie), nur aus BTP gespeist statt von Hand geschaltet.

**Wenn sie nicht kommen:** Dann bleibt die Verfügbarkeit Pflege in BTS Light.
Der heutige Pausen-Schalter deckt den Fall schon ab — er müsste dann nur
einen Tagesbezug bekommen, damit man ihn nicht jeden Morgen neu setzt.

## Der Schiedsrichter auf dem Zettel

Der gedruckte Schiedsrichterzettel trägt im Kopf **Schiedsrichter und
Service-Richter** des Spiels — aus derselben Quelle wie die Feldübersicht
(`court_officials`). Sperrlisten und Stammverein stehen **nicht** darauf; sie
bleiben der Pflege-Ansicht vorbehalten. Bedienung:
[schiedsrichterzettel.md](schiedsrichterzettel.md).
