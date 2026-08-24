# Das Tablet erkennt eine veraltete Fassung — Spezifikation

> Status: **umgesetzt 2026-08-24** (v0.9.266 Abgleich, v0.9.268 Fernbefehl).
> Quelle: Feldtest Köpi-Cup 21./22.08.2026 — Tablets „hingen", bis der
> Browser-Cache geleert wurde. Betroffene Crates: `src-tauri`, `relay`,
> `relay-proto`.
> ADR: keiner nötig (die eine Technik-Entscheidung — Zähler statt
> Broadcast-Kanal — ist unten begründet und hat keine Alternative mit
> vergleichbarer Tragweite).

## Kontext / Problem

Ein Turnier-Tablet läuft tagelang mit **derselben geladenen Seite**. Wird
mitten im Turnier ein Update ausgerollt — beim Relay passiert das bei jedem
main-Merge automatisch —, erreicht die neue Fassung das Gerät nur über ein
Neuladen. Von selbst passiert das nie: Die Seite läuft, der Browser hat keinen
Anlass, sie neu zu holen.

Der Auslöser war ein Feldtest-Befund, der zunächst falsch gedeutet wurde. Der
Turnierleiter berichtete, einzelne Tablets hätten Punkte übertragen, aber das
Spiel nicht abschließen können; behoben war es erst nach dem **Leeren der
Browserdaten** — Tab schließen, Browser-Neustart und selbst ein
Betriebssystem-Neustart halfen nicht.

Die naheliegende Erklärung (HTTP-Cache) ist **nachweislich falsch**:
`Cache-Control: no-store` steht seit dem ersten Tablet-Commit, es gibt keinen
Service Worker, und alle Seiten sind selbstenthaltend — kein einziges
`<script src>` oder `<link href>`. Es gibt schlicht nichts, was getrennt
veralten könnte.

Was „Browserdaten löschen" **auch** entfernt, ist der `localStorage`. Dort lag
das hängende `pendingResult`: Vor v0.9.254 wurde ein dauerhaft abgelehntes
Ergebnis endlos wiederholt, mit der beruhigenden Meldung „wird automatisch
wiederholt, bis es ankommt". Ein Neustart half nicht, weil `localStorage` ihn
übersteht. **Dieser Teil ist mit v0.9.254 erledigt** und nicht Gegenstand
dieser Spec.

Übrig bleibt das echte, allgemeine Problem: **Eine geladene Seite weiß nicht,
dass sie veraltet ist.**

## Zielbild & Erfolgskriterien

1. Ein Tablet, dessen Seite nicht mehr der ausgelieferten entspricht, **merkt
   das von selbst** — ohne Zutun und ohne dass jemand durch die Halle läuft.
2. Steht **kein Spiel** auf dem Feld, lädt es sich **sofort selbst neu**. Das
   trifft die meisten Geräte zwischen zwei Spielen.
3. Läuft ein Spiel, springt der Bildschirm **nicht**. Es erscheint nur ein
   Hinweis mit einem Knopf.
4. Die Turnierleitung kann zusätzlich **alle Tablets auf Befehl** neu laden
   lassen, wenn sie nicht auf den stillen Weg warten will.
5. Ein Neuladen holt wirklich die neue Fassung — der Browser darf die Antwort
   nicht aus seinem Zwischenspeicher bedienen.
6. Kein Gerät gerät in eine Neulade-Schleife.

## Nicht-Ziele

- **Versionierte Dateinamen / Cache-Busting an Unterdateien.** Es gibt keine
  Unterdateien; alle Seiten sind selbstenthaltend.
- **Automatisches Neuladen mitten im Spiel.** Nur der ausdrückliche Fernbefehl
  darf das; der stille Abgleich wartet.
- **Zustellgarantie für den Fernbefehl.** Er erreicht, wen er erreicht (siehe
  „Grenzen").
- **Monitore und TL-Sicht.** Beide holen ihren Inhalt ohnehin laufend neu; der
  Abgleich ist ein Tablet-Thema.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates:** `relay-proto` (`seiten_marke`, `ServerMsg::Pong.marke`,
  `ServerMsg::Reload`, `HostFrame::ReloadTablets`, `TlAction::ReloadTablets`),
  `src-tauri` (`tablet/state.rs`, `tablet/server.rs`, `tablet/relay_client.rs`,
  `tablet/tl.rs`, `assets/tablet.html`, `assets/tl.html`), `relay`.
- **Architekturregeln:** **R3** — beide Verbindungswege sind bedient, mit
  jeweils **eigener** Marke (siehe unten). **R4** bleibt unberührt: Der
  Fernbefehl gilt einem Namespace, nicht einem Feld. **R2**: BTP kennt weder
  Marke noch Befehl; nichts davon reist Richtung BTP.
- **Konfiguration:** **keine neuen Felder.** Nichts ist einstellbar — es gibt
  nichts sinnvoll zu entscheiden.
- **Datenschutz:** Marke ist ein Hash über HTML-Text, der Befehl trägt keine
  Nutzlast. Kein Personenbezug an irgendeiner Stelle.
- **Abwärtskompatibilität:** Jedes neue Feld trägt `#[serde(default)]`. Eine
  alte Seite an einem neuen Server bekommt eine Marke, die sie nicht liest.
  Eine neue Seite an einem alten Server bekommt **keine** Marke — und tut
  dann nichts (siehe A3).

## Lösung

### Teil 1 — Stiller Abgleich (v0.9.266)

Jede ausgelieferte Seite trägt einen **Fingerabdruck**: Beim Ausliefern wird
der Platzhalter `__SEITEN_MARKE__` durch `relay_proto::seiten_marke(…)`
ersetzt. Das ohnehin laufende Lebenszeichen (`ServerMsg::Pong`) nennt den, den
der Server **jetzt** hat. Weichen beide ab, ist die geladene Seite alt.

**Warum ein Fingerabdruck und keine Versionsnummer:** Die Seite steckt in zwei
Binärdateien — `bts-light` und `bts-relay` —, und die tragen verschiedene
Versionen. Ein Versionsvergleich meldete im Cloud-Betrieb dauernd „veraltet",
obwohl die Seite dieselbe ist.

**Warum über das *unersetzte* `TABLET_HTML`:** Sonst trüge jedes Feld eine
eigene Marke (der Platzhalter ist Teil des Textes) und keine passte je zur
anderen.

### Teil 2 — Fernbefehl (v0.9.268)

Die Turnierleitung hat in der Kopfzeile den Knopf **„⟳ Tablets"**. Nach einer
Rückfrage laden **alle** Zähltablets neu — auch die mit laufendem Spiel. Das
ist der Unterschied zum stillen Weg: Hier hat jemand bewusst entschieden.

**Kein Broadcast-Kanal, sondern ein Zähler.** `TabletState.tablet_reload_gen`
ist ein `AtomicU64`. Jede Tablet-Verbindung merkt sich seinen Stand **beim
Verbinden** und sieht im ohnehin laufenden 2-Sekunden-Takt nach, ob er sich
erhöht hat; der Relay-Client tut dasselbe in seinem Sweep. Das kostet keine
Verdrahtung in zwei Binärdateien und verzögert um höchstens einen Takt.

**Das Merken beim Verbinden ist der Kern des Entwurfs.** Würde bei 0
begonnen, führte ein Tablet, das sich nach dem Befehl frisch verbindet, ihn
nachträglich aus — obwohl es die neue Seite längst geladen hat — und liefe
beim nächsten Verbinden wieder hinein: eine Neulade-Schleife. Ebenso schickte
ein Relay-Client ohne Startwert bei **jedem Reconnect nach einem Netzwackler**
den letzten Befehl erneut, und alle Cloud-Tablets lüden mitten im Spiel neu.

Gekapselt ist beides in `ReloadWacht` (`tablet/state.rs`) — merken beim
Verbinden, melden genau einmal. Ein eigener Typ, weil beide Schleifen dieselben
zwei Fehler machen könnten und der Entwurf nur an dieser Stelle kippt.

**Die Lebensdauer des Wächters ist an beiden Enden verschieden** — Absicht,
und ein Review-Fund:

- **Tablet-Socket (LAN):** je Verbindung. „Verbunden" heißt hier „Seite frisch
  geladen"; ein Befehl von davor gilt zu Recht nicht mehr.
- **Relay-Client (Cloud):** über **alle** Sitzungen hinweg, angelegt in `run()`
  und als `&mut` durch `serve()` gereicht. „Verbunden" heißt hier nur „der
  **Host** hat wieder Leitung" — die Tablets dahinter sind dabei gerade *nicht*
  frisch. Je Sitzung gesetzt ginge ein Befehl, der in einen Abriss fällt
  (Backoff bis 30 s), still verloren, während die Turnierleitung „Die Tablets
  laden neu" gemeldet bekäme. So wird er beim Wiederverbinden nachgeholt — und
  ein Netzwackler ohne Befehl löst trotzdem nichts aus.

**Jeder Server setzt seine eigene Marke.** Der Host schickt
`HostFrame::ReloadTablets` **ohne Nutzlast** an den Relay; die Marke füllt der
Relay aus **seiner** eingebetteten Seite. Die Cloud-Tablets haben ihre Seite
von ihm geladen, nicht vom Turnier-PC — käme dessen Marke an, hielte sich jede
Seite sofort wieder für veraltet.

### Neu geladen wird mit der Marke in der Adresse

`neuLaden(marke)` setzt `?v=<marke>` und ruft `location.replace`. Ein
schlichtes `location.reload()` kann aus dem Zwischenspeicher bedient werden;
eine andere URL hat dort keinen Eintrag. Fehlt die Marke, tritt ein
Zeitstempel an ihre Stelle — damit bleibt auch dann jeder Versuch eine neue
Adresse.

## Akzeptanzkriterien

| # | Fall | Erwartung |
|---|---|---|
| A1 | Marke im Pong = eigene Marke | nichts passiert, kein Neuzeichnen |
| A2 | Marke weicht ab, **kein** Spiel, kein offenes Ergebnis | Seite lädt sofort neu |
| A3 | Marke weicht ab, Spiel läuft | **kein** Neuladen, Hinweis mit „Jetzt laden" erscheint |
| A4 | Marke weicht ab, Ergebnis noch in der Übermittlung | **kein** Neuladen |
| A5 | Server schickt gar keine Marke (alter Stand) | nichts passiert |
| A6 | Abweichung bereits gemerkt | kein zweiter Reload |
| A7 | Neuladen | Ziel-URL trägt `?v=<marke>`, Pfad bleibt |
| B1 | Fernbefehl, Spiel läuft | lädt **trotzdem** neu |
| B2 | Fernbefehl ohne Marke | lädt trotzdem (Zeitstempel-Adresse) |
| B3 | Fernbefehl im Cloud-Betrieb | **jedes** Tablet des Namespace bekommt ihn, mit der **Relay**-Marke |
| B4 | Rückfrage abgelehnt | kein Befehl geht raus |
| B5 | zweiter bewusster Druck | neue Vorgangs-Kennung → wird ausgeführt, nicht als Doppeltipp verworfen |
| B6 | Turnier-PC kennt den Befehl nicht (`can_reload_tablets` fehlt) | Knopf bleibt verborgen |
| B7 | Tablet verbindet sich nach dem Befehl neu | **kein** nachträgliches Neuladen |
| B8 | Fernbefehl, Gerät **ohne** `localStorage`, Spiel läuft | **kein** Neuladen, nur der Hinweis |
| B9 | Befehl fällt in einen Relay-Abriss | wird beim Wiederverbinden **nachgeholt** |

## Grenzen

- **Der Fernbefehl erreicht nur Geräte, deren geladene Seite ihn schon kennt.**
  Ein Tablet auf einem älteren Stand verwirft ihn still. Dort hilft nur der
  stille Abgleich — oder eine Hand am Bildschirm. Das ist kein Mangel des
  Entwurfs, sondern liegt in der Natur der Sache: Ein Befehl, den die alte
  Seite nicht versteht, kann sie nicht ausführen.
- **Ein Gerät ohne nutzbaren `localStorage`** (Kiosk mit gesperrtem Speicher,
  privater Modus) kann seinen Stand nicht sichern — `persistState()` schlägt
  dort still fehl, und die Geräte-Kennung bleibt leer, sodass der Server es
  nach einem Neuladen nicht wiedererkennt („Feld belegt"). Auf so einem Gerät
  verhält sich der Fernbefehl **wie der stille Abgleich**: Steht ein Spiel oder
  ein unübertragenes Ergebnis an, springt es nicht, sondern zeigt nur den
  Hinweis. Nur so bleibt die Zusage „der Stand geht nicht verloren" wahr.
- Ein Sync-Neustart (Speichern der Einstellungen) legt den Relay-Client neu an;
  ein Befehl in genau diesem Augenblick erreicht die Cloud-Tablets nicht. Das
  Fenster ist klein und hat dieselbe Ursache wie andere Merker, die nur in der
  Übertragung leben.

## Tests

**Rust**
- `relay-proto` `die_namen_des_fernbefehls_stehen_fest` — nagelt die
  **wörtlichen** Wire-Namen fest (`"type":"reload"`, `"action":"reload_tablets"`)
  und dass eine leere Marke wegfällt. Ein Serde-Roundtrip allein reicht hier
  nicht: Er ist mit sich selbst einig, während das Tablet die umbenannte
  Nachricht still verwirft — und die Seiten vergleichen Zeichenketten. Gegen
  Sabotage geprüft (ein umbenanntes `rename` lässt ihn fallen). Dazu der
  `TlAction`-Listen-Roundtrip ohne Sammelzweig.
- `tablet/state.rs` `der_fernbefehl_erreicht_nur_wer_ihn_vorher_beobachtet_hat`
  — B7 und B9 über `ReloadWacht`: ein frisch angelegter Wächter holt nichts
  nach, ein bestehender bekommt den Befehl **genau einmal**, und mehrere
  Befehle während einer Unterbrechung sind ein einziger Nachholbedarf.
- `relay` `der_fernbefehl_erreicht_jedes_tablet_des_namespace` — B3, inklusive
  der Herkunft der Marke. Gegen Sabotage geprüft (`.take(1)` lässt ihn fallen).
- `tl.rs` `the_state_never_carries_personal_data_beyond_its_purpose` — das
  Merkmal `can_reload_tablets` steht in der erlaubten Feldliste.

**Seiten-Logik**: `scripts/test-tablet-version.mjs` (in der CI). Die Assets
haben keinen Build-Schritt und die Logik lebt nur inline im HTML — der Test
schneidet die Funktionen deshalb per Anker heraus und prüft ihre
Seiteneffekte. Deckt A1–A7 und B1–B6 ab. Verschiebt sich ein Anker, fällt der
Test aus („Abschnitt nicht gefunden") statt still durchzuwinken.

Nicht durch einen Test gedeckt: dass die beiden Schleifen ihren Wächter mit
der **richtigen Lebensdauer** anlegen — je Verbindung am Tablet-Socket, über
alle Sitzungen im Relay-Client. Das hängt an der Lebensdauer je einer lokalen
Variable und ist an beiden Stellen kommentiert.

## Doku

`docs/tablet.md` (Verhalten am Gerät) · `docs/turnierleitung-web.md`
(Bedienung des Knopfs) · `docs/cloud-relay.md` (Wire-Ebene) ·
`docs/changelog.md`.
