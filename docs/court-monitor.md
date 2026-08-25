# Court-Monitor — TV-Anzeige am Spielfeld

> **Status: umgesetzt.** v0.7.0 brachte die Anzeige, v0.8.0 die
> Geräte-Verwaltung (Zuweisung + Fernsteuerung aus dem Tool). Offen
> bleibt der 2-Felder-pro-TV-Modus → [roadmap.md](roadmap.md).

## Ziel

Pro Spielfeld ein TV (32"–55"), betrieben von einem **Raspberry Pi** im
Vollbild-Browser. Zwei Zustände, automatisch umgeschaltet:

- **Kein Spiel auf dem Feld** → **Werbung** (rotierende Bilder).
- **Spiel auf dem Feld** → **Match-Ansicht** (Layout „A — Geteilt").

Reine Anzeige (read-only) — der Monitor schreibt nie etwas zurück. Er
pollt im Sekundentakt einen `…/state`-Endpunkt.

## Layout „A — Geteilt"

Bildschirm waagerecht geteilt: oben Mannschaft 1, unten Mannschaft 2.

```
┌ FELD 3 ─────────────────── Herreneinzel ┐
│  [DE]  Anna Müller          ●            │
│                  davor 21    ▏ 11 ▕      │
│  ─────────────────────────────────────   │
│                  davor 18    ▏  7 ▕      │
│  [PL]  Hilde Kowalski                    │
└──────────────────── Gruppe 2 · Spiel 14 ┘
```

- **Kopfzeile:** Feldnummer + Disziplin (Herren-/Dameneinzel, Herren-/
  Damendoppel, Mixed).
- **Je Mannschaft (Bildschirmhälfte):** Landesflagge + Spielername(n) groß
  links; der **laufende Satzstand** ganz rechts am größten; abgeschlossene
  Sätze als kleinere Spalte daneben.
- **Doppel:** zwei Namen je Hälfte gestapelt, eine Flagge pro Spieler.
- **Aufschlag:** Der **Satzstand der aufschlagenden Mannschaft wird
  farblich hervorgehoben** (zusätzlich ein `●`-Marker am Spieler).
- **Fußzeile:** Runde + Spielnummer (je einzeln abschaltbar).
- Alles über `vh`/`vw`/`vmin` skaliert → füllt jeden TV 32"–55" ohne
  Anpassung.
- **Schriftgrößen** (Turnier-Feedback „Namen zu klein", zuletzt
  12.08.2026 „noch größer, Fläche füllen"): Nachname **15vmin**
  (Doppel **11.5**), Vorname **5vmin** (Doppel 4.4), laufender Satz
  **19vmin**, abgeschlossene Sätze 9.5vmin (Endstand 15). Lange Namen
  kürzen weiterhin per Ellipsis statt umzubrechen.
- **Knappe Ränder** (12.08.2026): schmale Kopf-/Fußleisten
  (`padding .9vh/.7vh`) und schmales Seitenpolster der Hälften
  (`0 1.8vw`) füllen möglichst viel der TV-Fläche; die zwei Namen je
  Hälfte im Doppel passen dadurch trotz größerer Schrift (bei ~44vh
  Hälften-Höhe brauchen sie rund 34vh).

Die Anzeige-Seite ist `src-tauri/assets/monitor.html` — eine
eigenständige HTML/CSS/JS-Datei, read-only Geschwister von `tablet.html`.

## Geräte-Modus & TV-Verwaltung

Monitore sind **generische Geräte**: Jeder Raspberry Pi öffnet *dieselbe*
Adresse (`…/monitor`). Pi-Monitore melden sich mit ihrer CPU-Seriennummer
(`device=pi-<serial>`); ohne `?device` vergibt sich die Seite beim ersten
Start eine eigene, dauerhafte Geräte-ID (im `localStorage`). Solange dem
Gerät kein Feld zugewiesen ist, zeigt der TV groß einen **Kopplungs-Code**
(die **letzten** vier alphanumerischen Zeichen der ID). Bewusst das Ende:
alle Pi-Serials beginnen mit demselben Präfix (`00000000…`), die ersten
vier Zeichen wären sonst für jeden Pi gleich („PI00") und nicht
unterscheidbar.

Im Tool führt die Seite **„Court-Monitore"** (Dashboard → Court-Monitore)
alle Geräte auf, die sich gemeldet haben:

- **Online-Status** je Gerät (grün, wenn der letzte Poll < 6 s her ist).
- **Feld-Zuweisung** per Dropdown — jederzeit umstellbar; der Monitor
  übernimmt das neue Feld beim nächsten Poll (~1 s im LAN, ≤ 3 s Cloud).
- **Identifizieren** — der Monitor blendet Code + Feld groß ein, damit
  man Gerät und TV zuordnen kann. Wirkt in **allen** Anzeigen — Einzelfeld
  (`monitor.html`), Court-Übersicht (`overview.html`) und Kombi
  (`combo.html`) (seit v0.9.93; davor nur Einzelfeld).
- **Neu laden** — der Monitor lädt seine Seite neu (falls er hängt). Seit
  v0.9.255 meldet eine Anzeige, die stillsteht, das von sich aus ins Log
  (`stillstand`, siehe [logging.md](logging.md)) — bleibt ein Bild stehen,
  lohnt vor dem Neuladen ein Blick dorthin: Die Zeile sagt, ob überhaupt noch
  etwas ankam oder ob die Seite es verworfen hat.

Die Zuweisungen liegen in `monitor-assignments.json` im
App-Config-Verzeichnis und überstehen einen bts-light-Neustart.
Fernbefehle reiten auf dem normalen `…/state`-Poll mit — es gibt keinen
zusätzlichen Verbindungsweg zum Pi, daher funktioniert die Steuerung in
LAN **und** Cloud. Jeder Befehl trägt eine je Gerät hochzählende `id`;
der Monitor führt ihn genau einmal aus (auch nach „Neu laden" kein
Endlos-Reload).

**Direkt-Variante:** Wer einen Monitor fest auf ein Feld nageln will,
nutzt weiterhin `…/court/<Feld>/display` — ohne Zuweisungs-Schritt.

## Niedrig-latente Anzeige (WS-Nudge)

Der Spielstand erscheint auf dem TV **nahezu sofort** (statt erst beim nächsten
Poll). Grundlage ist ein **WebSocket-„Nudge"** (ADR 0016): Ändert sich an einem
Feld etwas (Punkt gezählt, Aufruf/Pause, Feld geräumt), schickt der Server/Relay
an die Monitor-Clients ein winziges `{"court":<id>,"seq":<n>}` — **keine**
Score-Daten. Der Client löst daraufhin seinen **bestehenden** `…/state`- bzw.
`/health`-`fetch` aus. So bleibt der Poll-Endpunkt die **einzige** Datenquelle
(eine Serialisierung, ein Renderpfad) — kein Flackern, kein Rückwärtsspringen.

- **Routen:** `GET /monitor-ws?court={id}` (LAN) bzw. `GET /{ns}/monitor-ws?court={id}`
  (Cloud, durch nginx `/bts-relay/` wie die Tablet-WS). `court` weggelassen →
  Nudges **aller** Felder (Feld-Übersicht `overview.html`); gesetzt → nur dieses
  Feld (fester Court-Monitor). Geräte-zugewiesene Monitore abonnieren alle Felder
  und filtern clientseitig auf ihr aktuelles Feld.
- **`seq`** zählt je Feld monoton hoch und **beginnt bei der Uhrzeit** (seit
  v0.9.239), damit die Zahl einen Neustart von Turnier-PC oder Relay
  übersteht — sonst hielte eine Anzeige mit gemerktem Wert danach jeden
  neuen Stand für veraltet und bliebe stehen.
- **Seit v0.9.239 tragen auch die Voll-Antworten dieselbe Zahl** — in
  `/health` als eigene Karte `seqs` (CourtID → Zahl) **neben** der
  Feld-Liste, in `/court/{id}/state` als Feld `seq`; in LAN und Cloud
  gleich. Die Karte steht bewusst neben der Liste: Die Marke der Antwort
  hängt an der Liste, und eine Zahl darin wechselte bei jedem Anstoß —
  damit wäre die Bestätigung ohne Nutzdaten wirkungslos. Damit ordnet die Anzeige Push und Abruf zueinander: Ein **Push**
  gilt nur bei echt größerer Zahl (ein doppelter Anstoß löste sonst einen
  Abruf ohne neuen Inhalt aus), eine **Voll-Antwort** schon bei gleicher —
  sie darf denselben Stand berichtigen, etwa wenn in BTP ein Satzstand von
  Hand zurückgenommen wird. Fehlt die Zahl (älterer Absender), gilt der
  Stand immer. Die Regel steht als `src/io/monitorSeq.mjs` mit eigenem
  CI-Schritt; beide Anzeige-Seiten tragen eine Inline-Kopie.
- **Herzschlag alle 10 s** (seit v0.9.241, Spec `monitor-livestand-push` S6):
  Neben dem WS-Ping — der für JavaScript unsichtbar ist — schicken Host und
  Relay ein sichtbares `{"hb":<ms>}` **ohne** `court`-Feld, das ältere Seiten
  folgenlos verwerfen. Erst dadurch kann eine Anzeige „die Leitung lebt" von
  „es passiert gerade nichts" unterscheiden: In einer ruhigen Halle vergehen
  zwischen zwei Ballwechseln Minuten, und ein halbtoter Socket meldet
  stundenlang `OPEN`, ohne je etwas zu liefern.
- **Der Relay verabschiedet Anzeigen aktiv** (ebenfalls seit v0.9.241): Kann
  er eine Verbindung nicht eintragen (Host noch nicht da, oder Fan-out-Deckel
  erreicht), schließt er sie sofort statt sie still offen zu lassen — der
  Herzschlag hielte sie sonst für gesund, obwohl nie ein Anstoß käme. Ebenso
  beim Aufräumen eines Namespace, dessen Host weg ist. Die Anzeige fällt
  dadurch auf den Poll und in die Offline-Blende und verbindet sich über
  ihren Reconnect-Wächter neu, sobald der Turnier-PC wieder da ist.
- **Poll bleibt Fallback:** Der Sicherheits-Poll läuft **durchgehend** weiter
  (seit v0.9.241 nicht mehr pausierend) — im 250-ms-Takt, und nur bei
  **gesundem** Kanal und gesetztem Schalter im 4-s-Takt. Er hört **nie ganz**
  auf, denn an ihm hängt mehr als der Spielstand: das Lebenszeichen des
  Geräts (20-s-Fenster), seine Fernbefehle, `redirectTo`, die
  Geräte→Feld-Zuweisung und jede Änderung, die nur die Antwort-Revision hebt,
  ohne anzustoßen (Feld-Beschriftungen, Hallen-Zuordnung, Aufruf-Schwellen,
  Hallen-Farben). Gemessen wird gegen den **letzten tatsächlichen Abruf** —
  in einer regen Halle fällt dadurch kein zusätzlicher an. Gesund heißt: Socket
  offen, letztes Frame (Anstoß **oder** Herzschlag) keine 25 s her, letzter
  Abruf erfolgreich; ein **einziger** Fehlversuch schaltet sofort zurück.
  Bleibt der Herzschlag über 25 s aus, schließt die Anzeige den Socket aktiv,
  damit der Reconnect-Watchdog mit Backoff neu verbindet. Die Regel steht als
  `src/io/pushHealth.mjs` mit eigenem CI-Schritt; beide Anzeige-Seiten tragen
  eine Inline-Kopie. **Kein Regress** — fällt der Push aus, verhält sich die
  Anzeige wie zuvor, nur mit schnellerem Poll.
- **Schalter `push_fallback_slow`** (`config.json`, Abschnitt `court_monitor`;
  **Standard aus**): Erst er erlaubt den 4-s-Takt. Ohne ihn pollt eine frisch
  aktualisierte Installation exakt wie vorher — die Entlastung ist der
  eigentliche Gewinn der Spec und zugleich ihr größtes Risiko, deshalb bleibt
  sie eine bewusste Entscheidung. Der Schalter reist als `pushFallbackSlow`
  zur Anzeige — in `/court/{id}/state` in der `config`, in `/health` im
  `callTimer`-Umschlag (und damit in der Marke der Antwort, sonst käme das
  Umlegen erst mit der nächsten Feld-Änderung an). Er wirkt nach dem nächsten
  Abruf.
- **Auch die Feld-/Match-Zuweisung wird angestoßen** (seit v0.9.238, Spec
  `monitor-livestand-push` S3). Sie ist BTP-Snapshot-getrieben und damit kein
  Einzel-Ereignis wie ein gezählter Punkt; deshalb vergleicht der Turnier-PC
  bei jedem neuen BTP-Stand die Belegung je Feld — Match und Satzstand — mit
  der des Vorgängers und weckt **nur die Abweichungen**. Das deckt Zuweisung,
  Räumung, Feldwechsel und den in BTP von Hand eingetragenen Satzstand ab.
  Über die Cloud tun dasselbe die Relay-Arme `MatchAssigned`/`MatchCleared`,
  jeweils erst nachdem ihr Zwischenstand steht.
- **Pause, Behandlung und Aufschlag stoßen selbst an** (in der Cloud seit
  v0.9.245): Dieser Zustand erschien vorher nur, wenn zufällig ein gezählter
  Punkt hinterherkam — er hatte keinen eigenen Anstoß. Eine begonnene
  Behandlung wird damit sofort sichtbar statt erst beim nächsten
  Sicherheits-Abruf.
- **Die Übersicht wird einmal gerechnet, nicht je Anzeige** (im Hallennetz
  seit v0.9.236, in der Cloud seit v0.9.245): Zwanzig Fernseher fragen viermal
  je Sekunde nach; der Zustand entsteht trotzdem nur, wenn sich wirklich etwas
  geändert hat — und spätestens nach einer Viertelsekunde ohnehin einmal neu,
  falls eine Änderung übersehen wurde. Gemeldet wird an drei Stellen: bei
  jedem Anstoß, bei einer neuen Feldliste und bei einem neuen
  Monitor-Datensatz (Aufruf-Timer, Fallback-Schalter) — die beiden letzten
  stoßen nicht an und brauchen deshalb eine eigene Meldung.
- **Bestätigung „nichts Neues"** (im Hallennetz seit v0.9.236, in der Cloud
  seit v0.9.243): Hat sich seit dem letzten Abruf nichts geändert, antwortet
  der Server mit einer leeren Bestätigung statt mit dem vollen Stand. Die
  Anzeige zeigt einfach weiter, was sie hat. Im Leerlauf sind das über 99 %
  der Abrufe. Die Marke hängt am **ausgelieferten Inhalt**, nicht an der
  Ordnungszahl — sonst wechselte sie bei jedem Anstoß, auch bei einem ohne
  sichtbare Folge, und die Bestätigung wäre wirkungslos.
- **Schmaler Abruf je Feld** (seit v0.9.242, Spec `monitor-livestand-push` S7):
  `…/health?court=<CourtID>` liefert dieselbe Antwortform, aber nur dieses eine
  Feld — samt seiner Ordnungszahl und **ohne** die der Nachbarfelder. In LAN
  und Cloud gleich. Eine unbekannte, negative oder nicht-numerische Nummer
  liefert `courts: []` mit HTTP 200; die Antwort verrät also nicht, welche
  Felder es gibt. Ohne den Parameter ist die Antwort unverändert.
- **Grenzen:** Der Nudge ist reine Anzeige-Beschleunigung; er ändert keine
  Ownership und keine Zählung.

## Datenfluss

Der Monitor braucht **keinen neuen Datenweg** — alle Daten liegen schon
vor:

- Der LAN-Server bzw. der Relay kennt pro Feld das aktuelle Match
  (`MatchBrief`, seit v0.7.0 mit `discipline`, `matchNumber` und je
  Spieler `nationality`) und den Satzstand.
- Zählt ein Tablet das Feld, spiegelt es laufend seinen vollen
  Spielzustand (`court_state`) an den Server/Relay — darin stehen
  Aufschlag-Seite und Pause. Der Monitor liest diesen Zustand **rein
  lesend** mit.

`monitor.html` baut die Anzeige aus dem `…/state`-JSON
([`relay_proto::MonitorState`](../relay-proto/src/lib.rs)): Match-Info +
roher `court_state` + Konfiguration + Werbebild-Liste.

### Verhalten ohne `court_state` (kein zählendes Tablet)

| Wert            | Tablet zählt        | kein Tablet              |
|-----------------|---------------------|--------------------------|
| Satzstand       | live vom Tablet     | aus BTP                  |
| Aufschlag       | angezeigt           | nicht angezeigt          |
| Pausen-Timer    | angezeigt           | nicht angezeigt          |

### Score-Spiegel des Hosts (v0.9.200): Cloud-Anzeigen auch bei LAN-Tablets

Bis v0.9.199 kannte der Relay Satzstand und `court_state` **nur von
Cloud-Tablets** (`score_update`/`state_sync` über die Tablet-WS). Zählte ein
Tablet im LAN — der Normalfall im `LanAndCloud`-Mischbetrieb —, blieben
Cloud-Monitor, Cloud-Court-Anzeige und Cloud-Übersicht auf 0:0 stehen
(Turnier-Befund 13.08.2026, Zwei-Hallen-Turnier).

Seit v0.9.200 spiegelt der Host jeden Feld-Stand als
`HostFrame::ScoreUpdate` (Satzliste + opaker `court_state`) an den Relay,
auf zwei Wegen:

- **Nudge-getrieben** (niedrige Latenz): Der Relay-Client abonniert den
  **eigenen** Monitor-Nudge-Kanal (A1, „alle Felder" wie die LAN-Übersicht)
  und schickt bei jedem Signal den Stand des Felds.
- **2-s-Sweep** im Zuweisungs-Tick, **nach** `push_all_courts` (gleiche
  FIFO-Wire → der Relay kennt das Match, bevor der Spiegel eintrifft). Der
  Sweep fängt die nudge-losen Fälle ein: Reconnect/Relay-Neustart (leerer
  Cache dort), Court-Wechsel und BTP-Handeingaben ohne Tablet. Ein
  Zuweisungs-Push verwirft den Fingerabdruck des Felds, damit der Sweep
  einen zuvor vom Relay verworfenen Spiegel sicher wiederholt.

Beide Wege deduplizieren über einen Fingerabdruck je Feld (das zuletzt
gebaute Frame). Der Relay übernimmt den Stand in
`court_scores`/`court_state` mit dem Stale-Schutz des Tablet-Wegs — plus
zwei Spiegel-Regeln: **Leere Sätze überschreiben keinen vorhandenen
Live-Stand** (frisch ersetzter Turnier-PC ohne `live-scores.json` darf ein
zählendes Cloud-Tablet nicht auf 0:0 zurückwerfen), und ein `court_state`,
dessen **eingebettete** `match.matchId` nicht zum gemeldeten Match passt,
wird verworfen (wie `store_court_state`). Damit zeigen die Cloud-Anzeigen
auch Aufschlag und Pausen-Timer des LAN-Tablets; nur das `serving_team` im
`/{ns}/health` der Übersicht bleibt vorerst `null`.

## Sponsor-Leiste (kleine Werbung neben dem Turnierlogo)

Jedes Werbebild lässt sich im Setup mit dem Haken **„Leiste"**
(`set_court_ad_bar`) als Leisten-Sponsor markieren. Die Markierungen liegen in
`court-ads/court-ad-bar.json` (getrennt von den Labels, abwärtskompatibel).
`/info/ad/state` weist sie als `barAds` aus, dazu `hasLogo`; das Turnierlogo
kommt über den neuen Endpunkt **`/info/logo`** (aus `config.tournament_logo`).

Die markierten Bilder erscheinen — neben dem Turnierlogo — klein in der oberen
Leiste von **Feldübersicht**, **Vorbereitung**, **Court-Monitor** (nur im
laufenden Spiel, der Werbe-Leerlauf bleibt Vollbild) und **Tablet** (breite
Geräte). In der Regel 1–2 Bilder, kein Rotieren. Fehlt ein Motiv, entfernt
`onerror` es. Spec + Phasen (Cloud, badhub-Seiten):
[features/werbung-leisten.md](features/werbung-leisten.md).

## Hintergrundfarbe und Feldbezeichnung je Werbebild (seit v0.9.247)

Das **Leerlauf-Vollbild** (Werbung, solange kein Spiel auf dem Feld läuft) lag
bisher immer auf Schwarz. Jedes Bild bekommt jetzt im Setup zwei eigene
Einstellungen — direkt unter seinem Namen, mit Vorschau daneben:

- **Hintergrund**: eine frei wählbare Farbe, die die Fläche füllt, die das
  Bild bei `object-fit: contain` nicht abdeckt. Sponsorlogos, die für weißen
  Grund gemacht sind, stehen damit nicht mehr im schwarzen Kasten.
- **Feld zeigen**: die Feldbezeichnung erscheint klein in der oberen linken
  Ecke. Dafür wird das Bild auf 84 % verkleinert, damit ringsum ein Rand in
  der Hintergrundfarbe bleibt — so steht die Bezeichnung **immer** auf der
  reinen Farbe und nie auf dem Motiv. Ein randfüllendes 16:9-Motiv auf einem
  16:9-Fernseher ließe sonst keinen Platz.

Die **Schriftfarbe** ist nicht einstellbar: Der Host rechnet sie aus der
Hintergrundfarbe (relative Luminanz nach WCAG) und schickt sie mit. Es gibt
also keinen Weg, die Feldbezeichnung unlesbar zu konfigurieren (ADR 0041).

Weitere Regeln:

- Trägt **mindestens ein** Bild eines Feldes den Haken, bleibt die
  Feldbezeichnung über die ganze Rotation stehen und alle Bilder werden gleich
  verkleinert — sonst spränge die Bildgröße im Rotationstakt.
- Ist die Feldbezeichnung **leer** (im Cloud der Fall, solange dem Feld noch
  nie ein Spiel zugewiesen war), entfällt sie samt Verkleinerung: volle
  Bildfläche statt leerem Rand.
- Der Farbwechsel zwischen zwei Bildern blendet weich über.
- Die reine **Werbe-Anzeige** auf Info-Bildschirmen (`/info/ad`) übernimmt die
  Farbe, aber keine Feldbezeichnung — sie hängt an keinem Feld.
- Ohne Einstellung bleibt alles wie vorher: Schwarz, keine Bezeichnung.

Ablage: `court-ads/court-ad-style.json` — die **dritte** Seiten-Datei neben
Labels und Leisten-Markierung. Bewusst nicht in die Labels-Datei mit hinein:
deren Deserializer ist strikt, ein Formatwechsel löschte beim Auto-Update
still alle Anzeigenamen. Command: `set_court_ad_style` (validiert `#rrggbb`,
löst **keinen** badhub-Push aus — anders als der „Leiste"-Haken). Spec:
[features/werbung-hintergrund-und-feld.md](features/werbung-hintergrund-und-feld.md).

## Zwischenspeichern der Bilder (seit v0.9.225)

Werbebilder und Turnierlogo waren ausdrücklich vom Zwischenspeichern
ausgenommen (`Cache-Control: no-store`). Weil die Werbeanzeige ihr Motiv
alle `ad_interval_s` (Standard 10 s) wechselt, holte jedes Gerät dabei jedes
Mal die vollen Bilddaten — bei einem 1-MB-Motiv rund 360 MB je Stunde und
Anzeige, im Cloud-Betrieb über die Internetleitung.

Beide Routen geben jetzt in **beiden** Betriebsarten eine Kennung (`ETag`)
mit und dürfen zwischengespeichert werden. Nach Ablauf der Frist bestätigt
der Server ein unverändertes Bild mit `304` (rund 200 Byte) statt es erneut
zu senden.

| Bild | Kennung aus | Frist |
|---|---|---|
| Werbebild (LAN, `/ads/{datei}`) | Größe + Änderungszeit der Datei — ohne sie zu lesen | 5 Min |
| Werbebild (Cloud, `/{ns}/ads/{index}`) | Inhalt des hochgeladenen Bildes | **1 Min** |
| Turnierlogo (LAN, `/info/logo`) | Inhalt der Base64-Daten | 5 Min |
| Turnierlogo (Cloud, `/{ns}/info/logo`) | Inhalt des hochgeladenen Logos | 5 Min |

Vier Entscheidungen, die dahinterstehen:

- **Kein `immutable`.** Zwar vergibt der Upload eindeutige Namen
  (`ad-<ms>.<endung>`), aber das `court-ads/`-Verzeichnis liegt offen: Wer
  eine Datei von Hand hineinlegt und später ersetzt, bekäme sonst tagelang
  das alte Bild.
- **Cloud-Werbebilder nur eine Minute.** Im Hallennetz bindet der Dateiname
  die Adresse an genau ein Bild, in der Cloud ist die Adresse der
  Listen-**Index**. Löscht die Turnierleitung ein Werbebild, rücken alle
  folgenden Indizes auf — eine Anzeige zeigte dann bis zum Ablauf der Frist
  nicht bloß ein altes, sondern ein **falsches** Motiv. Bei Sponsoren ist
  das etwas anderes als „veraltet". Das Turnierlogo hat eine feste Adresse
  je Namespace und darf deshalb länger gelten.
- **Die Cloud-Kennung hängt am Inhalt, nicht am Upload.** Der Host lädt sein
  Monitor-Bündel bei jedem Verbindungsaufbau neu hoch; wären die Kennungen
  an den Upload gebunden, entwertete jeder WLAN-Wackler sämtliche
  Bild-Zwischenspeicher.
- **Auch die Logo-Kennung hängt am Inhalt** und nicht an `(Länge, MIME)`.
  Sie ist zugleich der Schlüssel des Dekodier-Zwischenstands: Zwei
  verschiedene Logos gleicher Base64-Länge und gleichen Typs hätten sonst
  auch frischen Anzeigen dauerhaft die alten Bytes geliefert.

`If-None-Match` wird nach RFC 9110 ausgewertet — als Liste, mit `*` und mit
schwachem Vergleich. Ein Zwischenspeicher auf dem Weg (nginx vor dem Relay)
darf eine Marke abschwächen; ein reiner Gleichheitstest wäre dann still
wirkungslos, und der ganze Gewinn wäre weg.

**Die Sponsor-Leiste wird bewusst bei jedem Durchlauf neu gebaut**, ohne
Abgleich auf Unverändertheit. Ein solcher Abgleich könnte nur die *Namen*
der Bilder vergleichen, nicht ihren Inhalt — ein ausgetauschtes Logo oder
Werbebild unter gleicher Adresse bliebe dann für immer stehen, weil das
`<img>` nie neu entsteht und der Browser die Adresse nie wieder anfragt.
Teuer ist der Neuaufbau nicht mehr: Mit Kennung und Cache-Frist kostet er
höchstens einmal je Frist eine Bestätigung von rund 200 Byte je Bild.

## Last am Turnier-PC (seit v0.9.223)

Jeder Monitor fragt im 250-ms-Takt nach — der WS-Anstoß senkt die
Latenz, nicht die Zahl der Abrufe (das „frisch"-Fenster von 1,2 s greift
bei realem Ballwechsel-Abstand selten). Bei zwanzig Anzeigen sind das
rund achtzig Abrufe pro Sekunde. Drei Dinge machen die inzwischen
billig:

- Die **Konfiguration** wird geteilt statt kopiert und nur **einmal** je
  Abruf gelesen (vorher zweimal, jedes Mal inklusive des Turnierlogos als
  Base64-Text von bis zu 2,7 MB).
- Die **Werbebild-Liste** kommt aus einem Zwischenstand und wird nur neu
  eingelesen, wenn sich der Ordner geändert hat (vorher ein
  Verzeichnis-Lesen je Abruf). Dasselbe gilt seit v0.9.225 für die
  „Leisten-Sponsor"-Markierungen (`court-ad-bar.json`).
- Der gespiegelte **Spielstand** (`courtState`) geht ohne den
  Verlaufsspeicher des Tabletts an die Anzeigen: `history` (bis zu 50
  Zwischenstände, jeder mit einer Vollkopie des Ballwechsel-Protokolls)
  und `rallyLog` fallen weg — spät im Match zweistellige Kilobyte je
  Abruf. Die Anzeigen lesen daraus ohnehin nur Aufschlag, Pause, Aufgabe
  und Startzeit; das Tablet selbst bekommt beim Wiederverbinden
  weiterhin den vollen Stand (sein Rückgängig-Gedächtnis).

## Antwortcache der Übersicht (seit v0.9.236)

Die Route `/health` liefert allen Übersichts-Anzeigen den Zustand **aller**
Felder. Berechnet wurde er bisher bei **jedem** Abruf neu — je Feld ein
Durchlauf durch alle Spiele plus ein Auswerten des Tablet-Stands. Bei
zwanzig Anzeigen im 250-ms-Takt waren das rund siebzig Berechnungen je
Sekunde (gemessen, siehe Spec).

Jetzt wird einmal gerechnet und das Ergebnis wiederverwendet. Es gilt,
solange **beides** stimmt:

- **die Revision** — sie steigt bei jedem Nudge (ein Feld hat sich
  geändert), bei jedem neuen BTP-Stand und bei jedem Schreibvorgang an der
  Konfiguration (dort stecken Hallen-Farben und Aufruf-Timer);
- **die Hart-TTL von 250 ms** — das Sicherheitsnetz gegen eine
  Änderungsquelle, an die niemand gedacht hat. Schlimmstenfalls ist eine
  Anzeige eine Viertelsekunde alt, statt bis zum nächsten Ereignis falsch
  zu bleiben.

Ist der Zwischenstand kalt oder abgestanden, rechnet der Server wie vorher.
**Er ist Beschleuniger, nicht Wahrheit.**

Dazu bekommt jede Antwort eine Marke (`ETag`). Fragt eine Anzeige mit
dieser Marke nach und hat sich nichts geändert, antwortet der Server mit
einer leeren Bestätigung (HTTP 304) statt mit rund 16 KB. Der Relay kennt
weder Marke noch Zwischenstand — im Cloud-Betrieb läuft alles wie bisher.

Eine Folge davon steckt in der Anzeige selbst: Die Uhr „Zeit seit Aufruf"
lief früher beiläufig mit, weil viermal je Sekunde ein voller Stand kam.
Sie rechnet jetzt aus einem gemerkten Zeitversatz zur Server-Uhr und hat
einen eigenen Sekundentakt, der nur läuft, solange überhaupt eine Uhr
sichtbar ist.

## Feld-Übersicht: nur noch die Ziffern neu (seit v0.9.240)

Die Übersicht (`overview.html`) warf bei jedem eintreffenden Stand das
komplette Board weg und baute alle Kacheln neu — bei zwanzig Feldern rund
siebzig Mal je Sekunde, für eine Änderung von zwei Ziffern. Genau das
ruckelte auf schwächeren Pis.

Jetzt tauscht sie im Normalfall nur die Satzstand-Spalten der betroffenen
Karten aus. **Neu gebaut wird**, sobald sich mehr ändert als der
Punktestand: neuer Satz, anderes Spiel, Feld wird frei oder belegt,
Behandlungspause, Turnierleitung gerufen, andere Feld-Menge oder
-Reihenfolge, andere Halle oder Hallen-Farbe, geänderte Namen oder Nationen,
gewechselte Sichtbarkeit der Aufruf-Uhr — und beim Umschalten der
Hallen-Rotation. **Spätestens alle 30 Sekunden** ohnehin einmal komplett:
Sollte sich je etwas verschoben haben, richtet sich die Anzeige damit von
selbst wieder ein.

Die Zuständigkeitsgrenze steht als `src/io/courtPatch.mjs` mit eigenem
CI-Schritt (32 Prüfungen); `overview.html` trägt eine Inline-Kopie.

Der **Einzelfeld-Monitor bleibt unverändert** — er hat nie ein ganzes Bild
weggeworfen, sondern setzt seine Texte an bestehenden Elementen.

## Messen, was die Anzeigen kosten (seit v0.9.235)

Erste Etappe der Spec
[features/monitor-livestand-push.md](features/monitor-livestand-push.md):
Bevor an der Strecke etwas umgebaut wird, misst sie sich selbst. Drei
Werkzeuge, alle abschaltbar und ohne Wirkung auf die Anzeige:

**1. Zähler im Turnier-PC.** Gezählt werden die Zustands-Abrufe (`/health`
und `/court/{id}/state`, jeweils **getrennt** nach nudge-getrieben und
Fallback-Takt) samt ihrer Antwortgröße, die Bau-Dauer der Übersicht, die
Schreibvorgänge der `live-scores.json` und die verschickten Nudges. Die
Trennung liefert die Anzeige selbst über `&src=push|poll` am Abruf; eine
Seite aus einem älteren Stand sendet den Parameter nicht, ihr Abruf zählt
dann als `poll` — was er auch ist.

**2. Ausgabe.** Alle zehn Sekunden eine Zeile im Diagnose-Log (kommt über
den Log-Upload auch aus einem echten Turnier zurück, siehe
[logging.md](logging.md)), und im LAN zusätzlich `GET /debug/perf` als
JSON. Passiert nichts, wird auch nichts geschrieben. Die Route gibt es
**nur im LAN** — im Internet hätte eine unauthentifizierte Lastauskunft
nichts zu suchen. Sie trägt ausschließlich Zahlen, per Wächter-Test
abgesichert.

**3. Lastskript.** `node scripts/last-monitor.mjs --base
http://<turnier-pc>:8088/` simuliert zwanzig zählende Tablets, zwanzig
Feld-Übersichten und wahlweise feste Court-Monitore — mit demselben
Coalescing und Fallback-Takt wie die echten Seiten. Es misst zusätzlich die
Latenz vom gesendeten Punkt bis zu seinem Erscheinen in der Übersicht.
Gegen den Relay läuft es unverändert, nur mit dessen Basis-Adresse.
**Es braucht belegte Felder:** Ein Stand ohne passende Match-ID wird
verworfen, dann entstünde weder Schreibvorgang noch Nudge.

> ⚠️ **Nicht während eines echten Turniers.** Das Skript gibt sich als
> zählendes Tablet aus: Es **belegt Felder** (ein echtes Tablet bekäme
> danach „Feld belegt") und seine erfundenen Punktstände laufen den
> regulären Weg — bis in den öffentlichen Liveticker auf badhub.de und in
> die Spielzeiten-Statistik. Gedacht ist es für einen Probeaufbau. Der
> Schalter `--trocken` verbindet **kein** Tablet und misst nur die
> Anzeige-Seite; so ist es auch neben einem laufenden Turnier gefahrlos.

**4. Render-Messung auf dem Pi.** In der Browser-Konsole der Übersicht
`localStorage.ovRenderMessen = "1"` setzen; die Seite meldet dann alle zehn
Sekunden, wie oft und wie lange sie gezeichnet hat und wie viele Renders
länger als ein 60-Hz-Bild (16 ms) brauchten. Mit `"2"` zusätzlich jeden
einzelnen Render. Rein diagnostisch, über `localStorage` statt über die
Konfiguration — wie `tlRenderMessen` in der Turnierleitungs-Sicht.

## Endpunkte

Alle Routen gibt es doppelt — vom LAN-Server **und** vom Relay, damit der
Monitor in beiden Modi dieselbe Seite ist. Der Server setzt beim
Ausliefern den Basis-Pfad (`__BASE__`) ein; `monitor.html` baut daraus
absolute URLs, unabhängig von der Verschachtelungstiefe.

| Zweck                  | LAN                            | Cloud                          |
|------------------------|--------------------------------|--------------------------------|
| Anzeige (Gerät)        | `/monitor`                     | `/{ns}/monitor`                |
| Status (Gerät)         | `/monitor/state?device=`       | `/{ns}/monitor/state?device=`  |
| Anzeige (fest)         | `/court/{label}/display`       | `/{ns}/court/{label}/display`  |
| Status (fest)          | `/court/{label}/state`         | `/{ns}/court/{label}/state`    |
| **Court-Übersicht**    | `/info/overview`               | `/{ns}/info/overview`          |
| Übersichts-Daten       | `/health`                      | `/{ns}/health`                 |
| **In Vorbereitung**    | `/info/preparation`            | `/{ns}/info/preparation`       |
| Vorbereitungs-Daten    | `/info/preparation/state`      | `/{ns}/info/preparation/state` |
| **Werbung (Rotation)** | `/info/ad`                     | `/{ns}/info/ad`                |
| Flaggen                | `/flags/{code}.svg`            | `/{ns}/flags/{code}.svg`       |
| Flaggen (TL-Seite)     | `/flags/{code}.svg`            | `/flags/{code}.svg` (ns-los)   |
| Werbebild              | `/ads/{datei}`                 | `/{ns}/ads/{index}`            |
| Werbe-/Leisten-Zustand | `/info/ad/state`               | `/{ns}/info/ad/state`          |
| **Perf-Zähler** (S0)   | `/debug/perf`                  | — (bewusst nur LAN)            |
| **Turnierlogo**        | `/info/logo`                   | `/{ns}/info/logo`              |
| Werbe-Upload           | —                              | `POST /{ns}/monitor`           |
| Geräte-Steuerung       | —                              | `POST /{ns}/monitor/control`   |
| Geräteliste            | —                              | `GET /{ns}/monitor-devices`    |

Im Cloud-Modus pusht der bts-light-Host die Feld-Zuweisungen + Fernbefehle
alle ~3 s (nur bei Änderung) an `…/monitor/control` und holt von
`…/monitor-devices` die Geräteliste für die „Court-Monitore"-Seite.

**Zugriffsschutz:** Alle Relay-Namespace-Routen haben bewusst kein eigenes
Token – das Zugangsmerkmal ist die 128-Bit-UUID des Namespace
(`install_id`). Wer sie kennt, kann Werbung/Zuweisungen überschreiben oder
ein „Neu laden"/„Identifizieren" auslösen; mehr nicht (die Befehle sind
ein geschlossenes Enum). Das ist dasselbe Modell wie für die Tablet- und
Werbe-Routen und für eine zugangsfreie Plug-and-play-App akzeptiert.

## ETag/304 für den Status-Abruf (seit v0.9.254)

Alle drei Status-Endpunkte (`/monitor/state` am Relay und LAN sowie
`/court/{id}/state` im LAN) tragen jetzt einen **ETag** und beantworten
`If-None-Match` mit **304 ohne Body**. Der Client (`monitor.html`) schickt die
zuletzt gesehene Marke mit und hält den letzten Stand bei 304 — nur bei 200
wird der Zustand übernommen und die Marke aktualisiert. `ad.html` zieht
denselben Weg für seinen 1-s-Reassignment-Poll nach.

**Was in die Marke eingeht:** der Inhalt der Antwort, aber **nicht** die
je-nicht-inhaltlichen Felder — `serverNowMs` (je Aufruf neu) und `seq` (steigt
je Anstoß, auch ohne Inhaltsänderung). Beide bleiben im Body; nur die Marke
ignoriert sie. Dieselbe Begründung wie beim Übersichts-ETag (`uebersicht_marke`,
`relay/src/main.rs` :1808-1813). Muster: `DefaultHasher::new()` mit festem Seed
(wie `bild_marke`) → gleicher Inhalt, gleiche Marke, auch über Neustarts.

**Uhr:** Da 304 keinen Body trägt, speist `monitor.html` seinen Uhr-Offset aus
zwei Quellen — jedem vollen 200 (`serverNowMs`) und dem WS-Herzschlag
(`{"hb":<nowMs>}`). Ein Refetch-Cap (gesund → 10 min, ungesund → 60 s) erzwingt
regelmäßig einen vollen 200. Kanonische Fassung: `src/io/monitorClock.mjs`.

Das Lebenszeichen des Geräts (`monitor_seen`) wird **vor** dem 304-Check
aktualisiert — ein cachetreuer Monitor bleibt online.


Läuft kein Spiel, zeigt der Monitor Werbung:

- Werbebilder werden **direkt im Tool** hochgeladen (Setup → Abschnitt
  „Court-Monitor"). **Ein gemeinsamer Werbesatz** für alle Monitore.
- Sie liegen im App-Datenverzeichnis unter `court-ads/`; der LAN-Server
  liefert sie aus `/ads/` aus.
- **Cloud-Modus:** bts-light lädt die Bilder nach dem Verbinden per
  `POST /{ns}/monitor` zum Relay hoch (Base64-JSON) und prüft alle 30 s
  per Fingerabdruck auf Änderungen. Ad-Änderungen erreichen Cloud-Monitore
  daher binnen ~30 s. **Ops-Hinweis:** nginx muss für `/bts-relay/`
  `client_max_body_size` ≥ 25 MB setzen, sonst scheitert der Upload mit
  HTTP 413 (Standardwert 1 MB ist zu klein).
- Wechsel-Intervall einstellbar (Default 10 s).
- **Fallback** ohne konfigurierte Werbung: neutrale Seite mit Turniername
  und „Kein Spiel auf diesem Feld".
- **Abschaltbar:** Die Option „Werbung im Leerlauf anzeigen" steuert, ob
  ein freies Feld überhaupt Werbung zeigt. Aus → das Feld zeigt immer die
  neutrale Leerlauf-Seite, auch wenn Werbebilder hinterlegt sind.

## Spieldauer-Anzeige

Zählt ein Tablet das Feld, kennt der Monitor den Spielbeginn
(`court_state.startedAt`) und zeigt optional neben der Feldnummer die
laufende Spieldauer in Minuten (Stoppuhr-Symbol). Im Tool ein-/abschaltbar.
Ohne zählendes Tablet bleibt die Anzeige leer.

## Aufruf-Uhr „Zeit seit Aufruf" (Plan 4)

Grundlage ist `on_court_since_ms` (Zeitpunkt des 1. Feld-Aufrufs) + die
Server-Zeit (`serverNowMs`), damit die Anzeige nicht an der oft nicht
synchronen Pi-Uhr driftet. Gated durch die `call_timer`-Einstellung
(`enabled` + `secondCallMinutes`/`thirdCallMinutes`).

- **Einzelanzeige** (`monitor.html`, `renderCallTimer`): hochzählende Uhr
  `M:SS` + Ampel „1. Aufruf → 2. Aufruf → Letzter Aufruf" (neutral/gelb/rot),
  solange ein Spiel auf dem Feld steht.
- **Multifeld-Übersicht** (`overview.html`, v0.9.156): je Feld ein Chip
  „vor X min · 1./2./Letzter Aufruf" mit derselben Ampel — **nur in der
  Wartephase** (aufgerufen, aber noch nicht am Zählen; `hasStarted()` prüft
  Punkte/Aufschläger). Sobald das Spiel zählt, verschwindet der Chip, damit
  ein Board voller laufender Spiele nicht komplett rot wird. Datenquelle ist
  der `/health`-Poll (`courts` + `serverNowMs` + `callTimer`) — im **Cloud**
  (ab v0.9.193) derselbe Endpunkt `/{ns}/health`, den der Relay aus
  `courts`/`court_matches`/`court_scores`/`court_on_court_since` baut. Bewusst
  weggelassen im Cloud: Aufschlag-Highlight, Verletzungs-/TL-Badges (stehen im
  Relay nicht bereit); Feld × Spiel × Satzstand × Aufruf-Uhr sind vollständig.
  Der Satzstand kommt dabei seit v0.9.200 auch für LAN-Tablets an — über den
  Score-Spiegel des Hosts (siehe oben); davor blieb er im Cloud leer, sobald
  das Tablet nicht selbst über den Relay zählte.

## Entschiedenes Match (kein Geister-Satz)

Endet ein Best-of-3 in zwei Sätzen, schickt das Tablet die Sätze plus
einen leeren dritten 0:0-Eintrag — frühere Monitor-Versionen zeigten den
als „laufenden Satz" als ob ein dritter Satz käme. Sobald das Tablet im
gespiegelten `courtState` `finished: true` meldet, schaltet der Monitor
auf die **Endergebnis-Ansicht** um:

- Ein etwaiger 0:0-Geistersatz am Ende fällt weg.
- Alle wirklich gespielten Sätze werden als „fertig" gerendert; der
  große laufende-Satz-Box entfällt komplett. Die Done-Sätze werden in
  dieser Ansicht etwas größer gesetzt (`.scores.decided .set-done`),
  damit das Endergebnis aus der Distanz lesbar ist.
- Pro Satz wird das Gewinner-Team hell hervorgehoben (`.set-done.won`,
  Verlierer bleibt gedämpft).
- Die Sieger-Hälfte bekommt einen grünen Akzentbalken (`.half.winner`)
  und eine 🏆-Markierung.
- Der Aufschlag-Indikator (`serving`) ist in dieser Ansicht unterdrückt.

Sieger-Bestimmung in [monitor.html](../src-tauri/assets/monitor.html)
(`matchWinner`):

- Bei Aufgabe (`courtState.retired === true`) → `retiredWinner`
  (`'a'`/`'b'`).
- Sonst → Team mit den meisten Satzgewinnen (`a > b` zählt für Team 1).

Per-Satz-Hervorhebung wird bei einer Aufgabe absichtlich **nicht**
angewendet — der letzte Satz ist dort unvollständig, die Punkte-Mehrheit
ist daher kein zuverlässiger Satzgewinner. Der Match-Sieger (🏆 + grüne
Hälfte) bleibt korrekt.

Eingeführt in v0.9.15.

## Hallen-Farbmarke (Mehr-Hallen)

> Spec: [features/hallen-farben.md](features/hallen-farben.md)

Bei Mehr-Hallen-Turnieren tragen die Anzeigen eine kleine Farbmarke in
der Farbe ihrer Halle: `monitor.html` vor dem Feld-Label („● Halle 2 · 6"),
`overview.html` in der Kopfzeile neben dem Hallennamen (auch in der
Hallen-Rotation), `preparation.html` an den Hallen-Chips der Zeilen und —
bei `?halle=`-Filter — in der Kopfzeile. Die Farbe kommt vom Turnier-PC
(`hall_color`/`hallColor` in den bestehenden Zuständen, LAN wie Cloud);
Name/Label bleiben immer stehen. Alte Hosts/Relays liefern das Feld
nicht — die Seiten bleiben dann schlicht farblos. Jede Seite erzwingt die
strikte `#rrggbb`-Form, bevor der Wert in ein Style-Attribut gelangt.

## Spielernamen (Broadcast-Stil)

Namen werden zweizeilig dargestellt: Vorname(n) klein darüber, Nachname
groß darunter — wie in Sport-Übertragungen. Der letzte Namensteil gilt
als Nachname. So bleibt der Nachname auch bei langen Doppel-Namen aus der
Distanz gut lesbar, der Vorname geht nicht verloren, und das Bild ist für
alle Spieler:innen einheitlich. Ein einteiliger Name steht ohne Vornamen-
Zeile; ein sehr langer Einzelteil wird mit „…" abgeschnitten.

## Layout

Das Anzeige-Layout ist im Setup wählbar. Aktuell gibt es **„A — Geteilt"**
(Team 1 oben, Team 2 unten); die Auswahl ist die Grundlage für weitere
Layouts. Der Monitor setzt das gewählte Layout als `data-layout` am
Wurzelelement.

## Pausen-Timer (Retro-Klappanzeige)

Läuft eine Pause (`court_state.pause`), zeigt der Monitor einen
**Countdown im Split-Flap-Stil** (Klappanzeige wie eine alte
Flughafentafel). Greift bei den BWF-Satzpausen (Countdown) und bei
Behandlungspausen (ohne Countdown). Im Tool ein-/abschaltbar.

Seit v0.9.158 ist die Pausenuhr ein **kompakter, halbtransparenter Banner
am oberen Rand** (`#timer-overlay`, Plan 5) statt eines Vollbild-Overlays —
Satzstand und Namen der darunterliegenden Match-View bleiben lesbar. Das
Verhalten (`renderTimer`) ist unverändert; nur Layout/Optik.

## Leerlauf-Anzeige (kein Spiel auf dem Feld)

Steht kein Spiel auf einem zugewiesenen Feld, zeigt der Monitor
(`#ad-fallback`) den **Turniernamen**, die **Feldnummer sehr groß** und seit
v0.9.158 darunter prominent **badhub.de** (Plan 10) — Orientierung in der
Halle plus dezente Werbung. Gilt für LAN- und Cloud-TVs (gleiche
`monitor.html`).

## Konfiguration

Setup-Wizard, Abschnitt **„Court-Monitor"** ([`CourtMonitorConfig`](../src-tauri/src/config.rs)):

- **Aktivieren** — blendet die Monitor-Adressen in der Oberfläche ein.
- **Werbung im Leerlauf anzeigen** — steuert, ob ein freies Feld Werbung
  zeigt oder die neutrale Leerlauf-Seite.
- **Werbebilder** — hinzufügen/entfernen (JPG, PNG, WEBP, GIF; ≤ 8 MB je
  Bild).
- **Wechsel-Intervall** — 3–30 s.
- **Layout** — Anzeige-Layout des Monitors (aktuell „A — Geteilt").
- **Anzeige-Optionen** — Disziplin / Runde / Spielnummer / Spieldauer /
  Pausen-Timer je einzeln ein-/ausblenden. Eine Live-Vorschau im Setup
  zeigt die Wirkung jeder Option sofort.

Die Einrichtungs-Adresse und die Feld-Zuweisung der Geräte stehen auf der
Seite **„Court-Monitore"** (Dashboard → Court-Monitore).

## Kombi-Anzeige (`combo.html`)

Mehrere Felder auf einem TV (bis zu 3), als Bänder über- oder nebeneinander —
**je Gerät wählbar** (siehe unten). Datenquelle ist `/combo/state` —
derselbe `overview()`-Stand wie die Einzelanzeige. **Nur LAN:** der Relay
transportiert nur Einzelfeld-Zuweisungen, Kombi-Monitore laufen daher über
den Turnier-PC.

### Ausrichtung je Gerät (seit v0.9.270)

Bis v0.9.269 galt **ein** globaler Schalter für alle Kombi-Anzeigen. Das passte
nicht zur Halle: Ein TV über dem Mittelgang zwischen zwei Feldern will sie
nebeneinander, ein TV an der Stirnseite über drei Feldern übereinander.

Die Ausrichtung wird deshalb dort gewählt, wo ohnehin die Felder gewählt werden
— **Dashboard → Court-Monitore**, beim Gerät „Kombi-Anzeige → Felder wählen…":

- **Felder übereinander** — ein breites Band je Feld. Für einen TV über oder
  neben den Feldern.
- **Felder nebeneinander** — ein hohes Band je Feld (Team 1 oben, Spielstand
  als Satz-Paare mittig, Team 2 unten). Für einen TV zwischen den Feldern.

Ein neu eingerichtetes Kombi-Gerät wird mit der **zuletzt gewählten**
Ausrichtung vorbelegt — in einer Halle mit gleich montierten TVs stellt man also
nur beim ersten um. Drei Felder nebeneinander sind erlaubt; ob das aus der
letzten Reihe noch lesbar ist, entscheidet die Halle.

Der TV übernimmt die Änderung **binnen etwa einer Sekunde und ohne die Seite neu
zu laden** — der Satzstand bleibt durchgehend sichtbar, auch mitten im Spiel.
Die Ausrichtung reist dafür im laufenden `/combo/state`-Poll mit.

Gespeichert wird sie je Gerät in `monitor-combo-dir.json` neben der
Zuweisungsdatei ([ADR 0049](adr/0049-kombi-ausrichtung-eigene-geraete-datei.md));
sie bleibt deshalb erhalten, wenn ein Gerät zwischenzeitlich ein Einzelfeld
zugewiesen bekommt.

**Beim Update von einer älteren Version** wird der alte globale Schalter
einmalig auf alle vorhandenen Kombi-Geräte übernommen — am Bild ändert sich
nichts. Wird später eine ältere Version installiert, kennt sie die Datei nicht
und alle Kombi-TVs folgen wieder dem globalen Schalter; die Feld-Zuweisungen
bleiben dabei erhalten.

**Grenze:** `?dir=v` in einer **von Hand gebauten** Kiosk-Adresse wirkt weiterhin
— aber nur als Startwert. Eine solche Seite ohne `?device=` ist dem Turnier-PC
unbekannt und folgt Änderungen im Dialog nicht. Für gekoppelte Geräte (der
Normalfall über den Kopplungs-Code) gilt immer die Wahl aus dem Dialog.

- **Satz-Sieger deutlich hinterlegt** (seit v0.9.105): Der gewonnene Satz
  steht nicht nur weiß/grau, sondern als **grüner Block** (`.set.won` /
  `body.vertical .vset.won`) — aus der Ferne sofort als Sieger erkennbar
  (Feld-Wunsch 2026-06-15). Laufender Satz bleibt gelb (`.current`).
- **Pausen-Countdown am betroffenen Feld** (seit v0.9.105): Läuft an einem
  Feld eine Pause (`court_state.pause`), zeigt das Band dieses Felds die
  Restzeit (`Pause`/`Satzpause` + `m:ss`, `Behandlung` ohne Countdown) —
  „an der Seite, wo die Pause ist". `combo.html` rechnet den Countdown
  relativ zur Server-Zeit (`serverNowMs` im `/combo/state`-Payload), weil
  die Pi keine synchrone Uhr haben muss; das Tablet setzt `endsAt` in
  Server-Zeit. Das Feld `CourtOverview.pause` wird in `overview()` 1:1 aus
  dem Tablet-`court_state` übernommen (wie `serving`).

## Raspberry Pi — Kiosk-Einrichtung

Ausführliche, einsteigertaugliche Schritt-für-Schritt-Anleitung:
**[pi-setup.md](pi-setup.md)**. Kurzfassung:

1. Raspberry Pi OS (Desktop) mit dem Raspberry Pi Imager bespielen –
   dort gleich WLAN voreinstellen.
2. Auf dem Pi das Skript [`pi/setup-monitor.sh`](../pi/setup-monitor.sh)
   ausführen → Kiosk-Autostart steht.
3. Neu starten → der TV zeigt einen Kopplungs-Code; in bts-light unter
   „Court-Monitore" dem Code ein Feld zuweisen.

**Feste Adresse ohne feste IP:** Im LAN-Modus meldet sich der Turnier-PC
per mDNS unter `bts-light.local` (siehe [mDNS](#mdns-bts-lightlocal)). Die
Standard-Monitor-Adresse `http://bts-light.local:8088/monitor` passt
dadurch in **jedem** Turnier-WLAN – ein Master-Image braucht keine
Anpassung. Ist der PC-Port gesperrt, die Cloud-Adresse
(`https://badhub.de/bts-relay/<install_id>/monitor`) verwenden.

**Die Monitore bleiben auf HTTP.** Seit dem verschlüsselten LAN-Zugang
([ADR 0047](adr/0047-lan-tls-konkretisierung.md)) bietet der Server dieselben
Seiten zusätzlich über `https://bts-light.local` an — für die Pis ist das
aber **nicht** vorgesehen und auch nicht nötig:

- Port 8088 bleibt unverändert offen; der Subnetz-Scan der Pis findet den
  Server genau wie bisher. An den Kiosk-Skripten ändert sich nichts.
- Ein Kiosk-Browser könnte die Zertifikatswarnung ohne Tastatur gar nicht
  bestätigen — und `--ignore-certificate-errors` lädt die Seite zwar, stellt
  aber **keinen** Secure Context her. Der einzige Gewinn wäre die
  Verschlüsselung selbst, nicht die Akkuanzeige.
- Die Pis haben **keine Echtzeituhr**. Bootet einer ohne Internet und ohne
  NTP, steht seine Uhr falsch — ein Zertifikat mit „gültig ab heute" würde er
  dann **still** verwerfen. Genau deshalb ist das Zertifikat weit vordatiert;
  aber der einfachere Weg bleibt, die Monitore bei HTTP zu belassen.

## Info-Monitor (Hallen-Display)

Neben dem feld-bezogenen Court-Monitor (ein TV pro Feld) liefert bts-light
zwei **Hallen-weite Info-Anzeigen** unter dedizierten URLs aus — ideal für
ein Display am Halleneingang oder am Schiedsrichter-Tisch der TL. Beide
nutzen denselben Tablet-Server, brauchen also weder Internet noch
badhub.de.

> **TV-Launcher (Tippen sparen):** An einem Smart-TV ohne feste Zuweisung muss
> man nur die **kurze** Adresse `http://bts-light.local:8088` (oder `/tv`) tippen
> — es erscheint ein **Auswahl-Menü**, das man mit der **Fernbedienung
> (Pfeiltasten + OK)** bedient. Es bietet **Lokal** (bts-light: „Alle Hallen",
> je Halle ein Button, „Nächste Spiele") **und Online** (öffentlicher
> badhub-Liveticker je Halle, etwas andere Darstellung — aus dem konfigurierten
> Verband). Kein `?halle=` tippen. Direkt-Kurzpfade ohne Menü: `…/alle`,
> `…/h/1`, `…/h/2` (n-te Halle, alphabetisch), `…/next`. (Pi-Monitore brauchen
> gar nichts zu tippen — die werden im Tool zugewiesen.)

| URL | Was es zeigt |
|---|---|
| `http://bts-light.local:8088/info/overview` | **Court-Übersicht** — alle Felder mit Status („frei" / „läuft" / „Behandlung" / „TL"), aktuellem Spiel, Paarung und Sätzen. Bei Doppeln stehen die zwei Partner untereinander (wie der badhub-Hallen-Monitor). Bei Mehr-Hallen-Turnieren ohne `?halle=` **rotiert** die Anzeige automatisch durch die Hallen (jede einzeln im Vollbild). |
| `http://bts-light.local:8088/info/preparation` | **In Vorbereitung** — Liste der gerufenen und eingeplanten Spiele; aufgerufene mit gold-Pille „In Vorbereitung", Halle und „vor X Min." hervorgehoben. |

Beide Seiten verstehen zwei URL-Parameter:

- **`?halle=<Name>`** — filtert auf eine Halle. Court-Übersicht zeigt nur
  die Felder dieser Halle; Vorbereitungs-Monitor nur die Aufrufe für diese
  Halle. Vergleich getrimmt + case-insensitiv. Beim Court-Grid: kein
  Treffer → alle Felder (Tippfehler-Schutz). Beim Vorbereitungs-Monitor:
  kein Rückfall, der Operator soll explizit sehen, wenn nichts für die
  Halle gerufen ist.
- **`?rotate=90|180|270`** — Pivot-/Hochformat-Monitore: rotiert die
  gesamte Anzeige per CSS-Transform im Browser. Pi-OS-seitig keine
  Änderung nötig (kein `xrandr`, kein `display_rotate=` in config.txt).
  `0` oder weggelassen = normal.
- **`?hallSeconds=<n>`** — nur Court-Übersicht: Intervall der **Hallen-
  Auto-Rotation** in Sekunden (Default 12, min 3). Greift nur, wenn mehrere
  Hallen erkannt werden und **kein** `?halle=` gesetzt ist.

> **Links nicht von Hand bauen:** Die bts-light-Seite **Court-Monitore** zeigt
> unter „Court-Übersicht (Hallen-Display)" die fertigen Links automatisch — den
> öffentlichen Online-Liveticker, die lokale Gesamt-Übersicht und (ab 2 Hallen)
> je Halle einen `?halle=`-Link zum Kopieren auf den Hallen-TV. „Öffnen" zeigt
> die Vorschau am PC.
>
> **Pi direkt einer Halle zuweisen:** Im Zuweisungs-Dropdown eines Geräts
> stehen ab 2 Hallen unter „Informationen" automatisch „Court-Übersicht – alle
> Hallen" **und** je Halle „Court-Übersicht – Halle X". Wählt man eine Halle,
> wird der Pi fest auf `…/info/overview?halle=<Halle>` umgeleitet — kein
> manuelles URL-Eintippen am Pi nötig.

**Mehr-Hallen-Verhalten der Court-Übersicht (ein TV pro Halle ODER ein TV für
alle):**

- **Fester TV pro Halle:** `…/info/overview?halle=Halle%201` → zeigt dauerhaft
  nur diese Halle im Vollbild (bei 12 Feldern ein 4×3-Raster). Empfohlen, wenn
  pro Halle ein Display vorhanden ist.
- **Ein TV für mehrere Hallen:** `…/info/overview` (ohne `?halle=`) → erkennt
  mehrere Hallen und **wechselt automatisch** durch sie (Halle 1 Vollbild →
  nach `hallSeconds` Halle 2 → …). Der Kopf zeigt den Hallennamen + „1 / N".

Beispiele:

- Eingangs-TV Halle 1 im Pivot: `…/info/preparation?halle=Halle%201&rotate=90`
- Fester Court-TV Halle 2: `…/info/overview?halle=Halle%202`
- Ein TV, alle Hallen im 20-Sek-Wechsel: `…/info/overview?hallSeconds=20`

Eingerichtet wird das nach dem Pi-Standardablauf
([pi-setup.md](pi-setup.md)) — nur die `bts-monitor-url.txt` auf der
Boot-Partition zeigt nicht auf `/monitor`, sondern auf die passende
`/info/…`-Variante.

## Siegerehrung (Sieger-Monitor)

Eigener Menüpunkt **„Siegerehrung"** in der App (neben „Monitore"). Dort wählt
der Operator live, welche ausgespielte Disziplin auf dem Sieger-Monitor
erscheint (keine Rotation — ideal zum Fotografieren des Podiums). Die
Disziplin-Auswahl ist global (`set_winners_selection`/`winners_overview`), wirkt
also auf alle Sieger-Monitore gleichzeitig.

Die TV-**Zuweisung** bleibt unter „Monitore": ein Gerät bekommt „Siegerehrung —
ganzes Podium" oder „nur Platz 1/2/3" (drei Einzel-TVs vor dem Podest).

Anzeige (`winners.html`):

- Endpunkte `GET /info/winners` (ganzes Podium) bzw. `?only=1|2|3` (ein Platz je
  TV); Zustand über `GET /info/winners/state` (Disziplinen, `selected`,
  `tournament`).
- Podium: Namen zweizeilig (Vorname / Nachname), mehrere Vornamen gekürzt.
- Einzel-Modus: ganzer Name in **einer Zeile**, per `fitSolo()` dynamisch auf
  ~94 % der Breite skaliert (kurze Namen durch die Höhe begrenzt) — nutzt die
  **volle Breite** statt fixer `vmin`-Größen. Verein größer dargestellt.
- Layout = Flex-Spalte (wie `overview.html`): Header / `main` / Footer jeweils
  über die **volle Breite**. Footer zweizeilig: **Turniername** (klein) über der
  **Disziplin** (groß).
- **Vereinslogos** neben dem Vereinsnamen (sofern in Badhub vorhanden):
  - Quelle: `GET {base}/api/v1/club-logo?name=<verein>` — derselbe Singular-
    Resolver, den auch der Cloud-Modus der Turnierleitungs-Web direkt aus dem
    Browser aufruft (`docs/turnierleitung-web.md`). `base` = Origin aus
    `badhub.url` (kein Slug nötig → auch Teilnehmer aus anderen LVs bekommen
    ihr Logo). Badhub löst den Vereinsnamen **selbst** auf, inklusive gängiger
    Abkürzungen („BC" für „Badminton Club") und Klammerzusätzen
    („(Berlin)") — `tablet/club_logos.rs` dupliziert diese Zuordnung
    **nicht** mehr lokal (Befund 15.08.2026: die frühere Exakt-/Klammer-
    Normalisierung gegen die Plural-Liste `/api/v1/club-logos` traf
    Abkürzungen wie „BC" ≠ „Badminton Club" nicht — der Singular-Resolver
    löst sie trotzdem auf, LAN und Cloud verwenden jetzt denselben Weg).
  - Backend `tablet/club_logos.rs` fragt pro (normalisiertem) Vereinsnamen
    einmal ab und cacht Bildbytes (6 h / 60 s bei Fehler); Endpoint
    `GET /info/club-logo?name=…` liefert das Bild lokal aus (auch für LAN-TVs
    ohne Internet — nur der Turnier-PC braucht welches). SSRF-sicher:
    Bild-Origin (nach Redirect) == badhub-Origin.
  - Kein Treffer / kein Logo / offline → `<img onerror>` entfernt sich, es bleibt
    **nur der Name** (kein Platzhalter).
- Sonderfall „zwei dritte Plätze" (kein Spiel um Platz 3): `?only=3` zeigt beide
  Paare kompakter (`multi`-Modus).

## mDNS: `bts-light.local`

Im LAN-Modus gibt bts-light per mDNS (`tablet/mdns.rs`) den festen Namen
`bts-light.local` bekannt, der auf die aktuelle LAN-IP des Turnier-PCs
zeigt. Tablets und Monitore erreichen den PC darüber, **ohne seine
IP-Adresse zu kennen** – es ist keine feste IP nötig, weder im Router
noch am Laptop. Der Raspberry Pi löst `.local`-Namen über das
vorinstallierte avahi auf. Schlägt mDNS fehl (z. B. blockierende
Firewall), funktioniert die direkte IP-Adresse weiterhin.

**Verifikation 2026-05-25:** Test mit Raspberry Pi (Pi OS Lite 32-bit,
avahi-daemon) im FRITZ!Box-WLAN; bts-light-Bekanntmachung am Mac per
`dns-sd -P bts-light _bts-light._tcp local 8088 bts-light.local. <ip>`
simuliert → vom Pi mit `avahi-resolve -n bts-light.local` aufgelöst →
korrekte IP zurück, auch über die WLAN↔Ethernet-Bridge der FRITZ!Box
hinweg. Der frühere Fehlversuch von einem Windows-PC war ein
Windows-Client-Problem (Windows ist als mDNS-Client unzuverlässig), nicht
ein bts-light-Problem.

## Flaggen

Nationalität ist ein IOC-Code (`GER`, `POL`, …). bts-light bündelt einen
SVG-Flaggensatz (`src-tauri/assets/flags/`, ins Binary kompiliert),
Anzeige per `<code>.svg`. Fehlt der Code, zeigt der Monitor den Namen
ohne Flagge. Herkunft/Lizenz: [`NOTICE.md`](../NOTICE.md).

## Lizenz-Hinweis

Visuelle Referenz war `phihag/bup` (u. a. PR #43, Einzelturnier-Display).
Davon wurde nur die **Idee** übernommen — **kein Code**, da die
bup-Lizenz unklar ist. Diese Anzeige ist eine eigenständige
Clean-Room-Umsetzung.

## Nicht umgesetzt

- **2-Felder-pro-TV-Modus** (`…/display?courts=3,4`) — siehe
  [roadmap.md](roadmap.md).
- **Pro-Feld unterschiedliche Werbung** — bewusst ein gemeinsamer Satz.
