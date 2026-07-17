# ADR 0003 — Azure-TTS-Konfiguration wird über den Relay an Cloud-Slaves vererbt

Status: akzeptiert (2026-07-17)

## Kontext

Die hochwertige Sprachansage (Azure Neural TTS) braucht Key + Region, die in der
**lokalen** `config.json` jeder Installation liegen. Im Mehr-Hallen-Betrieb
(Master/Slave, [multi-hall.md](../multi-hall.md)) muss der Turnierleiter den Key
deshalb auf jedem Slave-Rechner von Hand eintragen. Das wurde beim ersten
Zwei-Hallen-Praxistest prompt vergessen — der Slave fiel **stumm** auf die
Web-Speech-Standardstimme zurück (Frontend prüft nur `enabled`, das Backend lehnt
ohne Key ab, der Fehler wird verschluckt). Zielgruppe der App sind explizit
Nicht-Techniker: Konfiguration, die man an zwei Orten pflegen muss, wird im
Turnierstress falsch sein.

Randbedingungen:

- Der Cloud-Slave spricht **nur** mit dem Relay (`GET /{ns}/info/announce/state`),
  nie direkt mit dem Master; der Relay hält den Announce-State je Namespace im
  Speicher (relay/src/main.rs `announce_state`).
- Es gibt **keine zusätzliche Auth** zwischen Slave und Master-Namespace: die
  zufällige `install_id` (UUID, R4/R6) ist die Bearer-Capability des Namespace.
- Der Transport ist TLS (wss/https zu badhub.de); der Relay-Prozess sieht
  Klartext-JSON. Relay-Server und App stammen aus demselben Repo/Betrieb.

## Entscheidung

Der Master sendet seine Azure-TTS-Konfiguration (Region, Key, Stimme) als
optionales Feld im bestehenden `HostFrame::Courts`-Push an den Relay; der Relay
speichert sie je Namespace und liefert sie als optionales Feld in
`AnnounceState` an Cloud-Slaves aus. Der Slave nutzt die geerbte Konfiguration
**nur als Fallback**: eine vollständige lokale Azure-Config (Key + Region)
hat immer Vorrang. Die geerbte Config wird ausschließlich im Arbeitsspeicher
(`AppState`) gehalten, **nie** in die `config.json` des Slaves geschrieben.
Gesendet wird nur bei `enabled && key && region`; ist Azure am Master aus,
wird `None` gesendet und ein zuvor geerbter Wert am Slave verworfen.

Erweiterungen als optionale Felder (`#[serde(default)]` / `skip_serializing_if`)
statt neuer Frame-Typen, damit alte Relays/Slaves neue Frames weiterhin parsen
(Deploy-Reihenfolge: zuerst Relay, dann App-Release — aber auch verkehrt herum
bricht nichts, es fehlt nur die Vererbung).

## Alternativen

- **Manuell bleiben (Status quo):** verworfen — hat im ersten Praxistest sofort
  versagt; widerspricht dem Plug-and-play-Anspruch der App.
- **Key per QR-Code/Einmal-Link vom Master übertragen:** verworfen — zusätzlicher
  manueller Schritt pro Slave-Rechner, löst das Vergessen-Problem nur halb und
  braucht trotzdem neuen UI-/Protokoll-Code.
- **Ende-zu-Ende-Verschlüsselung des Keys (Relay sieht nur Ciphertext):**
  verworfen — es gibt keinen zweiten Kanal für den Schlüsseltausch (der Slave
  kennt nur die `install_id`, die auch der Relay kennt); Relay und App werden
  vom selben Betreiber betrieben, der Zugewinn wäre theoretisch.
- **Slave holt den Key direkt beim Master (P2P):** verworfen — der Cloud-Modus
  existiert gerade, weil eingehende Verbindungen/gemeinsames LAN in
  Firmen-Firewall-Umgebungen nicht vorausgesetzt werden können (R3).

## Konsequenzen

- Slave-Rechner brauchen keine Azure-Eingaben mehr: Schalter am Master genügt,
  die ferne Halle klingt identisch. Weniger Turniertag-Fehlkonfiguration.
- Der Azure-Key verlässt den Master-Rechner und liegt im Relay-RAM sowie bei
  jedem, der die `install_id` des Namespace kennt. Das entspricht der
  Vertrauensstufe des übrigen Cloud-Modus (Ergebnisse, Spielernamen), ist aber
  eine bewusste Ausweitung auf ein **Secret**. Mitigation: Azure-Speech-Keys
  sind im Azure-Portal jederzeit rotierbar und kostenlimitierbar; die
  `install_id` ist eine zufällige UUID und wird nirgends öffentlich angezeigt.
- Relay-Server muss vor dem App-Release deployt werden, sonst greift die
  Vererbung nicht (bricht aber nichts).
- Neu zu bewerten, wenn der Relay je Namespaces mit erratbaren IDs zulässt,
  Namespace-IDs in Logs/URLs Dritter auftauchen oder ein echter
  Auth-Mechanismus (Token je Slave) eingeführt wird — dann sollte der Key an
  diesen gebunden werden.
