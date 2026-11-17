# 0105 — Ansage bei Netzausfall: harte Frist und Merker im Kern, Ersatzstimme an einer Stelle, nie mischen

- **Status:** accepted
- **Datum:** 2026-11-16

## Kontext

Fällt in der Halle das Internet aus, hängt die Ansage. Befund am Code (Stand v0.9.437):

- Der HTTP-Client entsteht je Anfrage neu und kennt nur eine Gesamtfrist (10 s, Gemini 20 s),
  keine Verbindungsfrist.
- Scheitert „Ansage in Teilen", folgt ein **zweiter Versuch für die ganze Ansage** — im PC-Weg
  (`azure_tts_speak`) und im Seiten-Weg (`seiten_cloud_audio`). Bei totem Netz wartet die
  Ansage damit zweimal die volle Frist ab.
- Das Frontend setzt um die Vertonung keine eigene Frist; Fristen gibt es nur für die
  Wiedergabe.
- Niemand merkt sich einen Ausfall. Jede folgende Ansage wartet wieder genauso lange.
- Wer bei einem Fehler spricht, entscheidet am PC das Frontend (`playPrepared` → `sprich`),
  für die Ansage-Seiten der Kern (`ansage_vertonen`). Eine neue Ersatzstufe müsste an zwei
  Stellen eingebaut werden.

Die Spec [ansage-offline-stimme](../features/ansage-offline-stimme.md) verlangt: höchstens 5 s
Stille bei der ersten Ansage nach einem Ausfall, danach kein Warten mehr, und eine
Offline-Stimme (ADR 0104) als erste Ersatzstufe.

## Entscheidung

1. **Harte Frist im Kern.** Die ganze Vertonung einer Ansage läuft unter einer Frist
   (`tokio::time::timeout`), im PC-Weg und im Seiten-Weg. Dazu ein geteilter HTTP-Client mit
   Verbindungsfrist. Die Frist deckt auch, was HTTP-Fristen nicht decken (hängende
   Namensauflösung, stumme Gegenstelle).
2. **Fehlerklasse an der Quelle.** Die vier Dienst-Module melden `Netz`, `Drossel` (429),
   `Zugang` (401/403) oder `Inhalt`. Nur `Netz` (Verbindungsfehler, Zeitüberschreitung, harte
   Frist, 5xx) gilt als Ausfall.
3. **Kein zweiter Versuch nach `Netz`, `Drossel`, `Zugang`.** Den Versuch „als Ganzes" gibt es
   nur noch nach `Inhalt`.
4. **Merker je Sprachdienst in `AppState`.** Ein reiner Zustandsautomat mit gereichter Uhr.
   Solange er gesetzt ist, geht keine Vertonung ins Netz; eine nicht vollständig gecachte
   Ansage geht sofort an die Ersatzstimme. Der Merker lebt bewusst **nicht** in der
   `SyncEngine` — die wird bei jedem Speichern der Einstellungen neu gestartet.
5. **Hintergrundprüfung ohne Kosten.** Nur solange der Merker gesetzt ist, baut der Kern im
   Takt eine TLS-Verbindung zum Host des Dienstes auf. Gelingt sie, fällt der Merker. Ohne
   Erfolg läuft er nach einer Höchstdauer ab.
6. **Gürtel im Frontend.** `playPrepared` wartet auf den Kern höchstens Frist + 0,5 s.
7. **Die Ersatzstimme wählt der Kern, an einer Stelle.** Eine Funktion liefert den Ton der
   Ersatzstimme: Offline-Stimme (wenn eingeschaltet, bereit, deutsche Ansage), sonst
   Windows-Stimme. PC-Weg und Seiten-Weg benutzen dieselbe Funktion. Die Reihenfolge „Cloud →
   Ersatz → Browser" bleibt im Frontend, wie sie ist; der Abspielbericht nennt die Stufe.
8. **Nie mischen.** Fehlt bei einem Ausfall auch nur ein Teil, spricht die Ersatzstimme die
   ganze Ansage. Eine vollständig gecachte Ansage wird dagegen immer aus dem Cache gespielt,
   auch bei gesetztem Merker.
9. **„Lokal (ohne Internet)" ist ein Wert von `AzureTtsConfig.dienst`** (`"lokal"`), kein
   eigener Schalter. Weil ein unbekannter Dienst heute als Azure gilt, lernt schon die
   Version, die die Offline-Stimme einführt, den Wert als „kein Cloud-Dienst" zu lesen; die
   Version, die ihn wählbar macht, setzt die Rückweg-Grenze (ADR 0101) auf jene Version.

Nach der Messung festgelegt (17.11.2026, siehe „Entscheidung zu den Fristen"): harte Frist
4 s, Verbindungsfrist 1,5 s. Der Ausgangswert der harten Frist war 2,5 s. Weiter Ausgangswerte,
erst am Bau prüfbar: Prüftakt 15 s, Höchstdauer des Merkers 10 Minuten.

## Alternativen

- **Nur kürzere HTTP-Fristen.** Verworfen: Eine hängende Namensauflösung und eine stumme
  Gegenstelle liegen außerhalb der HTTP-Frist; ohne Merker wartet jede Ansage erneut.
- **Netzstatus von Windows abfragen.** Verworfen: Ein Hotspot mit Anmeldeseite und ein Router
  ohne Internet melden „verbunden".
- **Probe-Vertonung als Hintergrundprüfung** (`tts_dienste_pruefen`). Verworfen: bucht Zeichen
  je Probe.
- **Merker in der `SyncEngine`.** Verworfen: geht bei jedem Speichern verloren.
- **Neue Ersatzstufe im Frontend (`sprich`).** Verworfen: Der Seiten-Weg bräuchte eine zweite
  Umsetzung, die Reihenfolge liefe auseinander.
- **Ganze Kette in den Kern ziehen.** Verworfen: Der Kern müsste die MP3-Antworten der Dienste
  selbst dekodieren; großer Umbau ohne Gewinn für dieses Ziel.
- **Fehlende Teile von der Ersatzstimme sprechen lassen (mischen).** Verworfen (Entscheidung
  des Nutzers): zwei Stimmen in einem Satz klingen schlechter als eine Ersatzstimme.
- **Eigener Schalter „offline" neben der Dienstwahl.** Verworfen: Zwei Felder können sich
  widersprechen; die Dienstwahl ist schon die Stelle für „wer spricht".

## Konsequenzen

- Ein langsames, aber lebendes Netz kann die Ersatzstimme auslösen. Die Frist ist eine
  Konstante und wird erst nach der Messung festgelegt.
- Eine Ansage, die heute nach einem Teil-Fehler als Ganzes doch noch gelang, geht bei
  Netzfehlern künftig direkt an die Ersatzstimme.
- Die Hintergrundprüfung kann „erreichbar" melden, obwohl die Vertonung scheitert. Dann
  kostet die nächste Ansage einmal die Frist, und der Merker steht wieder.
- Bei einem langen Ausfall wartet nach jedem Ablauf der Höchstdauer eine Ansage einmal die
  Frist ab. Das ist der Preis dafür, dass ein Fehler in der Prüfung die Cloud-Stimme nicht
  dauerhaft abschaltet.
- Ansagen mit Tabletbedienung oder Schiedsrichtern sind bei einem Ausfall fast nie
  vollständig im Cache — dort trägt die Ersatzstimme, nicht der Vorab-Bestand (ADR 0106).
- Kein neues gespeichertes Feld für die Ausfall-Erkennung; der Rückweg bleibt frei. Der Wert
  `lokal` verschiebt die Rückweg-Grenze einmal.
- Der Rückfall-Banner nennt künftig die Stufe, die spricht.

## Ergebnis der Messung

**Stand 17.11.2026: Am Windows-Turnier-PC gemessen sind gutes Netz, eine echte Vertonung,
„Netzwerkadapter aus", „Netzwerkkabel am PC gezogen" und „Router ohne Internet". Die harte
Frist von 2,5 s war danach zu knapp; sie ist auf 4 s festgelegt, die Verbindungsfrist bleibt
bei 1,5 s (siehe „Entscheidung zu den Fristen"). Ungemessen sind Hotspot mit Anmeldeseite,
langsames Netz und ein Ausfall mitten im Betrieb.**

Werkzeug: `src-tauri/tests/tts_ausfall_probe.rs`, Aufruf
`cargo test -p bts-light --test tts_ausfall_probe -- --ignored --nocapture`. Es ruft die vier
Sprachdienste auf drei Arten ab — wie die App heute (frischer Client, nur Gesamtfrist), mit
Verbindungsfrist, mit harter Frist von außen — und stellt fünf Ausfallarten am eigenen Rechner
nach. Welche Lagen am Turnier-PC zu messen sind, steht im Kopf der Datei.

### Nachgestellte Ausfallarten (Zeit bis zum Fehler in ms)

Linux (Entwicklungsrechner, 16.11.2026, ein Lauf):

| Ausfallart | wie heute (Frist 10 s) | mit Verbindungsfrist 1,5 s | mit harter Frist 2,5 s |
|---|---:|---:|---:|
| Verbindung wird abgelehnt | 31 | 23 | 19 |
| Name unbekannt, sofortige Absage | 58 | 49 | 47 |
| Verbindungsaufbau läuft ins Leere | 10.022 | 1.523 | 2.501 |
| Verbindung steht, Verschlüsselung bleibt stumm | 10.024 | 1.522 | 2.501 |
| Anfrage angenommen, keine Antwort | 10.022 | **10.022** | 2.501 |

Windows (Turnier-PC, 17.11.2026, Spanne über vier vollständige Läufe bei gutem Netz):

| Ausfallart | wie heute (Frist 10 s) | mit Verbindungsfrist 1,5 s | mit harter Frist 2,5 s |
|---|---:|---:|---:|
| Verbindung wird abgelehnt | **2.027–2.046** | 1.504–1.516 | 2.028–2.048 |
| Name unbekannt, sofortige Absage | 4–30 | 1–2 | 0–2 |
| Verbindungsaufbau läuft ins Leere | 10.006–10.008 | 1.505–1.516 | 2.507–2.517 |
| Verbindung steht, Verschlüsselung bleibt stumm | 10.003–10.015 | 1.503–1.513 | 2.505–2.514 |
| Anfrage angenommen, keine Antwort | 10.001–10.006 | **10.005–10.015** | 2.504–2.511 |

Ohne jede Frist gibt ein Verbindungsaufbau ins Leere unter Windows nach 21 s auf, unter Linux
nach 30 s noch nicht (dort bricht das Werkzeug ab). Zwei weitere Windows-Läufe wurden vorzeitig
abgebrochen und sind nicht gewertet; ihre Werte liegen in denselben Spannen.

„Verbindung wird abgelehnt" ist unter Windows kein Sofort-Fehler: Der Fehler kommt nach rund
2 s, in jedem Lauf. Die Verbindungsfrist kürzt das auf 1,5 s.

### Gutes Netz (je Dienst fünf Abrufe wie heute: frischer Client, nur Gesamtfrist)

Gemessen ist damit Verbindungsaufbau plus eine kurze Antwort, **keine Vertonung**.

Linux:

| Dienst | Median | Maximum | App-Weg mit ungültigem Schlüssel |
|---|---:|---:|---:|
| Azure | 138 | 143 | 192 (HTTP 401) |
| ElevenLabs | 267 | 284 | 149 (HTTP 401) |
| OpenAI | 92 | 99 | 196 (HTTP 401) |
| Gemini | 170 | 171 | 138 (HTTP 400) |

Windows (vier Läufe; Median je Lauf als Spanne, Maximum über alle):

| Dienst | Median | Maximum | Erster Abruf im Lauf | App-Weg mit ungültigem Schlüssel |
|---|---:|---:|---:|---:|
| Azure | 79–84 | 85 | 211–495 | 97–165 (HTTP 401) |
| ElevenLabs | 209–236 | 242 | 250–502 | 99–104 (HTTP 401) |
| OpenAI | 60–65 | 84 | 84–127 | 169–204 (HTTP 401) |
| Gemini | 92–131 | 144 | — | 89–110 (HTTP 400) |

Namensauflösung höchstens 15 ms, Verbindungsaufbau allein 11–43 ms. Der erste Abruf eines
Laufs ist der teure: bis 0,5 s.

### Echte Vertonung (Windows, gutes Netz, OpenAI, 109 Zeichen in einem Stück)

| Vertonung | 1 | 2 | 3 | 4 | 5 | Median |
|---|---:|---:|---:|---:|---:|---:|
| ms | 2.750 | 2.959 | 2.068 | 2.061 | 1.571 | 2.068 |

**Zwei von fünf Vertonungen lagen über der harten Frist von 2,5 s** — bei gesundem Netz. Mit
dem Ausgangswert spräche an diesem PC etwa jede zweite bis dritte Ansage die Ersatzstimme,
ohne dass etwas ausgefallen ist. Gemessen ist nur OpenAI (der am PC gewählte Dienst), nur
fünfmal, nur eine Ansage in einem Stück.

### Netzwerkadapter aus (Windows, zwei Läufe direkt nacheinander)

Alle vier Dienste scheitern auf jedem Weg nach 0–21 ms an der Namensauflösung; der zweite Lauf
gleicht dem ersten. Nichts hängt. Der Adapter war abgeschaltet, das Kabel nicht gezogen — für
Windows dieselbe Lage (keine Schnittstelle, keine Route).

### Lage `ohne-internet` (Windows, zwei Läufe direkt nacheinander)

Alle vier Dienste scheitern auf jedem Weg nach 0–15 ms an der Namensauflösung. Der
Verbindungsaufbau zu einer fremden Adresse scheitert nach 0–2 ms mit „nicht erreichbar". Der
zweite Lauf gleicht dem ersten. Nichts hängt.

**Das war nicht „Router ohne Internet".** Bei diesen Läufen war das Netzwerkkabel am PC
gezogen (Lage `kabel` des Messplans, nur anders benannt); der Router war vom PC aus nicht
erreichbar. Die Werte gleichen „Netzwerkadapter aus" in jeder Zeile — auch das scheitert
sofort. Der Fall, in dem der Router antwortet und nur das Internet dahinter fehlt, steht im
nächsten Abschnitt.

### Router ohne Internet (Windows, zwei Läufe direkt nacheinander)

Hergestellt mit einem Handy-Hotspot ohne mobile Daten: Der PC hängt per WLAN an einem Gerät, das
ihm einen Weg nach draußen nennt, aber kein Internet dahinter hat.

| Ziel | wie heute | mit Verbindungsfrist | mit harter Frist | ohne Frist |
|---|---|---|---|---|
| alle vier Dienste | 6–35 ms | 8–40 ms | 7–42 ms | – |
| fremde Adresse | 10,0 s | 1,5 s | 2,5 s | 21 s |

- **Die Dienste scheitern sofort**, auf jedem Weg, an der Namensauflösung. Der zweite Lauf
  zeigt dasselbe Bild.
- **Eine fremde Adresse läuft ins Leere.** Das Handy weist die Pakete nicht ab, es antwortet
  schlicht nichts. Bei „Adapter aus" und „Kabel gezogen" meldete Windows an derselben Stelle
  sofort „nicht erreichbar". Das ist die erste Messung am echten Netz, in der eine Verbindung
  still verschwindet.

Die Lage hat damit zwei Gesichter. Muss der PC den Namen des Dienstes neu auflösen, kommt der
Fehler sofort. Kennt er die Adresse noch oder hält er eine offene Verbindung, geht die Anfrage
ins Leere: 10 s wie heute, 1,5 s mit Verbindungsfrist, bei offener Verbindung erst mit der
harten Frist. Den zweiten Fall belegt die Messung nur mittelbar über die fremde Adresse. Bei
den Läufen hatte der PC das Netz gerade gewechselt; ein Ausfall mitten im Betrieb ist nicht
gemessen. Ein Hallen-Router kann sich zudem anders verhalten als ein Handy.

### Was das für die Entscheidung heißt

- **Die Verbindungsfrist allein reicht nicht** — unter Windows wie unter Linux. Sie deckt
  Verbindungsaufbau und Verschlüsselung, aber nicht die Gegenstelle, die eine Anfrage annimmt
  und schweigt. Nur die harte Frist bricht dort ab. Entscheidung 1 (beides) trägt.
- **Die harte Frist greift pünktlich**: unter Linux auf 1 ms, unter Windows 4–17 ms später.
- **Die Verbindungsfrist von 1,5 s hat Abstand.** Bei gutem Netz braucht selbst der erste Abruf
  samt Antwort höchstens 0,5 s.
- **Die harte Frist von 2,5 s hält nicht**, solange sie die ganze Vertonung umschließt und
  OpenAI in einem Stück vertont. Drei Wege standen zur Wahl (gewählt: der erste, siehe
  „Entscheidung zu den Fristen"):
  1. Frist anheben (über 3 s). Dann wird der Zielwert „höchstens 5 s Stille bei der ersten
     Ansage nach einem Ausfall" eng: Frist plus Rechenzeit der Ersatzstimme.
  2. Frist nur bis zum **Beginn der Antwort** (erstes Byte) zählen, den Rest unter der
     Gesamtfrist lassen. Das trennt „Dienst tot" von „Dienst rechnet". Das Werkzeug misst diese
     Zeit bisher nicht.
  3. Frist je Dienst.
- **Kein Netz ist der harmlose Fall.** Windows meldet den Fehler nach Millisekunden; die Ansage
  wartet dort schon heute nicht. Das gilt für „Adapter aus" und „Kabel am PC gezogen".
- **Router ohne Internet ist der Fall, für den die Fristen da sind.** Dort verschwinden
  Verbindungen still. Solange der PC die Adresse des Dienstes noch kennt, wartet die Ansage
  heute 10 s. Die Verbindungsfrist kürzt das auf 1,5 s, wenn die Verbindung neu aufgebaut
  wird. Hält die App noch eine offene Verbindung zum Dienst, bricht nur die harte Frist ab.
  Danach hält der Merker die folgenden Ansagen frei.
- **Die Verbindungsfrist umschließt auch die Namensauflösung.** Das ist an der Quelle von
  reqwest 0.13.5 gelesen, aber weiter nicht gemessen: In keiner gemessenen Lage hing die
  Namensauflösung.

### Entscheidung zu den Fristen (17.11.2026)

| Konstante | Wert | Stand |
|---|---|---|
| Harte Frist | **4 s** | festgelegt (Ausgangswert 2,5 s) |
| Verbindungsfrist | **1,5 s** | festgelegt (Ausgangswert bestätigt) |
| Prüftakt | 15 s | Ausgangswert, am Bau von E1 zu prüfen |
| Höchstdauer des Merkers | 10 Minuten | Ausgangswert, am Bau von E1 zu prüfen |

**Harte Frist anheben statt umbauen.** Die längste gemessene Vertonung brauchte 3,0 s; 4 s
lassen ein Drittel Abstand und liegen weit unter den 10 s von heute.

- Nicht gewählt: Frist nur bis zum Beginn der Antwort. Das Werkzeug misst diese Zeit nicht; es
  bräuchte eine Erweiterung und eine Nachmessung. Ob ein Dienst die Antwort beginnt, bevor er
  fertig gerechnet hat, ist nicht gemessen. Der Weg bleibt offen, falls 4 s im Feld stören.
- Nicht gewählt: Frist je Dienst. Gemessen ist nur ein Dienst.

**Der Preis.** Hält die App beim Ausfall noch eine offene Verbindung zum Dienst, wartet die
erste Ansage 4 s und dann die Rechenzeit der Ersatzstimme. Der Zielwert „höchstens 5 s Stille"
wird in diesem einen Fall voraussichtlich überschritten; die Spec nennt die Ausnahme. Muss die
Verbindung neu aufgebaut werden, endet das Warten nach 1,5 s. Alle folgenden Ansagen warten
dank Merker nicht.

**Worauf die 4 s stehen.** Auf fünf Vertonungen an einem Dienst, mit einer Ansage in einem
Stück, bei gutem Netz. Das ist dünn. Die Konstante ist per Hotfix änderbar; E1 schreibt die
Dauer jeder Vertonung ins Log, damit der Wert am ersten Turnier nachgeprüft werden kann.

### Offen

- **Ausfall mitten im Betrieb**: Internet fällt weg, während die App läuft und den Dienst kurz
  zuvor erreicht hat (Adresse bekannt, Verbindung offen). Gemessen ist nur der frische Start
  ohne Internet. Dazu ein echter Hallen-Router statt eines Handys. Nachholbar vor einem
  Turnier; in E1 am Bau zu prüfen.
- Hotspot mit Anmeldeseite und langsames Netz.
- Echte Vertonung breiter: mehr Wiederholungen, Azure zum Vergleich, Zeit bis zum Beginn der
  Antwort getrennt von der ganzen Vertonung.
- Das Werkzeug vertont **in einem Stück**; mit „Ansage in Teilen" stellt die App mehrere
  Anfragen je Ansage. Diese Lücke bleibt — die harte Frist für eine Ansage in Teilen ist danach
  zu schätzen (Teile × Einzelwert, soweit sie nacheinander laufen) und in E1 am Bau zu prüfen.
- Die Ausgabe des Werkzeugs nennt von einem Fehler nur feste Kennwörter, nie den Fehlertext:
  rustls schreibt bei einer Anmeldeseite die Namen aus deren Zertifikat hinein (Router, Hotel).
  Für die Lage `hotspot` heißt das Kennwort `tls-zertifikat`.

## Nachtrag 17.11.2026: eigene Frist für Gemini (v0.9.444)

Gemini braucht für eine Ansage oft länger als 4 s und fiel deshalb auch bei gutem Internet auf die
Ersatzstimme zurück — samt Merker, der dann 15 s lang jede weitere Ansage auf die Ersatzstimme legt.
Der Nutzer hat entschieden: Gemini bekommt eine eigene harte Frist von **10 s**
(`tts_netzlage::GEMINI_FRIST`, halb so lang wie die Anfragefrist von 20 s). Umgesetzt als Methode
`harte_frist()` am Trait `Stimme`; die Frontend-Sicherung folgt mit 10,5 s
(`vertonungFristMs`). Preis: Im Ausfall wartet die erste Gemini-Ansage bis zu 10 s. Ungemessen.
