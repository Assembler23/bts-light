# 0058 — Eigene Android-Kiosk-App statt Fully Kiosk, Suche nach dem Pi-Muster

- **Status:** accepted
- **Datum:** 2026-09-06

## Kontext

Die Zähl-Tablets (Fire-Tablets) laufen im Fully Kiosk Browser mit fest
eingetragener Start-URL. Der Turnier-PC hat je Halle eine andere IP, also
muss vor jedem Turnier jemand jedes Tablet anfassen. Die harte Sperre
(Buttons, kein Internet, Exit-PIN) gibt es nur in Fully PLUS, pro Gerät zu
bezahlen. Die Pi-Court-Monitore lösen die Adressfrage seit Monaten von
selbst: gemerkte IP → Subnetz-Scan auf `:8088/health` → mDNS-Rückfall.

## Entscheidung

- Eine **eigene, dünne Kotlin-App** (`android/`) mit WebView übernimmt
  Suche, Vollbild, Sperre, Wachhalten und Logging. Alle Turnier-Logik bleibt
  Webseite vom Turnier-PC; die App ändert sich nur, wenn sich die Hülle
  ändert.
- Die Suche folgt **exakt dem Pi-Muster** und läuft **nur bei Bedarf**
  (Start, WLAN-Ereignis, Ladefehler, Handgriff) — nach dem Laden der Lobby
  keine Pings mehr.
- Die Sperre kommt vom **Device Owner** (einmalig per ADB), App = Home-
  Launcher = Autostart, Lock-Task ohne Nachfrage. Ohne Device Owner weiche
  Anheft-Sperre als Rückfall.
- Die JS-Brücke gibt sich als **`fully`** aus (`getBatteryLevel`,
  `isPlugged`), damit `tablet.html` unverändert bleibt.
- Verteilung als **APK zum Sideload** von `badhub.de/download/bts-light/`,
  gebaut im Release-Workflow, Version aus `package.json`.

## Alternativen

- **Tauri 2 Android-Ziel:** Rust-Wiederverwendung wäre möglich, aber
  Device Owner, Lock-Task und Boot brauchen ohnehin Kotlin-Plugins, und die
  Werkzeugkette (NDK, `cargo-ndk`, Android Studio) ist schwer. Die
  Suchlogik hat rund hundert Zeilen — kein Gewinn.
- **Fully Kiosk fernsteuern** (Launcher-App setzt per REST/Intent die
  Start-URL): zwei Apps zu pflegen, PLUS-Lizenz weiter nötig.
- **Weiche auf badhub.de** (Start-URL ohne IP, Umleitung ins LAN): braucht
  Internet am Tablet, das die Sperre gerade verbieten soll.
- **Feld merken** statt Lobby: verworfen, weil ein Tablet nach Neustart
  sonst still ein belegtes Feld übernehmen könnte (ADR 0017).
- **Cloud-Rückfall in der App:** braucht Ersteinrichtung je Gerät
  (Namespace); Cloud-Tablets bleiben beim QR im Browser.

## Konsequenzen

- Zweite Sprache im Repo (Kotlin) und ein Android-SDK in der CI.
- Ein Signatur-Keystore als GitHub-Secret, der nie wechseln darf
  (Sideload-Updates verlangen dieselbe Signatur).
- Die Ersteinrichtung je Tablet verlangt ein zurückgesetztes Gerät ohne
  Amazon-Konto und einen USB/ADB-Schritt; ob Fire OS den Device Owner
  zulässt, klärt erst der Feldtest.
- Eine IP-Änderung des Turnier-PCs **mitten im Turnier** bemerkt die Hülle
  nicht von selbst (bewusst, kein Hintergrund-Takt); Handgriff im Menü,
  Anstoß aus der Seite als Roadmap-Punkt.
- Fully Kiosk bleibt als Alternative dokumentiert, ist aber nicht mehr der
  empfohlene Weg für das Verleih-Set.
