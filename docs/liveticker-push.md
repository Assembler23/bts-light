# Liveticker-Push: die Strecke Turnier-PC → badhub

Wie bts-light seine Daten an badhub sendet, was bei Störungen passiert und was
davon im Dashboard und im Log steht. Spec:
[badhub-push-entkoppeln](features/badhub-push-entkoppeln.md) ·
ADR [0086](adr/0086-ein-geordneter-sender-fuer-die-badhub-strecke.md) ·
[0087](adr/0087-voller-tset-nur-bei-strukturaenderung.md).

> **Stand:** v0.9.381 setzt den ersten Schritt um (Fristen, Fehlerklassen,
> Halt, Drossel, Backoff ohne Logo, Log-Flanken). Der Sync-Zyklus wartet
> weiterhin auf die Antwort von badhub — höchstens eine Frist je Versuch.
> Die volle Entkopplung (eigener Sender-Task) folgt mit den nächsten Schritten
> der Spec.

## Die fünf Kanäle

Alle gehen an denselben Endpunkt (`live_update.php`) mit derselben
Bearer-Anmeldung; sie unterscheiden sich nur im Nachrichtentyp.

| Kanal | Inhalt | Wann |
|---|---|---|
| `tset` | voller Turnierstand | erster Lauf, Struktur- oder Mehrfachänderung, alle 60 s als Lebenszeichen, nach jeder Störung |
| `tupdate_match` | ein Punktestand | je Tablet-Punkt sofort; aus dem Zyklus bei genau einem geänderten Spiel |
| `centry_list` | Meldeliste für den Hallen-Check-In | bei Änderung |
| `sched` | Spielplan | höchstens minütlich; ein leerer Plan wird nicht gesendet |
| `tverlauf` | Punktverlauf | bei Änderung |

Der Liveticker (`tset`, `tupdate_match`) ist die zeitkritische Funktion; die
drei anderen sind additiv und laufen danach.

## Fristen

Verbindungsaufbau 5 s, ganze Anfrage 15 s (`badhub/push.rs`). Ohne die
Verbindungsfrist wartete eine Anfrage in ein „schwarzes Loch" die volle
Gesamtfrist ab.

**Ein Status 200 allein ist kein Erfolg.** Ein WLAN mit Anmeldeseite (Captive
Portal) antwortet mit 200 und HTML. bts-light liest deshalb den Antwortkörper
(höchstens 64 KiB) und verlangt ein JSON-Objekt mit einem Text-Feld `type` —
so antwortet badhub immer (`answer`, `ok`, `error`).

## Fehlerklassen

„Strecke" wirkt auf alle Kanäle, „Kanal" nur auf den betroffenen
(`badhub/strecke.rs`, `klassifiziere`).

| Ausgang | `tset` | `tupdate_match` | Meldeliste | `sched` | `tverlauf` |
|---|---|---|---|---|---|
| 200 mit badhub-Antwort | ok | ok | ok | ok | ok |
| 200 ohne badhub-Antwort | Strecke gestört | ← | ← | ← | ← |
| keine Antwort (Netz, Frist, DNS, TLS) | Strecke gestört | ← | ← | ← | ← |
| 3xx, 408 | Strecke gestört | ← | ← | ← | ← |
| 5xx | Strecke gestört | Strecke gestört | Kanal: Backoff | Kanal: Backoff | Kanal: Backoff |
| 429 | Strecke gedrosselt | ← | ← | ← | ← |
| 401 mit `WWW-Authenticate: Basic` | Halt (Anmeldewand) | ← | ← | ← | ← |
| 401, 403 mit badhub-Antwort | Halt (Passwort) | ← | ← | ← | ← |
| 401, 403 ohne badhub-Antwort (Filter, Proxy) | Strecke gestört | ← | ← | ← | ← |
| 400 | abgelehnt | Punkt verwerfen | 30 min Pause | 30 min Pause | 30 min Pause |
| 404 | Strecke gestört | Strecke gestört | 30 min Pause | 30 min Pause | 30 min Pause |
| 409 | abgelehnt | Punkt verwerfen | 30 min Pause | 30 min Pause | Kanal: Backoff |
| 413 | einmal sofort ohne Logo, sonst abgelehnt | Punkt verwerfen | 30 min Pause | 30 min Pause | Kanal: Backoff |
| übrige 4xx | abgelehnt | Punkt verwerfen | 30 min Pause | 30 min Pause | 30 min Pause |

- **Gestört:** Wiederholung im Abstand 5, 10, 20, 40, 60, 60 … s. Der Versuch
  ist immer ein voller `tset`; alles andere wartet, bis er glückt. Nur ein
  gescheiterter `tset` verlängert den Abstand — melden zwanzig Felder
  zugleich einen Netzfehler, bleibt es bei 5 s.
  Tablet-Punkte werden in der Zeit verworfen — der volle Stand trägt sie nach.
- **Gedrosselt (429):** kein Versuch vor Ablauf von `Retry-After` (mindestens
  5 s, höchstens 60 min; ohne Angabe 60 s). badhub nennt bei seiner
  Tages-Sperre 24 Stunden; ein 429 zählt dort nicht als Fehlversuch, ein
  Versuch je Stunde verlängert die Sperre also nicht. Die Frist überlebt das
  Speichern der Einstellungen, nicht den Neustart der App.
- **Halt:** Nach der ersten Ablehnung des Passworts geht **nichts** mehr an
  den Liveticker-Endpunkt, auf keinem der fünf Kanäle, auch kein Tablet-Punkt
  — bis die Einstellungen gespeichert oder die Übertragung neu gestartet
  wird. Kein Probe-Versuch. Grund: badhub sperrt eine IP nach fünf
  Fehlversuchen für 15 Minuten und nach 50 am Tag für 24 Stunden.
  - Gehalten wird nur, wenn die Ablehnung erkennbar von badhub stammt (JSON
    mit `type`). Ein 401/403 eines Filters im Hallen-WLAN ist eine Störung.
  - Solange badhub das Passwort in der laufenden Übertragung noch nicht
    bestätigt hat, geht nur **eine** Anfrage zugleich hinaus — ein falsches
    Passwort kostet so genau einen Fehlversuch. Wird ein Passwort erst mitten
    im Turnier ungültig, können es so viele sein, wie gerade Anfragen
    unterwegs sind (Restrisiko).
  - Nicht am Halt hängen der Lese-Weg des Check-In-Zeitplans (pausiert nach
    einer Ablehnung selbst 30 Minuten) und der Sponsor-Endpunkt (nur beim
    Speichern). Sie gehen an andere Endpunkte.
- **Abgelehnt (`tset`):** nächster Versuch nach 60 s, dann 120, 240, 480,
  600 s. In der Zeit gehen auch keine Punkte.
- **30 min Pause:** badhub kennt den Kanal (noch) nicht oder lehnt ihn ab —
  etwa mit 409, wenn die Check-In-Kennung einem anderen Verband gehört. Der
  Liveticker läuft weiter.

## Logo

Das Turnierlogo (bis rund 2,7 MB) reist nur in einem vollen `tset` mit, und
nur wenn es nötig ist (geändert oder Stempel älter als 10 min), die Strecke
frei ist und der vorige `tset` geglückt ist. Im Fehlerfall geht es also nie
mit. Scheitert ein `tset` *mit* Logo, bleibt es 10 Minuten draußen; nach 413
bis zu einem anderen Logo. Der Stempel „Logo ist angekommen" überlebt das
Speichern der Einstellungen; die beiden Sperren nicht — nach dem Speichern
kann ein zu großes Logo also einmal wieder mitgehen.

## Im Dashboard

Eine Statuszeile; ein BTP-Fehler hat Vorrang. Die Texte der Strecke stehen in
[Oberflächen](oberflaechen.md). Orange heißt: es geht von selbst weiter. Rot
heißt: die Turnierleitung muss handeln (Passwort).

## Im Log

Je Zustandswechsel der Strecke **eine** Zeile (`badhub-Strecke: …`), solange
der Zustand anhält höchstens alle 10 Minuten eine Erinnerung, bei der Rückkehr
eine Zeile mit Dauer und Zahl der Versuche. Dasselbe gilt für
„BTP nicht erreichbar". Die Zeilen nennen Kanal, Klasse, HTTP-Status, Größe
und Dauer — nie einen Körper, nie einen Namen, nie das Passwort. Auf
Debug-Stufe steht je Sendung eine Messzeile (`badhub-Sendung`).

## Was bewusst nicht dabei ist

- Komprimierung: badhub nimmt kein gzip an.
- Der TP-Datei-Upload hat seinen eigenen Takt ([tp-datei-upload.md](tp-datei-upload.md)).
- Der Check-In-Lese-Weg und der Sponsor-Endpunkt laufen getrennt.
