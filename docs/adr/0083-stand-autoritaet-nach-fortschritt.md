# 0083 — Tablet-Stand: Autorität nach Fortschritt, Host-Sätze, Quellen-Regel

- **Status:** accepted
- **Datum:** 2026-11-11
- Ergänzt [ADR 0017](0017-reconnect-ownership.md) („Slot-Halter gewinnt") um den
  Fall „Feld frei".
- Spec: [`tablet-stand-robust`](../features/tablet-stand-robust.md).

## Kontext

ADR 0017 entscheidet am Server/Relay, wessen Stand beim Wiederanmelden gilt. Ist das Feld
frei (Halter abgeräumt), bekommt jedes Gerät `authoritative=true` — auch eines ohne eigenen
Stand. Es lädt das Spiel bei 0:0 und überschreibt den gesicherten Stand. Kommt umgekehrt
ein Gerät mit altem Stand zurück, nachdem ein zweites weitergezählt hat und ebenfalls
ausfiel, überschreibt es den neueren. Der Cache liegt nur im RAM; nach Host- oder
Relay-Neustart gibt es nichts zu übernehmen. Im Betrieb LAN+Cloud erreicht der Zustand
eines Cloud-Tablets den Host nie.

## Entscheidung

1. **Die Entscheidung bei freiem Feld fällt am Tablet**, in zwei Stufen entlang der
   Frame-Reihenfolge (`state_restore`, dann `match_assigned`). Server und Relay behalten
   `reconnect_decision` unverändert.
2. **Fortschritt statt `rev`:** Zwischen Ständen verschiedener Geräte gewinnt der höhere
   Fortschritt (abgeschlossene Sätze, dann Punktsumme). `finished`/`retired`/`walkover`
   zählen nicht. Innerhalb desselben Geräts ordnet weiter `rev`; dafür trägt der Zustand
   `schreiber`.
3. **Host-Sätze als Rückfall:** `match_assigned` trägt `hostSets` aus dem Live-Stand des
   Hosts. Sind sie weiter als der Tablet-Stand und nicht vom Tablet selbst gemeldet
   (Melde-Ring), steigt das Tablet mit ihnen ein (Aufstellung neu, Undo beginnt dort).
   Kein neuer Platten-Speicher.
4. **Über Kreuz:** Das Relay reicht den Zustand eines Cloud-Tablets als
   `RelayFrame::StateSync` an den Host. Der Host merkt die Quelle und spiegelt einen
   Cloud-Zustand nicht zurück — außer einmal je neuer Relay-Verbindung. Das Relay
   überschreibt den Zustand eines aktiven Cloud-Tablets nicht mit dem Host-Spiegel; der Host
   verwirft einen Cloud-Zustand, solange ein LAN-Tablet das Feld hält.
5. Der Laufzeit-Schalter `reconnect_legacy_rev` bleibt der Rollback: Legacy-Zweig
   unverändert, keine `hostSets`.

## Verworfene Alternativen

- **Entscheidung am Server/Relay** (Identify trägt Match-ID und Fortschritt): Host und
  Relay müssten den opaken Tablet-Zustand deuten, doppelt implementiert; ADR 0014 hat diese
  Kopplung verworfen. Alten Seiten hülfe es nicht.
- **„Lokal gilt, sobald ein lokaler Stand zum selben Spiel existiert":** heilt nur das
  frische Gerät, nicht das zurückkehrende Altgerät.
- **`rev` geräteübergreifend vergleichen:** in ADR 0017 verworfen (Zähler sind je Gerät).
- **`finished` als Fortschritt werten:** Ein veralteter beendeter Stand schlüge einen
  wiedereröffneten und zeigte einen Sende-Knopf für ein falsches Ergebnis.
- **Bediener fragen** bei Widerspruch: Ein Zähltafelbediener kann das nicht entscheiden.
- **Vollen Zustand auf Platte sichern:** mehr Schreiblast, neuer Speicher mit Namen und
  Ereignissen; der Satzstand genügt für einen sauberen Einstieg.
- **`hostSets` im `MatchBrief`:** Der Brief landet im gesicherten Zustand und wäre dort
  sofort veraltet.

## Folgen

- Ein Ersatzgerät startet nie bei 0:0, solange Server oder Host irgendeinen Stand kennen.
- Nach Host-Neustart überleben Undo und Verlauf eines ausgefallenen Geräts nicht (nur die
  Sätze).
- Der Melde-Ring kann in einem seltenen Fall (verlorene Meldung + langes Offline-Undo)
  eigene Sätze für fremde halten; Folge ist ein sichtbarer Einstieg mit Host-Sätzen.
- Zusätzlicher Uplink Relay→Host je Punkt eines Cloud-Tablets.
- Zwei gleichzeitig aktive Tablets (eines LAN, eines Cloud) am selben Feld bleiben ungelöst.
