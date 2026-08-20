# 0041 — Der Werbe-Stil reist positionell mit; die Kontrastfarbe rechnet der Host

- **Status:** accepted (umgesetzt 20.08.2026)
- **Datum:** 2026-08-20

Gehört zu [docs/features/werbung-hintergrund-und-feld.md](../features/werbung-hintergrund-und-feld.md).
Berührt [ADR 0033](0033-hallen-farben-hex-auf-dem-draht.md) (Farben auf dem Draht).

## Kontext

Werbebilder sollen je Bild eine Hintergrundfarbe und ein Häkchen
„Feldbezeichnung zeigen" tragen. Zwei Fragen waren dabei offen, und beide haben
zwei tragfähige Antworten.

**Erstens die Identität eines Bildes.** Im LAN adressiert eine Anzeige ihr Bild
über den **Dateinamen** (`/ads/{file}`); im Cloud kennt der Relay nur die
**Position** in einer Liste (`/{ns}/ads/{idx}`, `m.ads.get(i)`). `AdUpload`
trägt heute weder Namen noch Schlüssel. Eine Einstellung „je Bild" muss also
irgendwie an ein Bild gebunden werden, das auf beiden Wegen anders heißt. Der
Relay warnt im eigenen Code, dass nach dem Löschen eines Bildes alle
Folge-Indizes aufrücken und eine Anzeige bis zu 60 Sekunden ein **falsches**
Motiv zeigen kann.

**Zweitens die Kontrastfarbe.** Die Feldbezeichnung steht auf der gewählten
Hintergrundfarbe und braucht eine Schriftfarbe, die dazu kontrastiert. Im Repo
gibt es dafür bisher nichts — weder in Rust noch in JavaScript.

## Entscheidung

**Der Stil reist positionell.** `AdUpload` bekommt zwei Felder mit
`#[serde(default)]`, index-parallel zu `MonitorState.ads`. Kein stabiler
Schlüssel, keine Umstellung der Ad-Auflösung.

**Die Kontrastfarbe rechnet der Rust-Host** (relative Luminanz nach WCAG) und
schickt sie als fertigen Farbwert mit. Die Anzeigeseiten prüfen nur noch die
Form (`#rrggbb`) und setzen sie.

## Alternativen

**Stabiler Bild-Schlüssel (Dateiname oder Inhalts-Hash) im `AdUpload`, Relay
löst darüber auf.** Verworfen — für dieses Feature. Der Weg ist der bessere
Entwurf: Er räumt die Index-Verschiebung auf und machte nebenbei
`MonitorTarget::AdSingle` im Cloud möglich, das heute schlicht nicht
funktioniert. Aber er baut die Ad-Auslieferung mitten in der Turniersaison um,
für ein rein kosmetisches Feature. Die Verschiebung ist eine bestehende,
dokumentierte Altlast mit 60-Sekunden-Fenster; der Stil erbt sie, verschlimmert
sie aber nicht — ein verrutschtes Bild zeigt dann eben auch die verrutschte
Farbe. Der Umbau bleibt eine eigene Aufgabe mit eigenem Nutzen.

**Kontrastfarbe im Browser rechnen** (`src/io/*.mjs` plus Inline-Kopie in
`monitor.html` und `ad.html`, Node-Test, eigener CI-Step — das etablierte
Muster für geteilte Anzeige-Logik). Verworfen: Sein einziger Vorteil wäre, dass
Anzeige-Logik über den Relay-Auto-Deploy schneller draußen ist als ein
App-Release. Der greift hier nicht — **ohne** neuen Host gibt es gar keine
Hintergrundfarbe, die Anzeige hätte nichts zu rechnen. Dafür kostet er vier
Orte, die auseinanderlaufen können, statt einer Funktion mit `cargo test`.

**Den Stil in `court-ad-labels.json` mit ablegen** statt in einer dritten Datei.
Verworfen: Dessen Deserializer ist strikt `HashMap<String,String>` und schluckt
Fehler mit `unwrap_or_default()`. Ein Formatwechsel löschte beim Auto-Update
still **alle Anzeigenamen** — und beim Rollback auf die Vorversion noch einmal.
Ein dritter Store neben `court-ad-bar.json` kostet ein paar Zeilen und ist in
beide Richtungen folgenlos.

## Konsequenzen

- Wird ein Werbebild gelöscht, rücken im Cloud die Indizes auf. Bis zum nächsten
  Upload (max. 30 s Fingerabdruck-Takt plus 60 s Bild-Cache) kann eine
  Cloud-Anzeige ein Motiv mit dem Stil seines Nachbarn zeigen. Bekannt,
  begrenzt, und im LAN gar nicht vorhanden (dort ist der Dateiname der
  Schlüssel).
- `monitor_fingerprint` muss den Stil mitführen, sonst löst eine reine
  Farbänderung nie einen Upload aus. Der Test
  `monitor_fingerprint_reagiert_auf_stiländerung` hält das fest.
- Die Kontrastfarbe ist nur so aktuell wie der Host. Eine ältere App liefert
  keine — die Anzeige fällt dann auf ihre Vorgabe zurück, nicht auf Unlesbarkeit.
- Die Schriftfarbe ist damit **nicht** einstellbar. Das ist beabsichtigt: Es
  gibt keinen Weg, die Feldbezeichnung unlesbar zu konfigurieren.
