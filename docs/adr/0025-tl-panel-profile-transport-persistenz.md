# 0025 — TL-Panel-Profile: Hybrid-Transport, Persistenz installationsweit

- **Status:** accepted
- **Datum:** 2026-08-15

## Kontext

[TL-Web-Panelsystem](../features/tl-web-panelsystem.md) führt benannte Profile
ein (Panel-Sichtbarkeit/-Reihenfolge/-Höhe + Anzeige-Einstellungen), die jedes
Gerät dauerhaft einem Profil zuordnet. Zwei Fragen waren beim Grill offen
(`docs/features/_intake/tl-web-panelsystem/2-grill.md`) und wurden im How-To
(`docs/features/_intake/tl-web-panelsystem/3-how-to.md`) untersucht:

1. Wie erreichen Profildaten die Browser-Clients über LAN **und**
   Cloud-Relay (R3)?
2. Wo werden Profile persistiert — installationsweit oder turniergebunden
   (Muster ADR 0022)?

Zentraler Befund zu Frage 1: `relay::tl_state_route` liefert einen einzigen,
je Namespace gecachten `TlState`-Blob mit ETag aus `(tl_gen, rev)` —
identisch für jedes anfragende Gerät. Eine geräteindividuelle Information
(„welches Profil ist meins") lässt sich nicht in diesen geteilten Blob
schreiben, ohne entweder den Cache-/ETag-Vorteil zu zerstören (ein Blob je
Gerät) oder falsche Zuordnungen zu riskieren (letzter Poller gewinnt).

Zu Frage 2: Die Gründe von ADR 0022 (Personendaten, BTP-ID-Bezug,
Config-Rückschreib-Kollision) wurden einzeln gegen Profile geprüft und
treffen alle nicht zu — Profile enthalten keine Personendaten, referenzieren
keine BTP-IDs, und die Rückschreib-Kollision ist bereits durch den
etablierten `keep_host_managed_fields`-Mechanismus gelöst (nutzen `tl_web
.devices`/`hall_layouts` bereits). Zusätzlich sind Profile
geräteklassen-/installationsbezogen, nicht turnierbezogen.

## Entscheidung

**Transport — Hybrid:**
- Der **Profil-Katalog** (alle Profile inkl. Inhalt) wird in `TlState`
  eingebettet (`profiles: Vec<TlPanelProfileView>`, `default_profile_id`),
  Muster `TlHallLayout`/`layouts_view` — geteilt, klein, unkritisch, folgt
  dem bewährten Cache-Modell.
- Die **individuelle Geräte-Zuordnung** reitet auf dem bestehenden
  `HostFrame::TlAuth`-Spiegel: `TlAuthDevice.profile_id` (neu). Der Relay
  hält eine zweite Parallel-Map (`tl_token_profile`) neben `tl_tokens` und
  liefert sie als Antwort-Header `X-Tl-Active-Profile` auf jede
  `/tl/api/state`-Antwort — auch bei 304, da Header immer gesendet werden,
  der große Body cachebar bleibt. Der LAN-Pfad setzt denselben Header
  direkt aus `tl_device()`.
- **Schreiben** (Anlegen/Bearbeiten/Löschen/Wählen/Default) läuft über
  `TlAction` (`ProfileSave`/`ProfileDelete`/`ProfileSelect`/
  `ProfileSetDefault`), einmal validiert in `tl.rs::execute`, von LAN-Server
  und Relay-Client gleichermaßen aufgerufen (R5-Muster). `ProfileSelect`
  braucht keine Selbstidentifikation im Client — das aufrufende Gerät ist
  aus der Bearer-Token-Authentifizierung bekannt.

**Persistenz — `AppConfig`-weit**, installationsweit statt turniergebunden:
`TlWebConfig.profiles: Vec<TlPanelProfile>` + `default_profile_id: String`,
mit `#[serde(default)]`. Ergänzend: `keep_host_managed_fields` schützt live
editierte Profile vor Überschreiben durch den Setup-Wizard;
`identity_bundle` strippt Profile NICHT (kein Zugang/Secret, wandern bei
PC-Umzug mit wie `hall_layouts`); `apply_imported_identity` folgt dem
`hall_layouts`-Muster (leeres importiertes Feld überschreibt Bestehendes
nicht).

## Alternativen

- **Alles eingebettet in `TlState` (naiv)**: verworfen für die individuelle
  Zuordnung — der geteilte Cache-Blob kann keine per-Gerät unterschiedliche
  Information tragen, ohne seinen Cache-Vorteil zu verlieren oder falsche
  Zuordnungen zu riskieren.
- **Dedizierte Request/Response-Route** für Katalog + Zuordnung, Muster
  `/tl/api/officials/{id}`: verworfen — dieses Muster passt für seltenes,
  bewusstes Nachladen (Sperrlisten, Punktverlauf), nicht für eine Information,
  die bei **jedem** Poll-Zyklus (alle 2 s) gebraucht wird. Ein
  Request/Response-Umweg über `req_id`/Oneshot bei jedem Poll wäre unnötige
  Latenz und Komplexität.
- **Turniergebundene Datei** (Muster ADR 0022, `officials-state.json`):
  verworfen — Profile sind geräteklassen-/installationsbezogen; ein
  turniergebundener Speicher würde sie bei jedem Turnierwechsel verwerfen,
  ein Rückschritt gegenüber dem heutigen `localStorage`-Verhalten (das
  immerhin geräteweit über Turniere hinweg persistiert).

## Konsequenzen

- Zwei neue kleine Datenpfade statt einem: `TlState.profiles` (Katalog) und
  `X-Tl-Active-Profile`-Header (Zuordnung) — etwas mehr Fläche als eine
  Ein-Weg-Lösung, aber jeder Teil folgt einem bereits etablierten,
  getesteten Muster (`layouts_view` bzw. `TlAuth`-Spiegel).
- `security-reviewer` prüft explizit, dass der Header niemals über
  Namespace-Grenzen hinweg falsch zugeordnet wird und kein Token/keine
  Kennung leakt (analog zum bestehenden „Token eines Namespace erreicht nie
  einen fremden Host"-Testmuster).
- Ältere App-Versionen ignorieren die neuen Felder schlicht (`#[serde(default)]`)
  — Rollback bleibt gefahrlos, kein Migrationscode für alte `localStorage`-Werte.
- Profile überstehen Turnierwechsel und PC-Umzug (Identitäts-Export) —
  konsistent mit dem übrigen geräteklassenbezogenen Zustand (`hall_layouts`,
  `tl_web.devices`).
