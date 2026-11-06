# „Zuletzt gespielt" in der Turnierleitungs-Sicht — Spezifikation

> Status: **freigegeben 2026-11-06**, umgesetzt in v0.9.342. Quelle: Nutzer-Anforderung
> vom 06.11.2026, Grill im Gespräch (vier Entscheidungen, siehe unten).
> Betroffene Crates: `src-tauri`, `relay-proto`, `relay` (nur Neubau).
> Kein ADR — keine Architekturentscheidung, nur ein weiteres Anzeige-Feld.

## Kontext / Problem

> In der Turnierleitungssicht (web) wollen wir als weitere Option haben, die
> Zeit darzustellen, seit dem eine Paarung das letztes mal auf dem Feld stand
> (also unabhängig von der Wartezeit), soll einstellbar sein, sich die Zeit
> anzeigen zu lassen.

Die Spielliste zeigt heute nur die **Pflichtpause** (Eieruhr mit Countdown,
`TlBlocked::Pause`) — und nur, solange sie läuft und überhaupt eine
eingestellt ist. Danach ist eine Paarung „spielbereit", egal ob ihr letztes
Spiel 21 Minuten oder drei Stunden her ist. Wer fair aufrufen will („die
haben gerade erst gespielt, nimm lieber das nächste"), muss das heute in BTP
nachschlagen.

## Entscheidungen aus dem Grill (06.11.2026)

| Frage | Entscheidung |
|---|---|
| Bezug | **je Seite**: ein Wert pro Paarung = der Spieler dieser Seite mit dem **jüngsten** Spielende (kürzeste Pause) |
| Format | **Dauer „vor 42 min"**, läuft live mit |
| Ort | **Warteliste** und **gerufene Spiele (Vorbereitung)** — nicht an Feldkacheln |
| Was zählt | **jedes beendete Spiel außer Walkover** (Aufgabe und Disqualifikation zählen: da stand man auf dem Feld) |
| Schalter | Profil-Schalter, **Standard aus** (wie „Restzeit laufender Spiele zeigen") |

## Zielbild & Erfolgskriterien

- Im Profil-Dialog gibt es ein Häkchen **„Zuletzt gespielt zeigen"**.
- Ist es an, steht an jeder Seite einer wartenden oder gerufenen Paarung eine
  kleine graue Marke **„vor 42 min"**, die minütlich weiterzählt — unabhängig
  davon, ob eine Pflichtpause läuft, abgelaufen ist oder gar keine
  eingestellt ist.
- Hat keiner der Spieler einer Seite heute schon gespielt, steht an dieser
  Seite **nichts** (kein „—", kein „noch nicht gespielt": eine Marke an fast
  jeder Zeile am Vormittag wäre keine Information).
- Ist das Häkchen aus, sieht die Liste exakt aus wie heute.

## Nicht-Ziele (YAGNI)

- Keine Einfärbung nach Schwellen („unter 30 min = orange"). Die Pflichtpause
  hat schon ihre Eieruhr; eine zweite Ampel bräuchte eine zweite Einstellung.
  Später nachrüstbar.
- Keine Anzeige an Feldkacheln, beendeten Spielen oder offenen Paarungen
  (`TlOpenMatch`: Kandidaten spielen das Spiel womöglich nie).
- Kein Einfluss auf Sortierung, Automatik oder Prognose — reine Anzeige.
- Nicht auf Monitoren, Spielfolge-Anzeige, Desktop-Oberfläche oder badhub.

## Verhalten

### Woher die Zeit kommt

BTP liefert kein Spielende. bts-light hat aber zwei eigene, **persistente**
Stempel im Zeiten-Store (`match-times.json`, `tablet/match_times.rs`):

1. `finished_ms` (E3) — gesetzt, wenn das Ergebnis über Tablet, TL-Web oder
   Desktop eingetragen wurde: das echte Spielende.
2. `finished_seen_ms` — gesetzt, wenn bts-light das Spiel im BTP-Stand
   erstmals als beendet sieht (landet als `BtpMatch.finished_at` im
   Snapshot; dieselbe Quelle wie die Pflichtpause).

Verwendet wird **`finished_ms`, sonst `finished_at`**. Damit stimmt die Zeit
auch bei Ergebnissen, die erst später in BTP auftauchen; ein direkt in BTP
eingetragenes Ergebnis wird beim nächsten Sync-Takt (≤ wenige Sekunden)
gesehen.

### Wie der Wert je Seite entsteht

Einmal je Zustandsbau (`build_state_limited`) eine Tabelle
`player_key → jüngstes Spielende` über alle Spiele mit
`status == Finished` und `result != Walkover` — Schlüssel ist
`assign::player_key`, derselbe wie bei der Pausensperre (gleichnamige
Spieler werden nicht verschmolzen). Je wartendem Spiel und Seite:
**Maximum** über die Spieler dieser Seite. Fehlt jeder Spieler in der
Tabelle → `None`. Ein Spielende, das **älter als zwölf Stunden** ist, zählt
nicht (Review 06.11.2026): Bei mehrtägigen Turnieren stünde morgens sonst
„vor 14 h 05" vom Vortag an jeder Seite. Zwölf Stunden statt Kalendertag,
weil der Host keine Zeitzone kennt.

Steht ein Spieler der Seite **gerade auf einem Feld**, ist die Zeit
bedeutungslos (er spielt ja jetzt). Der Host liefert dann `None` für diese
Seite; den Zustand zeigen schon die rot gefärbten Namen (`TlBlocked::Playing`).

### Darstellung

- Marke `<span class="zuletzt num" data-last-played="…">` hinter der jeweiligen
  Seite im `pairOf`-Block, Text aus `tickClocks()` (nicht beim Rendern
  einbetten — sonst kippt der Keyed-Abgleich der Zeilen, wie bei der
  Gerufen-Uhr).
- Text: unter 1 Minute „gerade eben", sonst „vor N min", ab 60 Minuten
  „vor 1 h 05".
- Tooltip: „Letztes Spiel dieser Seite endete um 10:15".
- Farbe: gedämpft (`--ink-faint`), damit sie nicht mit der orangen Eieruhr
  konkurriert. Läuft die Pflichtpause noch, stehen beide nebeneinander —
  gewollt, „unabhängig von der Wartezeit".

## Betroffene Komponenten

| Stelle | Änderung |
|---|---|
| `config.rs` `TlDisplaySettings` | `show_last_played: bool` (Default `false` — Bestandsprofile bleiben aus, das ist hier der gewollte Standard) |
| `relay-proto` `TlDisplaySettingsWire` | `showLastPlayed`, `#[serde(default)]` |
| `tablet/tl.rs` | `TlMatch.team1_last_played_ms`/`team2_last_played_ms: Option<u64>` (`skip_serializing_if = None`), Tabelle in `build_state_limited`, Wire-Umsetzung in `display_settings_*` |
| `tablet/match_times.rs` | Lesezugriff auf `finished_ms` aller Spiele (eine Kopie je Bau, nicht je Zeile) |
| `assets/tl.html` | Häkchen im Profil-Dialog, `zeigeZuletztGespielt()`, Marke in `pairOf`, Tick in `tickClocks` |
| `relay/` | nur Neubau (das Profil-Feld reist als Wire-Typ mit) |

**R1/R2/R5** unberührt: reine Anzeige aus vorhandenen Daten, keine neue
Aktion, kein Schreibweg nach BTP.

**Revision/Rev-Churn:** Die Werte ändern sich nur, wenn ein Spiel endet — dann
ändert sich der Zustand ohnehin. Kein Sekundenwert auf dem Draht, das Ticken
macht die Seite.

**64-KiB-Fenster der Cloud:** zwei Zahlen je Zeile, ≈ 2 × 35 Byte ×
`QUEUE_LIMIT` (120) ≈ 8 KB im ungünstigsten Fall, praktisch weniger (`None`
bleibt weg). Die bestehende
Kappung (`build_state_limited`) greift unverändert; die Felder werden **nicht**
an die Schalter gekoppelt (der Schalter wirkt im Browser, wie bei allen
Profil-Optionen).

## Datenschutz

Ein Spielende ist aus dem öffentlichen Liveticker ablesbar und sagt über eine
Person nichts, was über „hat heute gespielt" hinausgeht. Keine Namen, keine
IDs im neuen Feld — nur ein Zeitstempel je Seite. Der Wächter-Test
`the_state_never_carries_personal_data_beyond_its_purpose` bleibt grün und
bekommt eine positive Zeile, dass das Feld an `TlOpenMatch` **fehlt**.

## Akzeptanzkriterien

1. Häkchen aus (Standard, auch bei Bestandsprofilen ohne das Feld) → Liste
   unverändert.
2. Häkchen an, Einzel: Spieler hat vor 42 min ein Spiel beendet → „vor 42 min"
   an seiner Seite; die andere Seite (heute noch nicht gespielt) ohne Marke.
3. Doppel: Partner A endete vor 50 min, Partner B vor 12 min → „vor 12 min".
4. Das jüngste Spiel war ein Walkover → es zählt nicht; das davor gilt (oder
   keine Marke).
5. Aufgabe/Disqualifikation zählen.
6. Spieler steht gerade auf einem anderen Feld → keine Marke an dieser Seite.
7. Pflichtpause läuft → Eieruhr **und** Marke sichtbar.
8. Ergebnis per Tablet um 10:15 eingetragen, in BTP erst um 10:20 gesehen →
   Marke rechnet ab 10:15.
9a. Letztes Spiel älter als zwölf Stunden (Vortag) → keine Marke.
9. App-Neustart/Speichern der Einstellungen → Werte unverändert (persistente
   Stempel).
10. Gerufene Spiele zeigen die Marke genauso; offene Paarungen nie.
11. LAN und Cloud zeigen dasselbe.

## Tests

- `tl.rs`: Kriterien 2–6, 8, 10 als Unit-Tests am Zustandsbau.
- `relay-proto`: Serde-Roundtrip `showLastPlayed`; altes Profil ohne Feld → `false`.
- `config.rs`: Default `false`, Roundtrip.
- Datenschutz-Wächter erweitert (Feld an `TlOpenMatch` abwesend).
- `tl.html`: Textformat („gerade eben", „vor 42 min", „vor 1 h 05") über die
  Blob-Import-Technik (siehe Memory „Asset-JS im Browser testen") oder eine
  kleine `src/io/*.mjs`-Funktion mit Inline-Kopie + Drift-Test, je nachdem,
  was sich beim Umsetzen sauberer schneidet.

## Risiken & Rollback

- **Frische Installation mitten im Turniertag:** Alle schon beendeten Spiele
  bekommen beim ersten Sync den Stempel „jetzt" — dann stünde an vielen
  Seiten „gerade eben". Die Pflichtpause hat dasselbe Verhalten seit
  v0.9.253. **Annahme:** hingenommen (die App läuft ab Turnierbeginn); im
  Handbuch erwähnt. Falls es stört: Stempel des allerersten Zyklus nach
  Turnierbindung als „Altbestand" markieren und dort keine Marke zeigen —
  bewusst nicht in dieser Etappe.
- Rollback: Häkchen aus. Das Feld ist rein additiv.

## Doku-Pflicht

- `docs/turnierleitung-web.md` — Bedienung, mit `<!-- pruef: -->`-Anker auf
  „Zuletzt gespielt zeigen" in `src-tauri/assets/tl.html`.
- `docs/cloud-relay.md` — Profil-Feld `showLastPlayed` auf der Wire-Ebene.
- `docs/roadmap.md` — Eintrag.
- `CLAUDE.md` — Zeile in der Code-Pfad-Tabelle.
- `docs/changelog.md` beim Release.
