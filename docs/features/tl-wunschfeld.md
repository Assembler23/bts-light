# Wunschfeld für die Automatikvergabe — Spezifikation

> Status: **abgestimmt 2026-08-24** (via /idee: Brief → Grill → Spec).
> Quelle: Nutzer-Anforderung vom 23.08.2026. Betroffene Crates: `src-tauri`,
> `relay-proto`.
> ADR: [0046](../adr/0046-wunschfeld-reserviert-ab-spielbereitschaft.md);
> Nachtrag zu [0030](../adr/0030-halle-bindet-die-feldvergabe.md).

## Kontext / Problem

> In der Turnierleitungssicht (web) soll es für ein Spiel in der
> Automatikvergabe eine Feldauswahl geben können, die er nehmen kann. z.B.
> will man Finalspiele so besser steuern.

Das Endspiel gehört auf das Hauptfeld — dorthin, wo Kamera, Tribüne und
Siegerehrung sind —, nicht auf irgendein Feld, das zufällig zuerst frei wird.
Bisher blieben zwei Wege, beide unbefriedigend: das Spiel von der Automatik
ausnehmen und später von Hand legen (jemand muss daran denken und den Moment
abpassen), oder die Automatik ganz abschalten (trifft alle anderen mit).

## Zielbild & Erfolgskriterien

Die Turnierleitung wählt in der Spielliste für ein Spiel ein Feld. Die
Automatik legt dieses Spiel dann **nur** dorthin und hält das Feld frei,
sobald das Spiel spielbereit ist. Alles andere läuft unverändert weiter.

1. Ein Wunschfeld ist in **zwei Tipps** gesetzt (⋯ → Feld wählen).
2. Das Wunschspiel bekommt sein Feld — und **kein anderes**, auch wenn eines
   früher frei wird.
3. Solange das Wunschspiel spielbereit ist, bekommt **kein anderes Spiel**
   dieses Feld.
4. Andere Spiele werden unterdessen normal weiterverteilt.
5. In der Liste ist sichtbar, **worauf** ein wartendes Spiel wartet.

## Verhalten

### Die Reservierung — und wann sie beginnt

Das Wunschfeld wird für sein Spiel freigehalten, **sobald das Spiel
spielbereit ist**: nicht ausgenommen, kein Spieler steht auf einem Feld oder
in der Pflichtpause. Nicht früher.

**Warum die Reservierung überhaupt sein muss:** Ein bloßer Filter („dieses
Spiel bekommt nur dieses Feld") hält das Feld nicht frei. Wird das Hauptfeld
frei, während das Endspiel noch in der Pflichtpause nach dem Halbfinale steckt
— die Lage, in der ein Endspiel praktisch immer ist —, nimmt ein anderes Spiel
das Feld, und es ist für vierzig Minuten weg. Das Feature verfehlte still
seinen einzigen Zweck.

**Warum erst ab Spielbereitschaft:** Eine Reservierung kostet Feldkapazität.
Ein Endspiel, dessen Spieler noch im Halbfinale stehen, würde das Hauptfeld
sonst stundenlang leer halten. So zahlt man den Preis nur, solange er
tatsächlich nötig ist (Nutzer-Entscheidung 24.08.2026,
[ADR 0046](../adr/0046-wunschfeld-reserviert-ab-spielbereitschaft.md)).

### Die Halle folgt dem Feld

Ein Wunschfeld legt die Halle des Spiels mit fest — es benennt ja ein
konkretes Feld, und das liegt in genau einer Halle. Damit kann der Widerspruch
„Wunschfeld in Halle B, Hallenzuordnung sagt Halle A" gar nicht erst
entstehen; sonst wartete das Spiel auf ein Feld, das seine eigene
Hallenbindung ihm verbietet (ADR 0030), und niemand sähe den Grund.

Technisch zählt der Wunsch als **`Manual`** und bekommt keine eigene
Kaskadenstufe: Er *ist* ein Hand-Eingriff für dieses eine Spiel. Damit greifen
Hallenbindung und Aufruf-Ersatz (seit v0.9.260) ohne Zusatzarbeit, und die
Wire bekommt keine neue Quelle, deren Bedeutung ältere Anzeigen nicht kennen.

### Was beim Setzen abgelehnt wird

| Fall | Grund |
|---|---|
| Unbekanntes Spiel | Ließe sich nicht mehr zurücknehmen. |
| **Feld gibt es nicht** | Das Spiel wartete für immer, in der Liste stünde nur „wartet". |
| **Feld ist gesperrt** | Die Automatik vergibt dorthin ohnehin nichts. Und man braucht die Sperre für diesen Zweck nicht mehr — der Wunsch hält das Feld selbst frei. Die Meldung sagt das. |
| **Halle nach Regel verboten** | Das Spiel bekäme dort nie ein Feld (`hall_allows_match`). Lieber jetzt ablehnen als später stumm warten lassen. |
| Kein Turnier geladen | Wie alle turnierbezogenen Aktionen. |

### Lebensdauer

Der Wunsch wird verworfen, sobald das Spiel **auf dem Feld steht**, beendet
ist oder aus dem Turnierstand verschwindet — und wenn es **sein Feld nicht
mehr gibt**. Bliebe er nach der Vergabe stehen, zöge er das Spiel nach einer
Ergebniskorrektur erneut auf ein Feld, das niemand mehr gemeint hat, und
hielte dieses Feld weiter besetzt.

Turniergebunden nach ADR-0022-Muster (`wish-courts.json`): Match- **und**
CourtIDs gelten nur innerhalb eines Turniers.

### Von Hand bleibt alles möglich

Der Wunsch steuert nur die Automatik. Legt jemand das Spiel von Hand auf ein
anderes Feld, kommt eine Rückfrage — zusammen mit den anderen Bedenken (andere
Halle, laufende Pflichtpause) in **einer** Meldung.

### Prognose

Spiele mit Wunschfeld bekommen **keine** Startzeit-Prognose. Die Simulation
kennt nur Hallen, keine Felder; sie gäbe dem Spiel die Startzeit des ersten
freien Felds — systematisch zu früh — und belegte in der Rechnung ein Feld,
das es real nicht nimmt, womit sich alle nachfolgenden Startzeiten
verschöben. Dieselbe Behandlung wie bei ausgenommenen Spielen.

## Nicht-Ziele

- **Mehrere** erlaubte Felder je Spiel („eines der beiden Center Courts").
- Ein Wunschfeld für eine ganze **Klasse oder Runde**.
- **Zeitsteuerung** („ab 18 Uhr auf Feld 1").
- Das Wunschfeld auf **Monitoren** oder im Liveticker.
- Automatisch ein Feld **freiräumen**, damit der Wunsch erfüllt werden kann.
- Anzeige im **Desktop**-Panel.

## Betroffene Komponenten / Architekturregeln

- `src-tauri/src/tablet/wish_court.rs` (Speicher) · `sync.rs` (Reservierung,
  Filter, Aufräumen) · `assign.rs` (`manual_hall_from_wish`) · `tl.rs`
  (Aktion, `TlMatch::wish_court`, `TlState::can_set_wish_court`, Prognose) ·
  `relay-proto` (`TlAction::SetWishCourt`) · `assets/tl.html`.
- **`relay/` braucht keine Code-Änderung** — typisiertes `TlAction` über die
  geteilte Crate, gemeinsam mit `tl.html` deployt.
- **R1/R2** — bts-light-eigen, BTP sieht den Wunsch nie. `touches_courts`
  bleibt unverändert.
- **R3** — LAN und Cloud über `tl::execute`.
- **Datenschutz** — eine CourtID, kein Personendatum; im Wächter eingetragen.
- Keine neuen Abhängigkeiten.

## Akzeptanzkriterien

- [ ] **E1** Ein Wunschspiel bekommt sein Feld, nicht das erste freie.
- [ ] **E2** Ein Wunschspiel bekommt **kein** anderes Feld, auch wenn eines
      frei ist — es wartet.
- [ ] **E3** Solange das Wunschspiel spielbereit ist, bekommt kein anderes
      Spiel dieses Feld.
- [ ] **E4** Ist das Wunschspiel **nicht** spielbereit (Pflichtpause), darf
      ein anderes Spiel auf das Feld.
- [ ] **E5** Andere Spiele werden normal weiterverteilt — der Wunsch
      blockiert die Schlange nicht.
- [ ] **E6** Setzen auf ein unbekanntes, gesperrtes oder nach Hallen-Regel
      verbotenes Feld wird mit verständlicher Meldung abgelehnt.
- [ ] **E7** Der Wunsch verschwindet, sobald das Spiel auf dem Feld steht.
- [ ] **E8** Ein Wunsch auf ein verschwundenes Feld wird selbsttätig
      verworfen.
- [ ] **E9** Ein Turnierwechsel leert die Wünsche.
- [ ] **E10** Die Liste zeigt das Wunschfeld und kennzeichnet, wenn es gerade
      belegt ist.
- [ ] **E11** Manuelles Zuweisen auf ein anderes Feld fragt nach.
- [ ] **E12** Ein Host ohne dieses Feature zeigt keinen Wähler.

## Tests

| Test | Ort | Sichert |
|---|---|---|
| Wunschspiel bekommt sein Feld, anderes läuft weiter | `sync.rs` | E1, E5 |
| Wunschfeld bleibt für sein Spiel frei | `sync.rs` | E3 |
| nicht spielbereit ⇒ Feld darf an andere | `sync.rs` | E4 |
| wartet lieber, als auf ein anderes Feld zu gehen | `sync.rs` | E2 |
| Speicher: setzen, Neustart, Turnierwechsel, Aufräumen, verschwundenes Feld | `wish_court.rs` | E7–E9 |
| Wire-Roundtrip + Vollständigkeits-Wächter | `relay-proto` | — |
| Oberfläche: Wähler und Rückfrage (10 Fälle) | Node-Prüfung | E10–E12 |

## Risiken & Rollback

- **Feldkapazität:** Die Reservierung hält ein Feld leer. Durch die Bindung an
  die Spielbereitschaft ist das Fenster klein; im Zweifel hebt man den Wunsch
  auf (zwei Tipps).
- **Ein vergessener Wunsch** ist weniger sichtbar als eine Feldsperre — dagegen
  steht die Marke in der Liste und das selbsttätige Verwerfen beim Vergeben.
- **Rollback:** Ältere Versionen ignorieren `wish-courts.json` und die neuen
  Felder; die Vergabe verhält sich dann wie vorher.

## Doku-Pflicht

`docs/turnierleitung-web.md` (Bedienung) · `docs/btp_protocol.md`
(Vergabe/Kaskade) · `docs/cloud-relay.md` (Wire) · `docs/changelog.md` ·
CLAUDE.md-Tabelle · Versions-Trippel.
