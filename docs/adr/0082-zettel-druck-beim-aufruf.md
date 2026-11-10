# 0082 — Zettel-Druck beim Aufruf: SR-Sicht, Nachdruck und Druck-Gedächtnis

- **Status:** accepted
- **Datum:** 2026-11-10
- Ergänzt [ADR 0042](0042-stiller-druck-ueber-elementliste.md) (Autodruck) und schränkt das dortige
  Nicht-Ziel „kein Autodruck beim Aufruf" ein; nutzt die Zuteilung beim Aufruf aus
  [ADR 0080](0080-rotation-teilt-beim-aufruf-zu.md) und die Sicht „Hand vor BTP" aus ADR 0078.
- Spec: [`zettel-druck-bei-aufruf`](../features/zettel-druck-bei-aufruf.md).

## Kontext

Der Autodruck druckt bei der Feldvergabe, genau einmal je Spiel. Seit ADR 0080 steht der
Schiedsrichter schon beim Aufruf in die Vorbereitung fest; die Turnierleitung will den Zettel zu
diesem Zeitpunkt. Damit entstehen drei Fragen: Welcher SR steht auf einem Zettel, solange das Spiel
wartet (BTP und lokale Zuteilung können abweichen)? Was passiert, wenn der gedruckte SR wieder
wegfällt (Rotation löst ihn am Feld ohne SR-Dienst, Rücknahme, Umteilung)? Wie merkt sich die App
den gedruckten SR, ohne dass ein Rollback Doppeldrucke auslöst?

## Entscheidung

1. **SR-Sicht:** Der Vorabzettel eines **wartenden** Spiels (Autodruck und Hand-Vorabzettel) zeigt
   die Sicht „Hand vor BTP" (`wartende_officials`) — dieselbe wie Warteliste und Meetingpoint.
   Laufende Spiele bleiben bei `effective` (BTP gewinnt, R2).
2. **Nachdruck:** Im Modus „beim Aufruf" gilt „höchstens ein Zettel" nicht. Ändert sich der SR
   vor der Feldvergabe und ist der neue 20 s stabil, wird nachgedruckt. Bei der Feldvergabe wird
   ein abweichender SR **sofort** mit Feldnummer nachgedruckt — aber erst, wenn BTP den lokalen
   Schreibwert bestätigt hat (`srF == srH`). Danach kein weiteres Blatt. Ein entfernter SR erzeugt
   keinen Druck, sondern eine Warnung. Der Modus „bei der Feldvergabe" bleibt unverändert.
3. **Druck-Gedächtnis:** `gedruckt.json` wird abwärtskompatibel erweitert (`printed` bleibt, neu
   `zettel` und `modus`); ein Spiel in `printed` ohne `zettel` ist abgeschlossen.
4. **Vormerken beim Umschalten/Einschalten** geschieht zustandsbasiert über `modus` in der Datei,
   im Sync-Lauf nach der Turnierbindung — nur gerufene Spiele mit SR.
5. Die 20-s-Frist liegt im RAM des Druck-Gedächtnisses im `TabletState` (übersteht Sync-Neustarts).

## Verworfene Alternativen

- **`effective` auch für wartende Spiele:** Zettel und TL-Liste nennen verschiedene Personen.
- **Nur drucken, wenn alle Felder der Halle SR wollen:** von der Turnierleitung zugunsten des
  Nachdrucks verworfen.
- **Hinnehmen ohne Nachdruck:** falsche Namen auf Papier.
- **Eigener Store neben `gedruckt.json`:** zweite Turnierbindung, zweites Schloss, Teilfehler.
- **`printed` als Map statt Set:** eine ältere Version könnte die Datei nicht lesen und würde nach
  einem Rollback das ganze Turnier erneut drucken.
- **Vormerken in `save_config`:** Ereignis statt Zustand; die spätere Turnierbindung lädt die Datei
  und würde den Vermerk überschreiben.
- **Merker in der SyncEngine:** geht beim Speichern der Einstellungen verloren.
- **Einstellbare Frist:** YAGNI.

## Folgen

- Mehr Papier bei häufigem Umteilen in der Vorbereitung; vor Ort ohne Schwall abschaltbar.
- `gedruckt.json` enthält Official-IDs je Spiel (lokal, turniergebunden).
- Bleibt die BTP-Bestätigung am Feld aus, gibt es weder Nachdruck noch Warnung (bewusst still,
  Log-Hinweis).
