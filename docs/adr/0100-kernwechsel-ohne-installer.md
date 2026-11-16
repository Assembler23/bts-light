# 0100 — Kernwechsel ohne Installer: Umbenennen-Tausch am Ort, signierter Kopf, Wächter aus dem alten Image

- **Status:** accepted
- **Datum:** 2026-11-16

## Kontext

Ein Update läuft heute über den NSIS-Installer (ADR 0057): Paket vorgeladen,
Installer-Lauf, Neustart, Wiederanlauf der Übertragung über das WebView. Die
Lücke wird auf rund 20 s geschätzt, gemessen ist sie nicht. Für einen Fix im
laufenden Turnier ist das zu lang, und es gibt keinen Weg zurück.

Befunde am Code:

- Die Programmdatei ist die ganze App (keine `bundle.resources`, `dist/`
  eingebettet); die Firewall-Regeln sind port-, nicht programmgebunden.
- `tauri_plugin_single_instance` hält einen benannten Mutex; ein zweiter
  Prozess beendet sich sofort.
- Ein gescheitertes Binden von Port 8088 wird nur geloggt.
- Das Relay weist einen zweiten Host im Namespace ab (R4) und hält 20 s ohne
  Host aus.
- BTP verbindet je Abruf neu; es gibt nichts zu übergeben.
- Das einzige Update-Artefakt ist der Installer; `latest.json` ist unsigniert.

## Entscheidung

- **Artefakt:** die nackte `bts-light.exe`, im Release-Job aus dem fertigen
  Installer entnommen. Signiert wird ein kleiner **Kern-Kopf** (Version,
  SHA-256, Größe, `rueckwaerts_lesbar_ab`); die exe hängt per Hash daran.
- **Tausch am Ort:** `bts-light.neu.exe` ins Installationsverzeichnis, dann
  `bts-light.exe` → `bts-light.vorher.exe` und `neu` → `bts-light.exe`.
- **Probelauf vor dem Angebot:** `bts-light.neu.exe --selbsttest`. Schlägt er
  fehl, wird der Tausch nicht angeboten.
- **Strikte Reihenfolge:** alte App hält die Übertragung an und schließt die
  Relay-Verbindung, tauscht, startet den Wächter und endet; erst danach
  startet die neue App.
- **Wächter** ist die alte Programmdatei mit `--waechter`, ohne Tauri und
  ohne Mutex. Er wartet auf das Ende der alten App, startet die neue und
  stellt die alte wieder her, wenn die neue vor ihrer Ankunftsmeldung endet
  oder die Frist abläuft.
- **Wiederanlauf im Kern** (`.setup()`) statt über das WebView.
- **Messung eingebaut:** Die App loggt „Port zu → Port wieder offen".
- **Rückfall auf den Installer-Weg**, wenn der Tausch nicht möglich ist
  (kein Schreibrecht, nicht Windows, Probelauf gescheitert).
- **„Beim Beenden einbauen" bleibt der stille Installer** (ADR 0057).

## Alternativen

- **Versionsordner im App-Datenverzeichnis plus Starter-Programm.**
  Verworfen: Verknüpfungen und pfadgebundene Firewall-Ausnahmen brechen, es
  braucht ein zweites Binary.
- **Beim Installer bleiben, nur den Rückweg ergänzen.** Verworfen: Die
  Laufzeit des Installers bleibt die Untergrenze der Lücke.
- **Neue App wartet selbst auf das Ende der alten, ohne Wächter.** Verworfen:
  Kein Rückfall möglich, wenn die neue Datei gar keinen Code ausführt
  (Virenscanner, beschädigte Datei).
- **Alte App bleibt als Wächter am Leben.** Verworfen: Sie hält Mutex und
  WebView und verlängert die Lücke.
- **Die exe direkt signieren.** Verworfen: Eine minisign-Signatur über eine
  nackte exe ließe sich Altversionen über ein gefälschtes `latest.json` als
  Installer unterschieben.
- **Socket-Übergabe oder Kern als Dienst** (Roadmap Stufe 3). Verworfen für
  diese Spec: großer Umbau, offene WebSockets reißen trotzdem ab.

## Konsequenzen

- ADR 0057 gilt weiter für Vorladen, Marker und „Beim Beenden"; der
  Wiederanlauf wandert in den Kern.
- Mehrere Annahmen zum Windows-Verhalten (laufende exe umbenennbar, Wächter
  überlebt das Beenden, Virenscanner, Reste des Deinstallierers) sind im Repo
  nicht prüfbar. Ein Messschritt (`tests/uebergabe_probe.rs`) läuft, **bevor**
  der Tausch scharf geschaltet wird; sein Ergebnis und die Wächter-Frist
  werden hier nachgetragen.
- Zwischen den zwei Umbenennungen gibt es ein Millisekunden-Fenster, in dem
  ein Stromausfall die Installation ohne `bts-light.exe` zurücklässt.
- Die Versionsanzeige unter „Apps & Features" bleibt bis zum nächsten
  Installer-Lauf alt.
- Der Kernwechsel gilt nur für Windows mit per-user-Installation; Linux und
  MSI behalten den bisherigen Weg.
- Der Release-Job veröffentlicht zwei zusätzliche Artefakte und einen Index.
- Der Deinstallierer braucht einen Hook, der die Hilfsdateien entfernt.

## Ergebnis der Messung (PR 7a)

Messwerkzeug: `src-tauri/tests/uebergabe_probe.rs` (von Hand, `-- --ignored
--nocapture`, ohne `--release`). Es spielt den Ablauf mit der echten
Programmdatei in einem Wegwerf-Verzeichnis durch: erster Start, Tausch bei
laufender Datei, Erfolg, Fehlstart, Hänger.

**Was die Sonde zeigt und was nicht.** Alte und neue App sind *gespielt*
(Rollen derselben Programmdatei, nur im Debug-Build): Die alte hält einen
Port und endet 800 ms nach dem Start des Wächters, die neue beendet sich
sofort, wenn der Port noch belegt ist. Damit prüft die Sonde, dass der
Wächter das Ende der alten App überlebt, wirklich auf ihr Ende wartet und
ihren Port nicht erbt. Die volle App mit Fenster und Einzelinstanz-Sperre
startet sie nie — ob der Wechsel mit ihr trägt, zeigt erst der Feldtest nach
dem Anschluss (PR 7b).

| Messwert | Linux (Entwicklungsrechner) | Windows-Runner der CI (GitHub, 16.11.2026, #557) | Turnier-PC, Windows 11 + Defender |
|---|---|---|---|
| erster Start der frisch abgelegten Datei | 20 ms | 335 ms | **offen** |
| zweiter Start | 20 ms | 23 ms | **offen** |
| Tausch bei laufender Datei | gelingt, 51 µs | **gelingt**, 2,4 ms (Annahme 1) | **offen** |
| Ende der alten App → neue Version angekommen | 752 ms | 757 ms (**Wächter überlebt**, Annahme 2) | **offen** |
| Fehlstart → alte Version wieder aktiv | 601 ms | 554 ms | **offen** |
| Hänger → Rückfall | Frist + rund 500 ms | Frist + 470 ms | **offen** |
| Installationsverzeichnis beschreibbar | nicht anwendbar | keine Installation vorhanden | **offen** |

In den Zeiten „bis angekommen" stecken 400 ms Nachlauf des Wächters und 300 ms
gespielte Startzeit der neuen App.

**Stand der Annahmen:** Auf einem echten Windows (Runner der CI) lässt sich
die laufende Programmdatei umbenennen, und der Wächter überlebt das Ende der
App und wartet auf sie. Offen bleibt, was nur der Turnier-PC zeigt: das
Verhalten unter seinem Virenscanner (der Runner maß 335 ms für den ersten
Start einer frischen Datei), das Schreibrecht im echten
Installationsverzeichnis und die volle App mit Einzelinstanz-Sperre.

Die CI lässt die Sonde bei jedem PR auf einem Windows- und einem
Linux-Runner mitlaufen (ohne den PR zu blockieren); die Werte stehen im Log
des Schritts „Übergabe-Sonde". Das ersetzt den Lauf am Turnier-PC nicht.
**Der Tausch wird erst an das Banner angeschlossen (PR 7b), wenn die rechte
Spalte gefüllt ist.**
Daraus folgt auch die endgültige Frist des Wächters (Startwert 15 s): das
Mehrfache des gemessenen ersten Starts auf dem langsamsten Turnier-PC.
Scheitert Annahme 1 oder 2, bleibt der Kernwechsel beim Installer-Weg.

Von Hand zu prüfen, nicht Teil der Sonde: Reste nach der Deinstallation und ob
die Prüfsumme der installierten Datei der aus `kern/kern-<version>.json`
gleicht (die Sonde druckt sie).

### Erster Lauf an einem Windows-11-Arbeitsrechner (16.11.2026)

Auf einem Firmen-Notebook mit Unternehmens-Schutzsoftware endete schon der
erste Schritt nicht rechtzeitig: Der Probelauf der frisch abgelegten
Programmdatei (`--selbsttest`) lief **länger als die Frist von 10 s** und
wurde abgebrochen. Auf dem Windows-Runner der CI dauert derselbe Schritt
335 ms.

Nachgemessen von Hand (Debug-Build, dieselbe Shell):

| Start | Dauer |
|---|---|
| eben gebaute Programmdatei, erster Start | 12,8 s |
| frische Kopie davon, erster Start | 15,2 s |
| dieselbe Kopie, zweiter Start | 0,6 s |

Alle drei endeten mit Code 0. Es ist also die Prüfung beim **ersten** Start
einer neuen Datei; danach ist sie schnell.

Was daraus folgt:

- **Die Probelauf-Frist steht jetzt bei 120 s** (vorher 10 s). Der Probelauf
  läuft im Hintergrund vor dem Angebot und kostet keine Lücke — und er
  erledigt die Prüfung, bevor jemand klickt. Er ist damit nicht nur Test,
  sondern Vorwärmen.
- **Offen und entscheidend:** Bleibt die Datei „bekannt", nachdem sie von
  `bts-light.neu.exe` in `bts-light.exe` **umbenannt** wurde? Wenn ja,
  startet die neue Version nach dem Tausch in Sekundenbruchteilen und die
  Wächter-Frist von 15 s hat reichlich Luft. Wenn nein, kostet der Wechsel
  hier rund 15 s und die Frist muss darüber liegen. Die Sonde misst genau
  das im Schritt „Erfolg" (`ende-der-app-bis-ankunft`): Dort wird eine schon
  zur Probe gelaufene Datei umbenannt und gestartet.
- Gemessen wurde der Debug-Build; er ist um ein Mehrfaches größer als die
  ausgelieferte Programmdatei (33,8 MB). Deren Prüfzeit ist nicht gemessen.
- Ein Firmen-Notebook ist kein typischer Turnier-PC. Maßgeblich bleibt der
  Lauf auf einem Rechner, wie er in der Halle steht.

Die Sonde misst seither mit einer Obergrenze von 300 s, statt abzubrechen,
misst zusätzlich den Start der schon vorhandenen Datei und richtet die
Fristen der folgenden Schritte am Gemessenen aus.

### Was der Review vor dem Anschluss geändert hat (16.11.2026)

- **Der Wächter wartet auf das Ende des Prozesses, nicht nur auf das
  Schließen der Leitung.** Startete die neue App, solange die alte noch
  läuft, beendete ihre Einzelinstanz-Sperre sie sofort — und der Wächter
  rollte ein gutes Update zurück. Er öffnet den Prozess der alten App gleich
  bei seinem Start (kein Verwechseln bei neu vergebener Nummer), wartet bis
  zu 20 s und lässt danach 400 ms Luft.
- **Der Wächter schreibt den Zustand „gestartet" selbst**, unmittelbar vor
  dem Start der neuen App. Ein „angekommen" von einer früheren Übergabe kann
  so nie als Ankunft gelten.
- **Nach einem Rückfall startet die alte Version in jedem Fall**, sobald die
  Dateien zurückgetauscht sind — auch wenn sich Grund und Marker nicht
  schreiben lassen.
- **Der Wächter bewacht nur seine Nachbardatei** (`x.vorher.exe` → `x.exe`);
  ein Pfad lässt sich nicht vorgeben.
- **Umbenennen mit Geduld** (bis 2 s), weil ein Virenscanner frische
  Programmdateien kurz festhält.

### Bekannte Grenzen des Wächters

- Der Wächter ist immer die **alte** Programmdatei. Ein Wechsel ist deshalb
  nur **von** einer Version möglich, die `--waechter` schon kennt — der
  erste Schritt auf eine solche Version läuft über den Installer.
- Steckt die App in einem Job, der beim Schließen alle Prozesse beendet und
  keinen Ausbruch erlaubt, stirbt der Wächter mit ihr. Dann startet niemand
  die neue Version; beim nächsten Start von Hand läuft sie.
- Startet jemand während des Wechsels von Hand eine zweite bts-light, kann
  der Wächter deren Lauf mit der bewachten verwechseln. PR 7b muss das
  abfangen (Ankunft an die Prozessnummer binden).
