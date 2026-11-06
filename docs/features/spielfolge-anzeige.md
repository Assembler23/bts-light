# Spielfolge-Anzeige (laufende + kommende Spiele) — Spezifikation

> Status: **Schnellumsetzung 2026-11-06 in v0.9.341** auf Wunsch der Turnierleitung (Turnierbeginn
> in einer Stunde) — Brief + Grill gelaufen, Spec nicht formal freigegeben. **Abweichungen
> vom Entwurf:** Anordnung und Anzahl reisen im Ziel/URL statt in einer eigenen Geräte-Datei;
> das Ziel wird vor dem Relay-Upload herausgefiltert (kein Relay-Deploy-Zwang); Ansage-Modus
> (`?ansage=`) fehlt; der Panel-Dialog ist durch zwei Felder neben der Ziel-Auswahl ersetzt.
> Quelle: Wunsch vom 06.11.2026. Betroffene Crates: `src-tauri/`, `src/`, `relay-proto/`
> (neue Ziel-Variante), `relay/` (kennt die Variante nur, zeigt sie nicht).
> ADR: [0076 — Warteliste samt Kandidatennamen auf öffentlichen Anzeigen](../adr/0076-warteliste-auf-oeffentlichen-anzeigen.md).

## Kontext / Problem

Die Pi-Monitore kennen heute Einzelfeld, Zähltafel, Kombi (bis 3 Felder), Feld-Übersicht
(`/info/overview`), Vorbereitung (`/info/preparation`), Sieger und Werbung. Keine dieser
Ansichten beantwortet die Frage, die Spieler in der Halle am häufigsten stellen: **„Wann bin ich
dran?"** Die Feld-Übersicht zeigt nur, was gerade läuft. Die Vorbereitung zeigt nur die schon
gerufenen Spiele, meist zwei bis fünf. Die eigentliche Reihenfolge, also die Warteliste, sieht
heute nur die Turnierleitung im TL-Web.

## Zielbild & Erfolgskriterien

Ein Hallen-TV zeigt **alle laufenden Spiele** und darunter oder daneben die **nächsten N Spiele
der Warteliste** (Standard **19**, je Gerät einstellbar), in genau der Reihenfolge, die die
Turnierleitung im TL-Web sieht. Passt die Liste nicht auf den Bildschirm, läuft sie langsam als
**Endlos-Laufband** durch.

**Erfolgskriterien**

- Ein Spieler findet sein nächstes Spiel samt Platz in der Reihenfolge und voraussichtlicher
  Startzeit, ohne die Turnierleitung zu fragen.
- Stellt die Turnierleitung im TL-Web um (Ziehen, „An den Anfang", Aufruf), zeigt der TV die neue
  Reihenfolge **spätestens 5 s** später, und zwar ohne Sprung im Laufband.
- Das Laufband ist aus 5 m Abstand lesbar: Jede Zeile bleibt mindestens **4 s** im Bild.

## Nicht-Ziele

- **Kein Cloud-Betrieb** in dieser Etappe, wie bei der Kombi-Anzeige. Der Relay kennt die neue
  Ziel-Variante nur, damit er Zuweisungs-Uploads nicht verwirft. Ein Cloud-Gerät mit diesem Ziel
  zeigt die Kopplungsseite. Cloud ist eine spätere, eigene Etappe.
- **Keine Bedienung am TV**, keine Interaktion. Es ist eine reine Anzeige.
- **Keine neue Reihenfolge-Logik.** Die Anzeige rechnet nichts selbst, sie zeigt die
  TL-Warteliste.
- **Keine Prognose ohne Prognose-Schalter.** Ist `prediction.enabled` aus, fehlt die Zeit-Spalte.
- **Kein Satzverlauf, keine Aufschlaganzeige** bei laufenden Spielen. Dafür gibt es
  Einzelfeld und Übersicht.

## Bedienung (Turnierleiter)

Dashboard → **Court-Monitore** → beim Gerät die Ziel-Auswahl → neue Gruppe **„Spielfolge"** →
**„Spielfolge einrichten…"**. Ein Dialog wie bei der Kombi-Anzeige bietet:

| Einstellung | Werte | Standard |
|---|---|---|
| **Anordnung** | „Laufende oben, Warteliste läuft darunter" · „Zwei Spalten" · „Eine durchgehende Liste" | Laufende oben |
| **Kommende Spiele** | 1–60 | 19 |
| **Halle** | „Alle Hallen" oder eine Halle des Turniers | Alle |

Die Option ist mit **„(nur LAN)"** beschriftet. In der Betriebsart *nur Cloud* ist sie
ausgegraut.

## Inhalt

**Laufende Spiele.** Jedes Feld, auf dem gerade ein Spiel liegt (zugewiesen oder laufend), in
Feld-Reihenfolge. Je Feld: Feldname (mit Hallenfarbe), Disziplin + Runde, beide Seiten, der
Satzstand kompakt (`21:15 · 8:4`). Freie Felder erscheinen nicht. Nation und Verein folgen den
vorhandenen Monitor-Schaltern (`court_monitor.name_davor`/`name_darunter`, serverseitig gefiltert
über `monitor::anzeige_filtern`). Setzplätze werden gezeigt wie auf allen Monitoren.

**Kommende Spiele.** Die ersten N Einträge der TL-Warteliste, einschließlich der Spiele mit
offener Paarung an ihrer Listenposition (gleiche Verschmelzung wie `src/io/offeneSpiele.mjs`).
Je Zeile:

- **Platznummer** 1, 2, 3 … (innerhalb der angezeigten, ggf. hallengefilterten Liste),
- **Disziplin + Runde** (`HD-A · Viertelfinale`),
- **beide Seiten**. Bei offener Paarung steht derselbe Text wie im TL-Web
  (`offener_platz_text`): „A oder B", „aus Spiel 42" oder „noch offen". Es steht nie „Sieger
  aus", weil bei Platzierungsspielen der Verlierer nachrückt.
- **voraussichtliche Startzeit** „ca. 14:35", nur bei eingeschalteter Prognose und nur wenn
  vorhanden; unsichere Werte mit „~",
- **Marken**: „In Vorbereitung" (gerufen), Halle mit Hallenfarbe, Wunschfeld „→ Feld 3".

Ausgeschlossene, gesperrte oder von der Automatik ausgenommene Spiele zeigt die Anzeige genauso,
wie die TL-Warteliste sie führt. Interne Marken (`blocked`, `excluded`, `highlight`, `manual`)
erscheinen **nicht**.

## Anordnungen

1. **Laufende oben** (Standard): Die laufenden Spiele stehen fest oben, kompakt mit einer Zeile je
   Feld. Darunter läuft die Warteliste. Belegt der laufende Block mehr als **55 %** der Höhe,
   läuft er selbst ebenfalls als Laufband (eigenes Band, eigener Takt).
2. **Zwei Spalten**: links die laufenden, rechts die kommenden Spiele. Jede Spalte läuft für sich
   durch, falls sie überläuft.
3. **Eine durchgehende Liste**: Zuerst die laufenden, dann (unter einer Trennzeile „Als
   Nächstes") die kommenden Spiele, als **ein** Laufband.

Je Bereich gilt: Passt der Inhalt, steht er still. Läuft er über, wird er zum Laufband.

## Laufband

- Der Inhalt wandert gleichmäßig **nach oben**, man liest also von oben nach unten. Ans Ende
  schließt nahtlos wieder der Anfang an, mit einer Trennzeile dazwischen.
- Tempo in Zeilenhöhen: Eine Zeile braucht **4 s**, um ihre eigene Höhe zu durchlaufen. Das ist
  unabhängig von der Bildschirmgröße.
- **Datenwechsel ohne Sprung**: Kommt neuer Inhalt, bleibt der Versatz in Pixeln erhalten (modulo
  der neuen Gesamthöhe). Es gibt keinen Neustart oben.
- Animation über `requestAnimationFrame` + `transform: translateY` (GPU, ruckelfrei auf dem Pi).
  Bei `prefers-reduced-motion` wird seitenweise alle 8 s geblättert.
- Die Rechenlogik (Versatz, Fortschreibung bei Höhenwechsel, „passt / läuft") liegt kanonisch in
  `src/io/laufband.mjs`, mit Inline-Kopie in der Seite und Drift-Test.

## `?halle=`

Wie bei Übersicht und Vorbereitung. Der Wert kommt aus dem Dialog und reist in der Umleitungs-URL
mit. Laufende Spiele: nur Felder dieser Halle. Kommende Spiele: nur Spiele, deren Halle (aus der
Kaskade `resolve_and_sort_key`: Wunschfeld, Aufruf, Hand, Automatik) passt, **plus** Spiele ohne
Halle. Ohne Parameter werden alle Hallen gezeigt (keine Hallen-Rotation, da es eine Gesamtliste
ist). Vergleich über `relay_proto::halle_passt`.

## Betroffene Komponenten / Architekturregeln / Daten

- **`relay-proto`**: `MonitorTarget::InfoSpielfolge { halle: Option<String> }`, `kind_str`
  `"info_spielfolge"`, `redirect_path()` → `/info/spielfolge` (+ `?halle=`). Serde-Roundtrip-Test.
- **`relay/`**: Die Variante wird deserialisiert, damit der Zuweisungs-Upload nicht abgelehnt
  wird, und fällt im Monitor-State-Handler in den Arm „unassigned" wie `CourtCombo`. Das Relay
  wird bei jedem main-Merge automatisch deployt. Die Variante muss **vor** der ersten App-Version
  live sein, die sie hochlädt; ein altes Relay verwirft sonst den **ganzen** Upload, und alle
  Cloud-Zuweisungen frieren ein. Reihenfolge: ein PR, Relay deployt beim Merge, die App kommt
  erst mit dem Tag.
- **`src-tauri/src/tablet/spielfolge.rs`** (neu): reine Projektion
  `spielfolge_view(&TlState, &[CourtOverview], &Einstellung, &CourtMonitorConfig) -> SpielfolgeView`.
  Sie arbeitet mit einer **Positivliste** von Feldern (kein `_ids`, keine Lizenznummer, kein
  Geburtsjahr, kein Check-In, keine SR-Daten, keine internen Marken). Unit-Tests: Reihenfolge,
  Verschmelzung offener Paarungen, Hallenfilter, Deckel N, Prognose aus, Datenschutz-Wächter.
- **Quelle der Warteliste**: `tl::build_state(...)`. Es ist dieselbe Funktion wie im TL-Web, also
  gibt es keine zweite Sortierlogik. Diese Funktion ist teuer und hat als Nebenwirkung
  `merge_predicted_starts`. Deshalb liegt ein **eigener Cache** im `TabletState`
  (`SpielfolgeCache { gebaut_ms, state }`) mit **3 s** Lebensdauer, gemeinsam für alle
  Spielfolge-Monitore. Gebaut wird **nur auf Abruf**, nicht im Takt. Er hängt nicht am
  TL-Web-Cache, weil der nur bei eingeschaltetem TL-Web und mit TL-Zuschauern lebt. Wird kein
  Spielfolge-Monitor abgerufen, entsteht keine Last.
- **`tablet/server.rs`**: Routen `/info/spielfolge` (Seite) und `/info/spielfolge/state?device=`
  (JSON). Der State-Abruf vermerkt den Herzschlag (`record_monitor_poll`) und liest die
  Geräte-Einstellung. Die Umleitung aus `monitor_device_state` funktioniert über `redirect_path()`
  ohne neuen Code.
- **Geräte-Einstellung** (Anordnung, Anzahl): eigene Datei `monitor-spielfolge.json` neben der
  Zuweisung, nach dem Muster von ADR 0049 (`ComboDirStore`), also **nicht** im Target. Deshalb
  reist sie nicht zum Relay. Die Halle steht dagegen im Target (wie bei `InfoOverview`), weil sie
  Teil der URL ist.
- **`commands.rs`**: `assign_monitor` bekommt einen optionalen Parameter
  `spielfolge: Option<SpielfolgeEinstellung>`; neu ist `monitor_spielfolge_settings`. R1 bleibt
  gewahrt.
- **`assets/spielfolge.html`** (neu) + `assets.rs` `include_str!`. Gemeinsam mit allen
  Geräteseiten: `?rotate=`, `?device=`, Umleitungs-Rückkehr (Ziel geändert → zurück zu
  `/monitor`), Turnierlogo/Sponsor-Leiste im Kopf, Uhr.
- **`pages/CourtMonitorPanel.tsx`**: Optgroup „Spielfolge", Dialog `SpielfolgeDialog`,
  Wert-Kodierung `spielfolge:<halle>`.
- **R2** bleibt gewahrt: Die Anzeige erfindet keine Zuordnung, sie spiegelt BTP und die
  TL-Warteliste.

## Datenschutz

Neu ist, dass die **Warteliste öffentlich** in der Halle hängt. Bei offenen Paarungen gehören dazu
die **Kandidatennamen** („A oder B"). Beides war bisher nur hinter dem TL-Gerätezugang
freigegeben. Die Turnierleitung hat das am 06.11.2026 bewusst freigegeben. Begründung: Die Namen
stehen ohnehin auf Auslosungsbogen, Spielplan und badhub-Liveticker, und der Zweck ist derselbe
(das richtige Spiel ans Feld bringen bzw. Spieler bereithalten). Weiterhin gilt: **keine
Lizenznummer, kein Geburtsjahr, keine SR-Daten**. Nation und Verein nur nach den
Monitor-Schaltern, serverseitig gefiltert. Festgehalten in ADR 0076 und im
Datenschutz-Abschnitt von `CLAUDE.md`. Wächter-Test in `spielfolge.rs`.

## Akzeptanzkriterien

1. Ein Gerät mit Ziel „Spielfolge" zeigt binnen 2 s nach der Zuweisung die neue Ansicht.
2. Die kommenden Spiele stehen in derselben Reihenfolge wie die TL-Warteliste, einschließlich
   Handsortierung und offener Paarungen an ihrer Position.
3. Die Anzahl der Zeilen ist ≤ N (Standard 19), einstellbar 1–60 je Gerät.
4. Ein Umsortieren im TL-Web erscheint binnen 5 s, ohne dass das Laufband nach oben springt.
5. Passt alles auf den Bildschirm, bewegt sich nichts. Läuft es über, läuft es endlos und
   nahtlos, mit ≥ 4 s je Zeile.
6. Alle drei Anordnungen sind je Gerät wählbar und greifen ohne Eingriff am Pi.
7. Mit `?halle=` werden nur Felder und Spiele dieser Halle gezeigt (plus Spiele ohne Halle).
8. Ist die Prognose aus, gibt es keine Zeit-Spalte. Ist sie an, steht „ca. HH:MM" (unsicher: „~").
9. Kein `_ids`-Feld, keine Lizenznummer, kein Geburtsjahr im JSON (Wächter-Test).
10. Ein Cloud-Gerät mit diesem Ziel zeigt die Kopplungsseite, und die übrigen Cloud-Zuweisungen
    funktionieren weiter.
11. Ohne Abruf einer Spielfolge-Seite wird `build_state` dafür nie aufgerufen.

## Tests

- Rust: `spielfolge.rs`-Projektion (Reihenfolge, Verschmelzung, Hallenfilter, Deckel, Prognose
  an/aus, Wächter), Cache-Lebensdauer, Store-Roundtrip `monitor-spielfolge.json`,
  `relay-proto`-Serde-Roundtrip der neuen Variante, Relay: Variante → unassigned.
- JS: `scripts/test-laufband.mjs` (Versatz-Fortschreibung, passt/läuft, Tempo),
  Drift über `scripts/test-inline-kopien.mjs`.
- Browser: Mock-State mit 8 laufenden und 19 kommenden Spielen in drei Anordnungen, als
  Screenshot im Chrome ohne Fenster bei 1920×1080 und gedreht (`?rotate=90`).
- Feldtest am Pi: Ruckelfreiheit des Laufbands (Pi 4, 1080p).

## Risiken & Rollback

- **Ruckeln auf dem Pi** bei langem Laufband. Gegenmittel: `transform` statt `scrollTop`,
  `will-change`, keine Schatten oder Verläufe im bewegten Bereich.
- **Last durch `build_state`**: Der 3-s-Cache deckelt sie auf höchstens einen Bau je 3 s,
  unabhängig von der Zahl der Geräte.
- **Altes Relay**: siehe Deploy-Reihenfolge oben.
- Rollback: Das Gerät bekommt ein anderes Ziel. Es gibt keine Datenmigration; die
  Einstellungsdatei bleibt liegen und schadet nicht.

## Offene Fragen / Annahmen

- Annahme: „von oben nach unten" bedeutet die Leserichtung, der Inhalt wandert also nach oben.
- Annahme: Freie Felder werden bei den laufenden Spielen nicht gezeigt.
- Annahme: Die Platznummer zählt innerhalb der gefilterten Liste, nicht in der Gesamt-Warteliste.
- Offen: Soll ein „In Vorbereitung" gerufenes Spiel immer ganz oben stehen? Laut TL-Sortierung
  stehen gerufene Spiele ohnehin vorn.

## Betroffene Doku-Dateien

`docs/court-monitor.md` (neuer Abschnitt, Handbuch) · `docs/oberflaechen.md` (Route) ·
`docs/cloud-relay.md` (Variante, Deploy-Reihenfolge) · `docs/changelog.md` · `docs/roadmap.md` ·
ADR 0076 · `CLAUDE.md` (Doku-Tabelle + Datenschutz).
