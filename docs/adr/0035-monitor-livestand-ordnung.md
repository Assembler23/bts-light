# 0035 — Monitor-Livestand: schmaler Abruf, Ordnung über `seq`, additive Nudges

- **Status:** proposed
- **Datum:** 2026-08-18

Gehört zu [docs/features/monitor-livestand-push.md](../features/monitor-livestand-push.md).

## Kontext

Ein gezählter Punkt ist das häufigste Ereignis im Turnierbetrieb und
damit der Haupttreiber der Last. Heute nudgt der Server die betroffenen
Anzeigen datenlos (`{"court":7,"seq":42}`), woraufhin jede Anzeige den
**vollen** Zustand per HTTP holt. Für den Court-Monitor ist das billig —
er abonniert nur sein Feld (`?court=`) und holt ~0,3 Abrufe/s. Die
Feld-Übersicht dagegen wird von **jedem** Punkt **jedes** Feldes geweckt
und zieht dabei den Zustand **aller** Felder: bei 20 Feldern und 20
Übersichts-TVs grob 1,6–8 MB/s Hallen-WLAN für eine Information von
~20 Byte.

Drei Kräfte wirken gegeneinander:

1. **ADR 0016** (Push/Poll-Ordnung) hat „Score inline im Push"
   ausdrücklich **zurückgestellt** — mit der Begründung „eine
   Datenquelle, ein Renderpfad → kein Flackern, kein Rückwärtsspringen".
2. **ADR 0034** hat für den Zwillingsfall TL-Web „Daten im Push" vor
   einer Woche **verworfen** (zweite Wahrheit für Auth, ETag,
   Kürzungsleiter).
3. **ADR 0014** ist die Hauspräzedenz gegen Inkremente: der Punktverlauf
   ist der einzige echte Delta-Strom im Repo und heilt sich **nicht**
   selbst — ein verlorenes Frame friert den Satzverlauf ein, bis ein
   explizites `rally_sync` kommt.

Zugleich zeigte die Analyse, dass die teuerste Seite gar nicht der
Fanout ist, sondern die **Rechnung je Abruf** (`overview()` scannt je
Feld alle Matches und parst je Feld JSON) und der **Vollschreibvorgang
je Punkt** (`persist_scores` schreibt die komplette `live-scores.json`).
Beide sind unabhängig vom Transport zu lösen.

## Entscheidung

**(a) Schmaler Abruf statt Nutzlast im Push.** Der Nudge bleibt
datenlos. Neu ist ein optionaler Selektor `GET /health?court=<id>`
(Host und Relay), der dieselbe Antwortstruktur liefert, aber nur das
eine Feld — bedient aus dem neuen Antwortcache. Die Übersicht holt nach
einem Nudge nur noch das betroffene Feld und patcht nur dessen Karte.

Damit bleibt der Grundsatz von ADR 0016 **wörtlich** erhalten: es ist
derselbe Builder auf derselben Route, nur mit kleinerem Zuschnitt. Es
entsteht keine zweite Wahrheit auf dem Draht, und die Entscheidung steht
nicht im Widerspruch zu ADR 0034.

„Nutzlast im Nudge" ist damit **nicht** verworfen, sondern als
messbedingte Ausbaustufe festgeschrieben. Auslösekriterium (in der Spec
verbindlich): *Wenn die Nachmessung zeigt, dass Übersichts-TVs weiterhin
mehr als 20 Requests/s je Gerät ziehen oder der Pi mehr als 10
Frames/min verliert, wird sie gebaut* — dann als eigener ADR.

Bei dieser Gelegenheit wechselt **ADR 0016 von *proposed* auf
*accepted***: seine Umsetzung (Push/Poll-Ordnung A1) läuft seit v0.9.196 im Turnier.

**(b) Ordnung über `seq` in der Voll-Antwort.** Solange zwei Kanäle
denselben Bildschirm speisen, braucht es eine gemeinsame Ordnung, sonst
überschreibt eine verspätete Antwort einen neueren Stand. Die bereits
vorhandene Pro-Court-Sequenz reist deshalb additiv in den Voll-Antworten
mit (`CourtOverview.seq`, `MonitorState.seq`). Regel im Client:

- **Push** anwenden bei `seq > gezeigt`,
- **Voll-Antwort** anwenden bei `seq >= gezeigt`.

Das `>=` ist bewusst: der angezeigte Stand hat zwei Quellen — fällt der
Tablet-Stand weg, greift der BTP-Rückfall, und dabei ändert sich der
Wert, **ohne** dass ein Nudge kommt und ohne dass `seq` steigt.

Die Sequenz wird je Feld mit `now_ms()` geseedet statt mit 0, damit sie
über Prozess-Neustarts hinweg monoton bleibt (Hauspräzedenz:
`set_monitor_command`). **`seq` ist prozesslokal** und nur innerhalb
eines `BASE` vergleichbar — Host und Relay zählen in getrennten Räumen,
und eine Anzeige spricht immer mit genau einem von beiden.

**(c) Nudge-Felder sind strikt additiv.** Es gibt keine
Protokollversion, und es wird keine eingeführt — die Feldpräsenz *ist*
die Aushandlung. Bindend:

1. `court` und `seq` bleiben an derselben Stelle mit derselben Bedeutung.
2. Neue Felder sind optional und stehen unter genau einem Schlüssel
   (`live`, `hb`) — nie verstreut daneben.
3. Eine alte Seite darf ein neues Frame nie missdeuten. Das ist heute
   schon gegeben: die Nudge-Clients verwerfen jedes Frame ohne
   numerische `court`+`seq` und ignorieren Zusatzschlüssel.
4. Eine neue Seite muss ein datenloses Frame vollwertig behandeln — ein
   älterer Host oder Relay schickt nie Nutzlast.
5. Cloud (Relay-Deploy bei jedem main-Merge) und LAN (Release-Tag) sind
   getrennte Auslieferungsstufen; fest verdrahtete Monitore haben keinen
   Reload-Kanal und können unbegrenzt alte Seiten fahren.

## Alternativen

- **Nutzlast im Nudge** (`live: {matchId, sets, serving, pause}`).
  Zurückgestellt, nicht verworfen: die Wirkung wäre maximal (null
  Requests je Punkt), der Aufpreis gegenüber (a) beträgt aber nur die
  letzten 5 % der Bytes, während die Ordnung Push↔Fetch von „nützlich"
  zu „tragend" würde und eine zweite Wahrheit auf dem Draht entstünde.
  Machbar wäre sie nur, wenn die Nutzlast **am Sender** aus derselben
  Quelle abgeleitet wird, aus der die Voll-Route liest — im Relay also
  erst **nach** Stale-Verwerfen, „leerer Spiegel überschreibt nicht" und
  Größengrenze.
- **Inkrementelle Punktstände.** Verworfen. ADR 0014 zeigt am
  Punktverlauf, dass ein Delta-Strom sich nicht selbst heilt. Alles hier
  transportiert absolute Werte.
- **Nur Antwortcache, keine Transport-Änderung.** Als Endzustand zu
  wenig: die Rechnung am Turnier-PC verschwände, die Funkbytes und die
  Pi-Last blieben. Als erste Etappe ohnehin enthalten.
- **`court_state` inkrementell übertragen.** Verworfen: es ist zugleich
  der StateRestore-Träger für Cloud-Tablets und reist bewusst mit
  `history`/`rallyLog`. Ein inkrementeller Transport bräche die
  Wiederherstellung eines Ersatz-Tablets mitten im Spiel.
- **Protokollversion einführen.** Verworfen: fest verdrahtete Monitore
  haben keinen Reload-Kanal, eine Version müsste also ohnehin
  unbegrenzt beide Stände bedienen. Additive Felder leisten dasselbe
  ohne Zustand.

## Folgen

- Eine neue optionale Query (`?court=`) auf zwei bestehenden Routen,
  ein neues optionales Frame (`{"hb":…}`) auf einem bestehenden Kanal,
  ein neues optionales Feld (`seq`) in zwei bestehenden Antworten. Kein
  neuer Endpunkt, kein neuer Kanal, keine `relay-proto`-Typänderung
  außer einem `serde(default)`-Feld.
- **Kein Bruch in keiner Richtung:** alter Relay + neue Seite → Nudge
  datenlos, `?court=` ignoriert, volle Antwort, Seite ordnet über
  `court_id` zu. Neuer Relay + alte Seite → Zusatzfelder ignoriert,
  Heartbeat verworfen.
- **Es entsteht kein neuer Schreibweg.** Alles hier ist lesend; R5
  (`process_result`) bleibt die einzige Ergebnis-Validierung. Neu
  validiert wird ausschließlich der Selektor `?court=`.
- Der Aufschlag steht künftig auch im Cloud zur Verfügung (bisher
  `serving_team: null`), weil der Relay die Anzeigefelder einmal beim
  Speichern extrahiert statt bei jedem Abruf zu parsen.
  **Bekannte Restlücke:** `injury`/`official_call` erreichen den Relay
  überhaupt nicht und bleiben dort `false`.
- Negativ: Die Übersicht führt künftig zwei Renderpfade (Voll-Render und
  Teil-Patch) mit einer Zuständigkeitsgrenze, die falsch sein kann. Sie
  wird als reine, getestete Funktion geführt, und ein Zwangs-Voll-Render
  mindestens alle 30 s begrenzt den Schaden einer übersehenen Bedingung.
- Negativ: Ein langsamerer Fallback-Poll macht die Anzeige davon
  abhängig, dass „Kanal ist gesund" korrekt erkannt wird. Deshalb der
  sichtbare Heartbeat und der Config-Schalter, der ohne Release
  zurückschaltet.
