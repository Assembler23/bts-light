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
| `rotation_mode` | `abwechselnd` | Einteilungsprinzip (seit v0.9.356, ADR 0079): `getrennt` (A), `abwechselnd` (B), `rollenwechsel` (C). Ein unbekannter Wert lädt als `abwechselnd`. Wirkt seit v0.9.357; Auswahl „Einteilungsprinzip" im Setup seit v0.9.362. |

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
| Rotationsreihenfolge | `order: Vec<i64>` | Reihenfolge der Auto-Rotation, manuell umsortierbar (`set_order`, `reorder`); in Modus A die SR-Liste |
| AR-Liste (seit v0.9.356) | `order_ar: Option<Vec<i64>>` | Eigene AR-Reihenfolge für Modus A (ADR 0079). `None` ⇒ gelesen wird eine Kopie von `order` (`order_ar()`, legt nichts an). Angelegt erst beim ersten Schreibbedarf (`reorder_in(Reihenfolge::Ar, …)`); IDs, die nur in `order` stehen, werden beim Lesen hinten ergänzt. Neue BTP-Officials kommen an beide Listen. |
| Zusatzdaten je Official | `OfficialExtra { paused, club, blocked_clubs, blocked_players, role }` | Pause, Stammverein (BTP liefert keinen), Sperrlisten, **Einsatzrolle** (seit v0.9.356: `beides`/`sr`/`ar`, `Einsatzrolle`, gilt in allen Modi; „beides" wird nicht geschrieben) |
| Lokale Zuweisungen | `assignments: Match-ID → MatchOfficials { sr, ar }` | Overlay für Spiele, an denen BTP nichts stehen hat |
| Feldweise Schalter | `courts: CourtID → CourtSwitches { sr, ar, operator }` | SR-Rotation, AR-Rotation, Tabletbediener-Vergabe je Feld |

**Rückweg auf eine ältere Version:** Eine App vor v0.9.356 liest die Datei
weiter, verliert aber `role` und `order_ar` beim nächsten Speichern, ohne
Rückmeldung (bewusst hingenommen, ADR 0079).

**Zuweisungen hängen am Match, nicht am Feld.** Nach Spielende bleiben sie
liegen — sie sind die Grundlage der Einsatz-Ableitung (Spec Nr. 11: keine
eigene Historien-Datenhaltung). Geräumt wird nur beim Abschalten von
`officials.enabled` (`clear_assignments`) und beim Turnierwechsel.

### Einsatz-Zählung: Walkover zählt nicht (seit v0.9.353)

Ein Einsatz ist ein **beendetes** Spiel, an dem die Person als SR oder AR
eingeteilt war (BTP-Wert oder lokale Zuweisung, `OfficialsStore::appearances`).
Seit v0.9.353 (Spec `tl-warteliste-schiedsrichter`, AK 17) zählt ein Spiel mit
Ergebnis **Walkover** (`MatchResult::Walkover`) **nicht** mehr: Kampflos wurde
nicht gespielt, also war auch niemand im Dienst. **Aufgabe** (`Retired`) und
**Disqualifikation** zählen weiter — da stand jemand am Feld.

- Das ist eine **bewusste Statistik-Änderung**: Wer im laufenden Turnier
  Walkover-Spiele in der Besetzung hatte, sieht nach dem Update einen
  kleineren Zähler.
- Betroffen sind alle vier Stellen, weil sie dieselbe Ableitung lesen:
  TL-Web-Zähler (`TlOfficial.appearances`), Desktop-Zähler
  (`OfficialView.appearances`) und die Detail-Listen (`official_detail_json`,
  `official_appearances`). Das Walkover-Spiel fehlt deshalb auch **in der
  Liste** — Zähler und Listenlänge stimmen immer überein.
- Die Eingabe baut überall `FinishedMatch::aus_btp(&BtpMatch)` (eine Stelle
  statt drei Kopien), das Feld `FinishedMatch.walkover` trägt die Kennung.
- Unverändert: die Rotation (`move_to_end`) und die Reservierung (ein
  Walkover beendet sie wie jedes Spielende).

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

Ein Official kommt in Frage, wenn er **alle** Bedingungen erfüllt: BTP kennt
ihn noch, er ist nicht pausiert, er tut nirgends sonst Dienst (SR *oder* AR),
er hat keinen Konflikt mit diesem Spiel und er ist nicht schon der andere
Dienst desselben Spiels. Konflikte werden dabei **still** übersprungen —
gewarnt wird nur bei manueller Zuweisung (Spec Nr. 4).

**Wer welchen Dienst bekommt** (seit v0.9.357, ADR 0079), entscheidet die
reine Funktion `officials.rs::waehle_besetzung` — der einzige Wahlweg; Feld-
Rotation und Meetingpoint-Vorab-SR rufen sie über `OfficialsStore::waehle`
auf. Sie beachtet die Rolle je Person (`nur SR`/`nur AR` nie im anderen Dienst)
und das Einteilungsprinzip `rotation_mode`:

| Modus | Regel |
|---|---|
| A `getrennt` | SR = erste passende Person der SR-Liste (`order`), AR = erste passende der AR-Liste (`order_ar`), die nicht schon SR ist. |
| B `abwechselnd` (Standard) | Beide aus `order`: erster Freier SR, nächster AR. |
| C `rollenwechsel` | Personenweise von oben: Jeder bekommt den offenen Dienst, den er im zuletzt beendeten Spiel **nicht** hatte (feste Rolle schlägt die Vorrolle, ohne Vorgeschichte SR vor AR). Ist der Wunsch vergeben, wird die Person übersprungen. Was offen bleibt, bekommt die erste passende freie Person (Rückfall). |

**Rolle je Person** (seit v0.9.362 im Schiedsrichter-Panel wählbar: „SR und
AR", „nur SR", „nur AR"; Command `official_set_role`). Eine Besetzung gegen
die Rolle — von Hand oder in BTP — ist erlaubt, warnt aber mit „SR: nur AR"
bzw. „AR: nur SR" an Feldkachel, TL-Feld, Übersicht und Warteliste und in der
Rückmeldung der Einteilung (`OfficialsStore::rollen_warnung`, eingehängt in
`TabletState::court_officials` und `official_assign`). Sie ist **kein**
Konflikt und keine Ablehnung — der SR⇄AR-Tausch muss weiter gehen.

**Vorschau „als Nächstes"** (seit v0.9.364): `OfficialsStore::prognose`
simuliert die Wahl Runde für Runde auf einem geklonten Stand, ohne etwas zu
speichern oder eine AR-Liste anzulegen, und liefert je Person Dienst und Runde
(0 = nächste Besetzung). Kandidaten sind die Freien laut `Belegung` (ohne
Konfliktprüfung, es gibt noch kein Spiel); offene Dienste nach den globalen
Schaltern; beide aus ⇒ keine Vorschau. `prognose_aus` ist die eine Rechnung
für das Panel am Turnier-PC (`OfficialView.next_role/next_rank`) und das
TL-Web (`TlOfficial.next_role/next_rank`, dazu `TlOfficial.role` und
`TlState.officials_rotation_mode`). Im Panel zeigt Modus A zwei Listen, jede
einzeln ziehbar (`official_reorder(…, list: "ar")`).

Die **Vorrolle** (`OfficialsStore::vorrollen`) wird je Takt aus den beendeten
Spielen abgeleitet, nicht gespeichert: Sicht `effective`, Walkover zählt
nicht, maßgeblich ist `(finished_at oder 0, match_id)`. Besetzungen desselben
Takts ändern sie nicht.

- **Nur beim Neu-Belegen:** Bestückt wird ein Feld in dem Zyklus, in dem ein
  anderes Spiel darauf kommt (Merker `officials_oncourt_prev`). Wer eine
  Zuweisung von Hand löscht, bekommt sie deshalb **nicht** im nächsten Poll
  zurück.
- **Der erste Blick ist Ausgangslage** (seit v0.9.253): Solange die Rotation
  noch gar keinen Stand kennt (`officials_oncourt_prev == None`), bestückt
  sie nichts — sie merkt sich nur, was gerade läuft. Das betrifft drei
  Fälle, die sich für sie gleich anfühlen: das **Einschalten** des Häkchens
  mitten im Turnier, das **Stoppen/Starten der Übertragung** (passiert bei
  jedem Speichern der Einstellungen) und den **App-Neustart**. Ohne diese
  Regel sah jedes belegte Feld wie frisch belegt aus, und alle laufenden
  Spiele bekamen auf einen Schlag Schiedsrichter zugeteilt (Feldtest
  22.08.2026). Laufende Spiele bleiben unbesetzt, bis die Turnierleitung von
  Hand zuweist; ab dem nächsten Feldwechsel läuft die Rotation normal.
- **BTP gewinnt:** Trägt das Match `Official1ID`/`Official2ID`, gilt dieser
  Wert (`store.effective(...)`); die Rotation füllt dort nichts nach.
- **Nach Spielende** rücken die Officials des beendeten Spiels ans Ende der
  Reihenfolge (`nach_spielende`); ihre Zuweisung bleibt am Match stehen
  (Einsatz-Ableitung). Seit v0.9.357 in der Folge **erst AR, dann SR**: Bleibt
  das Paar in der Liste zusammen, tauscht es beim nächsten Mal die Dienste
  (bis v0.9.356 `[SR, AR]` — dann bekamen dieselben Leute immer wieder
  dieselben Dienste). In Modus A rückt jeder nur in seiner eigenen Liste.
  <!-- pruef: "let folge: Vec<i64> = ar.into_iter().chain(sr).collect();" in src-tauri/src/tablet/officials.rs -->
  Werden mehrere Felder im selben Zyklus fertig, rücken ihre Officials **nach
  CourtID sortiert** ans Ende — dieselbe Deterministik wie bei der Zuteilung
  unten (bis v0.9.201 hing die Reihenfolge von der zufälligen
  HashMap-Iteration ab).
- **Mehrere Felder gleichzeitig:** nach CourtID sortiert, damit die
  Verteilung deterministisch ist; frisch Zugewiesene gelten sofort als im
  Dienst.
- **Globaler Schalter aus** ⇒ `clear_assignments()` (Spec Nr. 1).
- Niemand frei ⇒ das Feld bleibt ohne diesen Dienst, ohne Fehler.
- **Reservierte** überspringt die Rotation ebenfalls (seit v0.9.352, siehe
  nächster Abschnitt).

## Reservierung wartender Spiele (seit v0.9.352, ADR 0078)

Wer für ein **wartendes** Spiel als Schiedsrichter oder Aufschlagrichter
feststeht, ist für dieses Spiel **reserviert**. Die Rotation setzt ihn
nirgends sonst ein, bis sein Spiel beendet ist. Das gilt für Einteilungen von
Hand (Feld-Kachel, Turnier-PC) **und** für Schiedsrichter, die in BTP vorab
am Spiel eingetragen sind. Spec:
[tl-warteliste-schiedsrichter](features/tl-warteliste-schiedsrichter.md).

- **Welche Spiele reservieren?** Wartende Spiele, die **heute oder früher**
  angesetzt sind oder gar keine Ansetzung haben. „Heute" ist das Datum des
  Turnier-PCs. Ein Vorabeintrag für ein Spiel am nächsten Turniertag nimmt
  niemanden aus der Rotation.
  <!-- pruef: "t / 10_000 <= heute_schluessel" in src-tauri/src/tablet/officials.rs -->
- **Welche Besetzung zählt?** Die eigene Einteilung, sonst der BTP-Eintrag.
  Wer gerade umgeteilt wurde, ist sofort reserviert, nicht erst nach der
  Bestätigung durch BTP. „Ohne Schiedsrichter" reserviert niemanden, auch
  nicht den Schiedsrichter, den BTP vorher trug. Gezählt wird nur, wer in der
  Schiedsrichter-Liste von BTP steht.
- **Wann endet die Reservierung?** Kommt das Spiel aufs Feld, pfeift genau
  die eingeteilte Person; die Rotation ersetzt sie nicht. Ist das Spiel
  beendet (auch kampflos), ist sie wieder frei. Wird das Spiel vom Feld
  genommen, ist sie wieder reserviert.
- **Pause:** Eine pausierte Person bleibt eingeteilt.
- **Meetingpoint:** Die vorläufige Zuteilung beim Aufruf überspringt
  Reservierte ebenfalls (siehe unten).
- **Anzeige** (seit v0.9.354): In der Schiedsrichter-Liste am Turnier-PC
  trägt eine reservierte Person die Marke **„reserviert Sp. N"**, ohne
  Spielnummer nur „reserviert". Ist sie für mehrere Spiele eingeteilt, steht
  dort die kleinste Spielnummer. Dieselbe Marke steht seit v0.9.355 in der
  Schiedsrichter-Liste der TL-Web-Seite, und die Vorschau „nächster: …"
  überspringt Reservierte.
  <!-- pruef: "reserviert Sp. " in src/pages/OfficialsPanel.tsx -->
  <!-- pruef: "reserviert Sp. " in src-tauri/assets/tl.html -->
- **Einteilen in der TL-Web-Warteliste** (seit v0.9.355): ⋮-Menü der Zeile →
  **Schiedsrichter einteilen**, auch an offenen Paarungen. Der Dialog wählt
  die aktuelle Besetzung vor, kennzeichnet „(im Dienst)", „(reserviert Sp. N)"
  und „(Pause)" und sendet `official_assign`/`official_clear` mit
  `courtId: 0`. Wer als SR die Person wählt, die gerade AR ist (oder
  umgekehrt), tauscht beide: erst den anderen Dienst lösen, dann einteilen,
  dann die bisherige Person in den anderen Dienst — nie SR = AR, auch nicht
  kurz. Der Menüpunkt erscheint nur mit `can_assign_waiting_officials`
  (Host ab v0.9.354). Bedienung: [turnierleitung-web.md](turnierleitung-web.md#schiedsrichter-einteilen).
  <!-- pruef: "Schiedsrichter einteilen" in src-tauri/assets/tl.html -->
- **Warnungen an der Warteliste** (seit v0.9.354): Ist dieselbe Person an
  zwei oder mehr reservierenden wartenden Spielen eingeteilt, warnen alle
  diese Zeilen mit „SR doppelt" bzw. „AR doppelt". Pausiert eine eingeteilte
  Person, warnt die Zeile mit „SR in Pause" bzw. „AR in Pause". Dienst auf
  einem laufenden Feld ist keine Doppel-Einteilung. Mehrere Warnungen stehen
  in fester Reihenfolge: Konflikt (Verein/Person) · doppelt · Pause.
  Gewarnt wird nur über die Person, die die Marke zeigt. Seit v0.9.358 zeigt
  die Marke einer wartenden Zeile die frische Einteilung („Hand vor BTP"):
  Wer eben umgeteilt wurde, steht sofort an der Zeile, samt Warnung —
  nicht erst nach der Bestätigung durch BTP. Laufende Felder zeigen weiter
  den bestätigten Stand.
  <!-- pruef: "pub fn wartende_officials" in src-tauri/src/tablet/state.rs -->

### Einteilung wird geprüft (seit v0.9.354)

Der Turnier-PC prüft jede Einteilung und jedes Lösen, bevor er etwas
speichert, im TL-Web (LAN und Cloud) wie in der Desktop-App
(`officials::pruefe_einteilung`). Abgelehnt wird:

- ein Spiel, das der aktuelle Turnierstand nicht kennt,
- ein beendetes Spiel, auch beim Lösen,
- eine Person, die nicht in der Schiedsrichter-Liste aus BTP steht,
- dieselbe Person als Schiedsrichter **und** Aufschlagrichter desselben
  Spiels. Geprüft wird gegen die Besetzung, die nach BTP ginge (eigene
  Einteilung vor BTP-Wert).

Bei Ablehnung wird nichts gespeichert und nichts nach BTP geschrieben. Das
TL-Web bekommt `NotAllowed` mit dem Text, die Desktop-App zeigt ihn rot neben
dem Feld. Das Protokoll nennt Einteilungen mit `courtId 0` „Schiedsrichter für
Spiel N (Warteliste)" statt „(Feld 0)".

### Die automatische Feldvergabe wartet auf den Schiedsrichter

Steht der Schiedsrichter oder Aufschlagrichter eines wartenden Spiels noch auf
einem anderen Feld, legt die **automatische** Feldvergabe dieses Spiel nicht
aufs Feld. Sie nimmt das nächste Spiel und versucht es im nächsten Lauf
wieder. In einem Lauf vergibt sie nie zwei Felder mit derselben Person. Ein
Wunschfeld wird für das Spiel nicht freigehalten, solange sein Schiedsrichter
noch pfeift. Eine Pause blockiert nicht.

Ein Feld kann deshalb kurz leer stehen. Das ist gewollt: Kein Spiel soll ohne
seinen Schiedsrichter beginnen. Die Turnierleitung kann den Schiedsrichter
lösen oder umteilen, das Spiel von der Automatik ausnehmen oder es **von
Hand** aufs Feld legen — die manuelle Vergabe (Kachel, Turnier-PC, BTP) sperrt
nichts. Die Prognose kennt die Wartesperre nicht und ist für solche Spiele
etwas zu optimistisch.

> **Für bestehende Turniere:** Mit dem Update nehmen alle BTP-Vorabeinträge
> für heute ihre Schiedsrichter sofort aus der Rotation. Wer in BTP viele
> Schiedsrichter vorab einträgt, spielt das Update am besten vor
> Turnierbeginn ein.

Technik: Eine Quelle `officials.rs::Belegung` (`aus(store, snap, heute)`)
rechnet Dienst (laufende Spiele, Sicht `effective`) und Reservierung
(wartende Spiele, Sicht `fuer_schreiben` = Hand vor BTP). Verbraucher sind
`track_officials` (`belegt_ausser` je Feld), `auto_assign` (Wartesperre,
auch bei der Wunschfeld-Reservierung, vor dem strikten Meetingpoint-Gate),
`meetingpoint::gerufene_besetzen` (seit v0.9.359, ADR 0080). `officials_for_write` baut
auf `fuer_schreiben` auf.

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
| `officials_roster` | Liste in Rotationsreihenfolge: Name, Position, Pause, Verein, **Anzahl** Sperren, aktueller Dienst, Einsatz-Zähler, Reservierung (`reserved`, `reserved_match_num`, seit v0.9.354), Rolle (`role`, seit v0.9.362), Position in der AR-Liste und Vorschau (`position_ar`, `next_role`, `next_rank`, seit v0.9.364) |
| `official_assign` / `official_clear` | Zuweisung setzen/lösen; `assign` liefert die Konflikt-Kategorie zurück. Seit v0.9.354 vorher geprüft (`pruefe_einteilung`), Ablehnung als `Err(String)` |
| `official_pause`, `official_reorder`, `official_set_club` | Pause, Reihenfolge, Stammverein |
| `official_blocklists` / `official_set_blocklists` | Sperrlisten **gezielt** lesen/schreiben |
| `official_appearances` | Einsatz-Liste eines Officials (Spiel, Rolle, Feld, Endezeit) |
| `officials_court_switches` / `officials_set_court_switches` | feldweise Schalter |

**Sperrlisten reisen nie mit der Liste.** `officials_roster` trägt nur die
*Anzahl* der Einträge; die Inhalte kommen ausschließlich über
`official_blocklists` für genau einen Official — dasselbe Muster, das später
TL-Web über die authentifizierte Leseroute nutzt.

### Globale Schalter zur Laufzeit

`officials.enabled`, `rotation_sr`, `rotation_ar` und `rotation_mode` stehen
zwar in der `config.json`, werden aber zusätzlich im Roster-Speicher gespiegelt
(`set_enabled`/`set_rotation`/`set_rotation_mode`, gesetzt beim App-Start und beim Speichern der
Einstellungen). Grund: Der Sync-Lauf liest seine Konfiguration nur beim Start
— ohne diesen Spiegel bliebe ein frisch gesetztes Häkchen bis zum nächsten
Stoppen/Starten der Übertragung wirkungslos.

## TL-Web (Schritt 8)

Wire-Ebene und Bedienung sind in [cloud-relay.md](cloud-relay.md) bzw.
[turnierleitung-web.md](turnierleitung-web.md) beschrieben. Der Kern:

- **Broadcast-`TlState`** trägt `officials_managed`, die reduzierte Liste
  `officials` (`id`, `name`, `paused`, `on_duty_court_id`, `appearances`,
  seit v0.9.354 `reserved`/`reserved_match_num`) und je Feld
  `sr`/`ar`/`official_warn` plus die drei Feld-Schalter.
- **Einteilung an der Warteliste** (seit v0.9.354): Flag
  `can_assign_waiting_officials`, `TlMatch.sr_id`/`ar_id`, an offenen
  Paarungen `TlOpenMatch.sr`/`ar`/`sr_id`/`ar_id`/`official_warn`. Die
  Warnung an Warteliste und offener Liste ergänzt `wartelisten_warnung` in
  `build_state` um „doppelt" und „in Pause" — nicht `court_officials`, das
  auch Kachel und Tablet speist. Die Belegung wird einmal je Zustandsbau
  gerechnet.
- **Warteliste** (seit v0.9.348): jeder `TlMatch` trägt dieselben
  `sr`/`ar`/`official_warn` aus `court_officials` (wirksame Besetzung,
  BTP oder Hand). Leer bleiben sie auf dem Draht weg, damit das
  64-KiB-Fenster der Cloud nicht wächst. Die vorläufige Meetingpoint-
  Zuteilung (ADR 0072) zählt nicht dazu — sie zeigt die Meetingpoint-Marke.
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
   **Wartet, solange eine Feldzuweisung unterwegs ist** (seit v0.9.349):
   Hat die Auto-Vergabe oder das TL-Web ein Spiel gerade aufs Feld gelegt
   und BTP das noch nicht bestätigt (`reserved_courts`), zeigt der Snapshot
   kein Feld. Ein eigenständiger Write mit `CourtID 0` und leerem
   `Courts`-Block hätte die frische Feldzuweisung in BTP gelöscht — im
   selben Sync-Lauf direkt nach der Auto-Vergabe, z. B. bei der Übernahme
   des Meetingpoint-Vorab-SR. Die Besetzung reist ohnehin im
   Zuweisungs-Write mit (Weg 2); der eigenständige Abgleich setzt erst nach
   der Bestätigung (oder dem Ablauf des Anspruchs) wieder ein. Die
   Feldvergabe aus der Desktop-App vermerkt dafür seit v0.9.349 ebenfalls
   einen Anspruch (nach dem erfolgreichen Write, damit sie nie an einer
   TL-Web-Vormerkung scheitert).
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
- **„Ohne Schiedsrichter" bleibt stehen** (seit v0.9.358, ADR 0080): Ein
  ausdrückliches „keiner" (`Some(0)`) wird **nicht** losgelassen, wenn BTP
  ebenfalls niemanden führt — auch nicht, wenn die 0 unser eigener Write war,
  nachdem BTP vorher jemanden trug. Bis v0.9.357 fiel es dann weg, und ein
  Lauf über wartende Spiele (der vorläufige Meetingpoint-SR, künftig die
  Zuteilung beim Aufruf) besetzte das Spiel im nächsten Takt wieder. Der
  Ausgangswert steht danach auf 0: Trägt jemand in BTP eine Person ein,
  gewinnt BTP weiterhin. Solche Einträge bleiben in `officials-state.json` bis zum
  Turnierwechsel stehen, auch an beendeten Spielen — höchstens einer je
  Spiel, ohne zusätzlichen Schreibzugriff je Takt.
- **Loslassen nach Bestätigung** (`OfficialsStore::confirm`, im Sync-Loop vor
  dem Diff): Ein lokaler Dienst wird entfernt, sobald der Snapshot **genau
  den lokalen Wert** zeigt (außer beim ausdrücklichen „keiner", siehe oben) — oder
  sobald BTP etwas **anderes führt als beim Einteilen** (dann hat jemand in
  BTP umgeteilt, und BTP gewinnt, R2). Führt BTP noch den alten Wert, bleibt
  die Absicht stehen und geht nach BTP: So ersetzt eine Umteilung von Y auf
  X den Y in BTP (Spec `tl-warteliste-schiedsrichter`, AK 12; bis v0.9.349
  ließ `confirm` X fallen, sobald BTP *irgendeinen* Wert trug, und X wurde
  nie geschrieben). **Ohne das Loslassen** würde eine spätere Änderung *in
  BTP* bei jedem Sync-Zyklus wieder überschrieben, für den Rest des
  Turniers. Die Einsatz-Ableitung verliert dabei nichts: Sie liest denselben
  Wert dann aus dem BTP-Match.
- **Ausgangswert in der Datei, nicht im Sync-Lauf.** Den BTP-Stand beim
  Einteilen merkt sich `officials-state.json` (Feld `ausgang`, je Match und
  Dienst, `serde(default)`); erfasst wird er beim ersten Abgleich nach der
  Einteilung, eine neue Einteilung erfasst ihn neu. Der Sync-Merker
  `officials_written` taugt dafür nicht: Er wird bei jedem Speichern der
  Einstellungen geleert, und BTP übernimmt asynchron — ein Abgleich, der BTP
  noch beim alten Wert sieht, hielte das sonst für eine Umteilung in BTP und
  verwürfe die eigene Einteilung. Ältere Dateien ohne `ausgang` verhalten
  sich wie bisher. Teilt die Turnierleitung erneut um, während der erste
  Write noch unterwegs ist, bleibt der Ausgangswert stehen und die frühere
  Einteilung wird als eigene gemerkt (`sr_vorher`/`ar_vorher`): Ihre
  verspätete Übernahme in BTP gilt nicht als Umteilung dort. Grenze: Teilt
  die Turnierleitung im selben Sync-Takt um, in dem jemand in BTP ändert,
  gilt die Einteilung.
- **Merker `officials_written` nur bis zur Übernahme.** Zeigt BTP den
  geschriebenen Stand, fällt der Eintrag heraus. Sonst unterdrückte er
  einen späteren gleichen Write, nachdem jemand in BTP umgeteilt hat.

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
**Und er entscheidet über den Autodruck:** Ist er eingeschaltet, druckt
bts-light den Zettel nur für Spiele, denen ein **Schiedsrichter** zugeordnet
ist — ein Aufschlagrichter allein genügt nicht. Ist die Schiedsrichter-
verwaltung ganz aus, kennt der Host nie einen SR, und die Automatik bleibt
bauartbedingt stumm. Die Zuordnung darf dabei **nach** der Feldvergabe kommen
(Rotation oder von Hand): Geprüft wird ein Zustand, nicht ein Augenblick.

## Zuteilung schon beim Aufruf (seit v0.9.359, ADR 0080)

Wird ein Spiel **in Vorbereitung gerufen** (Turnier-PC, TL-Web oder
Aufruf-Automatik), teilt die Rotation Schiedsrichter und Aufschlagrichter
**fest** zu — auch ohne Meetingpoint, sobald die Schiedsrichter-Verwaltung und
die jeweilige Rotation an sind. Die beiden stehen sofort an der gerufenen
Zeile und können sich vor dem Feldaufruf treffen. Es wird nach denselben
Regeln gewählt wie am Feld (Einteilungsprinzip, Rolle, Pausen, Konflikte,
Reservierungen).
<!-- pruef: "pub fn gerufene_besetzen" in src-tauri/src/tablet/meetingpoint.rs -->

- **Fest:** Die Zuteilung ist eine echte Einteilung, geht sofort nach BTP und
  reserviert die Person (auch, wenn das Spiel an einem späteren Tag angesetzt
  ist). Eine Einteilung von Hand oder in BTP wird nie ersetzt;
  **Ohne Schiedsrichter** bleibt stehen.
- **Engpass:** Ist niemand frei, wird das Spiel mit Lücke gerufen. Wer frei
  wird, geht zuerst an ein Feld, das gerade ein neues Spiel bekommen hat, dann
  an die gerufenen Spiele in ihrer Vergabe-Reihenfolge. Das Nachrücken wird
  nicht angesagt.
- **Halle:** Zugeteilt wird ein Dienst nur, wenn mindestens ein Feld der
  Aufruf-Halle ihn will (Feldschalter). Kommt das Spiel auf ein Feld ohne
  diesen Dienst, wird die Rotations-Zuteilung dort wieder gelöst.
- **Rotation abschalten:** Bestehende Zuteilungen bleiben stehen, es kommt nur
  nichts Neues hinzu.
- **Zurücknehmen:** Wird ein Aufruf zurückgenommen oder gelöscht, werden nur
  die Zuteilungen der Rotation frei; eine Einteilung von Hand bleibt stehen.
  Wird ein Spiel vom Feld genommen und der Aufruf lebt wieder auf, bleibt die
  Zuteilung.
- **Nur am Turnier-PC:** Ein Aufruf an einem Slave-Rechner teilt nichts zu.

Technik: `meetingpoint::gerufene_besetzen` läuft synchron im Aufruf
(`hand_aufruf`, vor dem Ansage-Auftrag) und in jedem Sync-Takt nach
`track_officials` (Feld hat Vorrang), beides unter
`OfficialsStore::vergabe_sperre`. Gewählt wird über
`rotate_court(…, Herkunft::Aufruf)` → `waehle_besetzung`. Der
Herkunfts-Merker `vorab_rotation` in `officials-state.json` (Match → je Dienst
`{id, frei}`) unterscheidet Rotation von Hand/BTP; er gilt nur, solange die
Sicht „Hand vor BTP" seine Person trägt (`pflege_vorab_rotation`). Die
Freigabe (`rotation_freigeben`) hinterlässt kein bleibendes `Some(0)`: ohne
BTP-Gegenstück lokal weg, mit Gegenstück `Some(0)` + `frei` (gilt für die
Rotation als offen, `confirm` räumt nach der Bestätigung der 0). Lösen am Feld
ohne Dienst: `rotation_loesen_am_feld` vor jedem Feld-Write (Auto-Vergabe,
TL-Web, Turnier-PC) und zustandsbasiert für laufende Spiele. Endet ein Spiel
ohne Feld, verschwindet eine nie nach BTP gelangte Zuteilung (kein Einsatz ohne
Dienst).

**Ablösung des vorläufigen SR (ADR 0072):** `vorab_sr_verteilen`,
`uebernimm_vorab_sr` und der Merker `vorab_sr` am Aufruf entfallen. Noch
gespeicherte `vorab_sr` übernimmt der erste Lauf einmalig als feste Zuteilung
(`nimm_vorab_sr`), wenn die Person frei ist und die Rolle passt; sonst
verfallen sie. Ein SR-Check-in am Meetingpoint verfällt, wenn der SR der
Partie wechselt.

**BTP:** Dass der Officials-Write an Spielen ohne Feld eine frische
Feldzuweisung nicht löscht, ist eine Annahme; sie wird vor dem Release mit
`tests/btp_officials_ohne_feld_probe.rs` gemessen (siehe
[btp_protocol.md](btp_protocol.md)).
