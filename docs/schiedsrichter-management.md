# Schiedsrichtermanagement

BTS Light übernimmt die Schiedsrichterliste aus BTP (`Officials`-Container),
zeigt sie in Client und TL-Web, erlaubt SR/AR-Zuweisungen je Spiel (mit
Konflikt-Warnung und automatischer Rotation) und schreibt Zuweisungen nach
BTP zurück. Spec: [features/schiedsrichter-management.md](features/schiedsrichter-management.md) ·
ADRs: [0021 (Rücksync)](adr/0021-officials-ruecksync-eigenstaendiger-write.md),
[0022 (Ablage Turnierdaten)](adr/0022-officials-turnierdaten-eigene-datei.md) ·
BTP-Draht: [btp_protocol.md](btp_protocol.md) („Officials: Struktur & Schreibweg").

> **Stand: im Aufbau.** Umgesetzt sind die Schritte 1–6 des Spec-Plans
> (BTP-Messung, Parser, Konfiguration, Roster-Speicher, Rotation +
> Konflikt-Erkennung, feldweise Schalter). Bedienung, TL-Web,
> Tablet-Anzeige, Ansagen und Rücksync folgen mit den nächsten Schritten;
> dieser Text wächst mit.

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
(siehe [zaehltafelbediener.md](zaehltafelbediener.md)). Er gehört zur
Feldkonfiguration und gilt unabhängig von `officials.enabled` — sonst
änderte das Ein-/Ausschalten des Schiedsrichter-Betriebs still die
Bediener-Vergabe.

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
