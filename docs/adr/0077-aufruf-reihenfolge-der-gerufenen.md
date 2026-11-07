# 0077 — Aufruf-Reihenfolge der Gerufenen

- **Status:** accepted
- **Datum:** 2026-11-07

## Kontext

Gerufene Partien wurden nach dem Spielliste-Schlüssel vergeben. Der Schlüssel
kannte nur „gerufen ja/nein". Ein von der Aufruf-Automatik übersprungenes und
später nachgeholtes Spiel drängelte sich so vor alle Wartenden am
Meetingpoint. Gleichzeitig führte der `MeetingpointStore` die Aufrufe schon
in Aufrufreihenfolge, und zwar für alle Vorbereitungs-Aufrufe, unabhängig
vom Meetingpoint-Schalter. Diese Reihenfolge nutzten nur der
Cloud-Vorbereitungsmonitor und die Vorab-SR-Verteilung. Die
Turnierleitung will unter Gerufenen „wer zuerst gerufen ist, kommt zuerst
dran" und will diese Reihenfolge ändern können (Spec
`meetingpoint-reihenfolge`).

## Entscheidung

- Die **Position eines aktiven Aufrufs in `MeetingpointStore.calls` ist sein
  Aufruf-Rang**. Er ist die zweite Komponente des einen Sortierschlüssels
  (`ManualOrderSortKey`), direkt nach „gerufen?" und vor dem Präfix-Rang.
  `called: bool` wird zu `call_rank: Option<usize>`.
- Ein Aufruf **löscht** den Präfix-Eintrag des Spiels. „Nach oben ziehen" in
  der Warteliste bedeutet damit „als Nächstes gerufen".
- Umsortieren der Gerufenen verschiebt das Element in der Vec, nur innerhalb
  derselben Aufruf-Halle. Es gibt eine eigene TL-Aktion `CalledReorder` mit
  Fehlermeldung statt stillem No-Op. Am Slave ist die Aktion gesperrt.
- Für den Liveticker wird der Rang als transienter Stempel in den Snapshot
  geschrieben, im selben Moment wie `preparation_call_ts`.
- Eine einmalige Migration über die Marke `order_v` ordnet Altdateien nach
  dem bisherigen Schlüssel.

Ergänzt ADR 0023/0026 (Vorrang „gerufen vor Präfix" wird zu „Aufruf-Rang vor
Präfix") und präzisiert ADR 0071 (der Store ist jetzt Ordnungsquelle der
Vergabe).

## Alternativen

- **Eigenes Rangfeld bzw. eigener Store:** Das wäre eine zweite Wahrheit
  neben der Vec, die heute schon die Aufrufreihenfolge trägt. Verworfen.
- **Den Warteliste-Präfix für Gerufene mitbenutzen:** Jeder Warteliste-Zug
  baut den Präfix ohne die Gerufenen neu auf, und genau das ließ ihre
  Reihenfolge springen. Verworfen.
- **Rang als zusätzlicher Parameter neben `called: bool`:** Das erlaubt den
  widersprüchlichen Zustand „gerufen ohne Rang". Verworfen.
- **Den gerufenen Block an jeder Sortierstelle nachsortieren:** Das wäre eine
  zweite Berechnung an zehn Stellen. Verworfen.
- **Aufrufzeit `called_at_ms` als Schlüssel:** Sie ist nicht eindeutig, weil
  die Automatik mehrere Partien in derselben Millisekunde ruft, und sie ist
  nicht umsortierbar. Verworfen.

## Konsequenzen

- Alle Sortierstellen sind vom Compiler erfasst. LAN- und
  Cloud-Vorbereitungsmonitor zeigen dieselbe Folge.
- Turniere ohne Meetingpoint ändern ihr Verhalten bewusst: Die
  „In Vorbereitung" Gerufenen kommen in Aufrufreihenfolge.
- Wunschfeld und Hallenbindung schlagen die Reihenfolge weiterhin.
- Eine ältere App (Downgrade) ignoriert `order_v` und sortiert wieder nach
  Spielliste. Das ist unschädlich.
