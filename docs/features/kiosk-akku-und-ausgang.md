# Kiosk-App: Akku auf der Wartekarte + Drei-Finger-Geste — Spezifikation

> Status: **freigegeben 2026-11-06**, umgesetzt in v0.9.343. Quelle: Nutzer-Anforderung
> vom 06.11.2026, Grill im Gespräch. Betroffen: nur `android/` (Kiosk-App),
> keine Rust-, Relay- oder Seitenänderung.

## Kontext / Problem

> Im Tablet wollen wir den Akkustand auch sehen, wenn wir das Tablet starten
> und es keinen BTS-Server finden kann. Was oft zu Hause nach dem Turnier so
> ist. Außerdem brauchen wir irgendwas Einfaches, um aus dem Kioskmodus
> rauszukommen oder die App zu schließen.

1. **Akku:** Den Akkustand zeigt bisher nur die Webseite des Turnier-PCs
   (`tablet.html`/`lobby.html` über `FullyBruecke`, siehe
   `docs/tablet.md` „Akkustand am Tablet"). Ohne Turnier-PC steht die
   **native Wartekarte** („Suche Turnier-PC …", „Kein WLAN") — ohne Akku.
   Und die Statusleiste blendet der Kiosk aus. Zu Hause sieht man also nie,
   ob das Tablet geladen werden muss.
2. **Ausgang:** Das Hüllen-Menü (u. a. „Kiosk verlassen") öffnet nur
   2 s Fingerdruck **links oben**. Dort liegen auf der Zählseite andere
   Bedienelemente; die Ecke ist oft nicht erreichbar.

## Entscheidungen aus dem Grill (06.11.2026)

| Frage | Entscheidung |
|---|---|
| Ausgang | **geheim** bleiben, keine sichtbaren Knöpfe (auch nicht auf der Wartekarte) |
| Geste | **drei Finger 2 s irgendwo halten** |
| Alte Ecken-Geste | **bleibt** als zweiter Weg |
| PIN | **ja**, die bestehende Kiosk-PIN wie bisher |

## Verhalten

### Akku auf der Wartekarte

- Rechts oben auf der Wartekarte steht **„🔋 73 %"**, am Ladegerät
  **„⚡ 73 %"** — dieselben Zeichen und Schwellen wie auf den Tablet-Seiten
  (`src/io/akku.mjs`): unter 20 % orange, unter 10 % rot, sonst grau.
- Gilt für **jeden** Zustand der Wartekarte (Suchen, Kein WLAN).
- Quelle: `ACTION_BATTERY_CHANGED` als Broadcast-Empfänger, registriert
  zwischen `onStart` und `onStop` — Android meldet jede Änderung von selbst,
  also **kein eigener Timer** (die Anzeige darf nicht am Akku zehren).
- Unbekannter Wert (`level < 0` oder `scale <= 0`) → keine Anzeige.
- Ist die Lobby/Zählseite geladen, ändert sich nichts: Dort zeigt die Seite
  den Akku wie bisher selbst.

### Drei-Finger-Geste

- Liegen **genau drei** Finger gleichzeitig auf dem Bildschirm und bleiben
  **2 s** liegen, öffnet sich die PIN-Abfrage und danach das bekannte
  Hüllen-Menü.
- Beobachtet in `dispatchTouchEvent`, wie die Ecken-Geste: Die Berührungen
  gehen weiter an die Seite; feuert der Timer, bekommt die WebView ein
  `ACTION_CANCEL`, damit ein angetippter Knopf nicht zusätzlich auslöst.
- Abbruch: ein Finger hebt ab, ein vierter kommt dazu, `ACTION_UP`/`CANCEL`.
  Fingerbewegung bricht **nicht** ab (drei Finger halten nie ganz still).
  Nach dem Auslösen erst wieder scharf, wenn alle Finger abgehoben sind.
- Ein zweiter Finger bricht die Ecken-Geste ab — sonst lösten ein Dreigriff,
  der in der Ecke beginnt, beide Gesten aus.
- **Einschränkung (Review 06.11.2026):** Die Zählseite zählt schon beim
  Aufsetzen (`onScoreTap`, pointerdown), damit ein Punkt ohne Verzögerung
  fällt. Der erste der drei Finger erreicht die Seite, bevor die Geste
  erkennbar ist — liegt er auf einer Punktefläche, ist der Punkt gezählt;
  das `ACTION_CANCEL` verhindert nur einen Click beim Loslassen. Ein
  Zurückhalten jedes ersten Fingers kostete jedem Punkt Latenz und wurde
  deshalb verworfen. Die Doku sagt: auf der Zählseite die Kopfzeile nehmen
  oder danach korrigieren. Dasselbe galt schon immer für die Ecken-Geste
  über „Letzten Punkt zurück". Auf der Wartekarte (Hauptfall: zu Hause ohne
  Turnier-PC) gibt es keine Seite.

### Menü

Unverändert bis auf die Beschriftung: **„Kiosk verlassen und App schließen"**
statt „Kiosk verlassen" — der Punkt tat das schon immer (Sperre aus,
Home-Vorgabe geräumt, `finishAffinity`), der Name verriet es nicht.

## Nicht-Ziele

- Kein sichtbarer Knopf, kein Hinweistext auf die Geste.
- Kein Akku-Takt an den Turnier-PC aus der Wartekarte (es gibt keinen PC).
- Keine Änderung an `tablet.html`/`lobby.html`.

## Umsetzung

| Stelle | Änderung |
|---|---|
| `kern/AkkuAnzeige.kt` (neu, rein) | `text(level, scale, plugged)`/`stufe(...)` — Spiegel von `akkuText`/`akkuStufe` aus `src/io/akku.mjs` |
| `kern/DreiFinger.kt` (neu, rein) | kleine Zustandsmaschine: `beruehrung(fingerzahl) → Start/Abbruch/nichts` |
| `KioskActivity.kt` | Akku-Empfänger in `onStart`/`onStop`, Text rechts oben; Drei-Finger-Zweig in `dispatchTouchEvent` mit eigenem Runnable; Ecken-Geste unverändert |
| `res/layout/activity_kiosk.xml` | `TextView` `wartekarte_akku` rechts oben (FrameLayout statt reiner LinearLayout-Mitte) |
| `res/values/strings.xml` | `menu_verlassen` neu beschriftet |

## Akzeptanzkriterien

1. Tablet zu Hause ohne Turnier-PC: Die Wartekarte zeigt rechts oben den
   Akkustand; Laden einstecken → „⚡" innerhalb weniger Sekunden.
2. 15 % → orange, 8 % → rot.
3. Drei Finger 2 s auf der Kopfzeile der Zählseite → PIN → Menü; kein
   Punkt gezählt.
4. Drei Finger, einer hebt nach 1 s ab → nichts passiert.
5. Ecken-Geste funktioniert weiter.
6. „Kiosk verlassen und App schließen" + PIN → normales Android, App zu.

## Tests

- JVM-Unit-Tests für `AkkuAnzeige` (Schwellen 20/10 exklusiv, Laden, unbekannt)
  und `DreiFinger` (Start bei drei, Abbruch bei 2/4/UP, Bewegung egal).
- Drift-Schutz gegen `src/io/akku.mjs`: ein kleines Node-Skript liest die
  Schwellen aus `AkkuAnzeige.kt` und vergleicht sie mit `AKKU_NIEDRIG`/
  `AKKU_KRITISCH` (Muster `scripts/test-entschlackung-liste.mjs`).
- Feldtest auf einem Fire HD (Akzeptanz 1, 3, 6).

## Doku-Pflicht

- `docs/tablet-android-app.md` — Wartekarte (Akku) und Hüllen-Menü (Geste,
  neuer Menüname).
- `docs/tablet.md` „Akkustand am Tablet" — Satz zur Wartekarte.
- `docs/roadmap.md`, `docs/changelog.md`.
