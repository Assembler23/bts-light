# Installation und erste Schritte

Diese Seite führt von „nichts installiert" bis zum laufenden Liveticker. Rechne
mit einer knappen Viertelstunde.

## Was du brauchst

- Einen **Windows-PC (64 Bit)**, auf dem der **Tournament Planner** läuft oder
  den er über das Netz erreicht.
- Das **Turnier in BTP**, so wie du es auch sonst führst.
- Die **Zugangsdaten deines Verbands** für den Liveticker. Bei den
  voreingestellten Verbänden sind sie bereits hinterlegt — du wählst nur die
  Kachel.
- Die **turnier.de-Adresse** deines Turniers. Die volle Adresse genügt, die
  Kennung wird daraus selbst herausgezogen.

## Schritt 1 — Das Netzwerk in BTP einschalten

**Das ist der Schritt, an dem die meisten Ersteinrichtungen scheitern.** BTP
nimmt von sich aus keine Verbindungen an.

Im Tournament Planner: **Extras → Tournament Planner Network…** öffnen und das
Häkchen bei **„Enabled"** setzen.
<!-- Der Hinweis steht seit v0.9.281 auch im Fehlertext der App. Faellt er
     dort weg, ist dieser Schritt die einzige Quelle. -->
<!-- pruef: "Tournament Planner Network" in src-tauri/src/btp/client.rs -->

Steht in demselben Fenster ein **Passwort**, merk es dir — es gehört später in
BTS Light in das Feld „BTP-Passwort".

Die Einstellung bleibt in BTP gespeichert; du machst das einmal pro Rechner.

> **Fehlt das Häkchen**, meldet BTS Light später:
> *„Verbindung zu 127.0.0.1:9901 fehlgeschlagen … Zielcomputer verweigerte die
> Verbindung."* Wenn du diese Meldung siehst, ist fast immer genau dieses
> Häkchen die Ursache.

## Schritt 2 — Herunterladen und installieren

Die aktuelle Fassung liegt hier:

**<https://badhub.de/download/bts-light/BTS.Light-setup.exe>**

Dieser Link zeigt immer auf die neueste Version. Alle Versionen mit ihren
Änderungen stehen auf der [Download-Seite](https://badhub.de/download/bts-light/).

**Installiere den PC mit Internetverbindung.** Für das Herunterladen ist das
ohnehin nötig, und der Installer kann eine fehlende Windows-Komponente aus dem
Internet nachladen. Der Turnierbetrieb selbst läuft danach auch offline — nur
Liveticker, Cloud-Weg und Check-In brauchen dauerhaft Internet.

Beim Start des Installers meldet sich **Windows SmartScreen** mit einem blauen
Fenster und der Warnung vor einem unbekannten Herausgeber. Das liegt daran,
dass der Installer noch kein Code-Signing-Zertifikat trägt — es ist kein
Anzeichen für ein Problem mit der Datei. Über **„Weitere Informationen" →
„Trotzdem ausführen"** geht es weiter.

**Während der Installation fragt Windows zweimal nach Administratorrechten.**
Dabei geht es um die **Firewall**: BTS Light trägt zwei Regeln ein, damit
Tablets und Anzeigen den PC im Hallen-WLAN erreichen können.

| Regel | Port |
|---|---|
| BTS Light (Tablets) | TCP 8088 |
| BTS Light (Tablets, verschlüsselt) | TCP 443 und 8443 |
<!-- Die Regelnamen und Ports stehen woertlich im Installer-Hook. Aendert
     sie jemand, zeigt die Fehlersuche unten auf die falschen Ports. -->
<!-- pruef: "name=\"BTS Light (Tablets)\"" in src-tauri/installer/firewall-hooks.nsh -->
<!-- pruef: "localport=8088" in src-tauri/installer/firewall-hooks.nsh -->
<!-- pruef: "localport=443,8443" in src-tauri/installer/firewall-hooks.nsh -->

> **Bestätige beide.** Lehnst du ab, läuft die Installation zwar durch — aber
> **beim ersten Start in der Halle** kommt dann die gewöhnliche
> Windows-Firewall-Abfrage. Und die lässt sich **ohne Administratorrechte
> nicht bestätigen**. Genau dann steht man mit einem Laptop in der Halle, den
> niemand freischalten kann.

Wenn es doch passiert ist: Der **Cloud-Weg** braucht keine dieser Regeln, weil
er nur nach außen verbindet — siehe [Cloud-Verbindung](cloud-relay.md).

Ein Detail bleibt offen: Die automatische Bekanntgabe im Hallennetz läuft über
einen weiteren Weg, den diese beiden Regeln nicht abdecken. Windows kann dafür
beim ersten Start noch einmal nachfragen.

Bei einer **interaktiven** Deinstallation werden die beiden Regeln wieder
entfernt — auch dafür fragt Windows zweimal nach Administratorrechten. Wird
still deinstalliert oder werden die Abfragen abgelehnt, bleiben die Regeln
stehen.

## Schritt 3 — Der erste Start

Beim ersten Start öffnet sich **„BTS Light einrichten"** — derselbe Bildschirm,
den du später als **Einstellungen** wiederfindest.
<!-- pruef: "BTS Light einrichten" in src/pages/SetupWizard.tsx --> Drei Dinge trägst du jetzt
ein, alles andere kannst du zunächst überspringen:

1. **Liveticker-Ziel** — deinen Verband anklicken.

   > **Prüf das, auch wenn es schon richtig aussieht.** Beim ersten Start ist
   > **BVBB vorausgewählt**. Anders als bei den übrigen Angaben hält dich hier
   > nichts auf: Wer den Punkt überliest, sendet sein Turnier in den
   > Liveticker eines fremden Verbands, und niemand merkt es beim Speichern.
2. **Turnier bei turnier.de** — die Adresse deines Turniers hineinkopieren.
3. **BTP-Verbindung** — steht BTP auf demselben Rechner, passt die
   Voreinstellung `127.0.0.1` bereits. Trag das BTP-Passwort aus Schritt 1
   ein, falls es eines gibt.

> **Bei Liga- und Mannschaftswettbewerben ist der Port ein anderer.** Für
> Einzelturniere (BTP) gilt `9901` — das ist die Voreinstellung. Für Liga
> (BLP) ist es **`9911`**. Mit dem falschen Port bekommst du genau den
> Verbindungsfehler aus Schritt 1, obwohl dort alles richtig eingestellt ist.
<!-- pruef: /9911/ in docs/btp_protocol.md -->

Der Speichern-Knopf verlangt außerdem, dass **mindestens ein Verbindungsweg
für die Tablets** aktiv ist — voreingestellt ist LAN, das passt also meistens
von allein. Alle Bedingungen zählt die
[Einstellungs-Referenz](einstellungen.md#zwei-dinge-vorweg) auf; was jede
weitere Option bewirkt, steht ebenfalls dort.

## Schritt 4 — Verbindung prüfen

Drück im Abschnitt „BTP-Verbindung" auf **Verbindung testen**.
<!-- pruef: "Verbindung testen" in src/pages/SetupWizard.tsx -->

- **Erfolg:** Es erscheint der **Name deines Turniers**. Damit ist bewiesen,
  dass die Verbindung steht und du am richtigen Turnier hängst.
- **Fehlschlag:** Die Meldung nennt den Grund. Bei „Zielcomputer verweigerte
  die Verbindung" geh zurück zu Schritt 1.

Der Test **speichert nichts** — er prüft nur.

## Schritt 5 — Starten

Unten auf **„Speichern & Liveticker starten"**.
<!-- pruef: "Speichern & Liveticker starten" in src/pages/SetupWizard.tsx --> Der Knopf bleibt gesperrt,
solange etwas Wichtiges fehlt; siehe
[Einstellungen](einstellungen.md#zwei-dinge-vorweg).

Danach landest du im **Status-Fenster** und die Übertragung läuft.

## Schritt 6 — Geräte anbinden

Erst jetzt lohnen sich die Geräte in der Halle:

- **Zähl-Tablets** — QR-Code des Feldes scannen, fertig.
  Siehe [Das Zähltablett bedienen](tablet-bedienen.md).
- **Fernseher am Feld** — mit einem Raspberry Pi.
  Siehe [Court-Monitor am Raspberry Pi einrichten](pi-setup.md).
  Das fertige Speicherkarten-Abbild liegt auf der [Download-Seite](https://badhub.de/download/bts-light/).
- **Turnierleitung auf dem Tablet** — siehe
  [Turnierleitungs-Oberfläche im Browser](turnierleitung-web.md).
- **Aushang für die Halle** — siehe [Aushang für die Halle](aushang.md).

## Beenden — und was stattdessen gemeint ist

> **Das Schließen-Kreuz beendet das Programm wirklich.** Läuft der Liveticker,
> fragt BTS Light vorher nach („Der Liveticker läuft noch – … Trotzdem
> beenden?"). Wer das bestätigt, beendet die Übertragung für das ganze Turnier.

Willst du das Fenster nur loswerden, **minimiere** es. BTS Light läuft dann im
Hintergrund weiter.

Zurück holst du ein **minimiertes** Fenster am verlässlichsten über die
**Taskleiste**. Es gibt zusätzlich ein Symbol im **Infobereich** (rechts unten
neben der Uhr) mit Doppelklick und einem Menüpunkt zum Öffnen — das holt ein
verstecktes Fenster nach vorn, ein minimiertes aber nicht immer zuverlässig.

**Das Programm läuft pro Rechner nur einmal.** Ein zweiter Start öffnet keine
zweite Instanz, sondern versucht, das vorhandene Fenster nach vorn zu holen.
War es minimiert, greif auch hier lieber zur Taskleiste.

## Updates

BTS Light prüft **beim Start** auf neue Versionen; über **Wartung → Nach Update
prüfen** geht es auch von Hand. Ohne Internet passiert schlicht nichts — das
ist kein Fehler.

Gibt es eine neue Fassung, erscheint oben ein Banner. Sie wird **im Hintergrund
geladen**, während du weiterarbeitest, und du hast zwei Möglichkeiten:

| Wahl | Wann sinnvoll |
|---|---|
| **Jetzt neu starten** | Zwischen zwei Runden. Der Neustart unterbricht etwa **20 Sekunden**; die Tablets zählen weiter, und die Übertragung setzt von selbst wieder ein. |
| **Beim Beenden einbauen** | Mitten im Betrieb. Die neue Fassung wird still eingebaut, wenn du das Programm ohnehin schließt. |

Damit du die Entscheidung nicht blind triffst, sagt das Banner, was gerade auf
dem Spiel steht: **wie viele Felder belegt sind**, wenn die Übertragung läuft —
und sonst, dass gerade kein Spiel läuft beziehungsweise dass die Übertragung
ohnehin aus ist.

Hast du „Beim Beenden einbauen" gewählt und es dir anders überlegt, nimmt
**„Doch nicht beim Beenden"** die Vormerkung zurück.

## Wo deine Daten liegen

| Was | Wo |
|---|---|
| Einstellungen | `%APPDATA%\de.badhub.btslight\config.json` |
| Diagnose-Logs | eigenes Log-Verzeichnis, erreichbar über **Wartung → Logs öffnen** |
| Werbebilder, Zettel, Punktverlauf | im Datenverzeichnis der App |

Die Logs liegen bewusst **nicht** im Programmverzeichnis — dort würde ein
Update sie überschreiben.

## Wenn etwas nicht geht

**„Zielcomputer verweigerte die Verbindung".**
Zwei Ursachen sind möglich: Das Tournament Planner Network ist nicht
eingeschaltet (Schritt 1), oder es ist ein Liga-Wettbewerb und der Port müsste
`9911` statt `9901` lauten.

**Der Verbindungstest zeigt ein anderes Turnier.**
Öffne in BTP das richtige Turnier und teste erneut.

> **Achtung, hier prüft nichts mit:** Der Test sagt dir, mit welchem
> BTP-Turnier du verbunden bist — aber **nicht**, ob die eingetragene
> turnier.de-Kennung zu diesem Turnier gehört. Wechselst du in BTP das
> Turnier, ohne die Kennung anzupassen, läuft die Übertragung munter in das
> falsche Turnier auf badhub.de. Beides muss zusammenpassen.

**„Speichern & Liveticker starten" ist grau.**
Es fehlt eine Pflichtangabe: BTP-Adresse, Turnier-Kennung oder ein
Verbindungsweg für die Tablets. Die
[Einstellungs-Referenz](einstellungen.md#zwei-dinge-vorweg) zählt alle Fälle auf.

**Die Tablets finden den PC nicht.**
Zuerst prüfen, ob sie im selben WLAN sind. Dann die Firewall: Wurden die beiden
Abfragen bei der Installation abgelehnt, fehlen die Regeln für TCP 8088
beziehungsweise 443/8443. Als Ausweg funktioniert der **Cloud-Weg**, der nur
ausgehende Verbindungen braucht — siehe [Cloud-Verbindung](cloud-relay.md).

**Es hakt, und du willst Hilfe holen.**
Schalte unter [Diagnose-Logs](logging.md) den Versand ein oder öffne über
**Wartung → Logs öffnen** das Log-Verzeichnis.

## Weiterlesen

- [Einstellungen — was jede Option bewirkt](einstellungen.md)
- [Master und Slave](master-slave.md) — wenn in mehreren Hallen gespielt wird
- [Was ist BTS Light?](../README.md)
