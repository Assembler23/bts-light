# 0017 — Reconnect-Konfliktmodell: Ownership „Slot-Halter gewinnt" statt rev-Zähler

- **Status:** proposed
- **Datum:** 2026-08-13

Gehört zu [docs/features/turnier-robustheit.md](../features/turnier-robustheit.md)
(Paket A / A2).

## Kontext

Bei störanfälligem WLAN muss das zählende Tablet die **Wahrheit des laufenden
Spiels** halten: verliert es die Verbindung und kehrt im selben Spiel zurück,
soll sein lokaler Stand sich durchsetzen — **außer** ein anderes Tablet hat den
Feld-Slot übernommen und weitergezählt, oder das Match wurde per Hand finalisiert.

Heute entscheidet das ein **`rev`-Zähler** im Tablet (`tablet.html:1319`,
`sameMatch && localRev >= incomingRev`). `rev` wird beim Übernehmen aus dem
geerbten Stand **geseedet** (`applyPersistedState`, `:1847`) und nicht je
Persist hochgezählt (`persistState(true)`, `:1789`). Nach einem Split zählen
altes und übernehmendes Tablet von **derselben Basis** hoch → ihre `rev`-Werte
sind **geräteübergreifend nicht vergleichbar**. Ein spät zurückkehrendes
Alt-Tablet kann mit `localRev >= incomingRev` den **legitim weitergezählten**
Übernehmer überschreiben. `rev` kodiert **kein Ownership**.

## Entscheidung

**Autorität ist der Slot-Halter, nicht der Zähler.** bts-light kennt den aktiven
Zähler je Court bereits (R4): LAN `TabletState.active: HashMap<CourtId,(token,
device_id)>` (`state.rs:256`, `token` monoton aus `token_seq`), Cloud
`Namespace.tablet_devices` (`relay:155`). Das Ownership-Token ist
`(epoch = token, device_id)`.

Die Reconnect-Entscheidung wird eine **reine Rust-Funktion** (unit-testbar), an
beiden Reconnect-Eintritten aufgerufen (`server.rs` Identify/Reclaim; `relay`
`attach_tablet`):

```
reconnect_decision(returning_device, returning_epoch,
                   current_owner: Option<(epoch, device, scored_since_claim)>,
                   finalized) -> KeepLocal | StandDown
```

- `finalized` → **StandDown** (Hand-Ergebnis nicht überbügeln).
- Slot frei **oder** `owner.device == returning_device` → **KeepLocal**
  (Slot-Halter/Reclaimer setzt lokale Wahrheit durch).
- Fremder Owner **und** `scored_since_claim` → **StandDown**.
- Fremder Owner ohne Score seit Übernahme → aktueller Halter gewinnt.
- Echte Divergenz (beide gezählt) → **aktueller Halter gewinnt deterministisch**
  (stiller Verlierer, bewusst).

`scored_since_claim` = per-Court-Flag, gesetzt in `record_score`/
`court_scores.insert`, zurückgesetzt in `claim_court`. Server/Relay senden das
Ergebnis **explizit** im `StateRestore` (`authoritative: bool` + `owner_epoch`/
`owner_device`); das Tablet **folgt** dieser Direktive statt selbst per `rev` zu
entscheiden. `rev` bleibt nur noch Ordnung **innerhalb desselben Owners**.

Ein **Config-Flag** schaltet zwischen neuem Verhalten (Default) und Legacy
(rev) — Rollback zur Laufzeit im Turnier. Fehlt `authoritative` (ältere App),
greift per `serde(default)` der heutige rev-Zweig (Auto-Update-sicher).

## Alternativen

- **`rev`-Zähler beibehalten (Ist):** nachweislich fehlerhaft (überschreibt den
  legitimen Übernehmer). **Verworfen** als Autorität, **behalten** als Ordnung
  innerhalb eines Owners.
- **TL-Eskalation bei Divergenz:** Konflikt anzeigen, Turnierleitung entscheidet.
  Kein UI-Fluss, widerspricht dem Wunsch nach Determinismus im Turnierbetrieb.
  **Verworfen** (der stille Verlierer wird bewusst in Kauf genommen).

## Konsequenzen

- Stützt sich exakt auf R4 („ein aktives Tablet je Court") und die bereits
  getestete Ownership-Struktur; Entscheidung server-/relay-seitig **testbar**.
- Entfernt den rev-Divergenz-Bug; das Tablet wird zum **Folger** einer
  server-berechneten Autorität (klarere Verantwortung).
- **Negativ:** bei echter Doppel-Zählung geht der Stand des Nicht-Halters **still**
  verloren (kein Hinweis) — dokumentiert in `docs/tablet.md`. Neue Wire-Felder
  (`authoritative`, `owner_*`, `finalized`) und ein Config-Flag; Relay muss den
  Slot-Halter für `authoritative` heranziehen. Security-Review: Spoofing von
  `device_id`/Epoch (wessen Score gilt als Wahrheit).
