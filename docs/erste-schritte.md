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
Änderungen stehen auf der [Download-Seite](../).

Beim Start des Installers meldet sich **Windows SmartScreen** mit einem blauen
Fenster und der Warnung vor einem unbekannten Herausgeber. Das liegt daran,
dass der Installer noch kein Code-Signing-Zertifikat trägt — es ist kein
Anzeichen für ein Problem mit der Datei. Über **„Weitere Informationen" →
„Trotzdem ausführen"** geht es weiter.

**Während der Installation fragt Windows zweimal nach Administratorrechten.**
Dabei geht es um die **Firewall**: BTS Light trägt zwei Regeln ein, damit die
Tablets und Anzeigen den PC im Hallen-WLAN erreichen können.

- Bestätigst du beide, ist die Firewall fertig eingerichtet.
- Lehnst du ab, läuft die Installation trotzdem durch — die Tablets erreichen
  den PC dann aber möglicherweise nicht. Der Cloud-Weg funktioniert auch ohne
  diese Regeln, weil er nur nach außen verbindet.

Bei der Deinstallation werden die beiden Regeln wieder entfernt.

## Schritt 3 — Der erste Start

Beim ersten Start öffnet sich **„BTS Light einrichten"** — derselbe Bildschirm,
den du später als **Einstellungen** wiederfindest. Drei Angaben sind Pflicht,
alles andere kannst du zunächst überspringen:

1. **Liveticker-Ziel** — deinen Verband anklicken.
2. **Turnier bei turnier.de** — die Adresse deines Turniers hineinkopieren.
3. **BTP-Verbindung** — steht BTP auf demselben Rechner, passt die
   Voreinstellung `127.0.0.1` mit Port `9901` bereits. Trag das
   BTP-Passwort aus Schritt 1 ein, falls es eines gibt.

Was jede weitere Option bewirkt, steht in der
[Einstellungs-Referenz](einstellungen.md).

## Schritt 4 — Verbindung prüfen

Drück im Abschnitt „BTP-Verbindung" auf **Verbindung testen**.

- **Erfolg:** Es erscheint der **Name deines Turniers**. Damit ist bewiesen,
  dass die Verbindung steht und du am richtigen Turnier hängst.
- **Fehlschlag:** Die Meldung nennt den Grund. Bei „Zielcomputer verweigerte
  die Verbindung" geh zurück zu Schritt 1.

Der Test **speichert nichts** — er prüft nur.

## Schritt 5 — Starten

Unten auf **„Speichern & Liveticker starten"**. Der Knopf bleibt gesperrt,
solange etwas Wichtiges fehlt; siehe
[Einstellungen](einstellungen.md#zwei-dinge-vorweg).

Danach landest du im **Status-Fenster** und die Übertragung läuft.

## Schritt 6 — Geräte anbinden

Erst jetzt lohnen sich die Geräte in der Halle:

- **Zähl-Tablets** — QR-Code des Feldes scannen, fertig.
  Siehe [Das Zähltablett bedienen](tablet-bedienen.md).
- **Fernseher am Feld** — mit einem Raspberry Pi.
  Siehe [Court-Monitor am Raspberry Pi einrichten](pi-setup.md).
  Das fertige Speicherkarten-Abbild liegt auf der [Download-Seite](../).
- **Turnierleitung auf dem Tablet** — siehe
  [Turnierleitungs-Oberfläche im Browser](turnierleitung-web.md).
- **Aushang für die Halle** — siehe [Aushang für die Halle](aushang.md).

## Beenden — und was stattdessen gemeint ist

> **Das Schließen-Kreuz beendet das Programm wirklich.** Läuft der Liveticker,
> fragt BTS Light vorher nach („Der Liveticker läuft noch – … Trotzdem
> beenden?"). Wer das bestätigt, beendet die Übertragung für das ganze Turnier.

Willst du das Fenster nur loswerden, **minimiere** es. BTS Light läuft dann im
Hintergrund weiter und ist über das Symbol in der Taskleiste wieder da — ein
Doppelklick holt es zurück.

**Das Programm läuft pro Rechner nur einmal.** Startest du es ein zweites Mal,
kommt keine zweite Instanz, sondern das bestehende Fenster nach vorn.

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

Damit du die Entscheidung nicht blind triffst, nennt das Banner, **wie viele
Felder gerade belegt sind**.

## Wo deine Daten liegen

| Was | Wo |
|---|---|
| Einstellungen | `%APPDATA%\de.badhub.btslight\config.json` |
| Diagnose-Logs | eigenes Log-Verzeichnis, erreichbar über **Wartung → Logs öffnen** |
| Werbebilder, Zettel, Punktverlauf | im Datenverzeichnis der App |

Die Logs liegen bewusst **nicht** im Programmverzeichnis: Dort dürfte ein
normales Benutzerkonto nicht schreiben, und ein Update würde sie überschreiben.

## Wenn etwas nicht geht

**„Zielcomputer verweigerte die Verbindung" (Port 9901).**
Das Tournament Planner Network ist nicht eingeschaltet — Schritt 1.

**Der Verbindungstest zeigt ein anderes Turnier.**
BTP liefert immer das gerade geöffnete Turnier. Öffne in BTP das richtige.

**„Speichern & Liveticker starten" ist grau.**
Es fehlt eine Pflichtangabe: BTP-Adresse, Turnier-Kennung oder ein
Verbindungsweg für die Tablets. Die
[Einstellungs-Referenz](einstellungen.md#zwei-dinge-vorweg) zählt alle Fälle auf.

**Die Tablets finden den PC nicht.**
Zuerst prüfen, ob sie im selben WLAN sind. Dann die Firewall: Wurden die beiden
Abfragen bei der Installation abgelehnt, fehlen die Regeln. Als Ausweg
funktioniert der **Cloud-Weg**, der nur ausgehende Verbindungen braucht — siehe
[Cloud-Verbindung](cloud-relay.md).

**Es hakt, und du willst Hilfe holen.**
Schalte unter [Diagnose-Logs](logging.md) den Versand ein oder öffne über
**Wartung → Logs öffnen** das Log-Verzeichnis.

## Weiterlesen

- [Einstellungen — was jede Option bewirkt](einstellungen.md)
- [Master und Slave](master-slave.md) — wenn in mehreren Hallen gespielt wird
- [Was ist BTS Light?](../README.md)
