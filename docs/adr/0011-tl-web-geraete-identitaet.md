# 0011 — Geräte-Identität für Turnierleitungs-Geräte: host-ausgestellte, widerrufbare Tokens

- **Status:** accepted
- **Datum:** 2026-08-07

Gehört zur Spezifikation
[docs/features/turnierleitung-web.md](../features/turnierleitung-web.md).
Der Schreibkanal selbst ist Gegenstand von
[ADR 0010](0010-tl-web-schreibender-cloud-pfad.md).

## Kontext

Turnierleitungs-Geräte sollen sich einmalig koppeln und danach aus dem
Internet schreibend auf das Turnier zugreifen. Die naheliegende Antwort
wäre, den vorhandenen Kopplungscode wiederzuverwenden — der ist erprobt und
ausdrücklich für Fremdgeräte gebaut (ADR 0004).

**Er gibt aber die nackte `install_id` heraus** (`relay/src/main.rs:834`
antwortet mit dem Namespace). Und die `install_id` ist nach R6 weit mehr als
eine Kennung:

- Sie **ist** der Relay-Namespace — wer sie kennt, erreicht alle
  Namespace-Routen des Turniers.
- Sie öffnet den **geerbten Azure-Sprachschlüssel** (ADR 0003).
- Sie öffnet den **Host-Slot**: Nach der Zombie-Ablösung übernimmt ein
  zweiter Host den Platz, wenn der Master 15 Sekunden stumm ist.
- Sie ist zugleich die **Zuordnung der hochgeladenen Diagnose-Logs** (R6).

Der Kopplungscode ist zudem beliebig oft einlösbar, an kein Gerät gebunden
und nicht widerrufbar. Auf einem privaten Helfer-Handy im Internet läge
damit dauerhaft der Generalschlüssel des Turniers.

ADR 0004 hat diesen Punkt selbst vorweggenommen: Die Entscheidung sei „neu
zu bewerten, wenn Namespaces je ein echtes Auth-Token bekommen". Genau
dieser Fall ist eingetreten.

Weitere Randbedingungen:

- **Der Relay hält seinen Zustand nur im Arbeitsspeicher.** Ein
  Relay-Neustart — und der passiert bei **jedem** Deploy, global für alle
  Installationen — löscht ihn.
- **TL-Web muss auch im reinen LAN-Betrieb funktionieren** (R3), also ohne
  Relay.
- **Geheimnisse liegen bereits im Klartext in `config.json`** (BTP-Passwort,
  badhub-Passwort, Azure-Schlüssel). Ein Token dort ist kein neuer
  Präzedenzfall.
- **Die Zielgruppe ist Turnierleitung ohne IT-Kenntnisse.** Kopplung muss
  ohne Erklärung funktionieren.

## Entscheidung

**Der Turnier-PC stellt die Tokens aus; der Relay spiegelt sie nur. Die
`install_id` verlässt den Master für TL-Web nicht.**

1. **Ausstellung am Host.** Die Turnierleitung legt in bts-light ein Gerät
   mit Namen an; dabei entsteht ein Zufallstoken. Erzeugt wird es im
   Frontend mit demselben Mittel wie die `install_id` selbst — also **ohne
   neue Abhängigkeit**.
2. **Speicherung am Host** in `config.json` unter `tl_web.devices`, mit
   Vorgabewerten für alte Konfigurationen. Damit überlebt die Kopplung einen
   App-Neustart und ist im Programm sichtbar und löschbar.
3. **Spiegelung im Relay.** Der Host pusht die Token-Menge (nur die Tokens,
   keine Gerätenamen); der Relay hält eine Zuordnung Token → Namespace. Push
   sofort bei Änderung und regelmäßig als Auffrischung.
4. **Widerruf** = Eintrag löschen. Der nächste Push ersetzt die Menge, das
   Gerät ist binnen etwa zwei Sekunden gesperrt. Im LAN greift der Widerruf
   ohne Umweg, weil der Server die Konfiguration frisch von der Platte
   liest.
5. **Relay-Neustart heilt sich selbst:** Der Host verbindet sich neu und
   pusht die Tokens erneut. Bis dahin werden alle Anfragen **abgewiesen**,
   nicht durchgelassen (fail closed).
6. **TL-Routen sind namespacefrei.** Der Namespace steht nirgends in der
   URL; der Relay schlägt ihn über das Token nach. Bei der Kopplung steht
   das Token im **Fragment** der Adresse — es wird also nie an einen Server
   gesendet und erscheint in keinem Zugriffsprotokoll. Die Seite legt es
   lokal ab und sendet es fortan als Kopfzeile.
7. **Kopplung per QR-Code am Turnier-PC.** Kein abzutippender Code.
8. **Ein Identitäts-Umzug (ADR 0006) nimmt die Tokens nicht mit.** Sie
   werden beim Export entfernt, wie die Passwörter; die Geräte koppeln sich
   am neuen PC neu — ein Scan. Ein Test sichert das ab.
9. **Der Kopplungscode aus ADR 0004 bleibt unverändert**, was er ist: der
   Weg, einem Slave-PC oder Monitor den Namespace mitzuteilen. Er wird
   **nicht** mit dem TL-Token verheiratet; das Token ist geräte-, nicht
   namespacegebunden.

**Was das sicherheitlich wirklich ändert — ehrlich benannt:** Wer die
`install_id` kennt, erreicht weiterhin die Tablet- und Monitor-Seiten. Daran
ändert TL-Web nichts. Er erreicht aber **nicht** den
Turnierleitungs-Schreibkanal, denn der verlangt zusätzlich ein am Master
ausgestelltes, einzeln widerrufbares Token. Das ist eine echte Verschärfung
gegenüber dem Status quo, kein Etikett.

## Alternativen

**(a) `install_id` weiterreichen (heutiger Kopplungsmechanismus).**
*Verworfen:* Jedes gekoppelte Handy hielte dauerhaft den Generalschlüssel
des Turniers — inklusive der Möglichkeit, den Host-Slot zu übernehmen und
den Sprachschlüssel zu erben. Nicht widerrufbar, nicht gerätegebunden. Für
einen schreibenden Zugang aus dem Internet nicht vertretbar.

**(b) Der Relay stellt die Tokens aus** (Erweiterung von ADR 0004): Master
erzeugt einen Code, das Gerät löst ihn ein und erhält vom Relay ein Token.
*Verworfen:* Der Relay hält seinen Zustand nur im Arbeitsspeicher — **ein
Relay-Neustart würde mitten im Turnier alle Geräte auf einmal ausloggen**,
und der Deploy ist global, passiert also auch wegen eines fremden Features.
Um das zu heilen, müsste der Host die Tokens ohnehin kennen — dann ist man
bei der gewählten Lösung. Außerdem bräuchte der reine LAN-Betrieb einen
zweiten, andersartigen Mechanismus.

**(c) Konten und Rollen im badhub-Backend** (Muster ADR 0009).
*Verworfen:* Cross-Repo-Kopplung, die die Spec ausschließt; braucht Internet
auch im LAN-Fall; der Befehlsweg liefe trotzdem über den Relay zum Host.
Echte Konten wären fachlich reizvoll, lösen hier aber ein Problem, das wir
nicht haben — die Geräte gehören alle einem Team.

## Konsequenzen

**Positiv**

- Die `install_id` verlässt den Master für TL-Web nicht; ein verlorenes
  Gerät kostet ein Token, nicht das Turnier.
- Widerruf ist sofort wirksam, sichtbar und ohne Neustart — in beiden
  Verbindungswegen.
- Ein Relay-Neustart loggt niemanden dauerhaft aus; das System heilt sich.
- Ein und derselbe Mechanismus für LAN und Cloud, an einer Stelle geprüft.
- Kein Token im Zugriffsprotokoll, kein Namespace in der Adresse.
- Keine neue Abhängigkeit.

**Negativ / Kosten**

- Die Tokens liegen im Klartext in `config.json` — wie die vorhandenen
  Passwörter. Wer Zugriff auf den Turnier-PC hat, hat sie; das ist
  akzeptiert, weil er ohnehin alles hat.
- Eine zusätzliche Frame-Art und eine Zuordnungstabelle im Relay.
- Der Widerruf wirkt erst mit dem nächsten Push (etwa zwei Sekunden) — für
  den Anwendungsfall „Gerät verloren" ausreichend, für „Angreifer aktiv im
  System" nicht sofort.
- Der Identitäts-Umzug bekommt einen Sonderfall mehr: Geräte müssen neu
  gekoppelt werden. Das ist gewollt, muss aber dokumentiert sein, sonst
  gilt es als Fehler.
