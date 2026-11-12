# 0090 — Überlast am Relay und Mindestschutz gegen Schein-Turniere

- **Status:** accepted
- **Datum:** 2026-11-11
- Spec: [`relay-haerten`](../features/relay-haerten.md), Abschnitte b), g).
- Berührt [ADR 0084](0084-undo-stand-format-v2.md) (Deckel des Tablet-Zustands) und
  [ADR 0048](0048-substrom-adressierung-traeger.md) (Träger der fernen Halle).

## Kontext

Der Relay steht ohne Anmeldung im Internet; die `install_id` ist der einzige Zugang. Alle
fünf WebSocket-Wege laufen mit dem Bibliotheks-Standard von 64 MiB je Nachricht, etliche
Felder sind ungedeckelt (Satzliste, Feldnummern, Brief, Labels), ein Mengen-Takt fehlt bis
auf eine Stelle, und alle Kanäle sind unbegrenzt. Der Dienst hat keine Speicher-Obergrenze;
ein Speichermangel träfe den Server, auf dem auch badhub.de läuft. Bilder lassen sich für
jeden bestehenden Namespace hochladen, ohne dass ein Host verbunden sein muss.

Zwei Zwänge begrenzen die Lösung: Der Relay wird vor den Clients ausgerollt und muss mit
älteren Hosts, Seiten und der fernen Halle laufen (dort schicken alte Tablet-Seiten Zustände
bis rund 590 KB). Und `max_message_size` trennt bei Überschreitung die Verbindung — ein
Gerät, das immer wieder dieselbe zu große Nachricht schickt, hinge in einer
Verbinde-Schleife. Ein still verworfenes `MatchAssigned` wiederum friert ein Tablet ein.

Entschieden vom Nutzer: minimaler Schutz gegen Schein-Turniere in diesem Paket, **keine**
Host-Anmeldung.

## Entscheidung

1. **Fachliche Deckel verwerfen oder kürzen und melden einmal; die Verbindung bleibt.**
   Werte nach der Tabelle der Spec (u. a. Satzliste ≤ 10, Brief ≤ 16 KiB, Labels ≤ 128
   Zeichen, Feld-Schlüssel ≤ 128 je Namespace).
2. **Transport-Deckel sind reiner Speicherschutz und trennen.** Sie liegen weit über jeder
   legitimen Größe und sind aus den fachlichen Deckeln abgeleitet: Tablet 1 MiB, Monitor und
   TL 64 KiB, Träger 4 MiB, Host `2 × Zettel-Deckel + 64 KiB` (rund 10 MiB), gebunden per
   Compile-Zeit-Zusicherung.
3. **Mengen-Takt je Tablet-Sitzung** (Eimer-Verfahren): 60 Frames Schwung, 20 je Sekunde
   Nachwuchs für Einzel-Frames; 4 und 0,5 je Sekunde für Voll-Syncs. `ping` zählt nicht.
4. **Begrenzte Kanäle; ein voller Kanal trennt den Empfänger.** Größen: Host 1024, Tablet
   und Substrom 64, Monitor und TL 32, Träger-Sammelkanal 1024. Nichts wird still
   verworfen.
5. **Mindestschutz gegen Schein-Turniere:**
   - Speicher-Obergrenze des Dienstes (`MemoryHigh=512M`, `MemoryMax=768M`,
     `MemorySwapMax=0`).
   - Bilder-Tor: Bilder nur, wenn der Host verbunden ist und in dieser Sitzung ein
     `Courts`-Frame geschickt hat — jedes zählt, auch ein leeres. Ohne Host wird der Upload
     abgewiesen, ohne den Rumpf zu lesen.
   - Gesamt-Budget für alle Bilder des Prozesses: 256 MiB, darüber 507.
6. **Keine Host-Anmeldung, keine nginx- oder Cloudflare-Limits** in diesem Paket.

## Verworfene Alternativen

- **`max_message_size` als einziger Deckel:** eine Zeile je Weg, aber jede Überschreitung
  trennt — ein zu großer Tablet-Zustand würde zur Reconnect-Schleife.
- **Host-Transport-Deckel bei rund 8 MiB:** Der fachliche Zettel-Deckel samt JSON-Escaping
  kann darüber liegen; ein alter Host würde bei jedem großen Sammeldruck getrennt.
- **Träger-Umschlag bei 1 MiB:** Ein Format-1-Zustand landet doppelt escaped bei rund
  1 MiB; die ferne Halle mit alter App würde bei jedem Punkt getrennt.
- **Unbegrenzte Kanäle mit Tiefenzähler:** kleinerer Umbau, aber die Grenze ist nur gezählt,
  nicht hart.
- **Bei vollem Kanal verwerfen:** hält die Verbindung, kann aber eine Zuweisung verlieren —
  genau die Einfrier-Klasse.
- **Bilder-Tor nur bei nicht-leerer Feldliste:** wörtlicher, aber ohne geladenes
  BTP-Turnier gäbe es in der Cloud keine Werbung.
- **`limit_conn`/`limit_req` je IP in nginx:** Eine Halle hält 45–90 Dauerverbindungen
  hinter einer Adresse und pollt im Störfall mit über 80 Abrufen je Sekunde; ein wirksames
  Limit träfe genau den Störfall.
- **Host-Anmeldung:** die eigentliche Gegenmaßnahme, aber ein neues Auth-Modell mit
  Schlüsselverteilung an bestehende Installationen — eigener Entwurf.
- **Nur `MemoryMax`, kein Budget im Prozess:** schützt den Server, aber jeder Überlauf
  startet den Dienst für alle Turniere neu.

## Folgen

- Kein einzelner Frame, keine Satzliste, keine Feldnummern-Folge und kein Kanal wächst mehr
  unbegrenzt; legitime Geräte bleiben unter allen Deckeln.
- Ein zu langsamer Empfänger wird getrennt und verbindet neu; ein Tablet bekommt seine
  Zuweisung nach der Neuanmeldung wieder.
- Ein alter Host bekommt nach einem Relay-Neustart seine Bilder 30 s später (sein erster
  Upload kommt vor seiner Feldliste).
- Die Speicher-Obergrenze beendet im Ernstfall den Dienst — für alle Turniere wie ein
  Deploy, aber badhub.de bleibt unberührt. Die Dienst-Datei wird von Hand ausgerollt.
- **Restrisiko, bewusst:** Rund 15 voll bestückte Schein-Turniere reichen für die
  Obergrenze; ein gefülltes Bilder-Budget weist auch echte Werbung ab; 2000 offene
  Schein-Hosts füllen die Namespace-Tabelle. Mit Namespace-Kenntnis bleibt der Host-Slot
  nach 15 s Stille übernehmbar. Das schließt nur eine Host-Anmeldung (Roadmap).
- Für Transport-Deckel, Close-Code und „voller Kanal trennt" braucht der Relay erstmals
  Tests mit echtem WebSocket (dev-dependency `tokio-tungstenite`).
