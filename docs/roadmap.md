# Roadmap & offene Punkte

Lebende Liste der offenen Arbeiten an bts-light. Erledigte Versionen stehen
im [changelog.md](changelog.md); hier steht, was **noch** ansteht.

> Stand: 2026-05-22, nach Release v0.9.0 (Court-Monitor inkl. TV-Verwaltung
> und mDNS).

## Als Nächstes

- **Repo-Umbenennung** → Anzeigename „badhub BTP controller", GitHub-Repo
  `badhub-btp-controller`. Wichtig: Tauri-`identifier` `de.badhub.btslight`
  und der Updater-Pfad `download/bts-light/` bleiben **stabil**, sonst
  brechen bestehende Installationen beim Auto-Update. Der angezeigte
  `productName` kann separat und mit Bedacht wechseln.

## Geplant

- **Code-Signing des Windows-Installers.** Aktuell unsigniert → Windows
  zeigt beim ersten Start eine SmartScreen-Warnung. Optionen: Azure Trusted
  Signing vs. klassisches OV/EV-Zertifikat — Kostenentscheidung offen. Das
  Auto-Update ist davon unabhängig (eigenes Signaturschlüsselpaar).
- **CI-Wartung.** Die Release-/CI-Workflows nutzen Node-20-Actions
  (`actions/checkout@v4`, `actions/setup-node@v4`,
  `softprops/action-gh-release@v2`) — vor dem erzwungenen Node-24-Umstieg
  (ab 2026-06-02) aktualisieren. Außerdem leitet GitHub `windows-latest`
  ab 2026-06-15 auf `windows-2025` um — Build dort gegenprüfen.
- **Changelog pro Version sichtbar machen.** [`docs/changelog.md`](changelog.md)
  pflegt die Versionshistorie bereits, ist aber nirgends für Nutzer
  sichtbar — das Auto-Update zeigt aktuell nur „BTS Light X.Y.Z". Ziel:
  den Changelog-Eintrag der jeweiligen Version in die Update-Meldung
  (`latest.json` → `notes`) und in die GitHub-Release-Notes übernehmen,
  optional ein „Was ist neu"-Hinweis in der App. So sieht man, was sich
  von Version zu Version geändert hat.

## Feature-Wünsche

Von der Turnierleitung gewünscht, noch nicht eingeplant:

- **Aufgaben- & Walkover-Übersicht.** Eine Seite bzw. Kachel in bts-light,
  die während des Turniers alle Aufgaben und alle daraus gewerteten
  Walkovers auflistet — Überblick für die Turnierleitung.
- **Walkover zurücknehmen.** Eine kampflose Wertung wieder rückgängig
  machen können (Match in BTP zurück auf offen / `ScoreStatus = 0`),
  falls sie versehentlich oder falsch gesetzt wurde.
- **Tablet-Verbindungsanzeige im Cloud-Modus.** Schließt bts-light, bleibt
  das Tablet mit dem Relay verbunden und zeigt weiter „verbunden" — es
  erfährt nicht, dass der Host (bts-light) weg ist. Der Relay sollte den
  Tablets ein „Host offline"-Signal schicken, damit das Tablet ehrlich
  „Warte auf Turnier-PC" anzeigt.

## Court-Monitor — offene Punkte

Der Court-Monitor ist umgesetzt (v0.7.0–v0.9.0, [court-monitor.md](court-monitor.md),
[pi-setup.md](pi-setup.md), [pi-master-image.md](pi-master-image.md)).
Offen für das **Verleih-Set**-Konzept (Technik wird an Turnierleitungen
verliehen):

- **Master-Image erstellen + hosten.** Den „Golden Master"-Pi einmal auf
  echter Hardware bauen, die Karte als `bts-monitor.img.xz` sichern und in
  den Download-Bereich auf badhub.de legen. Ablauf: [pi-master-image.md](pi-master-image.md).
- **mDNS auf echter Hardware prüfen.** `bts-light.local` ist gebaut, aber
  noch nicht auf einem echten Pi + Windows-Laptop getestet (Windows-
  Firewall für mDNS/UDP 5353; Pi löst `.local` via avahi auf). IP-Rückfall
  ist eingebaut.
- **Online-Anleitung veröffentlichen.** [pi-setup.md](pi-setup.md) als
  echte Webseite (badhub.de) bereitstellen und **in bts-light verlinken**
  (Knopf „Einrichtungs-Anleitung" auf der Court-Monitore-Seite).
- **2-Felder-pro-TV-Modus.** Zwei benachbarte Felder auf einem großen TV
  (`…/display?courts=3,4`).

## Bekannte Einschränkungen / technische Schuld

- **Liga-Matches** (`PlayerMatches` in BTP) sind nicht abgedeckt — bts-light
  verarbeitet nur Einzel-/Doppel-Draws.
- **Spielsystem fest Best-of-3 bis 21.** BTP liefert das Spielformat im
  aktuellen Parser nicht zuverlässig; der Tablet-Spielzettel nimmt den
  Badminton-Normalfall an.
- **Liveticker-Staleness uneinheitlich.** Im `/live`-Picker fällt das
  „Live"-Badge nach 4 Min ohne Heartbeat weg, die Detailseite (`?t=`)
  zeigt „Nicht mehr live" erst nach 10 Min. Die 10-Min-Schwelle ist
  bewusst lose gehalten, solange Nicht-Heartbeat-Quellen (`letilo/bts`)
  pushen können. Angleichen, sobald bts-light die einzige Quelle ist.
- **Keine Frontend-Tests.** Der Rust-Kern ist per `cargo test` abgedeckt;
  die React-Seite (u. a. `announcer.ts` — Court-Phrase, Ansage-Segmente,
  Auto-Sprach-Regel) hat kein Test-Setup. badhub-tournament nutzt Vitest
  inkl. `announcer.test.ts` — das ließe sich übernehmen.
- **Alte Liveticker-Test-Turniere.** `lehiero`, `christian-zum-test` und
  die Legacy-Zeile `default` stehen in `liveticker_tournaments` noch auf
  `is_active = 1` und machen `/live` ohne `?t=` mehrdeutig. Im
  Liveticker-Admin auf inaktiv setzen.
- **`docs/ops/deployment.md` teils veraltet** (badhub-Repo): Der Abschnitt
  „Deploy: Produktion" beschreibt noch das KAS-`deploy_prod.sh`, obwohl
  Prod längst über `deploy_hetzner.sh` auf Hetzner läuft.
