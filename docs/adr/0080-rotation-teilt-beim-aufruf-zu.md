# 0080 — Feste Rotations-Zuteilung beim Aufruf mit Herkunfts-Merker

- **Status:** accepted
- **Datum:** 2026-11-09
- Löst [ADR 0072](0072-sr-vorab-zuteilung-vorlaeufig.md) ab. Ergänzt ADR 0021/0022, 0040, 0064,
  0077, 0078; nutzt die Wahl-Funktion aus ADR 0079.

## Kontext

SR und AR sollen schon beim Aufruf in Vorbereitung feststehen, damit sie sich vor dem Feldaufruf
treffen (Spec [`sr-ar-beim-aufruf`](../features/sr-ar-beim-aufruf.md)). ADR 0072 teilte nur mit
Meetingpoint einen vorläufigen SR zu, umverteilbar, im Meetingpoint-Eintrag statt in
`OfficialsStore`. Damals verworfen: direkt in `assignments`, weil eine Rücknahme `Some(0)`
hinterlassen und die spätere Feldrotation blockieren würde. Seit #452 (`confirm` mit Ausgangswert)
gibt `confirm` eine von BTP gespiegelte Einteilung frei; danach ist eine Rotations-Zuteilung nicht
mehr von einer TL- oder BTP-Einteilung zu unterscheiden. Außerdem fiel ein „ohne" an wartenden
Spielen im nächsten Lauf weg (`pruefe_dienst`). Ob BTP `Official1ID` an einem Spiel ohne Feld
annimmt, ohne die Feldzuweisung anzufassen, ist ungemessen. Die Turnierleitung geht davon aus,
dass es funktioniert.

## Entscheidung

- Die Zuteilung beim Aufruf ist eine **feste Einteilung in `assignments`** (reserviert nach
  ADR 0078), zustandsbasiert in jedem Lauf und zusätzlich synchron im Aufruf vor dem
  Ansage-Auftrag. Gewählt wird über `waehle_besetzung` (ADR 0079), nach der Feld-Rotation
  (Feld-Vorrang) und in der Vergabe-Reihenfolge der Gerufenen (ADR 0077). Nur am Master, unter
  einer Vergabe-Sperre.
- Ein **persistenter Herkunfts-Merker** in `officials-state.json` markiert Rotations-Zuteilungen.
  Er überlebt `confirm` und Neustart und verfällt, sobald Hand oder BTP die Rolle anders besetzen.
  Nur Einteilungen mit Merker werden bei Ende des Aufrufs ohne Feld (Rücknahme, Spiel beendet
  oder weg) oder an einem Feld ohne den Dienst wieder frei. Ein geparkter Aufruf (Spiel auf dem
  Feld) gibt nicht frei. „Vom Feld genommen" gibt nicht frei.
- Die Freigabe hinterlässt **kein bleibendes `Some(0)`**: nur lokal → Eintrag entfernen; schon in
  BTP → 0 schreiben und nach Bestätigung räumen.
- **„Ohne" bleibt dauerhaft**, solange BTP niemanden führt; eine BTP-Änderung gewinnt.
- Die Zuteilung geht **sofort** nach BTP, wie jede Hand-Einteilung, **ohne Weiche**. Die Annahme,
  dass BTP das an Spielen ohne Feld verträgt, wird vor dem Release gemessen. Wartende Zeilen
  zeigen „Hand vor BTP".
- Der Erst-Aufruf-Ansageauftrag trägt die Namen fertig vom Master (ergänzt ADR 0040/0064).
  Nachrücken wird nicht angesagt.
- Bestehende `vorab_sr` werden einmalig übernommen; `vorab_sr_verteilen`/`uebernimm_vorab_sr`
  entfallen.

## Alternativen

- **Vorläufig wie ADR 0072, nur ausgeweitet (ohne Meetingpoint, mit AR):** Umverteilung bis zum
  Feld, die Genannten treffen sich womöglich umsonst; zweiter Speicherort neben `assignments`.
  Verworfen (Turnierleitung will feste Zuteilung).
- **Merker im Meetingpoint-Eintrag (`StoredCall`):** lebt nur mit dem Aufruf, verwechselt Feld und
  Parken, zweite Wahrheit. Verworfen.
- **Freigabe über `Some(0)` mit heutiger `confirm`-Logik:** setzt ein flüchtiges „ohne" voraus und
  bricht dauerhaftes „ohne". Verworfen.
- **Kanten-Merker in der SyncEngine:** geht beim Sync-Neustart (Speichern der Einstellungen)
  verloren. Verworfen.
- **Weiche „kein Officials-Write an Spielen ohne Feld"** (Konstante, Besetzung reist mit der
  Feldzuweisung): sicher gegen die CourtID-0-Klasse (#450), verzögert aber auch die
  Hand-Einteilung aus #456 bis zum Feld. Zurückgestellt; sie ist die Rückfallebene, falls die
  Messung negativ ausfällt.
- **Weiche als Config-Feld:** erlaubt einen Feldtest ohne Release, kostet aber Handbuch, Anker und
  Sync-Neustart. Verworfen.

## Nachtrag 09.11.2026: Messergebnis und Absicherung

Die Messung fiel **negativ** aus: BTP übernimmt die CourtID aus jedem Officials-Write, `0` oder
ein fehlendes Element löscht ein Feld auch Sekunden nach der bestätigten Vergabe (Tabelle in
`docs/btp_protocol.md`). `reserved_courts` in `officials_entries` schützte eigene Vergaben nur
zum Zeitpunkt des Snapshots; offen blieben eine Hand-Vergabe in BTP und eine eigene Vergabe, die
erst danach startet. Statt der Weiche entschied die Turnierleitung: **Stand unmittelbar vor dem
Write neu lesen**, die CourtID daraus einsetzen und `reserved_courts` nach dem Lesen erneut
prüfen (`write_officials_to_btp`, v0.9.367). Die Zuteilung beim Aufruf bleibt sofort, die Hand-Einteilung
aus #456 auch. Restfenster: eine Hand-Vergabe, die BTP beim Lesen noch nicht zeigt (≤ 1 s).

## Konsequenzen

- Bestehende Installationen ändern ihr Verhalten mit dem Update: Gerufene binden SR/AR früher.
- Hand-Einteilungen wartender Spiele (#456) stehen erst mit dem Feld in BTP.
- Eine ältere App verwirft den Merker beim Schreiben; danach gelten diese Einteilungen als Hand
  (sichere Richtung). Sie lässt „ohne" wieder los.
- ADR 0072 wird mit der Umsetzung (PR 2) auf „superseded" gesetzt.
