# Update im Turnierbetrieb — Spezifikation

> Status: **umgesetzt 2026-09-06** (Stufen 1 + 2; Stufe 3 auf der Roadmap).
> Quelle: Nutzer-Frage vom 06.09.2026 („Wäre es möglich, bts-light zu
> updaten, ohne den Turnier-Betrieb zu unterbrechen?"). Betroffene Crates:
> `src-tauri`. ADR: [0057](../adr/0057-update-ablauf-im-rust-kern.md).

## Kontext / Problem

Bis v0.9.278 war das Auto-Update ein Sprung ins Kalte: Klick auf
„Herunterladen & neu starten" lud den Installer, beendete die App, der
Installer startete sie neu — und dann stand die Übertragung, bis jemand
„Starten" drückte. Vergaß das die Turnierleitung, blieb der Liveticker
stumm, die Tablets fanden keinen Host, die Monitore zeigten die
Offline-Blende. Dazu begann die Aufruf-Uhr auf allen belegten Feldern bei
null, weil der Feldstempel nur im RAM lag.

Was die Lücke ohnehin überbrückt: Tablets zählen offline weiter und schieben
ihren Stand nach dem Reconnect nach (ADR 0017), Ergebnisse liegen in der
BTP-Retry-Queue auf Platte, Satzstände in `live-scores.json`, Monitore und
TL-Web kommen über ihre Reconnect-Wächter zurück. Der Neustart selbst kostet
also Sekunden — das Problem war, was danach **nicht** von selbst passierte.

## Zielbild & Erfolgskriterien

1. Nach einem Update läuft die Übertragung **von selbst** wieder an, wenn sie
   vorher lief. Die Lücke ist eine Sache von Sekunden, nicht von Aufmerksamkeit.
2. Die Aufruf-Uhr auf belegten Feldern läuft nach dem Neustart **weiter**.
3. Das Paket wird **im Hintergrund vorgeladen**; der Einbau ist eine
   Entscheidung über den richtigen Moment, nicht über den Download.
4. Die Turnierleitung sieht im Banner, **wie viele Felder belegt** sind, und
   kann wählen: jetzt neu starten oder erst beim Beenden am Abend einbauen.
5. „Beim Beenden einbauen" hinterlässt **keine laufende App** — der Installer
   läuft stumm durch und die neue Version steht beim nächsten Start bereit.

## Nicht-Ziele

- Echtes Zero-Downtime (Prozess-Übergabe, Dienst + Oberfläche) — **Stufe 3**,
  siehe Roadmap „Geplant".
- Autostart der Übertragung bei **jedem** App-Start (Absturz, Neustart von
  Hand). Der Wiederanlauf ist bewusst an den Update-Pfad gebunden.
- Ein Update **erzwingen** oder zeitlich planen („um 18:00 einbauen").

## Entscheidungen

### Der Ablauf lebt im Rust-Kern (ADR 0057)

Prüfen, Laden, Einbauen und der Wiederanlauf-Marker sitzen in `update.rs`
(Tauri-frei, getestet) und den `update_*`-Commands in `commands.rs`. Das
Frontend stößt an und liest (`update_info`) — Regel R1. Vorher rief das
WebView das Updater-Plugin direkt; der Schließpfad in `lib.rs` hätte davon
nichts gewusst.

### Wiederanlauf-Marker statt Autostart-Schalter

`update-resume.json` im App-Datenverzeichnis, geschrieben unmittelbar vor dem
Einbau (`running`, `written_ms`, `from_version`). Der nächste Start liest
und **löscht** ihn und startet die Übertragung, wenn `running` war und der
Marker jünger als 15 Minuten ist. Ein liegen gebliebener Marker (Installer
abgebrochen, PC aus) startet Tage später nichts. Der Start läuft über
denselben `start_sync` wie der Knopf — mit derselben Fehleranzeige.

### Feldstempel aus dem persistierten Bruttostart

`reconcile_on_court` übernimmt beim **ersten** Abgleich nach dem
Prozessstart den Stempel `first_assigned_ms` aus `match-times.json` (der ist
seit ADR 0027 restart-fest). Danach gilt wieder „jetzt". Randfall: Ein Spiel,
das kurz vor dem Neustart das Feld gewechselt hat, bekommt den älteren
Stempel — die Uhr zeigt dann eher zu viel als zu wenig.

### Stiller Installer beim Beenden

Der Tauri-Updater startet den NSIS-Installer immer mit `/R` (App danach neu
starten). Für „beim Beenden" ruft die App den abgelegten Installer selbst
auf: `/S /UPDATE`, ohne `/R`. Der Installer läuft per-user (keine UAC), die
Firewall-Hooks greifen im stillen Lauf nicht (`IfSilent`-Guard). Fehler beim
Start werden nur geloggt — das Beenden scheitert daran nicht.

### Banner-Texte

| Lage | Text |
|---|---|
| Übertragung aus | „Die App startet dafür kurz neu." |
| Übertragung läuft, kein Feld belegt | „Kein Spiel läuft – die Übertragung setzt nach dem Neustart von selbst wieder ein." |
| N Felder belegt | „Auf N Feldern laufen Spiele – der Neustart unterbricht etwa 20 s, die Tablets zählen weiter und die Übertragung setzt von selbst wieder ein." |
| Vorgemerkt | „Es wird beim Beenden von bts-light still eingebaut." |

Kein Modal, keine Sperre: Die Turnierleitung entscheidet.

## Ablauf

```
App-Start ─► update_check ─► Manifest ─► neuer? ─► download (Hintergrund)
                                                        │
                        ┌──── Ready (Banner: Felder belegt: N) ────┐
                        │                                          │
              „Jetzt neu starten"                       „Beim Beenden einbauen"
                        │                                          │
     flush_scores + Marker + Plugin-Install (/P /R)       Vormerkung im AppState
                        │                                          │
              Installer → App neu                    Schließen → flush + /S /UPDATE
                        │                                          │
        take_update_resume → start_sync                  keine App, neue Version
```

## Tests

- `update.rs`: Marker-Frische (inkl. rückwärts gehender Uhr), Lesen-und-
  Löschen, kaputter/abgelaufener Marker, Installer-Argumente ohne `/R`,
  Dateiname-Filter, Phasenmaschine (Vormerken nur mit Paket, kein Paket in
  `Installing`).
- `tablet/state.rs`: Erster Abgleich übernimmt den persistierten Stempel,
  zweiter Abgleich stempelt „jetzt", ohne Stempel bleibt „jetzt".

## Offen / Feldtest

- Echter Update-Lauf mit laufendem Turnier (LAN + Cloud): Lücke messen,
  Wiederanlauf beobachten, Tablets/Monitore beim Zurückkommen zusehen.
- „Beim Beenden": prüfen, dass der stille Installer die neue Version legt und
  die App wirklich nicht wieder aufgeht.
- **Bekannte Grenze (Plugin):** Der Updater ignoriert den Rückgabewert von
  `ShellExecuteW` und beendet den Prozess trotzdem. Blockt ein Virenscanner
  den Installer, ist die App weg, ohne dass etwas eingebaut wurde; der
  Marker bleibt frisch, und der nächste Handstart setzt die Übertragung
  fort. Am Feldtest bewusst einmal beobachten.
- Der zwischengespeicherte Installer (`%TEMP%\bts-light-update-<v>-setup.exe`)
  bleibt nach dem stillen Einbau liegen (rund 10 MB je Version); Aufräumen
  beim nächsten Start ist offen.
- Ein erneutes Prüfen im Offline-Fall lässt ein geladenes Paket bereit
  (Review 06.09.2026) — am Feldtest gegenprüfen, dass die Vormerkung „beim
  Beenden" den Prüflauf überlebt.
