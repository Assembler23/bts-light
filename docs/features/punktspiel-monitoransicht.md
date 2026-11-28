# Monitoransicht „Begegnung“ für Punktspiele — Spezifikation

> Status: **abgestimmt 2026-11-28, freigegeben von Tilo** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Idee von Tilo vom 28.11.2026. Betroffene Crates: src-tauri, relay, relay-proto, src; dazu badhub.
> ADR: [0109](../adr/0109-begegnungs-sicht-ueber-den-relay-und-logo-per-vereins-id.md).

## Kontext / Problem

Bei einem Punktspiel (Quelle `punktspiel`, [punktspiel.md](punktspiel.md)) wollen Zuschauer und Mannschaften
wissen, wie die Begegnung steht. Die vorhandenen Monitoransichten sind für Turniere gebaut: je Feld, Feld-Übersicht
(nur laufende Spiele), Vorbereitung, Spielfolge. Keine zeigt beendete, laufende und offene Spiele zusammen, keine
einen Gesamtstand.

badhub zeigt die Begegnung öffentlich im Netz. Für den Hallen-TV reicht das nicht: Die Seite braucht Internet am
TV, ist kein TV-Layout, und ihre Live-Stände hängen am Liveticker, der ohne Windows-PC aus ist.

## Zielbild & Erfolgskriterien

Ein TV zeigt die ganze Begegnung auf einem Bildschirm, ohne Blättern und ohne Laufband.

**Kopf** (etwa ein Viertel der Höhe): Vereinslogo Heim, Mannschaftsname Heim, Gesamtstand in Spielen (z. B.
`5 : 3`), Mannschaftsname Gast, Vereinslogo Gast. Die Logos füllen die Kopfhöhe. Unter dem Gesamtstand eine
kleinere Nebenzeile mit Sätzen und Punkten (`11 : 7 Sätze · 352 : 318 Punkte`). Über oder unter den Namen die
Staffel. Hat eine Mannschaft kein Logo, steht an derselben Stelle eine gleich große Kachel mit den Initialen des
Mannschaftsnamens.

**Spiele**: alle Spiele der Begegnung in der Reihenfolge der Spielnummer, Einzel und Doppel. Die Zeilen teilen
sich die restliche Höhe gleichmäßig; die Schrift richtet sich nach der Zeilenhöhe. Je Zeile:

- Disziplin („1.HD“, „DE“)
- Spieler Heim und Gast (im Doppel zwei Namen je Seite)
- Sätze
- Zustand:

| Zustand | Anzeige |
|---|---|
| offen | Namen, keine Sätze. Ein nicht besetzter Platz heißt „noch nicht aufgestellt“; ein schon aufgestellter Doppelpartner bleibt sichtbar |
| läuft | Marke „Feld n“, Sätze mit dem Live-Stand vom Tablet, Zeile hervorgehoben |
| wird übermittelt | Endstand vom Tablet mit dem Hinweis „Ergebnis wird übermittelt“. Zählt noch nicht zum Gesamtstand |
| beendet | Sätze, Sieger hervorgehoben. Bei besonderem Ausgang das Wort „kampflos“, „Aufgabe“ oder „Disqualifikation“ |

**Gesamtstand** (Regel wörtlich wie badhub `Oeffentlich.php`): Ein Spiel zählt für die Seite des Siegers, sobald
badhub es als beendet mit Sieger führt, unabhängig vom Ausgang. Die Nebenzeile summiert Sätze und Punkte aus den
gemeldeten Sätzen genau dieser Spiele. Laufende und „wird übermittelt“-Spiele zählen nirgends mit.

Vor dem ersten Spiel steht `0 : 0` mit offenen Zeilen. Sind alle Spiele beendet, steht neben dem Gesamtstand
„Endstand“; die Ansicht bleibt stehen, solange die Übertragung läuft.

**Erfolgskriterien**

- Acht Spiele mit Doppelnamen sind auf einem 1080p-TV im Querformat ohne Überlauf lesbar; zwölf Spiele passen
  ebenfalls ohne Überlauf.
- Ein Punkt am Tablet steht im LAN nach höchstens 2 s, über die Cloud nach höchstens 3 s in der Zeile.
- Der Gesamtstand stimmt zu jedem Zeitpunkt mit der Begegnungsseite in badhub überein.
- Am Turnier-PC wählt man die Ansicht im Monitor-Panel wie jede andere; ohne Windows-PC öffnet man den Link aus
  Mein Badhub. Mehr ist nicht einzurichten.

## Nicht-Ziele

- Hochformat. Maßstab ist ein TV im Querformat 16:9.
- Mehr als zwölf sichtbare Spiele. Ab dem 13. Spiel wird die Liste abgeschnitten (die Sicht trägt höchstens 16).
- Logos je Spielzeile, Aufschlaganzeige, Sponsor-Leiste, Turnierlogo, Ansage-Modus-Großanzeige.
- Die Ansicht bei BTP- oder badhub-Turnieren.
- Slave-Brücke: Die Quelle Punktspiel startet im Slave-Modus nicht.
- Bearbeiten der Aufstellung oder Feldvergabe auf dieser Seite.
- Den Stand über einen Neustart ohne Internet retten (das Paket wird weiterhin nicht gespeichert).
- Monitor-Zuweisung im Betrieb ohne Windows-PC.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - `relay-proto`: neu `BegegnungSicht`, `BegegnungZeile`, `begegnung_anzeige`, `HostFrame::Begegnung`,
    `MonitorTarget::InfoBegegnung`, Deckel-Konstanten; die Satzregeln `set_is_complete` und
    `spiel_ist_entschieden` ziehen aus `tablet/server.rs` hierher um.
  - `relay`: Namespace-Feld für die Sicht, Frame-Arm mit Deckel und Nudge, Routen `/{ns}/info/begegnung` und
    `/{ns}/info/begegnung/state`, `club_logo_ziel` mit `?id=`, Ziel in der Umleitungs-Positivliste, Räumen in
    `leeren`.
  - `src-tauri`: `quelle/punktspiel.rs` (`begegnung_sicht`, Vereins-IDs in `PaketBegegnung`), `tablet/assets.rs`
    (Seite im `seiten!`-Makro), `tablet/server.rs` (Routen, Stand mit ETag, Logo-Query mit `id`),
    `tablet/club_logos.rs` (Abruf per ID, eigener Cache-Schlüssel), `tablet/relay_client.rs` (`push_begegnung`),
    neue Seite `assets/begegnung.html`.
  - `src`: `pages/CourtMonitorPanel.tsx` und `types.ts` (Option „Begegnung (Punktspiel)“), reine Seitenlogik in
    `src/io/begegnungAnsicht.mjs`.
  - badhub: `src/LiveScoring/Paket.php` (zwei Felder), `src/LiveScoring/OhnePc.php` `links()` und die Ansicht
    `mein/live_scoring_begegnung` (Link).
- **Architekturregeln:** R1 gewahrt (das Panel nutzt den vorhandenen Command `assign_monitor`). R2 sinngemäß:
  badhub ist bei Punktspielen die Wahrheit, deshalb zählt ein unbestätigtes Ergebnis nicht. R3: LAN und Cloud,
  auch `LanAndCloud`. R4, R5, R6 unberührt; die Seite schreibt nichts.
- **Konfiguration & Abwärtskompatibilität:** kein neues Feld in `config.rs`. Die Zuweisungsdatei bekommt eine neue
  Ziel-Variante `{"kind":"info_begegnung"}`; ältere Versionen überlesen den Eintrag (`monitor.rs`
  `read_assignments`). Der Datenformat-Abdruck (`release/datenformat.json`) wird geprüft und bei Abweichung mit
  Begründung nachgetragen. Die zwei Paketfelder sind optional; ein Paket ohne sie bleibt lesbar (Schema bleibt 1).
- **Datenschutz:** Die Sicht ist eine Positivliste: Mannschaftsnamen, Staffel, Vereins-ID, je Spiel Nummer,
  Disziplin, Namen, Feld, Status, Sätze, Sieger, Ausgang. **Keine** Lizenznummer, keine Personen-ID, kein
  Geburtsjahr, kein Verein und keine Nation je Person. Anonymisierte Namen (`***`) reisen unverändert durch. Neu
  ist, dass die Aufstellung schon vor dem Spiel und bei kampflosen Spielen auf einem Hallen-TV steht; das sind
  dieselben Namen wie auf dem Spielbericht und in der öffentlichen badhub-Anzeige. Ein Absatz dazu kommt in den
  Datenschutz-Teil von `CLAUDE.md`. Die Vereins-ID ist ein öffentlicher badhub-Schlüssel ohne Personenbezug.
- **Abhängigkeiten:** badhub `/api/ls/v1/begegnung` (zwei neue Felder), badhub `/api/v1/club-logo?id=` (vorhanden),
  Relay-Deploy vor dem Client. Keine neue Cargo- oder npm-Abhängigkeit.

### Wire

**`BegegnungSicht`**: `heim`, `gast`, `staffel` (je höchstens 80 Zeichen), `heim_verein_id`, `gast_verein_id`
(`Option<u32>`), Zählweise (`best_of`, `ziel`, `kappe`), `spiele` (höchstens 16 `BegegnungZeile`).

**`BegegnungZeile`**: `nr`, `disziplin` (höchstens 12 Zeichen), `doppel`, `heim` und `gast` (je höchstens zwei
Namen zu 60 Zeichen), `feld`, `status` (`offen`/`laeuft`/`beendet`), `saetze` (höchstens 5), `sieger`
(`heim`/`gast`), `ausgang`.

**Frame**: `HostFrame::Begegnung { sicht: Option<BegegnungSicht> }`. Der Relay kürzt Texte und kappt Listen; ein
Frame über 16 KiB wird zu `None` mit Log-Zeile. `None` heißt „kein Punktspiel aktiv“.

**Antwort** der Stand-Route (LAN und Relay gleich): `aktiv`, die Sicht, je Zeile zusätzlich `zustand`
(`offen`/`laeuft`/`wird_uebermittelt`/`beendet`), `stand` je Seite (`spiele`, `saetze`, `punkte`), `endstand`.
ETag über den Inhalt, 304 bei unverändertem Stand.

**Mischen**: `begegnung_anzeige(sicht, live)` ersetzt bei Zeilen mit Status `laeuft` die Sätze durch den
Live-Stand. Ist dieser nach der Zählweise entschieden, wird die Zeile `wird_uebermittelt`. Der Host liefert den
Live-Stand aus `laufende_saetze` (Match-ID = Spielnummer). Der Relay nimmt den Stand des Feldes, dessen laufendes
Spiel diese Match-ID trägt; ein fremdes Spiel auf dem Feld wird ignoriert.

## Akzeptanzkriterien

Sicht und Zählregel

- [ ] Aus einem Paket mit acht Spielen entsteht eine Sicht mit acht Zeilen in der Reihenfolge der Spielnummer.
- [ ] Ein Doppel mit nur einem aufgestellten Spieler trägt diesen einen Namen und `doppel = true`; die Seite zeigt
  für den zweiten Platz „noch nicht aufgestellt“.
- [ ] Die serialisierte Sicht und der serialisierte Frame enthalten weder `lizenz` noch eine Personen-`id`.
- [ ] Beendet + Sieger zählt für den Sieger, auch bei `kampflos`, `aufgabe`, `disqualifikation`. Beendet ohne
  Sieger zählt für niemanden.
- [ ] Die Nebenzeile summiert Sätze und Punkte nur aus beendeten Spielen mit Sieger, so wie die Sätze gemeldet
  sind.
- [ ] Ein laufendes Spiel zeigt den Live-Stand statt der Paket-Sätze und zählt nicht.
- [ ] Ein laufendes Spiel, dessen Live-Stand nach der Zählweise entschieden ist, hat den Zustand
  `wird_uebermittelt` und zählt nicht; nach der Bestätigung durch badhub ist es `beendet` und zählt.
- [ ] Sind alle Spiele beendet, ist `endstand = true`.
- [ ] Bei 16 Spielen in der Sicht zeigt die Seite zwölf Zeilen ohne Überlauf.

LAN

- [ ] `GET /info/begegnung` liefert die Seite, `GET /info/begegnung/state` den Stand mit ETag; derselbe ETag
  ergibt 304.
- [ ] Ist die Quelle nicht Punktspiel oder noch kein Paket gelesen, liefert die Stand-Route `aktiv: false`, und
  die Seite zeigt „Diese Ansicht gibt es nur bei Punktspielen.“
- [ ] `GET /info/club-logo?id=42` liefert das Logo des Vereins 42; `?id=abc`, `?id=-1` und ein fehlender Treffer
  ergeben 404. `?name=` verhält sich wie bisher.
- [ ] Schlägt das Logo fehl, zeigt die Seite die Initialen-Kachel in derselben Größe.
- [ ] Antwortet der Server nicht innerhalb der Abruf-Frist, blendet die Seite wie die anderen Anzeigen ab und
  holt den Stand von selbst wieder.
- [ ] Fällt nur die Verbindung zu badhub aus, bleibt der letzte Stand stehen und die Live-Stände laufen weiter.
- [ ] Im Monitor-Panel steht „Begegnung (Punktspiel)“ nur bei der Quelle Punktspiel zur Wahl. Eine bestehende
  Zuweisung bleibt bei anderer Quelle sichtbar und wird nicht gelöscht.
- [ ] `?rotate` dreht die Seite; `?ansage` wird beim Weiterleiten wie auf den anderen Geräteseiten mitgegeben.

Cloud

- [ ] Der Relay nimmt den Frame an, deckelt ihn und stößt die Monitore an. Ein Frame über 16 KiB wird verworfen
  (`None`) und geloggt.
- [ ] `GET /{ns}/info/begegnung/state` liefert dieselbe Form wie im LAN, mit dem Live-Stand aus den Feldern des
  Relays.
- [ ] Ohne Host innerhalb der Karenz antwortet die Stand-Route mit 503 „Host weg“, die Seite blendet ab. Nach
  Ablauf der Daten-Frist ist die Sicht geräumt.
- [ ] `POST …/monitor/control` mit dem Ziel `info_begegnung` antwortet 200; ein Gerät mit diesem Ziel wird auf
  `/{ns}/info/begegnung` umgeleitet.
- [ ] Der Host sendet den Frame einmal je Verbindung und danach nur bei geändertem Inhalt; ohne aktives
  Punktspiel sendet er `None`.
- [ ] Im Betrieb ohne Windows-PC zeigt Mein Badhub den Link zur Ansicht; der Link zeigt die Begegnung.
- [ ] `/{ns}/info/club-logo?id=42` leitet auf das badhub-Logo um; eine nicht numerische ID ergibt 404 und nie
  eine Umleitung.

badhub

- [ ] Das Paket trägt `heim_verein_id` und `gast_verein_id` (Zahl oder `null`). Eine bts-light-Version ohne
  diese Felder liest das Paket weiter.

## Tests

Rust (TDD, je PR vor dem Code):

- `relay-proto`: Serde-Roundtrip von Frame und Ziel (JSON exakt `{"kind":"info_begegnung"}`), `redirect_path`,
  Zählregel (alle Ausgänge, beendet ohne Sieger, unbestätigt), Nebenzeile, Mischen (Live ersetzt, fremdes Spiel
  ignoriert), `endstand`, umgezogene Satzregeln mit den bisherigen Fällen, Wächter „kein `lizenz`, keine `id`“.
- `relay`: Frame-Arm (speichern, Nudge), Deckel (Kürzen, Kappen, zu groß → `None`), Stand-Route (Form, ETag,
  503 ohne Host), `leeren`, Zuweisungs-Upload mit dem neuen Ziel → 200, Umleitung, `club_logo_ziel` mit ID und
  mit Müll.
- `src-tauri`: `begegnung_sicht` (acht und zwölf Spiele, Teilaufstellung, keine Lizenz, ohne Paket inaktiv),
  Paket mit und ohne Vereins-IDs, Stand-Route mit 304, Logo-Cache getrennt nach Name und ID,
  `push_begegnung` (Dedup, erstes Senden je Verbindung, `None` ohne Punktspiel), Ziel bleibt im Upload.
- Datenformat-Abdruck (`tests/datenformat_abdruck.rs`) läuft; bei Abweichung Abdruck und Begründung nachtragen.

JS: `src/io/begegnungAnsicht.mjs` (Initialen, Zustandstext, Zeilenzahl und Schriftgröße) mit
`scripts/test-begegnung-ansicht.mjs`; Inline-Kopie und die Kopien von Abruf-Frist und Push-Gesundheit in
`scripts/test-inline-kopien.mjs`; Schritt in `ci.yml`. badhub: Paket-Test für die zwei Felder.

Von Hand: Browsertest mit Attrappe bei 1920×1080 mit acht und zwölf Zeilen und langen Doppelnamen; Feldtest auf
einem Pi am TV, im LAN und über den Cloud-Link.

`cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt`, `npm run build` grün.

## Risiken & Rollback

- **Client vor Relay:** Weist ein Client das neue Ziel zu, bevor der Relay es kennt, frieren alle
  Cloud-Zuweisungen dieser Installation ein. Gegenmittel ist die Reihenfolge der PRs; Notbremse ist, das Ziel im
  Client vor dem Upload herauszufiltern (wie die Spielfolge).
- **Link vor Relay:** Der Link in Mein Badhub liefert 404, solange der Relay die Route nicht hat. Deshalb kommt
  der Link als letzter Schritt.
- **Aufgabe oder kampflos mit gescheiterter Meldung:** Der Live-Stand ist dann nicht „entschieden“; die Zeile
  bleibt „läuft“, bis badhub das Ergebnis hat. Bewusst hingenommen.
- **Hotfix-Seiten-Paket:** Die neue Seite kennt nur ein Kern ab dieser Version. Vor dem ersten Seiten-Paket nach
  dieser Version ist zu prüfen, dass ein älterer Kern eine unbekannte Datei im Paket überliest.
- **Rollback:** Alles ist additiv. Eine ältere bts-light-Version überliest das Ziel; die Paketfelder stören sie
  nicht. Vor einem Relay-Rollback die Zuweisung „Begegnung“ entfernen. Not-Aus im Betrieb: Monitor auf eine
  andere Ansicht stellen.
- **BTP- und badhub-Turniere:** Der Umzug der Satzregeln nach `relay-proto` berührt `process_result`. Die
  vorhandenen Tests dieser Regeln ziehen mit um und bleiben unverändert grün.

## Offene Fragen / Annahmen

- Annahme: Team 1 am Relay-Feldstand ist die Heimseite (so bildet `abbilden` ab). Wird in PR 1 mit einem Test
  gegen einen echten Frame belegt.
- Annahme: Die Initialen entstehen aus den ersten Buchstaben der ersten zwei Wörter des Mannschaftsnamens, ohne
  führende Ziffern („1. BC Muster 2“ → „BM“).
- Annahme: Eine Marke „Feld n“ genügt für laufende Spiele; die Spieldauer wird nicht gezeigt.
- Nebenfund außerhalb dieser Spec: `Paket::pruefen` erlaubt 1 bis 8 Felder, Doku und badhub nennen 1 bis 4.

## Betroffene Doku-Dateien

- `docs/court-monitor.md` und `docs/oberflaechen.md` (Handbuch) mit Ankern, z. B.
  `<!-- pruef: "/info/begegnung" in src-tauri/src/tablet/server.rs -->` und
  `<!-- pruef: "Begegnung (Punktspiel)" in src/pages/CourtMonitorPanel.tsx -->`
- `docs/cloud-relay.md` (Frame, Route, Logo per ID, Deploy-Reihenfolge)
- `docs/features/punktspiel.md`, `docs/features/punktspiel-kopflos.md` (Link ohne PC)
- `docs/adr/0109-…md`, `docs/adr/README.md`
- `CLAUDE.md`: neue Zeile in der Doku-Tabelle, Absatz im Datenschutz-Teil
- `docs/changelog.md` je Version, `docs/roadmap.md`
- badhub: `docs/features/live_scoring.md` (Paketfelder, Link)

## Umsetzungs-Hinweise

Reihenfolge, jeder Schritt ein eigener PR. Versionsnummer erst beim Merge festlegen, dann gemeinsam in
`src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` und `package.json`.

1. **badhub: Paketfelder.** `Paket.php` ergänzt `heim_verein_id` und `gast_verein_id` aus
   `Begegnungen::finde`. Test, Doku, Deploy. Review: code-reviewer.
2. **relay-proto + relay + Seite.** Typen, `begegnung_anzeige`, Umzug der Satzregeln, Frame, Ziel-Variante;
   Relay-Zustand, Frame-Arm, Deckel, Routen, Logo per ID; `assets/begegnung.html` vollständig (der Relay bettet
   sie ein) mit `src/io/begegnungAnsicht.mjs`. Der Relay deployt beim Merge von selbst. Review: code-reviewer und
   security-reviewer (neuer öffentlicher Endpunkt, Umleitung, Deckel, Namen).
3. **Client LAN.** `begegnung_sicht`, Paketfelder lesen, Seite im `seiten!`-Makro, Routen, Logo per ID, Option im
   Monitor-Panel, Handbuch. Review: code-reviewer und security-reviewer (Logo-Abruf per ID).
4. **Client Cloud.** `push_begegnung` im 2-s-Takt nach dem Fingerabdruck-Muster von `push_prepared`; läuft ohne
   Windows-PC ohne weiteren Schalter. Kann mit Schritt 3 zusammengehen, wenn Schritt 2 deployt ist.
5. **badhub: Link.** `OhnePc::links()` und die Ansicht in Mein Badhub. Erst nach dem Relay-Deploy aus Schritt 2.

Bestehende Muster: Stand-Route und ETag wie die Spielfolge (`tablet/server.rs`), Abruf mit Frist und Push-Nudge
wie `assets/overview.html`, feldloser Anstoß am Relay mit Feld 0, Deckel wie `MAX_MP_PARTIEN`.
