# 0086 — Ein geordneter Sender für die badhub-Strecke

- **Status:** accepted
- **Datum:** 2026-11-11
- Spec: [`badhub-push-entkoppeln`](../features/badhub-push-entkoppeln.md).
- Zusammen mit [ADR 0087](0087-voller-tset-nur-bei-strukturaenderung.md) (was gesendet wird).

## Kontext

Der Sync-Loop wartet je Zyklus vier Pushes an badhub nacheinander ab (`tset` bzw.
`tupdate_match`, Meldeliste, `sched`, `tverlauf`), jeden mit 15 s Gesamtfrist und ohne
`connect_timeout`. Hängt die Internet-Strecke, verzögert das den nächsten Zyklus — und damit
Feldvergabe, Rotation, Autodruck und Erst-Ansage. Daneben sendet je Feld ein eigener
Arbeiter die Tablet-Punkte (`ScorePushQueue`), ohne Backoff und ohne Auth-Sperre. badhub
kennt keine Reihenfolge und schreibt `tupdate_match` ohne Sperre: Zwei unabhängige Sender
können einen frischen Punkt mit einem älteren Vollstand überholen, und parallele Punkte
verschiedener Felder können sich bei badhub wegschreiben. Im Fehlerfall geht alle 5 s ein
voller `tset` samt Logo hinaus; ein falsches Passwort wird endlos wiederholt und läuft bei
badhub in die Auth-Sperre (429 mit `Retry-After` 900 bzw. 86400 s). bts-light und badhub
werden unabhängig ausgerollt — die Lösung darf keinen badhub-Deploy voraussetzen.

## Entscheidung

1. **Ein Sender-Task für alles** an den Liveticker-Endpunkt, einschließlich der
   Tablet-Punkte. Er wartet jede Antwort ab, bevor er den nächsten Auftrag wählt (streng
   seriell). Die `ScorePushQueue` geht darin auf.
2. **Der Loop liefert den Snapshot ab, der Sender baut die Nachricht** zum Sendezeitpunkt.
   Das Postfach hat drei Schlitze, je „neuester gewinnt": Stand, Spielplan-Auftrag,
   Feld-Marken. Die Ablieferung wartet nie auf das Netz.
3. **`handle_score` legt nur eine Feld-Marke ab**, keinen fertigen Punkt. Der Sender löst
   sie gegen den Live-Stand auf.
4. **Feste Auftragsreihenfolge:** voller `tset` → Feld-Marken → Punktänderungen aus dem
   Loop-Stand → Meldeliste → Spielplan → Punktverlauf; nach jeder Sendung wird neu gewählt.
5. **Fehlerklassen** je Kanal nach der Tabelle der Spec: Transport, 5xx auf dem Liveticker,
   200 ohne echte badhub-Antwort → Störung der ganzen Strecke mit Backoff 5/10/20/40/60 s,
   ab dem zweiten Versuch ohne Logo; 429 → Drossel nach `Retry-After` (5 s bis 60 min);
   400/409/413 → nur der betroffene Kanal pausiert; die additiven Kanäle halten den
   Liveticker nie an.
6. **401/403 und die Basic-Auth-Wand halten die ganze Strecke an**, bis die Einstellungen
   gespeichert werden oder die Übertragung neu startet. Kein Probe-Versuch. Der Halt lebt
   nur im Sender; `start_sync` baut ihn neu.
7. **Lebenszyklus:** Der Sender startet im Loop-Task unter einem `TaktWaechter` und endet
   mit ihm; am Slave gibt es ihn nicht. Logo-Stempel, „war zuletzt gestört" und die
   429-Frist liegen in `AppState` (nur RAM) und überleben den Sync-Neustart, nicht den
   App-Neustart.
8. **Umsetzung in Stufen:** zuerst Fristen und Fehlerklassen im bestehenden Loop mit einem
   geteilten Tor (PR 1), dann der Sender für den Loop-Stand (PR 2a), dann die Tablet-Punkte
   (PR 2b).

## Verworfene Alternativen

- **Nur Fristen und Backoff im bestehenden Loop:** begrenzt den Schaden, aber der Zyklus
  wartet weiter auf das Netz, und die zwei Sender bleiben (Überholen, Wegschreiben). Bleibt
  als erste Stufe, nicht als Endzustand.
- **Sender nur für den Loop, Queue daneben:** entkoppelt die Halle, lässt aber genau die
  Ursache des Überholens bestehen.
- **Fertige Nachrichten im Zyklus bauen und abliefern:** Der Loop müsste den bestätigten
  Stand des Senders lesen (geteilter Zustand, Wettlauf), und ein geparkter `tset` könnte
  älter sein als ein inzwischen eingetroffener Punkt.
- **Fertiger `tupdate_match` je Feld im Postfach:** näher an der heutigen Queue, braucht
  aber weiter die Veraltet-Prüfung und einen Abgleich mit dem `tset`.
- **Begrenzte Parallelität für Punkte verschiedener Felder:** schneller, behält aber das
  Wegschreiben bei badhub, solange dort die Sperre fehlt.
- **Periodischer Probe-Versuch im Halt:** Ein falsches Passwort heilt nicht von selbst;
  jeder Versuch zählt bei badhub auf die Auth-Sperre.
- **`Retry-After` wörtlich einhalten (bis 24 h):** Der Liveticker stünde einen Tag still,
  auch nach korrigiertem Passwort; ein 429 zählt bei badhub nicht als Fehlversuch.
- **Sendezustand auf Platte:** neuer Speicher ohne Nutzen — nach einem App-Neustart ist ein
  voller `tset` ohnehin richtig.

## Folgen

- Ein totes oder hängendes badhub verlängert den Sync-Zyklus nicht mehr.
- Punkte und Vollstände erreichen badhub in einer Reihenfolge; das Wegschreiben paralleler
  `tupdate_match` entfällt clientseitig, ohne badhub-Deploy.
- Der Durchsatz ist durch die Antwortzeit von badhub begrenzt (je Request ein
  bcrypt-Vergleich gegen alle Zugänge). Vor der Stufe mit den Tablet-Punkten wird gemessen;
  bei Bedarf kommt ein Sicherheitsventil (Sammel-`tset`).
- Im Fehlerfall werden Punkte verworfen, nicht gepuffert; der nächste volle `tset` trägt den
  Stand.
- Wird ein falsches Passwort bei badhub statt in der App korrigiert, bleibt die Übertragung
  angehalten, bis jemand speichert oder neu startet. Die Statuszeile sagt das.
- Ein langlebiger Task hält eine Kopie des Passworts — wie die heutige Config-Kopie des
  Loops.
- Das Speichern der Einstellungen verliert weiterhin Diff-Basis, Meldeliste und
  Verlauf-Stand.
