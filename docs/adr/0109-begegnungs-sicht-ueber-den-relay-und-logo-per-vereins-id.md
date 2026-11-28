# 0109 — Begegnungs-Ansicht für Punktspiele: fertige Sicht über den Relay, Vereinslogo per Vereins-ID

- **Status:** accepted
- **Datum:** 2026-11-28

## Kontext

Die Monitoransicht „Begegnung“ (Spec [punktspiel-monitoransicht](../features/punktspiel-monitoransicht.md)) zeigt
alle Spiele eines Punktspiels und den Gesamtstand. Sie soll im LAN und über den Cloud-Relay laufen, also auch beim
Punktspiel ohne Windows-PC.

Befund am Code:

- Der Relay kennt je Installation nur, was gerade auf einem Feld läuft oder gerufen ist. Beendete und offene
  Spiele, Heim und Gast kommen nie über die Leitung.
- Ein Relay, der einen neuen `HostFrame` nicht kennt, verwirft ihn still. Ein Relay, der ein neues
  `MonitorTarget` nicht kennt, lehnt den ganzen Zuweisungs-Upload ab; der Host versucht es endlos neu, alle
  Zuweisungen bleiben eingefroren. Eine Fähigkeitsmeldung des Relays gibt es nicht.
- Der Logo-Weg `/info/club-logo?name=` findet „BC Muster 2“ nicht, weil badhub einstellige Zahlen im Namen lässt
  und nur eindeutige Treffer liefert. badhub kennt die Vereine von Heim und Gast exakt und liefert ein Logo auch
  per `?id=`.

## Entscheidung

1. **Der Host schickt dem Relay die fertige Sicht** als `HostFrame::Begegnung { sicht: Option<BegegnungSicht> }`,
   nur bei Änderung und einmal je Verbindung. Die Sicht ist eine Positivliste ohne Personen-ID und ohne
   Lizenznummer. `None` heißt: kein Punktspiel aktiv.
2. **Eine Funktion für beide Seiten.** `begegnung_anzeige` in `relay-proto` mischt den Live-Stand der Felder in
   die Zeilen, bestimmt den Zustand je Zeile und rechnet Gesamtstand und Nebenzeile. Der Host ruft sie mit dem
   Tablet-Stand, der Relay mit seinen Feld-Ständen. Der Frame trägt weder Live-Stand noch Summen.
3. **Relay vor Client.** Der Relay lernt Frame, Route und Ziel-Variante zuerst; erst danach erscheint eine
   Client-Version, die das Ziel zuweisen kann. Notbremse: das Ziel im Client vor dem Upload herausfiltern wie die
   Spielfolge.
4. **Vereinslogo per Vereins-ID.** badhub ergänzt das Punktspiel-Paket um die Vereins-IDs von Heim und Gast.
   `/info/club-logo` nimmt im LAN und am Relay zusätzlich `?id=<Zahl>`; `name` bleibt. Ohne Treffer zeigt die
   Seite eine Initialen-Kachel.

## Alternativen

- **Die Cloud-Seite fragt badhub direkt** (`/api/ls-live`). Kein neuer Frame, aber der Live-Stand käme nur über
  den Liveticker, und der ist ohne Windows-PC aus. Verworfen.
- **Der Host rechnet alles und schickt bei jedem Punkt die ganze Anzeige.** Einfach, aber jeder Punkt eines
  Cloud-Tablets liefe Relay → Host → Relay, bevor der TV ihn sieht. Verworfen.
- **Sicht als undurchsichtiger Text** (wie der TL-Zustand). Der Relay könnte den Live-Stand nicht einmischen.
  Verworfen.
- **Logo über den Namen, Mannschaftsnummer abschneiden.** Kein badhub-Umbau, rät aber und bricht bei
  „1. BC …“, römischen Ziffern und Spielgemeinschaften. Verworfen.
- **Eigene Logo-Route je ID.** Sauberere Typen, aber zwei Routen und zwei Cache-Wege. Verworfen.

## Konsequenzen

- Die Sicht ist am Relay typisiert. Ein neues Feld braucht ein Relay-Deploy; `#[serde(default)]` hält alte und
  neue Stände verträglich.
- Die Satzregeln (`set_is_complete`, `spiel_ist_entschieden`) wandern nach `relay-proto`; der Host ruft sie über
  dünne Hüllen weiter auf.
- Zwei Änderungen in badhub (Paketfelder, Link in Mein Badhub) mit eigener Deploy-Reihenfolge.
- Ein Client, der vor dem Relay ausgerollt würde und das Ziel zuweist, fröre alle Cloud-Zuweisungen ein. Das
  verhindert nur die Reihenfolge; der Relay-Test „Upload mit dem neuen Ziel antwortet 200“ sichert sie ab.
