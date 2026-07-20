# ADR 0006 — Master-Identität per Export/Import auf einen neuen PC umziehen

Status: akzeptiert (2026-07-20)

## Kontext

Die `install_id` ist eine einmalig zufällig erzeugte UUID und laut Architektur
(R6) **gleichzeitig** der Relay-Namespace, der Bearer-Token für den Cloud-Zugriff
und die Zuordnung der Diagnose-Log-Uploads. Alle gekoppelten Geräte hängen an
genau dieser ID:

- **Tablets** im Cloud-Modus verbinden sich zu `…/<install_id>/…`.
- **Court-Monitore** (Pi/TV) pollen den Namespace der `install_id`.
- **Cloud-Ansage-Slaves** speichern die `install_id` als `master_namespace`
  (auch der Telefon-Kopplungscode löst nur auf diese ID auf, ADR 0004).

Wird der Turnier-PC getauscht (Defekt, anderes Gerät, Neuinstallation),
erzeugt das Frontend eine **neue** `install_id` → sämtliche gekoppelten Geräte
pollen weiter den alten Namespace und verlieren **stumm** die Verbindung. Das
war laut Turnier-Nacharbeiten die **Hauptursache des Turniertag-Chaos**: es gibt
heute keinen Weg, die Identität mitzunehmen, und keine Warnung, dass bekannte
Geräte offline gegangen sind.

Ohne Entscheidung bleibt jeder PC-Wechsel ein manuelles Neu-Koppeln aller
Geräte (QR neu scannen, Slaves neu einlösen, Monitore neu zuweisen) mitten im
laufenden Turnier.

## Entscheidung

Wir führen einen **geführten Identitäts-Umzug per Export/Import** ein:

1. **Exportieren** (alter/aktueller PC): erzeugt ein Umzugs-Bündel (Datei
   `bts-light-identitaet.json`) mit der `install_id` **und** den
   koppl­ungsrelevanten Einstellungen (Verbindungsmodus, Hallen-/Feld-Zuordnung,
   Ansage-/Monitor-Konfiguration — NICHT das BTP-Passwort). Bewusst getrennt vom
   normalen Config-Speicher, damit der Umzug ein expliziter Schritt bleibt.
2. **Importieren** (neuer PC): übernimmt die `install_id` aus dem Bündel als
   eigene Identität. Der Relay-Namespace + alle bestehenden Geräte-Kopplungen
   funktionieren danach **unverändert weiter** — kein Neu-Scannen.
3. **Offline-Warnung**: der Master zeigt bekannte, aber gerade nicht mehr
   erreichbare Geräte (Slaves über `cloud_slaves`, Monitore über die
   Monitor-Präsenz), damit ein Wegbrechen nach einem Wechsel sichtbar wird statt
   still zu passieren.

Die `install_id` bleibt für den Normalbetrieb weiterhin eine einmalig erzeugte,
danach unveränderte UUID — der Import ist der **einzige** Weg, sie zu ändern.

## Alternativen

- **`install_id` unveränderlich lassen, Geräte manuell neu koppeln:** verworfen —
  genau das ist die heutige Chaos-Ursache (viele Geräte, manuelles Neu-Scannen im
  laufenden Turnier).
- **Server-Konto / Login (PC-unabhängige Identität):** verworfen — widerspricht
  dem Plug-and-play-Grundsatz „App ohne Server-Konto" (Begründung der bewusst
  eingebetteten Secrets in CLAUDE.md); bräuchte Auth-Infrastruktur.
- **Identität cloud-seitig je Turnier hinterlegen:** verworfen — setzt ebenfalls
  ein Konto/Authentifizierung voraus und verlagert das Bearer-Token-Risiko auf
  den Server.

## Konsequenzen

- Ein PC-Wechsel bricht die gekoppelten Geräte nicht mehr still ab; die Identität
  zieht in einem Export/Import mit. Die Offline-Warnung macht ein Wegbrechen
  sichtbar.
- **Das Umzugs-Bündel enthält die `install_id` = Bearer-Token.** Wer die Datei
  hat, kann als dieser Namespace auftreten. Sie ist daher **wie ein Passwort** zu
  behandeln (nicht teilen, nicht hochladen) — im UI-Text und in der Doku klar
  benennen. Neue Exposition entsteht nur durch die Datei selbst; das Token war
  ohnehin je Installation vorhanden.
- **Zwei PCs mit derselben Identität = zwei Hosts auf einem Namespace** und
  verletzt R4 (genau ein Host je Namespace). Die Zombie-Host-Ablösung im Relay
  (Cluster A) entschärft den *sequentiellen* Wechsel (der neue Host löst den alten
  ab), aber **gleichzeitig** aktive Master sind undefiniert. Der Import muss daher
  warnen: nur **ein** Master zur selben Zeit; der alte PC ist danach zu beenden.
- Der Import überschreibt die lokale Identität — eine versehentliche
  Doppelnutzung (Import auf einem noch aktiven zweiten Turnier-PC) ist möglich.
  Mitigation: deutliche Bestätigungs-Abfrage beim Import.
- **Neu bewerten**, falls je ein Server-Konto-Modell eingeführt wird — dann würde
  eine kontogebundene Identität diesen Datei-Umzug ablösen (neues ADR).
