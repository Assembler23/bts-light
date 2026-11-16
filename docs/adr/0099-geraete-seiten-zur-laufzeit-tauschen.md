# 0099 — Geräte-Seiten zur Laufzeit tauschen: signiertes Paket vom Hotfix-Zweig, an eine Kern-Version gebunden

- **Status:** accepted
- **Datum:** 2026-11-16

## Kontext

Die Geräte-Seiten (`src-tauri/assets/*.html`) sind per `include_str!` in die
Programmdatei einkompiliert. Ein Fix an `tablet.html` braucht im LAN-Betrieb
einen Neustart der App — mitten im Turnier wird er deshalb nicht eingespielt.
Im Cloud-Betrieb liefert das Relay eigene Kopien und deployt bei jedem Merge.

Kräfte:

- Nachgeladene Seiten sind Code, der auf Tablets mit Ergebnis-Eingabe und im
  TL-Web hinter dem Gerätezugang läuft.
- `latest.json` ist unsigniert, und die minisign-Signatur des Updaters deckt
  nur die Bytes eines Artefakts, nicht dessen Version.
- `main` läuft den Release-Tags voraus (16.11.2026: neun Merges nach dem Tag,
  acht mit Rust-Änderungen); Seiten aus `main` können einen Kern
  voraussetzen, den der Turnier-PC nicht hat.
- Die Marke des Versionsabgleichs (`seiten_marke`) ist heute ein `OnceLock`,
  weil sie sich „zur Laufzeit nie ändert".

## Entscheidung

- **Ein-Datei-Paket** `seiten-<kern>-<folge>.btsp`: gzip-JSON mit `art`,
  `kern`, `folge` und allen Seiten, als Ganzes minisign-signiert. Bindung und
  Inhalt liegen unter einer Signatur; der unsignierte Index `seiten.json`
  liefert nur Zahlen, die Lade-Adresse baut die App selbst.
- **Quelle ist ein Hotfix-Zweig `hotfix/vX.Y.Z` ab dem Tag.** Der Paket-Job
  bricht ab, sobald der Zweig einen Kern-Pfad gegenüber dem Tag ändert.
- **Bindung an genau eine Kern-Version** (`kern` == eigene Version) und eine
  streng wachsende `folge` (Commits seit dem Tag). Das ist zugleich der
  Downgrade- und Replay-Schutz; der eingebaute Stand ist Folge 0.
- **Signiert wird erst nach Freigabe** durch einen Maintainer (geschütztes
  Environment, nur für `hotfix/v*`).
- **Globaler Seitenstand** `static RwLock<Arc<Seitenstand>>` in
  `tablet/assets.rs`; die Marke ist Teil desselben `Arc` und wechselt atomar
  mit. Die `pub const` entfallen. Die Slave-Brücke nutzt dasselbe Modul.
- **Alle 17 Seiten** sind tauschbar; die Seitenmenge eines Pakets muss exakt
  der eingebauten entsprechen, Namen werden nie als Pfad benutzt.
- **Ablage auf Platte** (`app_data_dir/seiten/`, aktiv + vorher) mit voller
  Prüfung bei jedem Start; jeder Fehler führt zum eingebauten Stand.
- Kein neues Auto-Neuladen: Tablets über den vorhandenen Abgleich, Monitore
  über den vorhandenen Befehl, der Rest von Hand.

## Alternativen

- **Paket bei jedem `main`-Merge** (erste Wahl im Grill). Verworfen: Bei
  Rust-Änderungen seit dem Tag wäre das Paket entweder unsicher oder — in der
  sicheren Form „nur ohne Kern-Diff" — beim heutigen Merge-Takt fast nie
  vorhanden.
- **Mindest-Kern statt exakter Bindung.** Verworfen: Ein Zweig ab dem Tag
  trägt die Seiten dieses Tags und würde auf einem neueren Kern dessen Seiten
  zurückdrehen.
- **Nur im RAM.** Verworfen: Nach jedem App-Neustart wären die alten Seiten
  zurück und die Tablets lüden rückwärts.
- **Zip mit Einzeldateien.** Verworfen: Pfad-Traversal-Fläche, `zip` wäre
  eine neue direkte Dependency.
- **Seiten-Quelle im `TabletState`.** Besser testbar, aber Slave-Brücke und
  freie `render_*`-Funktionen bräuchten neue Parameter — großer Umbau ohne
  Nutzen für das Ziel.
- **Seiten im LAN vom Relay oder von badhub holen.** Verworfen: Das LAN muss
  ohne Internet funktionieren.
- **Ohne Freigabe signieren.** Verworfen: Dann könnte jeder Push auf den
  Zweig Code an alle Installationen anbieten.

## Konsequenzen

- Ein Hotfix kostet einen Cherry-Pick auf den Hotfix-Zweig **und** den
  regulären Merge nach `main` (Cloud-Geräte, nächstes Release).
- Der Signaturschlüssel wird erstmals außerhalb des Tag-Wegs verwendet;
  Schranken sind Environment, Freigabe und der Kern-Diff-Abbruch.
- Eine Seite, die neue Platzhalter, Routen oder `relay-proto`-Felder braucht,
  kann kein Hotfix-Paket sein; sie kommt mit einem Release.
- Nach einem Kern-Update verfällt ein Paket von selbst (andere Kern-Version).
- Ein Seitenabruf kostet eine String-Kopie statt eines `&'static str`.
- Cloud-Geräte und nicht eingespielte Slave-PCs erreicht der Tausch nicht;
  das Banner sagt es.
- `seiten_marke` ist nicht mehr konstant; Tests müssen den Wechsel abdecken.
