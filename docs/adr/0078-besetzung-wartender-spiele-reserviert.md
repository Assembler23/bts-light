# 0078 — Die Besetzung wartender Spiele reserviert ihre Schiedsrichter

- **Status:** accepted
- **Datum:** 2026-11-08

## Kontext

Die Rotation (ADR 0021/0022) zählt nur Personen auf laufenden Feldern als belegt.
Für Finals und Topspiele will die Turnierleitung SR/AR aber vorab fest bestimmen, im
TL-Web oder schon in BTP. Solche Personen setzte die Rotation bisher anderswo ein. Die
Rechnung „wer ist belegt" stand außerdem an drei Stellen getrennt: in `track_officials`,
in `officials_im_dienst` für `auto_assign` und in `vorab_sr_verteilen` (ADR 0072). Eine
Hand-Umteilung gegen einen BTP-Wert verwarf `confirm` sofort (Spec
`tl-warteliste-schiedsrichter`).

## Entscheidung

- Eine Person ist **reserviert**, wenn sie an einem wartenden Spiel (`Scheduled`) mit
  `planned_time` heute, davor oder ohne Zeit eingeteilt ist. Das gilt für BTP-vorab und
  für Hand-Einteilung. Gezählt wird in der Sicht „Hand vor BTP" (`fuer_schreiben`); `Some(0)`
  heißt kein Dienst; nur Personen der BTP-Officials-Liste zählen. Die Reservierung endet
  mit `Finished` und lebt nach „vom Feld genommen" wieder auf.
- **Eine** Quelle `Belegung` in `officials.rs` rechnet Dienst und Reservierung. Rotation,
  Meetingpoint-Vorab-Verteilung, Übernahme und Auto-Vergabe nutzen sie.
- Die **Auto-Vergabe wartet** mit einem Spiel, dessen SR/AR anderswo Dienst tut. Dafür
  gibt es eine eigene Sperre in `auto_assign`. Die manuelle Vergabe bleibt frei.
- `confirm` gibt eine Hand-Einteilung erst frei, wenn BTP sie spiegelt oder sie
  geschrieben wurde. Danach gewinnt eine Änderung in BTP.
- Das TL-Web zeigt den Menüpunkt nur bei `can_assign_waiting_officials`, damit ein neues
  Cloud-`tl.html` nicht auf einen alten Host trifft.

Ergänzt ADR 0021/0022 (Rotation, Rücksync) und ADR 0072 (Vorab-SR weicht weiterhin jeder
BTP- oder Hand-Einteilung, überspringt jetzt aber auch Reservierte).

## Alternativen

- **Alle `Scheduled` reservieren, ohne Tagesgrenze:** Vorabeinträge für spätere Tage
  würden den Pool tagelang verkleinern. Verworfen.
- **Nur Hand-Einteilung reserviert, BTP-vorab nicht:** Das lässt die beobachtete Lücke
  bestehen. Verworfen.
- **Reservierung als Warnung statt Sperre:** Die Turnierleitung wollte eine feste
  Einteilung. Verworfen.
- **Wartesperre über `PlayerAvailability`:** Sie hätte auch die manuelle Vergabe, die
  Prognose und die Wire-Typen (`TlBlocked`) erfasst. Verworfen zugunsten einer Sperre nur
  in `auto_assign`.
- **Reservierung in der Anzeige-Sicht `effective` (BTP vor Hand):** Im Zeitfenster bis zur
  Bestätigung wäre die neu eingeteilte Person frei. Verworfen.
- **Notschalter in den Einstellungen:** Mit der Tagesgrenze nicht nötig; die TL löst
  einzelne Einteilungen per Hand. Verworfen.

## Konsequenzen

- Bestehende Installationen ändern ihr Verhalten mit dem Update: BTP-Vorabeinträge für
  heute nehmen Personen sofort aus der Rotation.
- Ein Feld kann leer stehen, solange der SR seines nächsten Spiels noch woanders pfeift.
  Das ist gewollt.
- Die Prognose kennt die Wartesperre nicht und ist für betroffene Spiele leicht zu
  optimistisch.
- Eine ältere App ignoriert die Reservierung. `officials-state.json` bleibt lesbar.
