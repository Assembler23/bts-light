# 0028 — Satzpausen enden erst mit „Weiterspielen", nicht mit dem Countdown

- **Status:** accepted
- **Datum:** 2026-08-16

## Kontext

Die Turnierleitung soll sehen, ob eine Satz-/Spielpause **überzogen** wird
(Spec `docs/features/spielzeiten-prognose.md`, Nebenauftrag E9/E10). Das
Tablet beendete die BWF-Pausen (60 s bei 11 Punkten, 120 s Satzpause) bisher
bei Countdown 0 **automatisch** (`tickBreak` → `endBreak`) — der
Pausen-Block verschwand aus dem gespiegelten Spielzustand, eine Überziehung
war damit prinzipiell unbeobachtbar. Die Behandlungspause (`injury`, ohne
Endzeit) fiel beim typisierten `TlPause`-Parse sogar komplett heraus.

## Entscheidung

**Das Tablet hält die Pause, bis der Schiedsrichter aktiv „Weiterspielen"
tippt** (Nutzer-Entscheid E9, bewusst gegen die Host-Heuristik):

- `tickBreak` beendet nicht mehr automatisch; nach Ablauf zählt das Overlay
  rot hoch („überzogen +m:ss") und meldet einmalig `break_overrun` ins
  Diagnose-Log. Reload/Übernahme behalten auch eine abgelaufene Pause.
- `STATE.pause` trägt zusätzlich `startedAt` (Server-Zeit) — reist opak
  über den bestehenden `court_state`-Spiegel, keine Protokolländerung.
- Der Host publiziert `TlPause { kind, ends_at_ms: Option, started_at_ms:
  Option }`; die TL-Seite rechnet Countdown/Überziehung selbst gegen die
  Server-Uhr und zeigt die Behandlungspause als „Behandlung seit …" (E10).

## Alternativen

- **Host merkt sich das letzte `ends_at_ms` je Feld und leitet „überzogen"
  ab** (Tablet unverändert): kein Eingriff ins SR-Gerät, aber neuer
  Host-Zustand plus Heuristik fürs Pausenende — „überzogen" stünde auch
  dann noch da, wenn längst gespielt wird (einziges Abbruchsignal wäre der
  nächste Score-Eingang), und die Überziehung wäre geschätzt statt
  gemessen. Verworfen: zwei Wahrheiten für einen Zustand.
- **Auto-Ende behalten und nur die letzte Pause anzeigen:** zeigt nie
  „läuft gerade über", sondern nur Historie — verfehlt den Zweck.

## Konsequenzen

- Verhaltensänderung am Schiedsrichter-Gerät: „Weiterspielen" ist jetzt der
  einzige reguläre Pausen-Ausgang (der Hinweistext im Overlay sagt das).
  Vergisst der SR den Tipp, klemmt die Pause **sichtbar** (rotes Overlay am
  Tablet, rotes „überzogen" in der TL-Sicht) — genau der gewollte Effekt;
  die Nettozeit ist davon unberührt (durchgehende Uhr).
- Altes Tablet + neuer Host: Pause endet wie bisher bei 0, „überzogen"
  erscheint nie fälschlich (sanfte Degradation). Neues Tablet + alter Host:
  der opake Spiegel bleibt unverändert, alte `tl.html` zeigt das statische
  Badge.
- Cloud-Geräte bekommen `tablet.html`/`tl.html` erst per Relay-Deploy —
  Reihenfolge Relay → Client-Release einhalten.
