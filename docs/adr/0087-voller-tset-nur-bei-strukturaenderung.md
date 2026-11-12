# 0087 — Voller `tset` nur bei Strukturänderung; Lebenszeichen = letzter voller `tset`

- **Status:** accepted
- **Datum:** 2026-11-11
- Spec: [`badhub-push-entkoppeln`](../features/badhub-push-entkoppeln.md).
- Setzt [ADR 0086](0086-ein-geordneter-sender-fuer-die-badhub-strecke.md) voraus (ein
  Sender, seriell).

## Kontext

Heute ergibt jede Runde mit zwei oder mehr geänderten Satzständen einen vollen `tset`
(gemessen 187 KiB bei 500 beendeten Spielen, bis rund 370 KiB am Deckel). Bei mehreren
zählenden Feldern ist das fast jeder 5-s-Zyklus — obwohl dieselben Punkte schon einzeln
über den Tablet-Weg hinausgingen. Umgekehrt hängt das 60-s-Lebenszeichen am letzten Push
**jeder** Art: Solange Einzel-Updates fließen, ist kein voller `tset` garantiert. badhub
frischt `updated_at` zwar auch bei `tupdate_match` auf, pflegt aber Spieler-Index, Gäste
(Fenster 30 min) und das Aufräumen der Kind-Turniere nur im `tset`-Zweig. Ein
`tupdate_match` für ein badhub unbekanntes Spiel erzeugt dort einen Rumpf-Eintrag.

## Entscheidung

1. **Ein voller `tset` geht nur bei Strukturänderung** — und sonst als Lebenszeichen.
   Struktur ist je Spiel: Status, Feld, Sieger, Sätze beendeter Spiele, Meldungen beider
   Seiten, Vorbereitungs-Aufruf. Satzstände laufender Spiele gehören nicht dazu.
2. **Lebenszeichen = letzter bestätigter VOLLER `tset` höchstens 60 s alt.** Ein
   `tupdate_match` verschiebt den Termin nicht. Das ist der Vertrag gegenüber badhub: Alles,
   was dort nur im `tset`-Zweig läuft, bekommt mindestens minütlich einen Anstoß.
3. **Punktänderungen gehen einzeln** als `tupdate_match` je Spiel, bis zu einem Deckel N
   (vorläufig 4, die Messung legt ihn fest); darüber ein voller `tset`.
4. **Tablet-Punkte gehen nur über den Punktweg.** Für tablet-getriebene Spiele leitet der
   Sender aus dem Loop-Stand keine Punkte ab; `apply_tablet_scores` meldet, welche Spiele
   das sind. Im vollen `tset` stehen sie mit dem Stand vom Sendezeitpunkt.
5. **Die Diff-Basis ist der zuletzt bestätigte volle `tset`**; daneben führt der Sender den
   bestätigten Satzstand je Spiel.
6. **`tset` vor Punkten:** Ohne bestätigte Basis geht kein `tupdate_match` hinaus.

## Verworfene Alternativen

- **So lassen (Vollstand ab zwei Änderungen):** einfach, aber im Normalbetrieb die größte
  Nutzlast im schnellsten Takt und doppelter Versand der Tablet-Punkte.
- **Immer N Einzel-Updates, nie ein Vollstand außer bei Struktur:** Ohne Deckel kostet eine
  Runde mit vielen Änderungen N serielle Anfragen (je eine Passwortprüfung bei badhub) und
  hält die Strecke länger auf als ein Vollstand. Und ohne das feste Lebenszeichen
  verhungerten Spieler-Index, Gäste und Kind-Turniere bei badhub.
- **Sammel-Nachricht „mehrere Punkte" in badhub:** die sauberste Form, setzt aber einen
  neuen Nachrichtentyp und einen badhub-Deploy voraus — in diesem Lauf ausgeschlossen.
- **Lebenszeichen am letzten Push jeder Art lassen:** siehe Kontext — bei fließenden Punkten
  nie ein voller `tset`.
- **Hand-Hallen und manuelle Reihenfolge in die Strukturmarke aufnehmen:** Sie leben
  außerhalb des Snapshots; der bestehende Verzug bis zum nächsten Vollstand (≤ 60 s) bleibt
  hingenommen.

## Folgen

- Im Normalbetrieb geht deutlich weniger hinaus: Vollstände nur bei Struktur und minütlich,
  jeder Tablet-Punkt genau einmal.
- Ein voller `tset` kann einen Tablet-Punkt nicht mehr zurückdrehen (ein Sender, Bau zum
  Sendezeitpunkt, Tablet-Stände unmittelbar davor übergelegt).
- Massenänderungen (Auslosung) können Vollstände häufen — höchstens einer je Zyklus; die
  Messung prüft, ob die Strukturmarke zu weit gefasst ist.
- Felder ohne Tablet bekommen ihren BTP-Stand als Einzel-Update binnen eines Zyklus.
- Ändert badhub, was nur im `tset`-Zweig läuft, oder seine Fristen (10/30 min), ist der
  60-s-Vertrag neu zu prüfen.
- Im Halt (Passwort) geht auch kein Lebenszeichen hinaus; der Liveticker gilt nach 10 min
  als veraltet. Das ist gewollt.
