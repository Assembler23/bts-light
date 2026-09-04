# 0054 — Liveticker: Kind-Turnier je turnier.de-GUID statt zusammengesetztem Schlüssel

- **Status:** accepted
- **Datum:** 2026-09-04
- **Spec:** [liveticker-mehrere-turniere-je-verband.md](../features/liveticker-mehrere-turniere-je-verband.md)

## Kontext

badhub hält den Liveticker-Stand je `tournament_key`, und der Schlüssel
entspricht bei den Verbands-Presets dem **Verband**, nicht dem Turnier (ein
Passwort je Verband, eingebettet in bts-light). Zwei parallele Turniere
desselben Verbands überschreiben sich gegenseitig. Ein Admin kann heute je
Turnier einen eigenen Zugang anlegen, aber das ist Handarbeit vor jedem
Wochenende — und parallele Turniere sind im Verband der Normalfall.

Gesucht war ein Weg, mehrere Turniere **unter einem Zugang** zu führen, ohne
die Auth zu ändern und ohne jeden Lesepfad in badhub anzufassen. Als
Turnier-Identität wurde die turnier.de-GUID gewählt (Nutzer-Entscheidung
04.09.2026, Pflichtfeld in bts-light; bestätigt ADR 0009). Die `install_id`
schied aus, weil sie eine Installation identifiziert, nicht ein Turnier, und
weil sie zugleich das Cloud-Token ist (ADR 0006) — sie darf weder in den Push
noch in eine URL.

## Entscheidung

badhub legt beim ersten Push mit einer neuen GUID unter dem Verbandszugang
automatisch ein **Kind-Turnier** in `liveticker_tournaments` an: eigener,
deterministisch abgeleiteter Schlüssel (`<eltern>-<8 Hex aus SHA-256 der
GUID>`), `parent_key` auf den Verband, kein Passwort, immer aktiv. Der
Schreibpfad löst nach der Auth den Eltern- auf den Kindschlüssel auf; alles
Weitere — Persistenz, Spieler-Index, Zeitplan, Check-In-Zuordnung, Live-Seite,
Auswahl, Ausblendung — läuft unverändert **je Schlüssel**.

Die Sperre eines Verbandszugangs wirkt allein über den Schreibpfad (kein
Push → Kind nach 30 min unsichtbar); Lesepfade schauen nicht auf den
Elterndatensatz. Kinder ohne Push seit 30 Tagen werden im Schreibpfad
aufgeräumt, kein Cron.

## Alternativen

- **Zweite Spalte (GUID) in allen Zustandstabellen, zusammengesetzter
  Primärschlüssel.** Sauberer im Datenmodell, aber jeder Lesepfad muss die
  GUID durchreichen (Ticker, Badge, Spielerseiten, Teilnehmerlisten,
  Check-In-Bezug, Live-Seite). Deutlich mehr Stellen und Fehlerfläche für
  denselben Effekt. Verworfen.
- **Selbstbereitstellung:** bts-light ruft mit Verbandspasswort plus GUID
  einen Endpunkt auf und bekommt eigenen Schlüssel und eigenes Passwort.
  Bringt Geheimnisverwaltung und Rotationsfragen in die App und einen zweiten
  Auth-Pfad nach badhub. Verworfen.
- **Kennung aus der `install_id` (Hash).** Null Konfiguration, aber
  Installation ≠ Turnier, und die Nähe zum Cloud-Token ist ein
  unnötiges Risiko. Verworfen zugunsten der GUID.
- **Turniername aus BTP als Kennung.** Namensänderung spaltet den Ticker,
  gleiche Namen kollidieren. Verworfen.

## Konsequenzen

- Zwei Installationen mit unverändertem Preset laufen parallel; kein
  Admin-Eingriff vor dem Wochenende.
- Die GUID wird in bts-light Pflicht — auch für Turniere, die nicht auf
  turnier.de liegen (dort genügt jede wohlgeformte GUID; badhub prüft die
  Herkunft nicht).
- Alte Sender ohne GUID (letilo/bts, ältere bts-light) landen weiter auf dem
  Elternschlüssel und kollidieren untereinander wie heute — aber nicht mehr
  mit neuen Versionen.
- Der Check-In-PIN hängt am Elternzugang; parallele Turniere teilen ihn.
  Bewusst offen gelassener Folgeschritt.
- Die Admin-Liste wächst um ein Kind je Turnier; die 30-Tage-Bereinigung
  hält sie klein.
- Ausrollreihenfolge: erst badhub, dann bts-light.
