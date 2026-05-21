# Roadmap & offene Punkte

Lebende Liste der offenen Arbeiten an bts-light. Erledigte Versionen stehen
im [changelog.md](changelog.md); hier steht, was **noch** ansteht.

> Stand: 2026-05-21, nach Release v0.5.0.

## Als Nächstes

- **v0.5.0 im Turnierbetrieb testen** — Walkover nach Aufgabe, Heartbeat
  und der Über-Dialog auf dem Windows-Turnierrechner. Beobachtungen
  zurückspielen, bevor neue Features dazukommen.
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

## Zurückgestellt

- **Teil B — Einzel-Court-TVs über Raspberry Pi.** Pro Spielfeld ein
  kleiner Bildschirm, der den Live-Stand des Courts zeigt. Aus dem
  Cloud-Relay-Konzept ausgeklammert, noch nicht geplant.

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
