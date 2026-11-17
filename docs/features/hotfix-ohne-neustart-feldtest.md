# Feldtest-Protokoll: Hotfix ohne Neustart

Gehört zur Spec [hotfix-ohne-neustart](hotfix-ohne-neustart.md) (Schritt 9).
Hier steht, **was am Windows-PC noch zu zeigen ist**, wie man es zeigt und was
dabei herauskam. Alles, was sich ohne Windows prüfen lässt, prüfen die Tests
der Spec (Abschnitt „Tests"; alle dort genannten Tests existieren unter diesem
Namen, Stand v0.9.441).

> **Stand 17.11.2026:** Ein echter Wechsel ist gelaufen (v0.9.439 → v0.9.440).
> Die Lücke der Übertragung ist noch **nicht** gemessen, der Rückweg noch nie
> gelaufen. Erfolgskriterium 2 ist offen.

## Was schon gemessen ist

| Datum | Rechner | Was | Ergebnis |
|---|---|---|---|
| 16.11.2026 | Windows-Runner der CI | Sonde `uebergabe_probe` | laufende exe umbenennbar (2,4 ms), Wächter überlebt, erster Start 335 ms |
| 16.11.2026 | Windows-11-Arbeitsrechner mit Unternehmens-Schutzsoftware | Sonde, vier Läufe | erster Start einer frischen Datei 5–20 s, danach ~0,2 s; eine Umbenennung je Tausch blockiert bis 9,7 s → Tausch liegt vor dem Anhalten (ADR 0100) |
| 16.11.2026 | Windows-PC | **erster echter Wechsel**, v0.9.439 → v0.9.440 | Wächter-Start → Ankunft 2734 ms (1825 ms bis zum Start der neuen Version, 908 ms bis zur Meldung); am Bildschirm „nur wenige Sekunden" |

Die 2734 ms sind **nicht** die Lücke der Übertragung: Die beginnt kurz vor dem
Start des Wächters und endet, wenn der Tablet-Port wieder offen ist.

## Die Lücke auswerten

Jeder Wechsel, Rückweg und Installer-Lauf **bei laufender Übertragung**
hinterlässt im Log eine Zeile `Wiederanlauf nach Update: Tablet-Port wieder
offen, Lücke N ms`. Das Skript sammelt sie aus allen Logdateien, ordnet sie
dem Weg zu und rechnet gegen Kriterium 2:

```
node scripts/luecken-auswerten.mjs --seit 2026-11-17 "%LOCALAPPDATA%\de.badhub.btslight\logs"
```

Es gibt eine Tabelle zum Einfügen in dieses Protokoll aus und sagt
**ERFÜLLT**, **VERFEHLT** oder **OFFEN**. Dazu vier Regeln:

- Wechsel und Rückweg sind derselbe Ablauf und zählen gemeinsam.
- **`--seit` auf den Tag des Feldtests setzen.** Das Kriterium verlangt beide
  Wege „im selben Verfahren" am selben Rechner; Lücken früherer Updates
  gehören nicht hinein. Ohne `--seit` zählt alles, was im Ordner liegt.
- Ein **Rückfall** (der Wächter hat zurückgerollt, die Zielversion hat sich
  nie gemeldet) steht in der Tabelle, zählt aber nicht als Tausch-Lauf.
- Eine Lücke **ohne erkennbaren Weg** hält das Urteil auf OFFEN, bis sie von
  Hand zugeordnet oder mit `--seit` ausgeblendet ist.

Die Drittel-Regel vergleicht Median mit Median.

Kriterium 2: am Referenz-PC je zehn Läufe, **Median ≤ 3 s, Maximum ≤ 5 s**,
und höchstens ein Drittel des Installer-Wegs.

## Zehn Läufe mit einer einzigen neuen Version

Voraussetzung: v0.9.441 ist installiert und es gibt **eine** neuere
veröffentlichte Version (hier vN). Die Übertragung läuft (BTP verbunden,
LAN-Modus), sonst wird keine Lücke geloggt.

Ein Durchgang:

1. Banner „Update (vN) liegt bereit" abwarten.
   - **Tausch-Lauf:** warten, bis der Knopf **Jetzt wechseln** heißt, dann
     klicken.
   - **Installer-Lauf:** sofort **Jetzt neu starten** klicken — den Knopf
     gibt es nur, solange die neue Programmdatei noch nicht vorbereitet ist
     (Laden und Probelauf dauern einige Sekunden).
2. Nach dem Start von vN: **Wartung → Zurück zu v0.9.441**. Das ist der
   zweite Lauf dieses Durchgangs (ein Tausch).
3. Zurücksetzen, damit vN wieder angeboten wird: bts-light beenden, in
   `%APPDATA%\de.badhub.btslight\` die Dateien **`rueckweg.json` und
   `uebergabe.json`** löschen, bts-light starten. (Nur `rueckweg.json` zu
   löschen genügt nicht — aus `uebergabe.json` trägt die App die Sperre beim
   Start wieder ein.)

Schritt 2 setzt nach einem Installer-Lauf voraus, dass Prüfung 6 unten
besteht (der Knopf ist da). Fehlt er, bleibt für den Rückweg nur der Durchgang
mit **Jetzt wechseln**.

Zehn Durchgänge mit Installer-Lauf ergeben zehn Installer- und zehn
Tausch-Läufe; dazu ein paar Durchgänge mit **Jetzt wechseln**, damit auch der
Wechsel vorwärts dabei ist.

Bei jedem Durchgang nebenbei ansehen:

- Läuft die Übertragung danach **ohne Klick** weiter?
- Ist eine vorher gesetzte **Pause der Feldvergabe** (TL-Web) noch gesetzt?
- Stehen die Spiele noch auf ihren Feldern, mit der alten Spielzeit?

## Einzelprüfungen

| # | Prüfung | Wie | Erwartet | Ergebnis |
|---|---|---|---|---|
| 1 | Lücke, zehn Tausch- und zehn Installer-Läufe | siehe oben | Kriterium 2 | offen |
| 2 | Wechsel unter Last | viele Tablets verbunden, auf mehreren Feldern wird gezählt, dann **Jetzt wechseln** | Tablets zählen weiter und verbinden sich von selbst wieder; kein Punkt geht verloren | offen |
| 3 | Cloud-Betrieb | Verbindungsart Cloud oder LAN + Cloud, ein Monitor über die Cloud, dann wechseln | keine Offline-Blende am Monitor; im Relay nie zwei Hosts (`/health`) | offen |
| 4 | Erzwungener Rückfall | siehe unten | nach spätestens ~20 s läuft die alte Version mit Übertragung, rote Meldung, Version nicht erneut zum Wechsel angeboten | offen (volle App); mit der Sonde am 16.11.2026: Fehlstart → alte Version aktiv nach ~1 s |
| 5 | Rückweg | **Zurück zu v…** im Banner oder unter Wartung | vorige Version läuft mit Übertragung; „… wurde zurückgenommen" im Banner; die Version wird nicht erneut angeboten | offen |
| 6 | Rückweg nach Installer | Update über **Jetzt neu starten**, danach Wartung | Knopf **Zurück zu v…** ist da (die vorige Version liegt als `bts-light.vorher.exe` neben dem Programm) | offen |
| 7 | „Beim Beenden einbauen" | vormerken, beenden, neu starten | neue Version läuft; unter Wartung steht der Rückweg | offen |
| 8 | Seiten-Tausch mit Tablet im Spiel | Seiten-Paket übernehmen, während ein Tablet zählt und eines wartet | wartendes Tablet lädt binnen zweier Ping-Takte neu; das zählende zeigt nur den Hinweis; keine Verbindung reißt ab | offen — braucht den ersten Lauf des Paket-Jobs (Environment `seiten-hotfix`, Admin) |
| 9 | Deinstallation | bts-light deinstallieren | im Installationsordner bleibt keine `bts-light.*.exe` | offen |
| 10 | Alte Installation | ein PC mit einer Version vor v0.9.431 | aktualisiert wie bisher über das Installationsprogramm | offen — Updates auf v0.9.439 liefen am 16.11.2026 über das Installationsprogramm, von welcher Version aus ist nicht festgehalten |

### Rückfall erzwingen (Prüfung 4)

Die ausgelieferte App hat dafür keinen Schalter. Von Hand geht es so: Die neue
Version wird beendet, bevor sie sich melden kann (das dauert rund eine
Sekunde). In einer PowerShell **unmittelbar vor** dem Klick auf „Jetzt
wechseln" starten:

```powershell
$t0 = Get-Date
while ($true) {
  $neu = Get-Process bts-light -ErrorAction SilentlyContinue | Where-Object { $_.StartTime -gt $t0 }
  if ($neu) { $neu | Stop-Process -Force; break }
  Start-Sleep -Milliseconds 20
}
```

Die Schleife beendet genau **einen** neu gestarteten Prozess `bts-light` und
hört dann auf — die danach vom Wächter gestartete alte Version läuft weiter.
(Der Wächter selbst heißt `bts-light.vorher` und ist nicht betroffen.) Dieser
Weg ist nicht erprobt; gelingt das Timing nicht, kommt die neue Version
einfach an.

Danach in `%APPDATA%\de.badhub.btslight\uebergabe-waechter.log` nachsehen:
eine Zeile `Rückfall: die neue Version endete mit Code …, bevor sie sich
meldete` (oder `… wurde beendet, bevor sie sich meldete`), dann
`alte Version wieder gestartet`.

## Ergebnisse

*(Ausgabe von `luecken-auswerten.mjs` hier einfügen, mit Datum und Rechner.)*

## Was danach in die Spec gehört

- Kriterium 2 mit den gemessenen Zahlen — oder, wenn verfehlt, die Zahlen und
  die Entscheidung dazu (ADR 0100).
- Die Wächter-Frist (Startwert 15 s) bestätigen oder ändern.
- Im Handbuch (`docs/erste-schritte.md`) den Satz „Der schnelle Wechsel ist
  neu …" streichen, wenn die Prüfungen 1–7 bestanden sind.
