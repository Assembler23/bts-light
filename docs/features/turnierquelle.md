# Turnierquelle: BTP oder badhub-Turnierkern

> **Status (20.11.2026):** Paket L1 umgesetzt (v0.9.448): BTP liegt hinter der Turnierquelle, ohne Verhaltensänderung.
> Ziel und Gesamtplan: badhub `docs/features/bts_light_steuerung.md`, Entscheidung badhub ADR-0084 (bts-light hält am
> Turniertag die Lease eines badhub-Turniers).

## Ziel

bts-light soll Turniere, die in badhub organisiert werden (eigener Turnierkern in Rust, Repo `Assembler23/turnierkern`),
genauso steuern wie BTP-Turniere, auch ganz ohne Internet. Dafür wird BTP eine von zwei **Turnierquellen**.

## Harte Bedingung: BTP-Turniere laufen unverändert

Nach jedem Paket verhält sich ein BTP-Turnier genauso wie vorher: gleiche Requests an BTP, gleiche Zeiten, gleiche
Oberflächen.

1. `config.json` bekommt `quelle` mit Vorgabe `btp`. Eine Konfiguration ohne das Feld (jede vor v0.9.448) gilt als BTP.
   Ein unbekannter Wert wird ebenfalls als BTP gelesen, damit ein Downgrade die Konfiguration nicht unlesbar macht.
2. Der BTP-Code wird nur verschoben: keine Logik- und keine Timing-Änderung (5-s-Takt, Nachschub-Queue, Schreib-Gate
   ADR 0060, `btp::eigene_writes`).
3. Kern, Outbox und Lease starten nur bei `quelle = badhub`, an einer Stelle beim Start, nicht verstreut über die
   Konsumenten.
4. Stores und `relay-proto` bleiben abwärtskompatibel. Kern-Turniere legen ihre Dateien getrennt ab.
5. Die bestehende Testsuite bleibt unverändert grün. Vor dem ersten echten Turnier mit L1 gibt es einen BTP-Probelauf mit
   Tablets, Monitoren und Auto-Feldvergabe.

## Paket L1 (umgesetzt)

| Was | Wo |
|---|---|
| Quelle in der Konfiguration | `config.rs` `QuelleArt` (nur `Btp`, `#[serde(other)]`), `AppConfig.quelle` (`#[serde(default)]`) |
| Lesen des Turnierstands | `quelle::stand(config)`; bei BTP genau der bisherige `client::fetch_snapshot`-Abruf samt Fehlertext; Aufrufer `SyncEngine::run_once` (`sync.rs`) |
| Schreibwege nach BTP | `quelle/btp.rs`: `write_result_to_btp`, `write_result_settled`, `write_courts_to_btp`, `write_officials_to_btp`, `write_highlight_to_btp` samt Hilfen, wortgleich aus `tablet/server.rs` verschoben; `tablet::server` exportiert sie unter den alten Namen weiter |
| Datenformat | `release/datenformat.json`: neuer Abdruck, Grenze bleibt (ältere Versionen übergehen `quelle`) |

Nicht in L1: `test_btp` im Einrichtungsassistenten prüft weiter gezielt eine BTP-Verbindung mit den eingegebenen Daten,
nicht die eingestellte Quelle.

## Nächste Pakete (Plan, badhub-Spec)

- **L2:** Quelle `badhub`: Kopplung, Turnierpaket laden, Kernstand auf das heutige Modell abbilden (stabile i64-Spiel-IDs
  `konkurrenz.nr * 100000 + spiel`), Wizard-Schritt „Quelle“.
- **L3:** Schreibpfad über neutrale Befehle hinter der Quelle, eigenes Protokoll auf der Platte, Outbox an badhub,
  Relay-Aktion `KernBefehl` für Eingaben aus badhub.
