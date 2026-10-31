# Sprachansagen für Feld-Aufrufe

Wird in BTP ein Spiel auf ein Feld gezogen, spielt bts-light auf dem
Turnier-PC eine gesprochene Ansage ab: **Gong → „Feld X" → Disziplin
(+ Klasse) → Paarung → „Feld X"**. Deutsch oder Englisch, wählbare Stimmen,
einstellbares Tempo. Eingeführt in v0.6.0; Klassen-Ansage
(„Herreneinzel **A**") seit v0.9.145. Steht am Feld eine Bedienung, folgt am Ende zusätzlich
„**Tabletbedienung: {Name}**" (`scorekeeperNames` in `AnnounceMatchInput`) —
seit v0.9.246 gesteuert vom Schalter `announce_scorekeeper`
([ADR 0040](adr/0040-ansage-besetzung-einstellbar.md)) statt von der Herkunft
des Namens; siehe [zaehltafelbediener.md](zaehltafelbediener.md).

## Funktionsweise

- **Technik (seit v0.9.318):** Gong und Stimme spielt der **Rust-Kern**
  über Windows (WASAPI) — siehe [Ausgabegerät](#ausgabegerät-seit-v09318).
  Der Gong wird im Fenster per Web Audio gerendert, die Stimme ist Azure
  (wenn eingerichtet) oder die **Windows-Stimme** (OneCore). Die
  Browser-Stimme (`speechSynthesis`) springt nur noch als Rückfall ein. Die
  Ansage spielt über die Lautsprecher des Turnier-PCs, auch wenn das Fenster
  ins Tray minimiert ist.
- **Auslöser am Turnier-PC (Master, seit v0.9.314):** Der **Kern** vergleicht
  in jedem Sync-Lauf die Feldbelegung mit der zuletzt gesehenen
  ([`tablet/erst_ansage.rs`](../src-tauri/src/tablet/erst_ansage.rs), Aufruf
  in `sync.rs` hinter dem Slave-Rücksprung, **nach** Schiedsrichter-Rotation
  und Zettel-Autodruck — erst dann steht die Besetzung des neuen Felds fest).
  Je neuer `match_id` auf einem Feld legt er einen Ansage-Auftrag
  **`court_assigned`** ab (Feld, Spiel, Halle des Felds). Gesprochen wird er
  am PC vom [`AnnounceJobPlayer`](../src/components/AnnounceJobPlayer.tsx)
  (bei „Ansagen aktivieren") und auf den Ansage-Seiten im Browser vom
  Vertoner — beide über `jobZuAnsage` → `courtAnsage`, also mit denselben
  Angaben wie zuvor: Klasse, Runde, Tabletbedienung und SR/AR gemäß
  Schaltern. Spec [ansage-im-browser](features/ansage-im-browser.md) (E2),
  Entscheidung [ADR 0064](adr/0064-erst-ansage-im-kern.md).
  <!-- pruef: "court_assigned" in src-tauri/src/tablet/state.rs -->
- **Auslöser am LAN-Ansage-Slave:** unverändert die Komponente
  [`MatchAnnouncer`](../src/components/MatchAnnouncer.tsx) (`erkennen` nur
  bei `slave_mode`). Sie pollt alle 2 s `tablet_overview()` und vergleicht
  mit [`src/io/announceBaseline.mjs`](../src/io/announceBaseline.mjs).
  Der Slave holt die Aufträge des Masters **ohne** `?auto=1` ab und bekommt
  `court_assigned` deshalb nicht — sonst hörte seine Halle jeden Aufruf
  doppelt. Der Cloud-Ansage-Slave (`CloudAnnounceSlave`) ist unberührt.
- **Regeln (Kern und Slave gleich):** Der **erste brauchbare Stand ist
  Bestandsaufnahme** — bereits laufende Spiele werden nicht nachträglich
  angesagt. Danach löst jede neue `match_id` auf einem Feld genau eine
  Ansage aus; dieselbe Match-ID wird im 5-s-Fenster nicht doppelt angesagt.
  Im Kern setzt zusätzlich ein **Turnierwechsel** (anderer Turniername) die
  Bestandsaufnahme zurück. Der Stand liegt im `TabletState`, nicht in der
  `SyncEngine` — er überlebt das Speichern der Einstellungen, das die Engine
  neu baut.
- **Eine leere Antwort taugt nicht als Baseline** (Befund 14.08.2026): Die
  ersten Abrufe nach dem Start kommen, bevor der Sync-Lauf seinen ersten
  BTP-Schnappschuss hat — `tablet_overview()` liefert dann **null Felder**.
  Galt das als Baseline, war beim nächsten Abruf jedes belegte Feld „neu",
  und die App sagte beim Start **alle laufenden Spiele** an. Die Baseline
  gilt deshalb erst als gesetzt, wenn überhaupt Felder dabei sind; Felder
  **ohne** Spiel sind dagegen ein gültiger Anfangsstand (Turniermorgen —
  sonst bliebe der erste Aufruf des Tages stumm). Festgehalten in
  `scripts/test-announce-baseline.mjs` (Slave) und den Rust-Tests in
  `tablet/erst_ansage.rs` (Port aller Fälle) und `sync.rs` (`erst_ansage_*`).
- **Ein Hallenwechsel holt nichts nach** (Befund 23.08.2026, seit v0.9.257):
  Wurde die Ansage-Halle im Betrieb umgestellt — oder auf „alle Hallen" —,
  sagte das Gerät schlagartig **jedes dort laufende Spiel** noch einmal an.
  Am Ansage-Slave war das eine halbe Turnierhälfte auf einmal.

  Ursache war ein eigener Reset im `MatchAnnouncer`, der die Baseline beim
  Hallenwechsel leerte, damit „die erste Belegung nach dem Wechsel angesagt
  wird". Er ist ersatzlos entfallen — und war nie nötig: `diffOccupiedCourts`
  führt die Baseline **vor** dem Hallenfilter und merkt sich deshalb auch
  Felder der gefilterten Halle, ausdrücklich damit ein Umschalten nichts
  nachholt. Nach dem Wechsel sind alle laufenden Spiele also bereits bekannt;
  angesagt wird, was danach aufs Feld kommt.

  Dieselbe Regel wie bei der Schiedsrichter-Rotation seit v0.9.253: Der erste
  beobachtete Stand ist Ausgangslage, kein Ereignis. Festgehalten in
  `scripts/test-announce-baseline.mjs` („Halle hinzunehmen holt nichts nach").

  Am Master (seit v0.9.314) gilt dasselbe auf anderem Weg: Der Kern filtert
  gar nicht nach Halle, jeder Auftrag trägt seine Halle, und erst die
  Zustellung an das Gerät filtert. Wechselt dort die Ansage-Halle, beginnt
  der `AnnounceJobPlayer` mit einer neuen Bestandsaufnahme — sonst holte er
  die Aufträge der fremden Halle aus der letzten Minute nach. Ebenso nach dem
  Aus- und Wiedereinschalten der Ansagen.
- **Die Halle wird an drei Stellen gesetzt** — Dashboard, Ansage-Einstellungen
  und (am Cloud-Slave) das Geräte-Panel, das mit `announce_hall` zugleich
  festlegt, welche Halle das Gerät bedient. Die Ansage-Einstellungen halten
  ihren Formular-Stand lokal; ohne Abgleich schrieben sie beim Speichern einen
  veralteten Wert zurück und wählten die eben woanders gewählte Halle wieder
  ab (Befund 23.08.2026, behoben in v0.9.257). Wer dort ein Feld ergänzt, das
  **auch anderswo** geschrieben wird, braucht denselben Abgleich.
- **Engine:** [`src/io/announcer.ts`](../src/io/announcer.ts) — portiert
  aus der Schwester-App badhub-tournament.

## Disziplin

Die Disziplin (Herren-/Dameneinzel, Herren-/Damendoppel, Mixed) kommt aus
dem BTP-**Event**, nicht aus dem Draw-Namen. Auflösungskette im Parser
([`btp/model.rs`](../src-tauri/src/btp/model.rs)):
`Match.DrawID → Draw.EventID → Event{GameTypeID, GenderID}`.

- `GameTypeID`: 1 = Einzel, 2 = Doppel.
- `GenderID`: 1 = Herren, 2 = Damen, 3 = Mixed.

Ergibt `GenderID` keinen dieser Werte (seit v0.9.297), liest der Parser
die Disziplin aus dem Event-Namen, sonst aus dem Draw-Namen: Kürzel
HE/DE/HD/DD/MX/GD und JE/ME/JD/MD, seit v0.9.307 auch die englischen
MS/WS/WD/XD/MIX, ausgeschriebene Formen („Jungeneinzel") und zweiteilige
(„Jungen Doppel"). **MD** ist doppeldeutig: Benutzt das Turnier irgendwo
MS, WS, WD oder XD (Event- oder Draw-Name), heißt MD „Men's Doubles"
(Herrendoppel), sonst wie im deutschen Jugend-Schema Mädchendoppel
(`englisches_schema` in `btp/model.rs`). Anlass: Ein DBV-Jugendturnier
(10/2026, „JE U15") lieferte einen anderen `GenderID`-Wert, die Ansage
ließ Disziplin und Klasse komplett weg. Bekannte BTP-Werte haben weiter
Vorrang. Nennt auch der Name kein Geschlecht („U11 offenes Doppel") oder
lässt sich das Event nicht auflösen, ist die Disziplin `Unknown` und wird
in der Ansage weggelassen — samt Klasse.

### Klasse (seit v0.9.145)

Direkt hinter der Disziplin wird das **Klassen-Kürzel** angesagt
(„Herreneinzel **A**"). Es kommt aus `model::class_label`: bevorzugt aus dem
**Event-Namen** (trägt die Klasse auch in der Gruppenphase, wo Draws nur
„Gruppe 1…n" heißen), sonst aus dem **Draw-Namen** („HE A" in der
K.-o.-Phase). Getrennt wird an Leerzeichen **und Bindestrichen** (seit
v0.9.288: die BBB-Ranglisten nennen ihre Events „HD-A", „MX-C"), dann
werden bekannte Disziplin-Wörter entfernt; übrig bleiben darf
nur EIN kurzes Kürzel (≤ 4 Zeichen, z. B. „A", „B2", „U15") — **Gruppen-
oder Auslosungsnamen („Gruppe 3", „Hauptrunde") werden nie angesagt**
(Nutzer-Vorgabe vom Turnier 17.07.2026). Ohne erkennbares Kürzel bleibt die
Ansage wie bisher („Herreneinzel").

**Abgesetzt gesprochen** (seit v0.9.312): Mit nur einem Leerzeichen zogen
die Stimmen das Kürzel ans Wort — es klang wie „Herreneinzela". Gesprochen
wird deshalb „Herreneinzel, A": Die Gerätestimme bekommt ein Komma (ein
Bindestrich wird oft mitgezogen), Azure eine echte Pause von 250 ms, und ein
reines Buchstaben-Kürzel (1–2 Buchstaben) wird per `say-as characters`
buchstabiert — sonst liest eine englische Stimme „A" als Artikel. „U15" oder
„B2" bleiben unverändert. Die Anzeige (Vorbereitungsliste) zeigt weiter
„Herreneinzel A". Code: `src/io/klasseSprechen.mjs`, getestet von
`scripts/test-klasse-sprechen.mjs`.

**Englische Kürzel** (seit v0.9.307): MS/WS/WD/XD/MIX zählen ebenfalls
als Disziplin-Wörter (MD war es schon) — aus „MS A" ging sonst die Klasse
„A" verloren. **GD** bleibt Mixed (Gemischtes Doppel), auch in einem
englisch benannten Turnier; das englische Jugend-Schema (BS/GS/BD/GD =
Girls' Doubles) wird nicht unterstützt.

**Jugendklassen** (seit v0.9.296): Jugendturniere nennen ihre Events
„JE U15", „MD U17" oder „U11 offenes Doppel". Die Kürzel JE/JD/ME/MD,
die ausgeschriebenen Jungen-/Mädchen-Wörter und „offenes" zählen deshalb
ebenfalls als Disziplin-Wörter — sonst bliebe neben „U15" ein zweiter Rest
übrig und die Klasse fiele weg. Weil BTP nur Herren/Damen/Mixed kennt,
benennt `src/io/disziplinName.mjs` die Disziplin bei einer Klasse „U" + Zahl
als **Jungeneinzel/Mädcheneinzel/Jungendoppel/Mädchendoppel** („Boys'/Girls'
Singles/Doubles" auf Englisch). Dieselbe Tabelle steht wortgleich in
`tl.html` (Kürzel JE/ME/JD/MD/MX) und `monitor.html` (der Monitor bekommt das
Kürzel dafür als `MonitorMatch.classLabel`); `scripts/test-disziplin-name.mjs`
hält die Kopien gleich. Die Stimmenwahl je Disziplin bleibt am
snake_case-Schlüssel — Jungeneinzel nutzt die Stimme für `mens_singles`. Durchgereicht wird das Kürzel als
`class_label` (CourtOverview, `MatchBrief.classLabel` für den Cloud-Slave,
`PreparationCandidate`) und als `className` in den Announcer-Eingaben.

## K.-o.-Runde (ab Viertelfinale)

Steht ein Spiel im **Viertelfinale, Halbfinale, Finale oder Spiel um Platz 3**,
wird die Runde **vor der Paarung** mitangesagt (z. B. „Feld 2. Herrendoppel.
**Halbfinale.** … gegen …"). **Frühere Runden, Gruppen und das Achtelfinale
werden NICHT angesagt.**

- Quelle: die rohe BTP-Runde `RoundName` (`btp/model.rs` `BtpMatch.round_name`),
  durchgereicht als `CourtOverview.round_name` (Rust + `types.ts`) bis zum
  Announcer (`AnnounceMatchInput.roundName`).
- Erkennung: `knockoutRoundLabel()` in [`announcer.ts`](../src/io/announcer.ts) —
  normalisiert (Punkte/Bindestriche/Leerzeichen) und matcht robust:
  `VF`/`Viertelfinale`/`QF`/`Quarterfinal`, `HF`/`Halbfinale`/`SF`/`Semifinal`,
  `Finale`/`Final`/`Endspiel`, `Spiel um Platz 3`/`Bronze`/`3rd`. Gruppen
  (`G1`, `Gruppe …`), `Achtelfinale`, `Runde/Round N`, Quali/Vorrunde → kein
  Label (→ keine Ansage). de/en gemäß Ansage-Sprache.

## 2./3. Aufruf (gestaffelte Feld-Ansage, v0.9.174)

Der **„Aufrufen"-Knopf** je Feld in der Spielübersicht
([`FieldOverviewPage.tsx`](../src/pages/FieldOverviewPage.tsx)) zählt bei
wiederholtem Drücken hoch: 1× = normaler Aufruf, danach **„2. Aufruf" / „3.
Aufruf"**. Ab Stufe 2 wird ein Label-Segment **vor der Paarung** angesagt:

> Gong → „Feld X" → Disziplin → (Runde) → **„Zweiter Aufruf" / „Dritter und
> letzter Aufruf"** → Paarung → „Feld X".

- Label: `callStageLabel()` in [`announcer.ts`](../src/io/announcer.ts);
  eingefügt in `buildAnnouncementSegments` **und** `buildAnnouncementSsml`
  (Segment- und Azure-SSML-Pfad identisch). `AnnounceMatchInput.callStage`
  (1/2/3, Default 1) trägt die Stufe; `announceCourt(court, …, callStage)`.
- Stufe je **Feld+Match** (`callStages`-Map, Key `court_id:match_id`) — ein
  neues Match auf dem Feld beginnt wieder bei 1.
- **Synchron mit dem zeit-basierten Aufruf-Badge:** Der Knopf sagt **mindestens**
  die Stufe an, die der [`CallTimerBadge`](../src/components/CallTimerBadge.tsx)
  (`callInfo().stage`, nach `second_/third_call_minutes`) bereits als fällig
  zeigt — so laufen Badge und Ansage nicht auseinander; manuelles Drücken
  eskaliert darüber hinaus. Der Knopf zeigt die **nächste** Stufe als Label.
- Betrifft nur diese manuelle Feld-Ansage; die automatische Erst-Ansage
  (`MatchAnnouncer`) und die Vorbereitungs-Ansage sind unberührt.
- **Gezählt wird am Turnier-PC** (`TabletState::call_stages`, Command
  `note_court_call`). Die `callStages`-Map im Frontend beschriftet nur noch
  den Knopf. Ohne das zeigte die Turnierleitungs-Seite „1. Aufruf", während
  hier schon zweimal gerufen wurde — die Stufe muss auf **allen** Geräten
  dieselbe sein. Die zeitliche Fälligkeit reicht der Knopf als `atLeast`
  durch; dieselbe Rechnung stellt der Host für die Seite an
  (`due_call_stage` in `tablet/tl.rs`).

## Ansage-Aufträge der Turnierleitungs-Seite

Die Turnierleitungs-Seite (TL-Web) **spricht nie selbst**: Sie läuft im
Zweifel im Büro oder auf einem Tablet ohne Verbindung zur Hallenanlage und
kennt weder die eingestellte Stimme noch die Aussprache-Korrekturen. Sie
legt stattdessen einen **Auftrag** am Turnier-PC ab; gesprochen wird er auf
den Ansage-Geräten — mit demselben Code wie jeder andere Aufruf.

| Baustein | Ort |
|---|---|
| Auftragstypen `AnnounceJob`/`AnnounceJobKind` (`court_call`, `prep_call`, `officials`, `start_play`, `scorekeeper_call`; seit v0.9.314 `court_assigned` aus dem Kern) | `tablet/state.rs` |
| Ablegen und Abholen (`publish_announce_job`, `announce_jobs_since`; höchstens 128 Aufträge) | `tablet/state.rs` |
| Auslösende Aktionen (`AnnounceCourtCall`, `AnnouncePrepCall`, `AnnounceStartPlay`, `AnnounceScorekeeper`) | `tablet/tl.rs` |
| Erst-Ansage (`court_assigned`, Feldbelegung im Sync-Lauf) | `tablet/erst_ansage.rs`, `TabletState::erst_ansage_pruefen` |
| Abholweg | Route `/info/announce/jobs` (`court_assigned` nur mit `?auto=1`), Command `pending_announce_jobs` |
| Sprecher | [`AnnounceJobPlayer.tsx`](../src/components/AnnounceJobPlayer.tsx) |

Eigenschaften, die im Turnierbetrieb zählen:

- **Struktur statt Text.** Der Auftrag trägt Feld, Spiel und Stufe — nicht
  fertige Worte. Ein Freitext von der Seite klänge anders als derselbe
  Aufruf vom Turnier-PC, und die Namenskorrektur bliebe außen vor.
- **Hallengenau.** Ein Auftrag für Halle B erklingt nicht in Halle A. Ein
  Gerät ohne eingestellte Halle (Einzelhallen-Betrieb) hört alles — dieselbe
  Regel wie beim Freitext.
- **Verfall nach 60 s.** Ein Gerät, das eine Minute weg war, plärrt beim
  Wiederkommen nicht die Aufrufe der letzten Minute nach; die Spiele laufen
  längst.
- **Ehrliche Rückmeldung.** Wer Aufträge abholt, gilt dem Turnier-PC als
  Ansage-Gerät seiner Halle (30 s Frist). Hört dort niemand zu, meldet die
  Seite das im Klartext — die Aktion gilt trotzdem als ausgeführt und die
  Stufe zählt hoch, sonst stünde sie später auf einem anderen Stand als das,
  was die Halle gehört hat.
- **Nichts wird nachträglich falsch gesprochen.** Steht auf dem Feld
  inzwischen ein anderes Spiel, schweigt der Sprecher lieber.
- **Je Partei ansagbar.** Beide Aufruf-Aufträge tragen eine Partei
  (`side`: `both` / `team1` / `team2`) — siehe den nächsten Abschnitt.
- **Eine unbekannte Ansageart bringt niemanden zum Schweigen.** Ein
  Ansage-Slave mit älterem Stand (Auto-Update-Fenster: zwei Rechner, einer
  aktualisiert) überspringt einen Auftragstyp, den er nicht kennt, und
  spricht die übrigen normal. Vorher scheiterte an einem einzigen
  unbekannten Eintrag die **ganze Charge**, und die zweite Halle blieb eine
  Minute lang still — auch für gewöhnliche Aufrufe. Abgesichert an zwei
  Stellen, die zusammengehören: `announce_jobs_aus_json` (`tablet/state.rs`)
  überspringt Unbekanntes beim Lesen, und `AnnounceJobPlayer.tsx` schweigt
  bei einer Art, die es nicht kennt, statt sie in den Vorbereitungs-Zweig
  durchfallen zu lassen — dort spräche es sonst einen **falschen** Aufruf.
- **Sind Ansagen ausgeschaltet, wird gar nicht erst abgefragt.** Sonst
  meldete sich das Gerät als Ansage-Gerät, ohne je zu sprechen — und die
  Turnierleitung bekäme ein beruhigendes „Aufruf ausgelöst", während in der
  Halle nichts passiert.

### „Bitte mit dem Spielen beginnen" (seit v0.9.230)

Ein Feld ist besetzt, die Spieler stehen da, es fällt kein Punkt. Die
Turnierleitung sieht das an der roten Aufruf-Uhr — und kann es jetzt auch
sagen, statt hinzulaufen oder einen Aufruf zu wiederholen.

Der Knopf **„Bitte anfangen"** steht im ⋯-Menü der Feld-Kachel in TL-Web,
sichtbar nur bei belegtem Feld, auf dem **noch kein Punkt** gefallen ist.
Ansage-Bild:

> 🔔 „**Feld 3. Bitte mit dem Spielen beginnen.**"
> (englisch: „Court 3. Please start playing.")

**Kein Aufruf.** Das ist der Kern dieser Ansage und in `tablet/tl.rs`
ausdrücklich kommentiert: Sie zählt **keine** Aufruf-Stufe hoch, lässt das
Aufruf-Abzeichen an der Kachel stehen und ändert nichts an der Fälligkeit.
Zählte sie mit, glaubte die Turnierleitung, sie hätte schon zweimal
gerufen — und die kampflose Wertung hängt am dritten Aufruf.

Folgerichtig ohne Obergrenze: Wer nach fünf Minuten immer noch nicht
spielt, wird eben erneut gebeten. Ohne Paarung (auf dem Feld steht genau
eine) und ohne Stufenwort („Zweiter Aufruf. Bitte anfangen." wäre ein
Widerspruch). Der Wortlaut liegt in
[`src/io/startPlayText.mjs`](../src/io/startPlayText.mjs) und wird in der
CI geprüft — beide Synthese-Pfade (Web Speech und Azure-SSML) nutzen
dieselben Bausteine.

Wie jede TL-Web-Ansage erklingt sie **nur in der Halle des Felds** und
**nicht** in einer per Relay angebundenen fernen Halle (siehe „Cloud" oben).
### Nachruf an die Zähltafelbedienung (seit v0.9.232)

Kommt die zugewiesene Bedienung nicht ans Feld, ruft die Turnierleitung sie
über „Bedienung nachrufen" im ⋯-Menü der Feld-Kachel:

> 🔔 „**Feld 3. Meier / Kraus, bitte als Tabletbedienung melden.**"
> Ab dem zweiten Mal mit „Zweiter Aufruf." bzw. „Dritter und letzter Aufruf."
> davor.

**Kein Spieler-Aufruf**, und das ist der Grund für die eigene Ansageart:
Der Host führt einen **eigenen** Zähler; `call_stages` und
`prep_call_stages` bleiben unberührt. Ohne die Trennung zöge ein Nachruf an
die Bedienung die angezeigte Aufruf-Zahl der Spieler hoch. Der Zählerstand
verlässt den Host nicht — er bestimmt allein die Ansage-Stufe.

Sichtbar nur bei **zugewiesenem** Bediener. Die Namen laufen ohne
Aussprache-Korrektur, wie bei „Tabletbedienung: …" am Ende der Feld-Ansage
und bei den Schiedsrichtern: eine Zuständigkeits-Ansage, keine
Spieler-Vorstellung. Details: [zaehltafelbediener.md](zaehltafelbediener.md).

### Aufruf je Partei — am Meeting Point und am Feld

Erscheint nur **eine** Partei, ruft die Turnierleitung gezielt sie nach.
Das gibt es an zwei Orten, mit derselben Wire-Angabe
(`relay_proto::PrepCallSide`: `both` / `team1` / `team2`) und demselben
Ansage-Bild (nur die genannte Partei wird vorgelesen, die andere bleibt
ungenannt):

| Ort | Aktion | Partei | Wo die Stufe lebt |
|---|---|---|---|
| Meeting Point (Spiel in Vorbereitung) | `AnnouncePrepCall` | **Pflichtfeld** | je **(Spiel, Partei)** — `prep_call_stages` |
| Feld (Spiel steht schon auf dem Court) | `AnnounceCourtCall` | **optional**, fehlend = beide | je **Feld** — `call_stages` |

Der Unterschied ist Absicht: Am Meeting Point warten die Parteien
unabhängig voneinander, am Feld gilt **eine** Stufe für alle Geräte
(„Zweiter Aufruf" muss überall dieselbe Zahl sein).

**Die Stufe am Feld zählt einmal je Aufruf-Runde.** Ein Partei-Aufruf ist
ein vollwertiger Aufruf; wer aber nacheinander Partei A und Partei B ruft,
hat **einmal** gerufen — beide hören „Zweiter Aufruf". Erst ein Aufruf an
eine Partei, die auf dieser Stufe schon dran war, eröffnet die nächste
(„Dritter und letzter Aufruf"). Der Turnier-PC merkt sich dazu je Feld die
bereits gerufenen Parteien der aktuellen Stufe
(`note_court_call_at_least`, Maske `SIDE_TEAM1`/`SIDE_TEAM2`). Ein Aufruf
aus der Desktop-Oberfläche (`reached_court_call`) gilt immer beiden
Parteien und schließt die Runde ab.

**Über den dritten Aufruf hinaus** (Profil-Option „Aufrufe unbegrenzt" in
TL-Web, seit 17.08.2026) zählt der Turnier-PC ehrlich weiter (4, 5, …) —
gesprochen wird ab Stufe 4 aber die **schlichte Feld-Ansage ohne
Stufenwort** (`AnnounceJobPlayer` bildet `stage >= 4` auf `callStage: 1`
ab): „Dritter und letzter Aufruf" noch einmal wäre gelogen, und eine
„vierte" Stufe gibt es im Sprachbild der Halle nicht. Die Klemme fällt
nur, wenn irgendein TL-Web-Profil die Option führt — sonst hält der
Turnier-PC den 3er-Deckel selbst (`tablet/tl.rs`
`unlimited_court_calls`). Bei einem **laufenden** Spiel (Punkte gefallen)
hebt der Host den Auftrag unabhängig von der Stufe auf `>= 4` — auch ein
„zweiter" Aufruf mitten ins Spiel kommt ohne Stufenwort. Der Desktop-Knopf
der Felderübersicht rechnet mit derselben Regel (`FieldOverviewPage`,
Prop `unlimitedCalls`): ab Stufe 4 heißt er „Erneut aufrufen" und sagt
ohne Stufenwort an.

**Vorgelesen** wird die Angabe am Ansage-Gerät: Der Auftrag trägt sie mit
(`AnnounceJobKind::CourtCall.side`), `AnnounceJobPlayer` reicht sie an
`announceCourt(..., side)` weiter, und das setzt — exakt wie der
Vorbereitungs-Nachruf — nur die genannte Partei als `teamANames` und lässt
`teamBNames` leer. Auch die Sprachwahl richtet sich dann nur nach den
Nationen der genannten Partei.

Ein älterer Browser, der `side` beim Feld-Aufruf nicht mitschickt, ruft
unverändert beide Parteien — `None` ist hier die neutralere, nicht die
weitreichendere Variante (Begründung: [cloud-relay.md](cloud-relay.md),
Abschnitt „Erneute Aufrufe — je Partei").

### Grenzen (Stand Schritt 9)

- **Cloud-Ansage-Slaves bekommen noch keine Aufträge.** Der Relay-Zustand
  (`cloud_announce_state`) trägt sie bis heute nicht; der Weg dorthin
  entsteht mit der Cloud-Anbindung der Turnierleitungs-Seite. Bis dahin
  bleibt eine per Relay angebundene ferne Halle stumm — sichtbar, weil sich
  dort kein Ansage-Gerät meldet und die Seite das meldet.
- **Ein Ansage-Gerät ohne eingestellte Halle hört alles.** In einem
  Mehr-Hallen-Turnier spricht es damit auch Aufrufe fremder Hallen. Das ist
  dieselbe Regel wie beim Freitext und bewusst nicht geändert; die Abhilfe
  ist `announce.announce_hall` je Gerät zu setzen.

## Ansage-Seite im Browser (seit v0.9.311)

Tablets, Handys oder ein Pi sprechen die Ansagen einer Halle — **ohne**
Windows-Programm. Der Turnier-PC baut die Ansage wie für seinen eigenen
Lautsprecher, vertont sie mit Azure und legt sie ab; die Seite spielt sie nur
ab. Spec: [features/ansage-im-browser.md](features/ansage-im-browser.md),
Entscheidung: [ADR 0063](adr/0063-ansage-seiten-text-im-host-kern-vertont.md).

**Einschalten:** Auf der Seite **Ansagen** den Schalter **Audio für
Ansage-Seiten erzeugen** setzen (voreingestellt aus).
<!-- pruef: "Audio für Ansage-Seiten erzeugen" in src/components/AnsageSeitenPanel.tsx -->
Er ist unabhängig von „Ansagen aktivieren": Der PC kann im Büro stumm bleiben
und trotzdem die Hallen versorgen. Darunter stehen Link und QR-Code je Halle.

**Am Gerät:** Die Seite `http://<PC>:8088/ansage` öffnen (mit der
verschlüsselten Strecke `https://<PC>:8443/ansage`), bei mehreren Hallen die
Halle wählen — ohne Wahl startet sie nicht —, dann einmal **Ton
freischalten** tippen. Danach spricht sie selbst.
<!-- pruef: "/ansage" in src-tauri/src/tablet/server.rs -->
<!-- pruef: "Ton freischalten" in src-tauri/assets/ansage.html -->

**Mitlesen und Wiederholen (seit v0.9.315):** Die zuletzt gespielte Ansage
steht **sehr groß** auf dem Bildschirm — die Schrift passt sich der
Textlänge an, im Hoch- wie im Querformat, damit auch Hörlose mitlesen können.
Darunter (quer: rechts daneben) stehen die **Letzten Ansagen**: die fünf
zuletzt gespielten, die neueste oben, je mit Uhrzeit (Uhr des Geräts) und
einem kleinen **↻**-Knopf.
<!-- pruef: "Letzte Ansagen" in src-tauri/assets/ansage.html -->
<!-- pruef: "auf diesem Gerät wiederholen" in src-tauri/assets/ansage.html -->
<!-- pruef: "ANSAGE_VERLAUF_MAX = 5" in src/io/ansageSeite.mjs -->

**Hochkant (seit v0.9.316)** ist die Liste auf eine Leiste
**Letzte Ansagen ▾** (in Klammern die Anzahl) eingeklappt, damit die große
Ansage deutlich mehr als die halbe Bildschirmhöhe bekommt (auf einem
üblichen Handy gut drei Viertel); ein Tipp auf die Leiste klappt sie auf und
wieder zu. Das Gerät merkt sich, ob sie offen war — gespeichert wird nur
diese Vorliebe, kein Ansagetext. Zum Wiederholen der letzten Ansage muss
man nicht aufklappen: Die große Ansage hat oben rechts ihren eigenen
**↻**-Knopf. Quer bleibt die Liste immer offen.
<!-- pruef: "Letzte Ansage auf diesem Gerät wiederholen" in src-tauri/assets/ansage.html -->
<!-- pruef: "bts-ansage-verlauf-offen" in src-tauri/assets/ansage.html -->
<!-- pruef: /id="verlauf-auf" type="button" aria-expanded="false" aria-controls="verlauf-inhalt"/ in src-tauri/assets/ansage.html -->

- **↻ spielt nur auf diesem Gerät** — der Turnier-PC erfährt davon nichts,
  andere Seiten der Halle bleiben still. Die Wiederholung wartet, bis eine
  laufende Ansage zu Ende ist, und kommt mit Gong wie das Original. Sie gilt
  auch, wenn inzwischen ein anderes Spiel auf dem Feld steht oder die Minute
  abgelaufen ist — sie ist ja ausdrücklich gewünscht. Mehrfaches Tippen
  spielt sie trotzdem nur einmal; war der Ton angehalten (Anruf, Bildschirm
  aus), gibt der Tipp ihn wieder frei.
- **Audio:** Die Seite hält das Audio der fünf im Speicher, denn auf dem
  Turnier-PC verfällt es nach einer Minute. Ohne Audio (Gerätestimme) spricht
  die Gerätestimme den Text erneut.
- **Was in die Liste kommt:** nur, was die Seite gespielt hat — keine
  verfallenen, keine für ein anderes Spiel, keine einer fremden Halle. Eine
  Wiederholung ist kein neuer Eintrag.
- **Nichts bleibt liegen:** Die Liste lebt nur in der offenen Seite. Neu
  laden leert sie; im Gerätespeicher stehen keine Spielernamen (nur die
  gewählte Halle und, ob die Liste aufgeklappt war). Beim
  Wechsel der Halle bleibt sie stehen — sie zeigt, was dieses Gerät gespielt
  hat.
- **Grenze:** Text erscheint nur, wenn der Turnier-PC die Ansage für diese
  Halle erzeugt — also nur, solange in der Halle eine Seite mit
  freigeschaltetem Ton zuhört. Eine Seite nur zum Mitlesen, ohne Ton, gibt es
  (noch) nicht. Die Monitor-Seiten im Ansage-Modus zeigen keinen Verlauf.

Was die Seite spricht: die Aufträge der Turnierleitungs-Seite (Aufruf,
Nachruf, Besetzung, „Bitte beginnen", Bedienung), Freitexte, die
Ansage-Knöpfe der Desktop-App (Feld aufrufen, Vorbereitungs-Aufruf und
-Nachruf, SR/AR ansagen) und seit v0.9.314 (Etappe E2) auch die
**automatische** Ansage, wenn ein neues Spiel auf ein Feld kommt.

**Was die Seite nie tut:**

- **Nachsprechen.** Die erste Abfrage ist Bestandsaufnahme; was länger als
  eine Minute liegt, verfällt — auch nach einem Funkloch.
- **Falsch sprechen.** Steht beim Abspielen ein anderes Spiel auf dem Feld,
  schweigt sie für diesen Aufruf.
- **Vertonen auf Zuruf.** Die Seite liest nur (GET). Der PC vertont nur,
  wenn in der Halle gerade eine Seite zuhört — ohne Zuhörer keine
  Azure-Kosten.

**Stimme:** Audio vom Turnier-PC — Azure, wenn eingerichtet, sonst seit
v0.9.319 die **Windows-Stimme des PCs** (dieselbe wie am PC, als WAV; ADR 0066).
Gibt es auch die nicht (Linux, kein Sprachpaket) oder spielt das Audio nicht,
spricht die Gerätestimme den korrigierten Text. Hat das Gerät
keine Stimme und der PC kein Audio, erklingt nur der Gong, die Seite warnt
„Dieses Gerät kann nicht sprechen" — und meldet sich **nicht** als
Ansage-Gerät.

**Turnierleitung:** Eine Seite mit freigeschaltetem Ton zählt als
Ansage-Gerät ihrer Halle; die Warnung „kein Ansage-Gerät" verschwindet dann
wie bei einem Ansage-Slave.

**Doppelbetrieb:** Spricht in derselben Halle zusätzlich diese App (mit
„Ansagen aktivieren") oder ein Ansage-Slave, klingt jeder Aufruf doppelt.
Die Seite **Ansagen** warnt dann — abschalten müssen Sie selbst.

**Bildschirm:** Nur über die verschlüsselte Adresse (8443) kann die Seite den
Bildschirm anhalten; sonst zeigt sie „Bitte den Bildschirm anlassen".

**Am TV (seit v0.9.313, Etappe E1b):** Die Monitor-Seiten (Einzelfeld,
Feld-Übersicht, Kombi) sprechen mit `?ansage=<Halle>` in der Adresse dieselben
Ansagen — ohne Knopf, der Pi startet Chromium dafür mit Autoplay-Freigabe.
Ohne Freigabe zeigt der TV einen kleinen Hinweis und meldet sich nicht als
Ansage-Gerät. Einzelheiten: [court-monitor.md](court-monitor.md)
„Ansage-Modus", Pi-Seite: [pi-setup.md](pi-setup.md).
<!-- pruef: "ansageMonitorStarten(ANSAGE_BASIS, window, document);" in src-tauri/assets/combo.html -->

**Die Seite lädt auf einem Android-Handy nicht („ERR_TOO_MANY_RETRIES"),
in Firefox geht es.** Auf neueren Android-Versionen braucht Chrome eine
eigene Erlaubnis, Geräte im lokalen Netz zu erreichen — also auch den
Turnier-PC in der Halle. Fehlt sie, scheitert jede Verbindung, und Chrome
zeigt nur „ERR_TOO_MANY_RETRIES" (gesehen auf Android 17 mit Chrome 156,
bei `http` wie `https`). Abhilfe: In den Android-**Einstellungen → Apps →
Chrome → Berechtigungen** den Eintrag **Geräte in der Nähe** bzw. **Lokales
Netzwerk** zulassen und Chrome neu starten. Chrome fragt beim ersten
Aufruf auch selbst nach — wer dort abgelehnt hat, kommt nur noch über die
Einstellungen zurück. Die Kiosk-App auf den Zähl-Tablets ist nach
bisherigem Stand nicht betroffen.

**Grenzen (Stand E3):** nur im LAN (Cloud folgt mit E4); ohne Azure kommt
das Audio vom PC als Windows-Stimme (seit v0.9.319, nur unter Windows). Wechselt eine Seite die Halle, zählt sie noch bis
zu 30 Sekunden auch für die alte — die Seite liest nur und meldet sich nicht
ab. Als Zuhörer zählt eine Seite nur mit einer Halle, die es im Turnier gibt.

## Sprache: Deutsch / Englisch / Automatisch

Einstellbar im Setup unter „Sprachansagen":

- **Deutsch** / **Englisch** — feste Sprache.
- **Automatisch** — Englisch, sobald **mindestens die Hälfte** der Spieler
  auf dem Feld international ist, sonst Deutsch. International =
  Nationalität gesetzt und ≠ `GER`. Praktisch: Einzel ab 1 von 2, Doppel
  ab 2 von 4 ausländischen Spielern.

## Feld-Bezeichnung

Endet das BTP-Court-Label auf einer Zahl (`"1"`, `"Feld 2"`, `"Court 3"`),
wird „Feld <Zahlwort>" gesprochen (Zahl als Wort, sonst spricht der
Browser „Feld erste"). Bei frei benannten Feldern (`"Center Court"`) wird
das Label wörtlich gesprochen.

## Ausgabegerät (seit v0.9.318)

Spec [ansage-ausgabegeraet](features/ansage-ausgabegeraet.md), ADR 0065/0066.
Auslöser: Am Senior-Cup (31.10.2026) kam die Ansage nicht über die per
Klinke angeschlossene Hallenanlage, alle anderen Programme schon.

- **Gerät je Ansage neu:** Der Kern fragt Windows **vor jedem Stück** (Gong,
  Stimme), welches Gerät gerade Standard ist. Wer zwischen zwei Ansagen eine
  Klinke einsteckt oder eine USB-Box abzieht, hört die nächste Ansage schon
  über das neue Gerät — ohne Neustart, ohne Klick. Reißt das Gerät **während**
  einer Ansage ab, beginnt sie auf dem neuen Gerät einmal von vorn.
- **Anzeige:** Auf der Seite **Ansagen** steht unter dem Test-Knopf, worüber
  die Ansage läuft.
  <!-- pruef: "Ausgabe: Windows-Standard" in src/components/AnsageAusgabeAnzeige.tsx -->
- **Gerät wählen (seit v0.9.321):** Darunter legt **Ansage über:** ein festes
  Gerät fest, z. B. den Kopfhörer-Ausgang zur Hallenanlage — auch wenn Windows
  ein anderes als Standard führt. Ist das gewählte Gerät nicht angeschlossen,
  kommt die Ansage über den Windows-Standard, ein gelbes Banner nennt das
  fehlende Gerät und verschwindet von selbst, sobald es wieder da ist; die
  nächste Ansage läuft dann wieder darüber. **Windows-Standard** stellt die
  Wahl zurück.
  <!-- pruef: "Ansage über:" in src/components/AnsageAusgabeAnzeige.tsx -->
  <!-- pruef: "nicht verfügbar – Ansage läuft über den Windows-Standard" in src/components/AnsageAusgabeBanner.tsx -->
  Das Banner erscheint mit der ersten Ansage über den Ersatz; vorher zeigt es
  nur die Seite **Ansagen**.
  Gemerkt wird das Gerät über seine Windows-Kennung. Steckt eine USB-Box
  später an einem **anderen Anschluss**, sieht Windows ein neues Gerät — dann
  einmal neu wählen.
  Die Wahl gilt **nur für diesen PC**: Sie wandert beim PC-Umzug nicht im
  Identitäts-Bündel mit, ein Ansage-Slave erbt sie nicht vom Master, und das
  Speichern anderer Einstellungen setzt sie nicht zurück.
- **Windows-Stimme statt Browser-Stimme:** Ohne Azure spricht die
  Windows-Stimme aus dem Kern — dieselben Stimmen wie bisher (Katja, Hedda,
  Stefan …), aber über denselben Weg wie der Gong. Die Stimmenliste in den
  Ansage-Einstellungen kommt jetzt aus Windows; früher gewählte Stimmen
  gelten weiter.
- **Rückfall, sichtbar:** Klappt die Ausgabe über Windows nicht, spielt das
  Programmfenster wie früher, und ein gelbes Banner sagt es. Es verschwindet
  von selbst, sobald die nächste Ansage wieder normal läuft.
  <!-- pruef: "Kein Ausgabegerät gefunden" in src/components/AnsageAusgabeBanner.tsx -->
  <!-- pruef: "Keine passende Windows-Stimme für die Ansage-Sprache installiert" in src/components/AnsageAusgabeBanner.tsx -->
- **Fehlt die Windows-Stimme der Ansage-Sprache** (Sprachpaket), spricht die
  Browser-Stimme — nie eine englische Stimme für einen deutschen Text.
- **Pause nach dem Gong:** Die Windows-Stimme wird erst nach dem Gong
  erzeugt. Beim allerersten Mal nach dem Start kann die Pause deshalb
  knapp eine Sekunde länger sein, danach unter 0,1 s.
- **Sehr lange Freitexte** teilt die App an Satzenden in Stücke von höchstens
  1000 Zeichen; gesprochen wird alles.
- **Notfall-Schalter:** In den Ansage-Einstellungen holt **Alter Abspielweg
  (über das Programmfenster)** das Verhalten bis v0.9.317 zurück — ohne
  Update. Dann folgt die Ansage einem Gerätewechsel nicht mehr von selbst.
  Der Schalter ist befristet und fällt drei Versionen später weg.
  <!-- pruef: "Alter Abspielweg (über das Programmfenster)" in src/components/AnnounceSettings.tsx -->
- Unter Linux bleibt es beim Weg über das Programmfenster.

### Ansage kommt aus dem falschen Lautsprecher

1. Seite **Ansagen**: Was steht bei „Ausgabe"? Ist es nicht das Gerät der
   Anlage, unter **Ansage über:** das Gerät der Anlage wählen (oder in Windows
   als **Standard** setzen) — die nächste Ansage folgt.
2. Steht dort schon das richtige Gerät und es klingt trotzdem woanders:
   Windows kann einem Programm ein festes Gerät zuordnen. **Einstellungen →
   System → Sound → Lautstärkemixer**, bei **BTS Light** das Ausgabegerät auf
   **Standard** stellen. (Bis v0.9.317 hieß der Eintrag „Microsoft Edge
   WebView2" — eine alte Zuordnung dort wirkt seitdem nicht mehr.)
3. Kommt auch aus dem **Browser** kein Ton über das Gerät, hängt es in
   Windows (gesehen nach häufigem Ab- und Anstecken einer USB-Box). Gerät aus-
   und wieder einschalten bzw. neu anstecken; danach die Test-Ansage.
4. Bluetooth-Boxen, die zwischen den Ansagen schlafen, verschlucken manchmal
   die ersten Silben — dafür kann bts-light nichts; eine Kabelverbindung hilft.

## Tonausgabe freischalten (WebView2)

Betrifft seit v0.9.318 nur noch den Rückfall-Weg über das Programmfenster.
Windows-WebView2 startet die Tonausgabe erst nach einer Nutzergeste. Der
**Test-Knopf** in den Einstellungen ist diese Geste; zusätzlich schaltet
ein einmaliger Klick irgendwo im Fenster das Audio frei. Empfehlung: vor
dem Turnier einmal die Test-Ansage drücken.

## Einstellungen (`AppConfig.announce`)

| Feld | Bedeutung |
|---|---|
| `enabled` | Ansagen an/aus (Default aus) |
| `language_mode` | `de` · `en` · `auto` |
| `voice_de` / `voice_en` | bevorzugte Stimme je Sprache (`voiceURI`), leer = Browser-Standard |
| `rate` | Sprech-Geschwindigkeit 0,5–1,5 (Default 0,8) |
| `gong` | Gong vor der Ansage (Default an) |
| `name_overrides` | Phonetische Aussprache-Korrekturen (Nutzer), Liste `{ name, say }` (Default leer) |
| `name_overrides_enabled` | Korrekturen anwenden (Basis-Wörterbuch + Nutzer)? Default `true` |
| `announce_hall` | Mehr-Hallen: nur Spiele dieser Halle (BTP-Location-Name) ansagen; leer = alle (Default). Am Master filtert die Zustellung der Aufträge (`announce_jobs_since`), am LAN-Slave der `MatchAnnouncer` neue Feldbelegungen auf `court.location`. Siehe [multi-hall.md](multi-hall.md). |
| `saved_announcements` | Gespeicherte Ansage-Blöcke (wiederkehrende Freitext-Ansagen), Liste `string` (Default leer). |
| `announce_scorekeeper` | Zähltafelbedienung mit ansagen? Default `true`. An = angesagt wird, **was am Feld steht** (zugewiesene Bedienung *oder* pro-Feld-Hinweis), aus = nie. Löst die feste Regel aus ADR 0007 ab ([ADR 0040](adr/0040-ansage-besetzung-einstellbar.md)). |
| `announce_umpire` | Schiedsrichter/Aufschlagrichter in der Feld-Ansage nennen? Default `true`. Die eigenen Knöpfe „SR/AR ansagen" und „Bedienung nachrufen" bleiben unberührt. |
| `page_audio` | „Audio für Ansage-Seiten erzeugen": Der PC vertont Ansagen für die Browser-Seite `/ansage`, unabhängig von `enabled`. Default `false`. Siehe [Ansage-Seite im Browser](#ansage-seite-im-browser-seit-v09311). |

> **UX:** Ist `azure_tts.enabled` aktiv, blendet [`AnnounceSettings`](../src/components/AnnounceSettings.tsx) die
> Standard-Stimmen-Auswahl (`voice_de`/`voice_en`) aus — Azure spricht die ganze Ansage, die lokale Stimme
> wäre wirkungslos (Fallback bei Fehler/offline greift weiterhin automatisch).

## Gong-Klang (Spielaufruf vs. Freitext/Info)

`playGong(ctx, kind)` in [`announcer.ts`](../src/io/announcer.ts) erzeugt zwei **deutlich verschiedene**
Gongs: **Spielaufruf** (`kind="match"`, Default) = tiefer, zweitöniger absteigender Sinus-Gong (A5 → D5);
**Freitext/Info** (`kind="info"`) = heller, dreitöniger aufsteigender Dreiklang (C5-E5-G5, Triangle). So ist
sofort hörbar, ob es ein Spielaufruf oder eine sonstige Durchsage ist.

**Timing (v0.9.155):** Der Gong wird auf der Audio-Uhr (`ctx.currentTime`) geplant, sein Ende aber am
**echten Audio-Ende** signalisiert — `gongFinished()` löst auf dem `onended` des zuletzt stoppenden
Oszillators auf (nicht auf einer festen Wall-Clock-`setTimeout`), plus `GONG_BREATH_MS` (150 ms) Atempause.
Startet der AudioContext in WebView2 verzögert, verschiebt sich das Ende real mit → die Sprache setzt nicht
mehr in den Nachklang ein (Tilo-Befund 19.07.). Ein Fallback-Timer kurz nach dem geplanten Ende verhindert,
dass die Ansage-Queue hängt, falls `onended` ausnahmsweise ausbleibt.

## Geteiltes Community-Wörterbuch (crowd-sourced)

Zusätzlich zum mitgelieferten Basis-Wörterbuch und der lokalen Nutzer-Tabelle
lädt bts-light ein **gemeinsames Aussprache-Wörterbuch** von badhub
(`GET /api/v1/pronunciations`):

- **Laden:** beim Start (kurz nach dem Config-Load) und danach alle 3 h, solange
  Internet da ist (`App.tsx`). Die Liste geht über `setSharedOverrides()` in
  [`announcer.ts`](../src/io/announcer.ts).
- **Offline:** Der Rust-Command `fetch_pronunciations` cached die Liste lokal
  (`pronunciations_cache.json` im App-Config-Verzeichnis) und liefert sie ohne
  Internet aus dem Cache → der reine LAN-Hallenbetrieb spricht weiter korrekt.
- **Priorität** in `buildOverrideMap`: Basis-Wörterbuch < **geteiltes
  Wörterbuch** < lokale Nutzer-Tabelle (eigene Korrekturen gewinnen immer).
- **IPA für Azure:** Einträge können zusätzlich ein `ipa`-Feld tragen. Im
  Azure-Pfad baut `buildIpaMap` daraus eine Map und `nameSsml` spricht den Namen
  über inline `<phoneme alphabet="ipa" ph="…">` (ganzer Name oder wortweise).
  Web Speech (offline) ignoriert `ipa` und nutzt `say`. Quelle: kuratiertes
  W3C-PLS-Lexikon (badhub).
- **`say`-Ersatzschreibweise auch bei Azure (seit v0.9.167):** Fehlt IPA, nutzt
  der Azure-Pfad jetzt ebenfalls die phonetische Ersatzschreibweise `say` (als
  gesprochenen Text), statt sie zu ignorieren. Die Rangfolge je Name/Wort regelt
  `resolveNameCorrection` (`src/io/nameCorrection.mjs`, node-testbar):
  **IPA (`<phoneme>`) → `say`-Text → `<lang>`-Erkennung**. Damit wirkt eine im
  Setup getippte Korrektur (z. B. „Chybych" → „Chübüch") auf **beiden** Stimmen
  — vorher war `say` auf dem Azure-Pfad totes Feld. Der ganze-Name-`<lang>`-
  Fallback greift unverändert nur, wenn dieser Name **keinen** Wort-Treffer hat
  (bessere Sprach-Erkennung als wortweise).
- **Sprach-Erkennung (`<lang>`-Pfad):** `detectNameLang` (`src/io/transliterate.ts`)
  erkennt die Herkunftssprache: markante CN/VN-Nachnamen + kuratierte Namenslisten
  `NAME_LANG_BASE` (es/fr/pl/tr/ms/in, generiert aus `data/name-lists/*.xml` via
  `scripts/gen-name-lang-base.mjs`). **Konfidenz:** nur bei eindeutiger Sprache wird
  `<lang xml:lang="…">` gesetzt (Locale-Map in `announcer.ts`), sonst deutscher
  Default — mehrdeutige Namen werden nicht „geraten". Nur cn/vn haben zusätzlich
  eine deutsche Umschrift (Web-Speech); die übrigen Sprachen wirken nur im
  Azure-`<lang>`-Pfad. Plan: `docs/features/name-pronunciation-plan.md`.
- **Manuelle Sprach-Korrektur:** `NameOverride.lang` (Nutzer-Tabelle) erzwingt je
  Name die Sprache, wenn die Erkennung daneben liegt. `buildLangOverrideMap` +
  `nameSsml` werten sie aus; Vorrang: Override (`"de"` = kein Tag, sonst
  `<lang>`) → kuratiertes IPA → `say`-Ersatzschreibweise → automatische
  Erkennung. Cache-Key in
  `azure_tts.rs` hasht das vollständige SSML (inkl. `<lang>`/`<phoneme>`), daher
  keine Kollision zwischen gleich geschriebenen Namen mit anderer Sprache.
- **Teilen (opt-in):** Schalter „Meine Korrekturen mit der Community teilen"
  (`announce.share_corrections`, Default aus). Beim Speichern werden die eigenen
  Einträge via Rust-Command `share_pronunciations`
  (`POST /api/v1/pronunciations`) gesendet. Qualitätsmodell: sofort live, Admin
  räumt nach. Server-Doku: badhub `docs/features/tts_pronunciations.md`.

## Verlauf, erneutes Abspielen & gespeicherte Blöcke

Auf der Ansagen-Seite ([`AnnouncePage.tsx`](../src/pages/AnnouncePage.tsx)):

- **Verlauf (letzte 10):** Jede **manuell** ausgelöste Ansage (Freitext + manuelle Feld-Ansage) wird in
  [`src/state/announceHistory.ts`](../src/state/announceHistory.ts) protokolliert (localStorage, max. 10,
  neueste zuerst). **Automatische Spielaufrufe landen NICHT im Verlauf.** Jeder Eintrag hat „Erneut" —
  Freitext wird neu über den Master verschickt (`publish_freetext`), eine Feld-Ansage lokal neu abgespielt
  (aus dem mitgespeicherten `CourtOverview`-Schnappschuss).
- **Gespeicherte Blöcke:** Aktuellen Freitext per „Als Block speichern" in `announce.saved_announcements`
  ablegen (dedupliziert, persistiert). Jeder Block lässt sich direkt ansagen (Halle wie im Freitext-Selektor,
  Master → Slaves), ins Textfeld laden oder löschen.

## Aussprache-Korrekturen (`name_overrides`)

Spricht die TTS-Stimme einen Namen falsch (die deutsche/englische Stimme liest
fremdsprachige Buchstaben nach ihren eigenen Regeln — betrifft viele Herkünfte:
asiatisch/indisch `ph tr x zh q`, französisch stille Endungen/`j`, spanisch
`j ll ñ z`, türkisch `ç ş ğ y`, polnisch `sz cz ł` …), lässt sich pro
**Name oder Namensteil** eine **Ersatz-Schreibweise** hinterlegen, die die
Stimme besser trifft (z. B. `Nguyen` → `Nujen`, `Lefebvre` → `Löfäwr`).

- **Basis-Wörterbuch** (`src/io/nameOverrideBase.ts`, `BASE_NAME_OVERRIDES`):
  mitgelieferte Liste häufiger fremdsprachiger Nachnamen (abgeleitet aus den
  häufigsten Namen der Badhub-Spieler-DB), wird **automatisch** angewendet. Die
  **Nutzer-Tabelle** (`name_overrides`) hat **Vorrang** (gleicher Schlüssel
  überschreibt die Basis). Schalter `name_overrides_enabled` schaltet beides ab.
- **Anwendung** (`src/io/announcer.ts`, `joinNames`): pro Spielername zuerst ein
  **exakter Voll-Name-Treffer**, sonst **Wort für Wort** — ein einmal
  eingetragener Nachname wirkt also für alle Spieler:innen mit diesem Namen.
- **Matching diakritik-/sonderzeichen-unabhängig** (`normalizeName`): NFD-Faltung
  + ı/İ/ø/ł/đ → „Nguyên"/„Nguyen", „Yıldız"/„Yildiz", „García"/„Garcia" treffen
  denselben Eintrag. Whitespace + Wort-Satzzeichen bleiben beim Vorlesen erhalten.
- **Reichweite:** Es ist **keine zusätzliche Sprache** — die Ansage bleibt
  de/en, nur die Aussprache einzelner Namen wird ersetzt. Läuft offline
  (kein externer Dienst).
- **Pflege:** Tabelle im Setup → Abschnitt *Ansagen* → *Aussprache-Korrekturen*.
  Das Basis-Wörterbuch wirkt automatisch; in der Tabelle pflegst du nur eigene
  Korrekturen/Ergänzungen (Vorrang). ▶ je Zeile spielt die Aussprache ab.
- **SSML/Phoneme** sind bewusst NICHT genutzt — Browser-`SpeechSynthesis`
  (WebView2/Chrome) ignoriert `<phoneme>`; nur die Ersatz-Schreibweise wirkt.

### Regelbasierte Umschrift (CN/VN) — `src/io/transliterate.ts`

Greift, wenn Wörterbuch UND Nutzer-Tabelle keinen Treffer haben, damit auch
NICHT gelistete chinesische/vietnamesische Namen besser klingen. Reihenfolge je
Wort in `applyOverride`: **Wörterbuch/Tabelle → Regel-Engine → unverändert**.

- **Erkennung** (`detectNameLang`): nur wenn ein Token ein **markanter** CN- bzw.
  VN-Nachname ist (Trigger-Listen; kurze/mehrdeutige wie „Le"/„Do"/„Ma" sind KEIN
  Trigger). Deutsche/andere Namen → keine Umschrift. Innerhalb eines erkannten
  Namens sind aggressive Regeln (v→w, x→s) gefahrlos.
- **Pinyin** (`pinyinToGerman`): Silben-Segmentierung (greedy Initial+Finale) →
  zh→dsch, ch→tsch, sh/x→sch, q/j→tsch/dsch, c→ts, z→ds, w→u, y→j; apikales i
  (zhi/chi/shi/zi/ci/si/ri)→„i"; nach j/q/x wird u→ü.
- **Vietnamesisch** (`vietnameseToGerman`): konsonant-fokussiert — tr→tsch, th→t,
  ph→f, ch→tsch, kh→ch, nh→nj (Anlaut)/n (Endung), ng/ngh→ng, gi→j, qu→ku, x→s,
  v→w; Endung -c/-ch→k. **„d" bleibt „d"** (ASCII verschluckt das đ; weiche
  d-Fälle wie „Duong"→„Juong" deckt das Wörterbuch ab).
- **Grenze:** Konsonanten zuverlässig; Vokale/Töne/Dialekt (südvietnamesisch,
  Wade-Giles) nur Näherung → Spezialfälle in der Nutzer-Tabelle (Vorrang).

## Vorbereitungs-Ansage (Spiele in Vorbereitung)

Neben der Feld-Ansage gibt es eine zweite Variante: aus dem
„In Vorbereitung"-Tab kann die Turnierleitung je gerufenem Spiel eine
**Hallen-Ansage** auslösen — Knopf „Ansage" neben dem Aufruf
([preparation.md](preparation.md)). Format:

> Gong → **„In Vorbereitung."** → Disziplin → Paarung („… gegen …") →
> **„Bitte in *Halle X*."** (entfällt bei Ein-Hallen-Turnieren)

Englisch entsprechend: „Preparation call." → Discipline → „… versus …" →
„Please report to *hall*."

Unterschiede zur Feld-Ansage:

- **Kein Feld** — die Ansage trägt keinen Court (das Spiel ist noch nicht
  auf einem Feld); statt des wiederholten Court-Worts steht am Ende die
  Halle, in die gerufen wurde.
- **Manueller Auslöser, kein Auto-Detektor.** Eine eigene `MatchAnnouncer`-
  Schwester gibt es nicht — der Knopf-Klick im Panel ist gleichzeitig die
  User-Geste, die WebView2 zum Entsperren der Tonausgabe braucht.
- **Sprach-Auflösung geteilt** mit der Feld-Ansage:
  `resolveAnnouncementLanguage(nationalities, mode)` in `announcer.ts` —
  Auto-Modus nutzt dieselbe Regel (≥ Hälfte international ⇒ Englisch).
- **`AnnounceConfig.enabled` gilt für beide.** Ist die Ansage global
  abgeschaltet, ist auch der „Ansage"-Knopf im Vorbereitungs-Tab
  ausgeblendet.

Engine: `playPreparationAnnouncement` / `buildPreparationSegments` in
[`src/io/announcer.ts`](../src/io/announcer.ts).

Eingeführt in v0.9.16.

### Zweiter/Dritter Aufruf je Partei (Plan 1)

Erscheint nur **eine** Partei, ruft die Turnierleitung gezielt die
**fehlende** Seite nach. Je gerufenem Spiel in der Vorbereitungs-Übersicht
gibt es dafür zwei kleine Knöpfe (einer je Partei, mit dem Nachnamen zur
Unterscheidung). Ansage-Format (an Tilos BTS angelehnt, `callStage` in
`AnnouncePreparationInput`):

> **„Zweiter Aufruf für: {nur die genannte Partei}."** → „Bitte in *Halle*."

Ein zweiter Druck auf dieselbe Partei macht daraus **„Dritter und letzter
Aufruf für: …"**. Der Zähler je (Spiel, Partei) lebt clientseitig im
Master-Fenster (kein Server-/Ticker-Zustand — wie Tilos Pass-Through-
Zweitaufruf). Es wird **nur die genannte Seite** angesagt (die andere ist
schon da). Englisch: „Second call for: …" / „Third and final call for: …".

**Auch am fernen Slave (Stufe 2, v0.9.154).** Derselbe Nachruf geht jetzt
vom Cloud-Slave der fernen Halle aus — und wird **lokal auf dem Slave-
Rechner** angesagt, direkt dort, wo die fehlende Partei steht (kein
Rückkanal zum Master nötig). Datenweg:

- Der Master pusht seine **aufgerufenen Spiele** (nur gerufene, noch
  ruf-bare Paarungen) periodisch als `HostFrame::Prepared` an den Relay –
  gebündelt, nur bei Änderung (Fingerabdruck in `push_prepared`,
  `relay_client.rs`; reine Liste über `build_prepared_list`). Ein leerer
  Push leert die Relay-Liste (kein Aufruf mehr offen).
- Der Relay hält sie je Namespace (`Namespace.prepared`) und gibt sie in
  `GET /{ns}/info/announce/state` **hallengefiltert** zurück
  (`AnnounceState.prepared`, gleiche Filterregel wie die Court-Matches).
- Der Slave holt sie über `cloud_announce_state` (`CloudPrepared`) und
  zeigt sie auf der Ansagen-Seite unter **„Aufgerufene Spiele"**. Die
  zwei Nachruf-Knöpfe je Partei rufen dieselbe
  `playPreparationAnnouncement`-Funktion mit `callStage` auf wie der
  Master; der `(Spiel, Partei)`-Zähler lebt clientseitig im Slave-Fenster.

Damit ist der Nachruf in beiden Rollen identisch – nur die Kandidatenliste
kommt am Slave aus der Cloud statt aus BTP. Siehe auch
[docs/multi-hall.md](multi-hall.md) und [docs/cloud-relay.md](cloud-relay.md).

## Bekannte Grenzen

- Der Sync-Takt (am Slave der 2-s-Poll) kann ein extrem kurz belegtes und
  sofort wieder geräumtes Feld verpassen — für reguläre Feld-Aufrufe
  unkritisch.
- Wird die Übertragung längere Zeit gestoppt und wieder gestartet, gilt der
  Stand vor dem Stopp weiter als Bestandsaufnahme: Felder, die in der Pause
  neu belegt wurden, werden nach dem Start einmal angesagt (Aufträge
  verfallen nach 60 s). Dasselbe Verhalten hatte der `MatchAnnouncer`.
- Verfügbare Stimmen hängen vom Windows-System ab; ist die gewählte
  Stimme auf dem Rechner nicht vorhanden, nutzt der Browser seine
  Standardstimme.

## Beteiligte Dateien

- `src-tauri/src/btp/model.rs` — `Discipline`, Event-Parsing.
- `src-tauri/src/tablet/state.rs` — `CourtOverview` (`match_id`,
  `discipline`, Nationalitäten).
- `src-tauri/src/config.rs` — `AnnounceConfig`, `NameOverride`.
- `src/io/announcer.ts` — Gong + Sprachsynthese + Aussprache-Korrekturen (`normalizeName`-Faltung, `buildOverrideMap`, `applyOverride`, `playNameTest`).
- `src/io/nameOverrideBase.ts` — mitgeliefertes Basis-Wörterbuch (`BASE_NAME_OVERRIDES`).
- `src/io/transliterate.ts` — regelbasierte CN/VN-Umschrift (`detectNameLang`, `pinyinToGerman`, `vietnameseToGerman`).
- `src/io/azureAnnounce.ts` — baut die `AnnounceOptions.azure`-Option (nur wenn aktiv).
- `src-tauri/src/azure_tts.rs` + `commands::azure_tts_speak` — Azure-Synthese (Key im Backend) + Datei-Cache.
- `src-tauri/src/tablet/state.rs` — `AnnounceJob`/`AnnounceJobKind`, `call_stages`
  (Aufruf-Stufen am Host, inkl. der Parteien-Maske der laufenden Runde),
  `side_mask`/`SIDE_TEAM1`/`SIDE_TEAM2`, `announce_listeners` (wer hört gerade zu).
- `src-tauri/src/tablet/tl.rs` — `AnnounceCourtCall`/`AnnouncePrepCall`,
  `due_call_stage` (zeitliche Fälligkeit), `announcement_response` (Warnung
  ohne Ansage-Gerät).
- `src/components/AnnounceJobPlayer.tsx` — spricht die Aufträge der
  Turnierleitungs-Seite und (seit v0.9.314) die Erst-Ansage `court_assigned`.
- `src/io/announceJobs.ts` — `jobZuAnsage`: Auftrag → fertige Ansage, geteilt
  von `AnnounceJobPlayer` und `AnsageVertoner`.
- **Ansage-Seite:** `src/io/announcer.ts` `prepare*`/`playPrepared`,
  `src/io/announceCourt.ts` (Builder `…Ansage` + Desktop-Knöpfe, die auch an
  die Seiten geben), `src/io/ansageTeilen.ts`, `src/components/AnsageVertoner.tsx`,
  `src/components/AnsageSeitenPanel.tsx`, `src-tauri/src/tablet/ansage.rs`
  (`AnsageStore`), Routen `/ansage*` in `tablet/server.rs`,
  `commands.rs` `ansage_*`, `src-tauri/assets/ansage.html` mit Inline-Kopien
  von `src/io/ansageSeite.mjs` (Entscheidungen), `src/io/gongKlang.mjs` und
  `src/io/ansageKern.mjs` (Abfrage, Warteschlange, Fristen, Präsenz; seit
  v0.9.313; Verlauf + Wiederholen mit `verlauf: true` seit v0.9.315 —
  `ansageText`/`verlaufAufnehmen`/`wiederholungsAuftrag`/`schriftSuchen` in
  `ansageSeite.mjs`) — dieselben drei Kopien tragen `monitor.html`, `overview.html`
  und `combo.html` für den Ansage-Modus (`ansageMonitorStarten`)
  (`scripts/test-ansage-seite.mjs`, `scripts/test-gong-klang.mjs`,
  `scripts/test-ansage-kern.mjs` — Verhalten mit Attrappen + Drift über alle
  vier Seiten).

## Azure Neural TTS (hochwertige Cloud-Ansage, opt-in)

Ist `azure_tts.enabled`, wird die **ganze Ansage als ein SSML** (`buildAnnouncementSsml` /
`buildPreparationSsml` in `announcer.ts`) an Azure geschickt: deutscher/englischer Rahmen +
**jeder Name in seinem `<lang>`-Span** (`detectNameLang` → `zh-CN`/`vi-VN`), sodass die neuronale
Stimme die Namen **nativ** spricht. Ablauf:

- Frontend baut SSML → Tauri-Command **`azure_tts_speak`** (`commands.rs`) → `azure_tts::synthesize`
  (POST an `https://<region>.tts.speech.microsoft.com/cognitiveservices/v1`). **Key/Region bleiben im
  Backend** (aus `AppConfig.azure_tts`). Antwort = MP3 (Base64) → Web Audio spielt es nach dem Gong.
- **Cache:** MP3 je SSML-Hash unter `app_data_dir/tts-cache/` → Wiederholungen/„nochmal aufrufen"
  kosten kein Netz/Geld.
- **Fallback:** Azure aus / kein Key / Netzfehler → nahtlos die lokale Web-Speech-Ansage (mit Wörterbuch
  + Regel-Umschrift). Nie stumm, nie blockierend — aber **sichtbar**: jeder Azure→Web-Speech-Rückfall
  setzt über `reportAzureFallback` (`src/state/azureStatus.ts`) den App-weiten
  [`AzureFallbackBanner`](../src/components/AzureFallbackBanner.tsx) (quittierbar) und landet als
  `console.warn` im Diagnose-Log. Zusätzlich warnt `AnnounceSettings`, wenn der Schalter an ist,
  aber Key/Region fehlen (das Frontend-Gate `azureOption` liefert dann gar keine Azure-Option mehr).
- **Konfig** (`AppConfig.azure_tts`): `enabled`, `region`, `key`, `voice`, `discipline_voices`.
  Einrichtung im Setup → *Ansagen*. Spielernamen werden XML-escaped in die SSML eingesetzt.
- **Stimme je Disziplin (optional, seit v0.9.168):** `discipline_voices` bildet Disziplin-Kürzel
  (`mens_singles`, `mens_doubles`, `womens_singles`, `womens_doubles`, `mixed`) → Azure-Stimme ab.
  Vor der Synthese wählt [`voiceForDiscipline`](../src/io/disciplineVoice.mjs) (node-testbar,
  `scripts/test-discipline-voice.mjs`) je Ansage die Stimme: hinterlegter Eintrag, sonst die
  Standard-`voice`. Damit lässt sich z. B. Herren-Disziplinen männlich (Florian), Damen-Disziplinen
  weiblich (Seraphina) ansagen — frei pro Disziplin, leer = Standard (kein Zwang, abwärtskompatibel
  per `#[serde(default)]`). Greift für Feld- **und** Vorbereitungs-Ansage; Freitext nutzt die
  Standard-Stimme. **Vererbung (seit v0.9.169):** Die Pro-Disziplin-Stimmen werden über den
  Azure-Vererbungs-Pfad (ADR 0003) an eine ferne Cloud-Halle mitvererbt —
  `AzureTtsShare.discipline_voices` (Master → Relay → Slave-Backend) und
  `CloudAnnounce.azure_discipline_voices` (Slave-Frontend). Die automatische Feld-Ansage der fernen
  Halle (`CloudAnnounceSlave`) nutzt so dieselbe Zuordnung wie der Master; ein Slave mit **eigener**
  vollständiger Azure-Config nutzt ohnehin seine eigene.
- **Vererbung an Cloud-Slaves** ([ADR 0003](adr/0003-azure-tts-vererbung-relay.md)): Der Master
  schickt seine Azure-Config (nur wenn aktiv **und** vollständig) huckepack im
  `HostFrame::Courts`-Push an den Relay; der Relay liefert sie im `AnnounceState`
  (`azureTts`-Feld) an den Cloud-Slave aus. Dort hält `AppState.inherited_azure` sie **nur im
  RAM** (nie in der `config.json`); `azure_tts_speak` wendet die Vorrangregel `effective_azure`
  an: vollständige lokale Config gewinnt, sonst die geerbte. Das Frontend bekommt bewusst **nur
  die Stimme** (`CloudAnnounce.azure_voice`) — der Key bleibt im Backend. Die Ansage-Einstellungen
  zeigen am Slave „Vom Master geerbt ✓". Ein Slave braucht damit **keine** Azure-Eingaben mehr;
  der Zwei-Hallen-Bug „Schalter an, Key fehlt → still Standardstimme" ist doppelt abgedeckt
  (Vererbung + Warnung/Banner).
- **Datenschutz:** Bei aktivem Azure-TTS werden die **Namen der gerufenen Paarung** zur Synthese an
  Azure (Region wählbar, z. B. West Europe/EU) gesendet — öffentliche Wettkampfdaten, opt-in über
  `enabled`. Ist Azure aus, verlässt nichts das Gerät.
- `src/state/useAvailableVoices.ts` — System-Stimmen.
- `src/components/MatchAnnouncer.tsx` — immer eingehängt (Audio-Freischaltung, Abbruch beim Abschalten); erkennt neue Feldbelegungen nur noch am Ansage-Slave.
- `src-tauri/src/tablet/erst_ansage.rs` — Erst-Ansage im Kern (Master, seit v0.9.314).
- `src/pages/SetupWizard.tsx` — Einstellungen.

## Schiedsrichter und Aufschlagrichter

Läuft das Turnier mit Schiedsrichtern (Spec
[schiedsrichter-management](schiedsrichter-management.md)), nennt die
Feld-Ansage nach der Tabletbedienung:

- „Schiedsrichter: {Name}." / englisch „Umpire: {Name}."
- „Aufschlagrichter: {Name}." / englisch „Service judge: {Name}."

Angesagt wird nur, was auch zugewiesen ist; ohne Schiedsrichter-Betrieb
entfällt beides ersatzlos. Der Schalter `announce_umpire` (Default an)
schaltet die Nennung ganz ab.

**Bis v0.9.245 fehlten SR/AR in der AUTOMATISCHEN Feld-Ansage**: Der
Ansage-Detektor (`MatchAnnouncer.tsx`) baute sein `AnnounceMatchInput` selbst
zusammen und ließ beide Listen weg, während der manuelle Aufruf und jeder
Nachruf sie über `announceCourt` mitgaben. Dieselbe Belegung klang dadurch je
nach Auslöser anders (Nutzer-Befund 20.08.2026, behoben in v0.9.246).

Segment- und SSML-Bauer nutzen dieselbe Quelle (`officialSegments` in
`io/announcer.ts`), damit Browser-Stimme und Azure-Stimme identisch sprechen.

**Die ferne Halle sagt sie seit v0.9.248 ebenfalls an.** Bis dahin fehlten sie
dort, obwohl die Namen längst ankamen: Der Host füllt sie in `match_brief`, der
Relay reicht den `MatchBrief` unverändert durch — nur die Umwandlung in
`CloudAnnounceCourt` (`commands.rs`) ließ sie fallen. Wächter:
`die_ferne_halle_bekommt_auch_die_besetzung`.

> **Die beiden Schalter gelten je Gerät, nicht je Turnier.** Anders als die
> Azure-Stimme (ADR 0003) werden sie **nicht** vom Master geerbt: Der
> Cloud-Slave liest `announce_scorekeeper`/`announce_umpire` aus seiner
> **eigenen** Config. Wer am Turnier-PC „Schiedsrichter mit ansagen" abwählt,
> hört sie in der fernen Halle weiterhin, bis der Haken auch dort fällt.

**Nachträgliche Zuweisungen sagen nicht von selbst an.** Wer mitten im Spiel
einen Schiedsrichter einteilt, löst die Ansage mit dem Knopf „ansagen" aus —
in der Schiedsrichter-Seite des Clients oder an der Feld-Kachel in TL-Web.
Diese Ansage nennt nur Feld und Besetzung, nicht noch einmal die Paarung.
