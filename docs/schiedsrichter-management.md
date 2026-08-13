# Schiedsrichtermanagement

BTS Light übernimmt die Schiedsrichterliste aus BTP (`Officials`-Container),
zeigt sie in Client und TL-Web, erlaubt SR/AR-Zuweisungen je Spiel (mit
Konflikt-Warnung und automatischer Rotation) und schreibt Zuweisungen nach
BTP zurück. Spec: [features/schiedsrichter-management.md](features/schiedsrichter-management.md) ·
ADRs: [0021 (Rücksync)](adr/0021-officials-ruecksync-eigenstaendiger-write.md),
[0022 (Ablage Turnierdaten)](adr/0022-officials-turnierdaten-eigene-datei.md) ·
BTP-Draht: [btp_protocol.md](btp_protocol.md) („Officials: Struktur & Schreibweg").

> **Stand: im Aufbau.** Umgesetzt sind die Schritte 1–3 des Spec-Plans
> (BTP-Messung, Parser, Konfiguration). Roster, Rotation, Bedienung,
> TL-Web, Tablet-Anzeige, Ansagen und Rücksync folgen mit den nächsten
> Schritten; dieser Text wächst mit.

## Konfiguration

`AppConfig.officials` (`config.rs::OfficialsConfig`, Spiegel in
`src/types.ts`, Schalter im SetupWizard-Abschnitt „Schiedsrichter"):

| Feld | Default | Bedeutung |
|---|---|---|
| `enabled` | `false` | Mit Schiedsrichtern spielen. Aus ⇒ keine SR/AR-Elemente in Client, TL-Web, Tablet und Ansagen (Bestandsverhalten). |
| `rotation_sr` | `false` | Automatische Rotation für Schiedsrichter (Official1). |
| `rotation_ar` | `false` | Automatische Rotation für Aufschlagrichter (Official2). |

Alle Felder tragen `#[serde(default)]` — ältere `config.json` bleiben lesbar,
Bestandsinstallationen verhalten sich nach dem Auto-Update unverändert
(Tests `officials_default_off_and_old_config_stays_readable`,
`officials_block_with_missing_keys_falls_back_to_defaults`,
Roundtrip in `save_then_load_roundtrip`).

**Bewusste Aufteilung (ADR 0022):** In der `config.json` liegen nur diese
**geräteweiten** Schalter. Alles Turnier-Spezifische — feldweise Schalter,
Rotationsreihenfolge, Pausen, Sperrlisten, Vereins-Overrides, lokale
Zuweisungen — kommt in eine **turniergebundene Datei** im
App-Datenverzeichnis (Schritt 4). Sperrlisten sind Personendaten: Sie dürfen
weder ins Identitäts-Export-Bündel noch in den Broadcast-TL-State wandern;
die Datei wird bei Turnierwechsel verworfen.

## Gelesene BTP-Daten (Schritt 2)

- `BtpSnapshot::officials` — Liste aus dem `Officials`-Container
  (`BtpOfficial { id, name, first, nationality }`, `display_name()`);
  fehlender Container ⇒ leere Liste, kein Fehler.
- `BtpMatch::official1_id` / `official2_id` — SR bzw. AR am Spiel
  (`0` gilt als nicht gesetzt; Semantik an der BTP-Maske verifiziert,
  Messung 13.08.2026).
- BTP liefert **keinen Verein** am Official — der Stammverein wird ab
  Schritt 4 in BTS Light gepflegt (Basis der Vereins-Konflikt-Warnung).
