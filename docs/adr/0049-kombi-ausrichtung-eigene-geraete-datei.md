# 0049 — Die Kombi-Ausrichtung wohnt in einer eigenen Geräte-Datei, nicht im Zuweisungs-Target

- **Status:** accepted (umgesetzt 25.08.2026)
- **Datum:** 2026-08-25

Gehört zu [docs/features/kombi-ausrichtung-je-monitor.md](../features/kombi-ausrichtung-je-monitor.md).

## Kontext

Die Kombi-Anzeige (`combo.html`) zeigt bis zu drei Felder auf einem TV, entweder übereinander oder
nebeneinander. Welche der beiden Anordnungen gilt, entscheidet bisher **ein globaler Schalter**
(`CourtMonitorConfig.combo_vertical`, seit v0.9.97): Der Server hängt bei gesetztem Schalter
`&dir=v` an die Redirect-URL, und zwar an genau einer Stelle (`tablet/server.rs`), nachdem
`MonitorTarget::redirect_path()` den Pfad `/combo?courts=…` gebaut hat.

Die Ausrichtung soll je Gerät gelten. Die Feld-Auswahl tut das längst
(`MonitorTarget::CourtCombo { court_ids }`); die Ausrichtung ist der letzte global gebliebene Teil
derselben Zuweisung. Wo sie künftig wohnt, hat drei tragfähige Antworten mit sehr ungleichen
Folgen — vor allem für den **Downgrade** auf eine ältere App-Version, der bei einer Plug-and-play-
App mit Auto-Update jederzeit vorkommen kann.

Zwei Eigenschaften des Bestands prägen die Entscheidung:

**Erstens verwirft `read_assignments` Einträge mit unbekanntem `kind`-Tag still**
(`tablet/monitor.rs`, belegt durch einen eigenen Test). Ein Eintrag, den eine ältere Version nicht
deuten kann, ist nicht etwa unvollständig — er ist weg.

**Zweitens ist `MonitorTarget` ein Typ aus `relay-proto`**, und der Host lädt die vollständige
Target-Liste bei jedem Heartbeat zum Relay hoch (`tablet/relay_client.rs`). Alles, was an diesem
Typ hängt, ist damit Wire-Ebene — obwohl der Relay Kombi-Ziele bewusst **gar nicht** umleitet
(`relay/src/main.rs`), die Kombi-Anzeige also reiner LAN-Betrieb ist.

## Entscheidung

**Die Ausrichtung wohnt in einer eigenen Geräte-Datei** `monitor-combo-dir.json`, nach dem Vorbild
von `monitor-halls.json` — das im selben Modul genau für nicht-feldgebundene Geräte-Eigenschaften
existiert. Der Wert ist ein `bool`; bei zwei möglichen Anordnungen kann so kein unbekannter Wert
entstehen, der einen Eintrag verwerfen würde. Die Datei trägt zusätzlich `last` als Vorschlagswert
für neu angelegte Zuweisungen.

Die Ausrichtung reist im ohnehin laufenden `/combo/state`-Poll mit; `combo.html` schaltet nur eine
CSS-Klasse um. Sie gehört **nicht** in die Redirect-URL: Das erzwänge einen vollen Seitenaufbau
(≈ 1 s schwarz, mitten im Spiel), und der URL-Vergleich in `combo.html` ist positionsabhängig —
hinter `device` stehend löste ein `dir`-Parameter eine Endlos-Umleitung aus.

**Migration:** einmalig beim Start, Merker ist die **Existenz** der Datei. `combo_vertical` bleibt
eine Version lang lesbar, wird aber nicht mehr geschrieben.

### Verworfene Alternativen

**(a) Feld im `CourtCombo`-Payload** — `MonitorTarget::CourtCombo { court_ids, vertical }`.
Kompakt und an einem Ort, aber die Ausrichtung reiste bei jeder Änderung zum Relay: eine
Wire-Änderung samt Relay-Deploy für ein Merkmal, das der Relay nicht einmal auswertet. Dazu zwei
fachliche Nachteile: Der Dropdown-Schlüssel des Bedienpanels kodiert das Target als `combo:1,2,3`
und trägt die Ausrichtung nicht — ein erneutes Auswählen derselben Option setzte sie still zurück.
Und ein zwischenzeitlicher Wechsel auf ein Einzelfeld verlöre sie ersatzlos.

**(b) Neue `MonitorTarget`-Variante** — etwa `CourtComboVertical`. Verworfen, weil
`read_assignments` unbekannte `kind`-Tags still verwirft: Nach einem Downgrade stünde der TV nicht
etwa falsch herum, sondern **unzugewiesen** auf der Kopplungsseite. Die Zuweisung wäre weg, nicht
nur die Ausrichtung.

**(c) Eigene Geräte-Datei** — gewählt.

## Konsequenzen

**Positiv:** `relay-proto` und der Relay bleiben unberührt — kein Wire-Change, kein Deploy. Die
Serde-Form der Zuweisungsdatei ändert sich nicht, der vorhandene Einfrier-Test bleibt unverändert
grün, und ein Downgrade behält die Zuweisungen. Die Ausrichtung überlebt einen Wechsel
Kombi → Einzelfeld → Kombi. Das Bedienpanel kann sie nicht mehr versehentlich über den
Dropdown-Schlüssel zurücksetzen.

**Negativ:** eine dritte Geräte-Datei neben Zuweisungen und Hallen. Und die zwei Dateien, die ein
Dialog-Klick schreibt, sind nur **einzeln** atomar — deshalb die feste Reihenfolge: erst die
Ausrichtung, dann die Zuweisung. Andernfalls entstünde ein Fenster, in dem der TV bereits auf
`/combo` steht, der Store aber noch die alte Ausrichtung meldet.

**Rückfall:** Eine ältere Version kennt die Datei nicht und folgt wieder dem globalen Schalter —
alle Kombi-TVs stehen dann einheitlich. Das ist bewusst akzeptiert und in der Spec als Risiko
notiert.
