# 0107 — Punktspiel als dritte Turnierquelle: Zustand in badhub, ohne Kern und Lease

- **Status:** accepted
- **Datum:** 2026-11-22

## Kontext

Vereine sollen Punktspiele (Ligabegegnungen) live zählen, im ersten Schritt gesteuert über bts-light (badhub
ADR-0092). Ein Punktspiel hat rund acht Einzelspiele auf ein bis vier Feldern, die Aufstellung kommt aus badhub.
bts-light kennt zwei Turnierquellen: BTP und das badhub-Turnier mit Turnierkern, Lease und Ereignisprotokoll
(Spec [turnierquelle](../features/turnierquelle.md)). Einen Mannschaftskampf kennt keine von beiden.

Langfristig soll das Zählen ohne bts-light gehen (Zählgeräte per QR-Code oder Link). Der Zustand muss deshalb in
badhub liegen, nicht in bts-light.

## Entscheidung

Eine dritte Quelle `punktspiel` (`quelle/punktspiel.rs`), bewusst klein:

1. **badhub hält den Zustand.** bts-light liest ein Paket (`GET /api/ls/v1/begegnung`) und meldet je Spiel den
   ganzen Zustand zurück (`POST /api/ls/v1/begegnung/spiel`). Kein Kern, keine Lease, kein lokales Protokoll.
2. **Abbildung auf `BtpSnapshot`**, damit Tablets, Monitore, Turnierleitungs-Web und Liveticker unverändert
   arbeiten. Match-ID = Spielnummer, Feld-ID = Feldnummer.
3. **Nur das Gerätetoken liegt auf der Platte** (verschlüsselt, `punktspiele/<uuid>/`). Das Paket wird bei jedem
   Start geholt, im Speicher gehalten und bei Netzfehlern weiterverwendet.
4. **Liveticker unter der Kennung der Begegnung** (`tournament_uuid_kanonisch`), über den vorhandenen Push.

## Alternativen

- **Das Punktspiel als badhub-Turnier über den Turnierkern.** Verworfen: Der Kern kennt keinen Mannschaftskampf,
  und Meldungen, Auslosung und Lease wären leer mitzuschleppen. Kern, badhub und bts-light müssten gemeinsam
  wachsen, für einen Fall, der später ohne bts-light laufen soll.
- **Lokaler Zustand mit Outbox** wie beim badhub-Turnier, um ohne Netz schreiben zu können. Verworfen für den
  ersten Schritt: doppelter Zustand mit Abgleichsregeln. Das Tablet behält ein nicht angenommenes Ergebnis und
  schickt es erneut, das genügt für acht Spiele.
- **Paket auf der Platte ablegen.** Verworfen: Es enthält Namen, und ohne Netz lässt sich ohnehin nichts melden.

## Konsequenzen

- Ein Punktspiel braucht am Spieltag Internet am Turnier-PC. Ohne Netz zählen die Tablets weiter, Ergebnisse
  kommen erst an, wenn das Netz zurück ist.
- `config.json` bekommt `quelle = "punktspiel"` und den Block `punktspiel`. Ältere Versionen lesen den Wert als
  BTP (`serde(other)`), die Grenze `rueckwaerts_lesbar_ab` bleibt.
- Neun Weichen in `quelle/mod.rs` haben einen dritten Arm. BTP- und badhub-Turniere laufen unverändert.
- Funktionen, die am badhub-Turnier hängen (Abgleich-Panel, Standby, Kernbefehle im TL-Web), bleiben für
  Punktspiele ausgeblendet, weil sie auf `QuelleArt::Badhub` prüfen.
- Übernimmt badhub später selbst die Rolle des Hosts, entfällt diese Quelle nicht: Vereine mit eigener Halle und
  Monitoren können weiter über bts-light zählen.
