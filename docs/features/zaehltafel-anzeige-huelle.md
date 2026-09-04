# Zähltafel fürs Tablet + Anzeige-Hülle — Spezifikation

> Status: **freigegeben 2026-09-04 · PR 1 umgesetzt 2026-09-04 · PR 2 (Hülle + Einstiege) umgesetzt 2026-09-04**
> (via Brief → Grill → How-To → Review).
> Quelle: Idee vom 04.09.2026. Betroffene Crates: `src-tauri/` (Assets, Server, Monitor-Zuweisung),
> `relay/` (Routen, Umleitung), `relay-proto/` (`MonitorTarget`), `src/` (Court-Monitor-Panel,
> `io/`-Module).
> ADR: [0055 — Zähltafel: Anzeige-Hülle als iframe-Container, Tafel als eigenes Zuweisungsziel](../adr/0055-zaehltafel-anzeige-huelle-und-zuweisungsziel.md).

## Kontext / Problem

Am Feld gibt es heute zwei Bildschirme mit Spielstand: das Zähl-Tablet des Schiedsrichters
(`tablet.html`, klein, für die Hand) und den Court-Monitor auf dem TV (`monitor.html`, mit Namen,
Flaggen, Disziplin, für 32"–55"). Was fehlt, ist die **klassische Zähltafel**: ein Tablet am Netz
oder am Schiedsrichterstuhl, das Spielern und Zuschauern nur den Spielstand zeigt, groß genug
für einen 8–11-Zoll-Bildschirm, ohne Namen und ohne Beiwerk.

Ein zweites Tablet kann das heute nicht leisten. Öffnet es die Tablet-Seite eines belegten
Feldes, sieht es nur das Belegt-Overlay „Dieses Feld wird bereits geschiedst" mit dem einzigen
Knopf „Court übernehmen" — und der würde dem zählenden Gerät das Feld wegnehmen. Der
Court-Monitor kennt nur ein Layout (`layout = "split"`), das auf einem Tablet zu klein und zu
voll ist, und hat weder ein Menü noch einen Weg zurück.

Den Schmerz hat der Schiedsrichter (kein sichtbarer Stand für die Spieler) und der Turnierleiter
(ein Tablet lässt sich nicht als Anzeige nutzen).

## Zielbild & Erfolgskriterien

Nach Umsetzung gibt es zwei neue Seiten und ein neues Zuweisungsziel:

1. **Tafel-Layout** `tafel.html` unter `/court/{id}/tafel`: reine Anzeige wie `monitor.html`,
   ohne Menü. Zwei sehr große Punktzahlen nebeneinander, klein darüber der Satzstand (gewonnene
   Sätze, z. B. `1 : 0`), ein Aufschlag-Punkt an der aufschlagenden Seite. Keine Namen, keine
   abgeschlossenen Sätze. Dunkler Grund, Ziffern in Klapp-Tafel-Optik. Läuft auch auf Pi-TVs
   über die Court-Monitor-Zuweisung „Zähltafel – Feld X".
2. **Anzeige-Hülle** `anzeige.html` unter `/anzeige`: die Tablet-Seite für Anzeigen. Sie bettet
   ein Layout in einem seitenfüllenden iframe derselben Herkunft ein und liefert alles, was ein
   Tablet braucht, ein TV aber nicht: Zahnrad mit PIN, Layout-Wahl (Zähltafel, Feld-Monitor,
   Hallen-Übersicht, Spiele in Vorbereitung), Feldwechsel, Seiten spiegeln, Zum Zählen wechseln,
   Neu laden, Vollbild, Wake-Lock.
3. **Einstiege am Zähl-Tablet**: im Zahnrad-Menü „Anzeige (nur Spielstand)" und im
   Belegt-Overlay „Nur Spielstand anzeigen". Beide öffnen die Hülle mit dem aktuellen Feld und
   dem Layout Zähltafel.

Erfolgskriterien beim nächsten Turnier:

- Ein zweites Tablet zeigt innerhalb von 30 Sekunden nach dem Auspacken den Spielstand eines
  Feldes, ohne dass jemand eine Adresse tippt: Tablet-Seite öffnen, Feld antippen, im
  Belegt-Overlay „Nur Spielstand anzeigen". Das zählende Tablet merkt nichts davon.
- Der Punkt erscheint auf der Tafel so schnell wie auf dem Court-Monitor (WS-Nudge, ADR 0016).
- Die Ziffern sind aus 5 m Entfernung lesbar (Ziffernhöhe ≥ 35 vmin in Quer- und Hochformat).
- Ein Turnierleiter kann einen Pi-TV im Court-Monitor-Panel auf „Zähltafel – Feld 3" stellen
  und der TV zeigt die Tafel beim nächsten Poll.

## Nicht-Ziele

- Keine Namen, Flaggen, Vereine, Disziplin oder Spielnummer auf der Tafel.
- Keine abgeschlossenen Sätze als Zeile (bewusst Variante „laufender Satz groß, Rest klein").
- Kein Zählen aus der Tafel heraus; sie schreibt nie etwas zurück (R2, R5 unberührt).
- Keine neue Rolle am Tablet-WebSocket (`/ws`); die Hülle und die Tafel öffnen ihn nie (R4).
- Keine Änderung an `monitor.html`, `overview.html`, `preparation.html`, `combo.html`.
- Keine Fernsteuerung der Spiegelung durch den Turnierleiter; sie ist eine Geräteeinstellung.
- Kein Versionsabgleich der Hülle über die Seiten-Marke des Tablets (`__SEITEN_MARKE__`); ein
  Menüpunkt „Neu laden" reicht.
- Kein Wake-Lock im unverschlüsselten LAN (technisch unmöglich, siehe unten).
- Keine Konfigurationsfelder in `config.rs`.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - `src-tauri/assets/tafel.html` (neu) und `src-tauri/assets/anzeige.html` (neu).
  - `src-tauri/src/tablet/assets.rs`: `TAFEL_HTML`, `ANZEIGE_HTML` per `include_str!`.
  - `src-tauri/src/tablet/server.rs`: Routen `/court/{id}/tafel` und `/anzeige`; Render mit
    `__MODE__`, `__BASE__`, `__COURT_LABEL__` (Tafel) bzw. `__BASE__`, `__TABLET_PIN__` (Hülle),
    `Cache-Control: no-store`.
  - `relay/src/main.rs`: eigene `include_str!` beider Seiten, Routen `/{ns}/court/{id}/tafel`
    und `/{ns}/anzeige`, `__BASE__` = `/bts-relay/{ns}/`, `__TABLET_PIN__` = `""` (wie Tablet),
    `valid_namespace`. Umleitungs-Allowlist im Geräte-State um `CourtTafel` erweitern.
  - `relay-proto/src/lib.rs`: `MonitorTarget::CourtTafel { court_id }`, Serde-Tag
    `"court_tafel"`, `redirect_path()` → `/court/{id}/tafel`, `court_id()` liefert auch für
    `CourtTafel` das Feld.
  - `src/pages/CourtMonitorPanel.tsx`: Option `tafel:<id>` je Feld („Zähltafel – Feld X"),
    Sortierrang wie Feld-Ziele.
  - `src/io/tafelSeiten.mjs` + `scripts/test-tafel-seiten.mjs` (Seiten/Aufschlag/Satzstand,
    Inline-Kopie in `tafel.html`); `src/io/anzeigeZiel.mjs` + `scripts/test-anzeige-ziel.mjs`
    (Layout-Allowlist → Pfad, Inline-Kopie in `anzeige.html`).
  - `src-tauri/assets/tablet.html`: Menüeintrag und Overlay-Knopf.
- **Architekturregeln:** R1 unberührt (keine neuen Tauri-Commands; das Panel nutzt die
  bestehende Zuweisung). R2: Tafel liest nur `/court/{id}/state`, BTP bleibt die Wahrheit. R3:
  beide Pfade, LAN (8088 und TLS 8443 über denselben `router()`) und Cloud (`/bts-relay/{ns}/`);
  die Hülle nutzt `BASE` relativ, damit iframe und Layout dieselbe Herkunft haben. R4: Hülle und
  Tafel melden sich nie als Tablet an; das Belegt-Overlay bleibt der einzige Übernahme-Weg.
  R5 unberührt (kein Ergebnispfad). R6 unberührt.
- **Konfiguration & Abwärtskompatibilität:** keine neuen Config-Felder. Die Zuweisungsdatei
  `monitor-assignments.json` bekommt die neue Variante `court_tafel`. Aufwärts lesbar. **Abwärts
  verwirft `read_assignments` unbekannte Varianten still** (Befund ADR 0049): Nach einem Downgrade
  stehen Tafel-TVs auf der Kopplungsseite und müssen neu zugewiesen werden. Bewusst akzeptiert,
  siehe ADR 0055. Ein **altes Relay** lehnt den gesamten Zuweisungs-Upload mit unbekanntem `kind`
  ab (422); weil Host `assignments` und `targets` in einem Body hochlädt, frieren dabei **alle**
  Zuweisungen und Fernbefehle des Turniers ein, nicht nur die Tafel. Die Regel „Relay-Deploy beim
  Merge, App-Tag danach" deckt das, ein Relay-Rollback nicht. `identifier` und Updater-Pfad bleiben unangetastet.
- **Datenschutz:** Die Tafel zeigt keine Namen. Die Feldwahl der Hülle nutzt `/courts`, das wie
  heute die Paarung des laufenden Spiels enthält; die eingebetteten Layouts sind bestehende freie
  Anzeige-Routen. **Keine neue Exposition**, aber auch keine Verringerung. Kein Geburtsjahr.
- **Abhängigkeiten:** keine neue Cargo-/npm-Abhängigkeit. Wake-Lock ist Browser-API
  (`navigator.wakeLock`), nur im Secure Context vorhanden: Cloud (https) und LAN-TLS 8443
  (ADR 0047). Im LAN über `http://…:8088` ist die API nicht vorhanden; dort gilt wie beim
  Zähl-Tablet die Geräteeinstellung „Bildschirm an lassen" bzw. Fully Kiosk (`docs/tablet.md`).

## Verhalten im Detail

### Tafel-Layout (`tafel.html`)

- **Betriebsarten** wie `monitor.html`: `fixed` (Aufruf über die Hülle oder direkt) und `device`
  (`?device=…`, nach Umleitung durch die Zuweisung). Im Gerätemodus liest die Tafel — wie
  `monitor.html` im Gerätemodus — `monitor/state?device=…` im Sekundentakt (Heartbeat,
  Online-Fenster 20 s, Fernbefehle „Identifizieren"/„Neu laden"). Der Host liefert für
  `CourtTafel` den **vollen Feld-Stand plus `redirectTo = /court/{id}/tafel`**; die Tafel
  vergleicht den Pfad, bleibt und rendert aus derselben Antwort. Fehlt `redirectTo` (Zuweisung
  geändert), geht sie nach drei Polls (Entprellung wie `overview.html`) zurück auf `/monitor`.
- **Datenweg** wie `monitor.html`: `…/court/{id}/state` als einzige Datenquelle, WS-Nudge über
  `monitor-ws?court={id}`, `kanalIstTot`-Erkennung, Fallback-Poll, `seq`-Guard gegen
  Rückwärtssprünge, ETag/304. Dieser Verbindungsblock wird **unverändert** übernommen.
- **Seiten und Aufschlag** (reine Funktion `tafelSeiten(sets, courtState, spiegel)` →
  `{ links: {punkte, saetze, aufschlag}, rechts: {…}, leer: bool }`):
  - Seiten aus `courtState.teamOnSide` (`a: 'left'|'right'`). Ohne `teamOnSide` (Seitenwahl noch
    nicht abgeschlossen) oder ohne `courtState` (kein Tablet zählt, Papierzettel): **Team 1
    links, kein Aufschlag-Punkt**.
  - Aufschlag aus `courtState.serving.team`; `null` (außerhalb des laufenden Spiels) → kein Punkt.
  - `spiegel = true` tauscht links und rechts **nach** der Seitenbestimmung, wirkt also auch ohne
    `teamOnSide`.
  - Satzstand = gewonnene Sätze je Team aus den abgeschlossenen Einträgen von `match.sets`
    (Satzsieger-Regel wie `monitor.html`, 21 mit 2 Vorsprung bzw. 30).
  - **Laufend:** letzter Eintrag von `sets` ist der laufende Satz → groß.
  - **`finished`:** ein nachgestellter `0:0`-Geistersatz wird gestrichen; der letzte gespielte
    Satz steht groß **und** zählt im Satzstand. Der Endstand bleibt stehen, bis BTP das Feld räumt.
  - **`retired`:** der unvollständige letzte Satz steht groß, zählt aber **nicht** im Satzstand.
  - **Kein Spiel** (`match` fehlt): `leer = true`, die Tafel zeigt groß die Feldbezeichnung
    (`courtLabel`; im Relay ohne verbundenen Host ist das Label leer → Fallback CourtID aus dem
    Pfad).
- **Spiegeln** kommt als Query `?spiegel=1` von der Hülle. Im Gerätemodus wirkt `spiegel` nicht
  und löst keine Redirect-Schleife aus: `tafel.html` vergleicht beim Umleitungs-Check nur
  `location.pathname !== dest` — die Query fällt bei diesem Vergleich ohnehin weg, ein
  gesondertes Streichen von `spiegel` ist nicht nötig (Befund ADR 0049).
- **Verbindung weg** (Host/Relay nicht erreichbar): Offline-Marke wie `monitor.html`, letzter
  Stand bleibt sichtbar.
- **Layout:** Quer- und Hochformat, Ziffern nebeneinander, Ziffernhöhe ≥ 35 vmin; alles in
  `vmin`/`vw`/`vh`. Kein Zahnrad, kein Wake-Lock (TV-Seite).

### Anzeige-Hülle (`anzeige.html`)

- **Adresse** `/anzeige?layout=<tafel|feld|uebersicht|vorbereitung>&court=<CourtID>`.
  Pfadbau ausschließlich über die Allowlist in `anzeigeZiel.mjs`:
  `tafel → court/{id}/tafel`, `feld → court/{id}/display`, `uebersicht → info/overview`,
  `vorbereitung → info/preparation`. Unbekanntes `layout` → `tafel`; `court` muss eine ganze
  Zahl sein, sonst öffnet das Menü mit der Feldwahl (ohne PIN — es wird noch nichts angezeigt).
  Freier Text aus der Adresse landet nie im iframe-`src` (security-reviewer bei der Umsetzung).
- **iframe** `position: fixed; inset: 0`, `src = BASE + pfad` (+ `?spiegel=1` bei `tafel`, wenn
  gemerkt). Die eingebetteten Seiten laufen ohne `?device` und leiten daher nie um.
- **Zahnrad** klein, halbtransparent in einer Ecke; Tipp → PIN-Pad (Kopie aus `tablet.html`,
  `__TABLET_PIN__`, Cloud immer `0000` wie beim Tablet) → Menü:
  1. Anzeige wählen: Zähltafel · Feld-Monitor · Hallen-Übersicht · In Vorbereitung.
  2. Feld wechseln (Liste aus `/courts`; nur bei `tafel`/`feld` sichtbar).
  3. Seiten spiegeln (nur bei `tafel`; gemerkt je Gerät in `localStorage`).
  4. Zum Zählen wechseln → vorher `/courts` abfragen; ist das Feld `occupied`, erscheint ein
     Warnhinweis mit Bestätigung („Auf diesem Feld zählt bereits ein Gerät …"), sonst direkt
     `court/{id}`. Die Relay-Feldliste liefert `occupied` seit v0.9.275; fehlt das Feld
     (älterer Relay), gibt es keine Warnung.
  5. Neu laden.
  6. Vollbild ein/aus.
  7. Schließen.
- Layout und Feld stehen in der Adresse (`history.replaceState`) und in `localStorage`, damit
  ein Reload dieselbe Anzeige bringt.
- **Wake-Lock:** `navigator.wakeLock.request('screen')` nach der ersten Berührung, erneut bei
  `visibilitychange`; fehlt die API (LAN-http), passiert still nichts.

### Einstiege am Zähl-Tablet (`tablet.html`)

- `#settings-main-view`: neuer Eintrag „Anzeige (nur Spielstand)" →
  `BASE_PATH + '/anzeige?layout=tafel&court=' + COURT_ID`.
- `#occupied-overlay`: zweiter, neutral gestalteter Knopf „Nur Spielstand anzeigen" **vor**
  „Court übernehmen", ohne PIN (die Seite ist wie alle Anzeige-Routen frei erreichbar; das
  Zählen bleibt durch das Overlay geschützt).

### Zuweisungsziel „Zähltafel – Feld X"

- `MonitorTarget::CourtTafel { court_id }` mit Serde-Tag `court_tafel`, JSON
  `{"kind":"court_tafel","court_id":3}`.
- `redirect_path()` → `Some("/court/3/tafel")`; `court_id()` → `Some(3)`, damit
  `build_device_list` Feldname und Halle liefert und das Panel das Gerät wie ein Feld-Gerät
  einsortiert. Ein **altes** Relay bekommt die CourtID der Tafel nie zu sehen: Es lehnt den
  gesamten Zuweisungs-Upload (`assignments` + `targets` in einem Body) mit unbekanntem `kind`
  ab (422) — akzeptiert, weil das Relay beim Merge vor dem App-Tag deployt.
- LAN: die Umleitung greift generisch (`server.rs` behandelt nur `Court` gesondert). Relay: die
  explizite Allowlist der Umleitung wird um `CourtTafel` erweitert.
- Panel: Optionsgruppe „Zähltafel" mit einer Option je Feld, Schlüssel `tafel:<id>`.

## Akzeptanzkriterien

Tafel-Layout:
- [ ] `GET /court/{id}/tafel` (LAN 8088 und 8443) und `GET /{ns}/court/{id}/tafel` (Relay)
      liefern 200, `Cache-Control: no-store`, und keinen unersetzten `__`-Platzhalter.
- [ ] Bei laufendem Spiel zeigt die Tafel die beiden Punktzahlen des letzten Satzes groß, den
      Satzstand klein und den Aufschlag-Punkt auf der Seite des aufschlagenden Teams.
- [ ] Nach einem Seitenwechsel am Zähl-Tablet tauschen die Zahlen auf der Tafel die Seiten.
- [ ] Mit `?spiegel=1` (fest) sind links und rechts vertauscht; im Gerätemodus wirkt `spiegel`
      nicht und löst keine Redirect-Schleife aus.
- [ ] Ohne `teamOnSide` oder ohne `courtState` steht Team 1 links und es gibt keinen
      Aufschlag-Punkt.
- [ ] Bei `finished` verschwindet ein nachgestellter `0:0`-Satz, der letzte gespielte Satz steht
      groß und zählt im Satzstand; bei `retired` zählt der unvollständige Satz nicht.
- [ ] Wird das Feld in BTP geräumt, zeigt die Tafel binnen 3 s groß die Feldbezeichnung.
- [ ] Fällt Host bzw. Relay aus, bleibt der letzte Stand mit Offline-Marke stehen; nach Rückkehr
      läuft die Anzeige ohne Neuladen weiter (auch nach Relay-Neustart mit `seq`-Sprung).
- [ ] Ein Punkt am Zähl-Tablet erscheint auf der Tafel per WS-Nudge, im LAN unter 1 s.
- [ ] Ziffernhöhe ≥ 35 vmin in Quer- und Hochformat (10-Zoll-Tablet).
- [ ] Im Gerätemodus meldet das Panel den TV binnen 20 s als online; „Identifizieren" zeigt Code
      und Feld; „Neu laden" lädt einmal; eine Umzuweisung auf Feld-Monitor oder Übersicht greift
      beim nächsten Poll.

Anzeige-Hülle:
- [ ] `GET /anzeige` (LAN, Relay) liefert 200 ohne unersetzten Platzhalter.
- [ ] `layout=tafel&court=3` bettet `court/3/tafel` ein; `layout=feld` → `court/3/display`;
      `uebersicht` → `info/overview`; `vorbereitung` → `info/preparation`; jedes andere `layout`
      → Zähltafel; `court=abc` → Menü mit Feldwahl. Kein anderer Pfad ist über die Adresse
      erreichbar (Testfälle mit `../`, absoluten URLs, `javascript:`).
- [ ] Zahnrad → falsche PIN öffnet nichts; richtige PIN öffnet das Menü mit den sieben Punkten,
      „Feld wechseln" und „Seiten spiegeln" nur bei Feld-Layouts.
- [ ] „Seiten spiegeln" wirkt sofort und überlebt Neuladen und App-Neustart (Gerät).
- [ ] „Zum Zählen wechseln" auf einem belegten Feld zeigt eine Warnung; erst die Bestätigung
      öffnet `court/{id}`. Auf einem freien Feld öffnet es direkt.
- [ ] Hülle und Tafel öffnen nie `/ws`; das zählende Tablet bleibt Slot-Halter (Server-Log ohne
      `identify` von der Hülle).
- [ ] Im Secure Context (Cloud, LAN-TLS) bleibt das Display nach 10 Minuten ohne Berührung an.
- [ ] Auf iPad (Safari) und Android (Chrome) füllt das iframe den Bildschirm ohne Scrollbalken,
      in beiden Ausrichtungen.

Zähl-Tablet:
- [ ] Zahnrad-Menü hat „Anzeige (nur Spielstand)"; das Belegt-Overlay hat „Nur Spielstand
      anzeigen" vor „Court übernehmen". Beide öffnen die Hülle mit Layout Zähltafel und dem
      aktuellen Feld.
- [ ] Tablet B öffnet über das Overlay die Tafel; Tablet A zählt unterbrechungsfrei weiter.

Zuweisung:
- [ ] Panel bietet „Zähltafel – Feld X" je Feld; das Gerät erscheint mit Feld und Halle.
- [ ] `monitor-assignments.json` mit `court_tafel` wird gelesen und geschrieben (Roundtrip).
- [ ] LAN und Relay liefern für ein so zugewiesenes Gerät `redirectTo == "/court/{id}/tafel"`.
- [ ] `docs/cloud-relay.md` nennt die Deploy-Reihenfolge (Relay vor App-Tag) und das
      422-Verhalten eines alten Relays.

## Tests

- **Rust (`relay-proto`)**: Serde-Roundtrip `court_tafel`; `redirect_path()`; `court_id()`;
  alte JSON-Formen (`court`, `info_overview`, …) bleiben lesbar.
- **Rust (`src-tauri`)**: Router-Tests `/court/{id}/tafel` und `/anzeige` (200, `no-store`, keine
  `__`-Reste, PIN-Filter wie `court_page`); `read_assignments`/`write_assignments` mit
  `court_tafel`; Geräte-State liefert `redirectTo` für `CourtTafel`; `build_device_list` liefert
  Feld/Halle für `CourtTafel`.
- **Rust (`relay`)**: Routen `/{ns}/court/{id}/tafel` und `/{ns}/anzeige` (`__BASE__` =
  `/bts-relay/{ns}/`, PIN leer); Umleitung `CourtTafel` → `redirectTo`; Label-Fallback ohne Host.
- **JS**: `scripts/test-tafel-seiten.mjs` (laufend, Seitenwechsel, spiegel, ohne `teamOnSide`,
  ohne `courtState`, `serving` null, `finished` mit Geistersatz, `retired`, kein Spiel);
  `scripts/test-anzeige-ziel.mjs` (vier Layouts, Unbekanntes, `court` ungültig, Einschleusungen).
  Beide Skripte hängen ausschließlich in `.github/workflows/ci.yml` (es gibt keine
  `package.json`-Testskripte, anders als der ursprüngliche Plan hier annahm).
- **Manuell (Feldtest)**: iPad + Android-Tablet in LAN-http, LAN-TLS und Cloud; Pi-TV per
  Zuweisung; Szenario „Tablet B über Overlay zur Tafel, Tablet A zählt weiter"; Hochformat.
- `cargo test --workspace` grün, `cargo clippy --workspace --all-targets -- -D warnings` sauber,
  `cargo fmt --check`, `npm run build` fehlerfrei.

## Risiken & Rollback

- **iOS Safari und iframes**: Safari kann iframes an den Inhalt statt an den Rahmen anpassen.
  Alle eingebetteten Seiten sind auf Bildschirmgröße gebaut, das sollte tragen
  (Wrapper mit `width:1px; min-width:100%` als Gegenmittel eingebaut); der iPad-Feldtest
  ist Abnahmekriterium. Rückfall, falls es scheitert: `anzeige.html` navigiert statt einzubetten
  (dann ohne Wake-Lock und ohne Zahnrad auf den Fremd-Layouts) — für die Zähltafel selbst wäre
  ein eigenes Menü direkt in `tafel.html` der Ausweg.
- **Downgrade** verwirft `court_tafel`-Zuweisungen still (Kopplungsseite statt Tafel); Feld-,
  Info- und Kombi-Zuweisungen bleiben. Config bleibt lesbar.
- **Altes Relay** weist den ganzen Zuweisungs-Upload mit 422 ab; nur bei Relay-Rollback relevant.
- **Zwei Geräte am Feld**: Die Hülle macht „Zum Zählen wechseln" zum Ein-Tipp-Weg. Bei totem
  Tablet A claimt B still (Slot-Halter-Modell, ADR 0017). Die Belegt-Warnung vor dem Wechsel
  mildert das, verhindert es nicht — bewusst, denn genau dieser Weg ist die Recovery bei einem
  ausgefallenen Tablet.
- **Wake-Lock im LAN-http** fehlt technisch; ohne Geräteeinstellung geht das Display aus.
- Rollback: ältere Version installierbar, keine Config-Migration nötig.

## Offene Fragen / Annahmen

- Annahme: Die Klapp-Tafel-Optik ist reines CSS (Ziffern auf dunklen Kacheln); es wird keine
  Animation verlangt.
- Annahme: Die PIN der Hülle ist dieselbe wie die des Tablets; ein eigener Wert ist nicht nötig.
- Annahme: Ein Schiedsrichter, der die PIN nicht kennt, kommt über die Feldwahl-Seite (`/felder`)
  oder den Browser zurück; das Menü bleibt bewusst komplett hinter der PIN (Schutz vor Spielern).
- Offen (Feldtest): ob 35 vmin auf 8-Zoll-Tablets aus 5 m reicht.

## Betroffene Doku-Dateien

- `docs/court-monitor.md` — Tafel-Layout, Betriebsarten, Zuweisungsziel „Zähltafel".
- `docs/tablet.md` — Menüeintrag, Overlay-Knopf, Anzeige-Hülle (Bedienung, PIN, Wake-Lock-Grenze).
- `docs/cloud-relay.md` — neue Routen, `court_tafel` auf der Wire-Ebene, Deploy-Reihenfolge.
- `docs/multi-hall.md` — ein Absatz: die Slave-Monitor-Brücke liefert die Tafel nicht;
  ferne Halle nutzt die Cloud-Adresse des Masters.
- `docs/changelog.md`, `docs/roadmap.md`, diese Spec, ADR 0055.
- `CLAUDE.md`-Tabelle: neue Zeile „Zähltafel + Anzeige-Hülle" mit den Code-Pfaden oben.

## Umsetzungs-Hinweise

Zwei PRs, damit die Wire-Änderung getrennt vom reinen Asset reist:

**PR 1 — Tafel + Zuweisungsziel** (relay-proto, relay, src-tauri, Panel):
1. `src/io/tafelSeiten.mjs` + Test (TDD, alle Fälle aus „Verhalten im Detail").
2. `tafel.html` (Verbindungsblock und `checkReassignment` aus `monitor.html`/`overview.html`
   übernehmen, Inline-Kopie von `tafelSeiten`).
3. `assets.rs` + `server.rs` Route/Render + Tests.
4. `relay-proto` `CourtTafel` + Tests; `relay/` Route, Render, Allowlist + Tests;
   `CourtMonitorPanel.tsx` Option.
5. Doku (`court-monitor.md`, `cloud-relay.md`, `multi-hall.md`), Version bump, Changelog.

**PR 2 — Hülle + Tablet-Einstiege** (src-tauri, relay-Route, tablet.html):
1. `src/io/anzeigeZiel.mjs` + Test (TDD, inkl. Einschleusungsfälle).
2. `anzeige.html` (PIN-Pad aus `tablet.html`, Menü, iframe, Wake-Lock, Belegt-Warnung).
3. Routen LAN + Relay + Tests.
4. `tablet.html` Menüeintrag + Overlay-Knopf.
5. Doku (`tablet.md`, `court-monitor.md`), Version bump, Changelog.

Review: `code-reviewer` nach jedem Schritt; `security-reviewer` für PR 2 Schritt 1–3
(Query → iframe-`src`). Version in `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`,
`package.json` gemeinsam bumpen — die Nummer erst beim Merge festlegen (parallele PRs).
Relay-Deploy läuft beim Merge automatisch; der App-Tag folgt danach (Admin).
