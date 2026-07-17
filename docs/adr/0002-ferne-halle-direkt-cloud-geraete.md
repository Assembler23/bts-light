# 0002 — Ferne Halle: Tablets & Monitore per Direkt-Cloud statt Slave-Multiplex

- **Status:** accepted
- **Datum:** 2026-07-08

## Kontext

Zwei-Hallen-Turnier, Hallen km entfernt, getrennte LTE-/WLAN-Netze (kein
gemeinsames LAN). **Ein** Windows-PC mit BTP steht in Halle A (Master).
Beide Hallen haben TVs (Court-Monitore) **und** Tablets (digitale
Spielzettel). Turnierleitung und Feldvergabe passieren **ausschließlich** in
Halle A. Die ferne Halle B soll ihre Felder zählen und die Ergebnisse müssen
im BTP des Masters landen.

Die bisherige Mehr-Hallen-Cloud-Arbeit (`docs/multi-hall.md`) hatte für
diesen Fall einen großen, phasierten Plan skizziert (Weg B / B2): die ferne
Halle bekommt einen eigenen bts-light-Rechner (Slave), der die Tablets
**lokal per LAN** trägt und Ergebnisse gebündelt über den Relay an den Master
weiterleitet.

Beim Trace des Ist-Codes zeigte sich: der Ergebnis-Datenpfad für Cloud-Tablets
ist **bereits vollständig** und kennt **keine Hallentrennung**:

- Der Master pusht *alle* Felder des Turniers (auch die der fernen Halle) an
  den Relay — `push_all_courts` über `ctx.tablet.courts()`
  (`src-tauri/src/tablet/relay_client.rs`), ohne Hallenfilter.
- Ein Tablet an *jedem* CourtID kann sich per `/{ns}/court/{id}` verbinden,
  bekommt sein `MatchAssigned` und liefert das Ergebnis per `/{ns}/result` →
  Master-Host → `process_result` → `SENDUPDATE` ins Master-BTP
  (`relay/src/main.rs`, `src-tauri/src/tablet/server.rs`).
- Court-Monitore der fernen Halle laufen als Cloud-Monitor
  (`/{ns}/court/{id}/display`) schon heute.

Damit war die eigentliche Frage nicht „geht es", sondern „welchen Schnitt
bauen wir" — und die Antwort hängt an **Resilienz**, nicht an Fähigkeit.

## Entscheidung

**Weg A — Direkt-Cloud-Geräte in der fernen Halle.** Die Tablets und Monitore
der fernen Halle verbinden sich **direkt** mit dem Cloud-Relay des Masters
(dessen `install_id`-Namespace). Der Master läuft im Modus `LanAndCloud`
(eigene Halle per LAN, ferne Halle per Cloud). Die Feldvergabe bleibt zentral
beim Master (Auto-Vergabe mit den vorhandenen Disziplin-je-Halle-Regeln,
Phase 1b, oder manuell). Der Slave-PC in der fernen Halle bleibt **read-only**
und macht wie bisher nur die **Ansage** (Cloud-Ansage-Slave, B1a).

Da der Datenpfad bereits steht, beschränkt sich die Umsetzung auf
**Onboarding**: Die Crew in Halle B muss ohne Zugriff auf den Master-Bildschirm
an die Cloud-Tablet-QR-Codes und Monitor-Links **ihrer** Felder kommen. Dafür:

1. `hall` als (serde-default) Feld an `CourtBrief` — der Master füllt es beim
   `Courts`-Push, der Relay liefert es unter `/{ns}/courts` mit aus.
2. Slave-Command `slave_devices`, das die Feldliste des Master-Namespace holt,
   die Hallen-Optionen (`distinct_halls`) bestimmt, auf die eigene Halle filtert
   und die Relay-Basis (`…/bts-relay/<master_ns>`) liefert.
3. Slave-UI „Geräte in dieser Halle anschließen": **zuerst die Hallen-Auswahl**
   (aus der Relay-Feldliste, weil der Cloud-Slave kein BTP hat), dann je Feld
   der Tablet-QR (`<relay>/<ns>/qr/<court_id>`) und der Monitor-Link
   (`<relay>/<ns>/court/<court_id>/display`).
4. Master-Warnung, wenn bei ≥2 Hallen keine Ansage-Halle gewählt ist (sonst
   sagt der Master beide Hallen an). Wizard-Hinweis, dass der Master
   `LanAndCloud` sein muss.

## Alternativen

**Weg B — Slave trägt die Tablets lokal (LAN) und leitet weiter.** Verworfen
für diesen Use-Case:

- **Aufwand.** Erfordert einen kompletten Neubau: lokaler Tablet-Server +
  mDNS auf dem Slave (heute im `slave_mode` bewusst aus), ein **neuer
  Relay-Rückkanal** Slave→Master→BTP (existiert nicht — der einzige
  Ergebnis-Rückweg ist die Host-WS des Masters), Ergebnis-Pufferung und
  Konflikt-/Reihenfolge-Logik. Mehrere PRs, neue Relay-Routen,
  security-review-pflichtig.
- **Kein zusätzlicher Nutzen für diesen Fall.** Weg B rechtfertigt sich nur
  über **lokale Pufferung**, wenn die ferne Halle vom Internet-Dauerzustand
  entkoppelt sein muss. Der Betreiber schätzt das LTE/WLAN der fernen Halle
  als brauchbar ein; kurze Aussetzer fängt der Bediener durch erneutes Senden
  ab.

Weg B ist damit nicht „falsch", sondern **aufgeschoben**: sollte sich die
Fern-Halle-Verbindung in der Praxis als zu wackelig erweisen, kann Weg B als
spätere Ausbaustufe additiv daraufgesetzt werden (die Direkt-Cloud-Geräte
bleiben dann als Fallback bestehen).

## Konsequenzen

**Positiv**

- Minimaler, risikoarmer Change: der sicherheitskritische Ergebnispfad
  (`process_result`, SENDUPDATE) wird **nicht** angefasst; nur ein additives
  serde-default-Feld und eine read-only Onboarding-Ansicht kommen dazu.
- Sofort nutzbar für das nächste Turnier; keine neue Relay-Route mit
  Schreibrechten.
- Der bestehende Cloud-Sicherheitsmechanismus greift unverändert: jedes
  Ergebnis wird beim Master gegen das Court-Match validiert (Regel R5).

**Negativ / Grenzen**

- **Keine lokale Pufferung.** Jede Ergebnis-Übermittlung läuft synchron
  Tablet→Relay→Master-WS→BTP→Ack (20-s-Timeout). Fällt in der fernen Halle
  das Internet oder beim Master die Relay-Verbindung aus, schlägt `/result`
  sofort fehl und muss vom Bediener wiederholt werden.
- **Master-Dauerpräsenz nötig (Single Point of Failure).** Der Master muss
  durchgängig cloud-verbunden (`LanAndCloud`) sein; ohne Host-Verbindung
  bekommt die ferne Halle weder Match-Zuweisungen noch Ansagen, und Ergebnisse
  landen nicht im BTP (nur der Master schreibt). Kein Slave-Failover.
- **`announce_hall` ist die load-bearing Einstellung** — an **beiden** Enden
  nötig: der Master muss seine Halle setzen (sonst sagt er beide an), der Slave
  seine (sonst hört Halle B auch Halle A / bekommt keine Feld-Codes). Der
  Cloud-Slave hat kein BTP, deshalb speist sich seine Hallen-Auswahl aus der
  Relay-Feldliste; der Vergleich ist byte-genau (`court_hall == announce_hall`).
- **Feldvergabe bei zwei gleichzeitig bespielten Hallen ist nicht
  „vollautomatisch".** Die aktive Halle bleibt leer → die Auto-Vergabe belegt
  nur pro Halle **„in Vorbereitung" gerufene** Matches; die Disziplin-je-Halle-
  Regeln wirken als Constraint, nicht als Verteiler.
- **Cloud-Tablet-PIN = `0000`** und das Feldwechsel-Menü listet alle Hallen →
  ein Halle-B-Tablet ließe sich auf ein Halle-A-Feld umstellen (abgemildert
  durch Hallen-Präfix im Label). Bekannte Grenze, nicht neu durch dieses ADR.
- Die ferne Halle kann **nicht** selbst Felder zuweisen — das ist hier
  gewollt (Steuerung nur in Halle A), aber eine bewusste Einschränkung.
