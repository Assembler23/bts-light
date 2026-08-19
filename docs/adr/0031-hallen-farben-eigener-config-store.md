# 0031 — Hallen-Farben: eigene `hall_colors`-Struktur statt `HallLayoutConfig`-Umbau

- **Status:** accepted
- **Datum:** 2026-08-16

## Kontext

Hallenfarben (Spec `docs/features/hallen-farben.md`) brauchen eine
namensbasierte, turnierübergreifende Persistenz in der Client-Config.
`hall_layouts` trägt bereits genau so eine Zuordnung (Hallenname →
Raster-Einstellungen) — die Farbe dort als Feld anzuhängen liegt nahe.

## Entscheidung

Eigenes Config-Feld **`AppConfig.hall_colors: Vec<HallColorConfig>`**
(`{ hall, color }`, `#[serde(default)]`), unabhängig von `hall_layouts`,
mit denselben Matching-Regeln (getrimmter, case-insensitiv verglichener
Hallenname; `upsert_hall_color` ersetzt statt dupliziert).

## Alternativen

- **Farbe als Feld in `HallLayoutConfig`:** erzwänge, dass eine Halle mit
  Farbe auch ein Raster hat — `columns`/`origin`/`serpentine` sind dort
  Pflicht. Entweder Phantom-Layouts oder ein Umbau auf optionale Felder
  mit Migrationsrisiko für bestehende Configs (Auto-Update!). Verworfen.
- **Farben im TL-Web-Profil:** Farben sind geräteübergreifende Wahrheit
  des Turniers, keine Ansichtssache je Gerät. Verworfen.

## Konsequenzen

- Zweite namensbasierte Hallen-Zuordnung neben `hall_layouts` — bewusstes
  YAGNI-Duplikat (Muster ADR 0022/0029): Trim/Case-Matching wird je Store
  implementiert und getestet.
- `HallLayoutConfig` bleibt unangetastet, kein Migrationsrisiko.
- `keep_host_managed_fields` muss `hall_colors` wie `hall_layouts` vor dem
  SetupWizard-Speichern schützen (eigener Wächter-Test).
