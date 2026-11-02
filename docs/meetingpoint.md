# Meetingpoint: Einchecken und „spielbereit"

Der Meetingpoint ist ein fester Ort je Halle, an dem sich die Beteiligten
einer zum Meetingpoint gerufenen Partie melden. Dort steht ein Tablet oder ein
Touch-Bildschirm mit der **Meetingpoint-Seite**. Jeder Spieler (und ggf. der
Schiedsrichter) tippt seinen Namen an und ist damit für **diese Partie**
eingecheckt. Sind alle da, steht die Partie auf **spielbereit**. Die
Turnierleitung sieht das im TL-Web und am Turnier-PC und kann — auf Wunsch
automatisch — nur spielbereite Partien aufs Feld bringen.

Spezifikation: [features/meetingpoint-spielbereit.md](features/meetingpoint-spielbereit.md) ·
Entscheidungen: ADR [0071](adr/0071-meetingpoint-zustand-ein-store.md)–[0075](adr/0075-bedarf-der-aufruf-automatik.md).

> **Nicht zu verwechseln mit dem Hallen-Check-In.** Der
> [Hallen-Check-In](spieler-check-in.md) meldet Spieler **je Klasse** vor der
> Auslosung an. Der Meetingpoint gilt **je Partie**, kurz bevor sie aufs Feld
> geht. BTP erfährt von beidem nichts.

## Einschalten

Einstellungen → Abschnitt **Meetingpoint** → Häkchen **Meetingpoint verwenden**.
<!-- pruef: "Meetingpoint verwenden" in src/pages/SetupWizard.tsx -->
Alle Meetingpoint-Schalter sind ab Werk aus.
<!-- pruef: /enabled: false,\n            require_umpire: false,\n            strict_auto_assign: false,/ in src-tauri/src/config.rs -->

Solange das Häkchen fehlt, gibt es die Seite nicht (der Turnier-PC antwortet
mit „nicht gefunden"), und an der Feldvergabe ändert sich nichts.

## Die Meetingpoint-Seite

| Wo | Adresse |
|---|---|
| Hallennetz | `http://<Turnier-PC>:8088/meetingpoint?halle=<Hallenname>` |
| Cloud | `https://badhub.de/bts-relay/<Kennung>/meetingpoint?halle=<Hallenname>` |
<!-- pruef: /\.route\(.\/meetingpoint., get\(meetingpoint_page\)\)/ in src-tauri/src/tablet/server.rs -->
<!-- pruef: /\.route\(.\/\{ns\}\/meetingpoint., get\(meetingpoint_page\)\)/ in relay/src/main.rs -->

**Wo finde ich die Adresse?** Am Turnier-PC unter **Tablets → QR-Codes**:
Bei eingeschaltetem Meetingpoint steht dort oben der Block
**Meetingpoint-Seite** mit je Halle und Verbindungsweg einem QR-Code (zum
Scannen am Meetingpoint-Gerät), der Adresse als Link und einem
Kopier-Knopf. Auf den QR-Code tippen zeigt ihn groß.
<!-- pruef: "Meetingpoint-Seite" in src/pages/TabletPanel.tsx -->

- **Ein Meetingpoint je Halle.** Im Mehr-Hallen-Turnier gehört der
  Hallenname in die Adresse. Fehlt er oder ist er unbekannt, zeigt die Seite
  zuerst eine Hallenwahl. Im Ein-Hallen-Turnier genügt `/meetingpoint`.
- Die Seite zeigt alle Partien, die **in diese Halle** zum Meetingpoint
  gerufen sind, die älteste oben, mit der Uhrzeit des Aufrufs.
- **Namen antippen = einchecken**, noch einmal antippen = auschecken (falls
  sich jemand vertippt hat). Eingecheckte Namen werden grün mit Haken.
- Sind alle nötigen Personen da, trägt die Karte **spielbereit**.
<!-- pruef: "spielbereit" in src-tauri/assets/meetingpoint.html -->
- Die Seite zeigt nur Namen — keine Lizenznummer, keinen Verein, keine Nation.
- Am Kiosk-Tablet: In der Anzeige-Hülle (`/anzeige`) gibt es die Ansicht
  **Meetingpoint**; die gewählte Halle merkt sich die Hülle. Ein Tipp auf die
  Seite schaltet dort die Ansicht **nicht** weiter — er checkt ein.
<!-- pruef: /meetingpoint: .Meetingpoint./ in src-tauri/assets/anzeige.html -->
- Ferne Halle mit Slave-PC: Die Adresse am Slave (`:8088/meetingpoint`)
  funktioniert ebenso; der Slave reicht alles an den Turnier-PC weiter.

**Wichtig:** Am Meetingpoint erscheinen nur Partien, die in bts-light
gerufen wurden (Spielliste, TL-Web oder Aufruf-Automatik). Wer nur in BTP
gelb markiert oder eine Halle setzt, ruft nicht zum Meetingpoint.

Der Liveticker-Bildschirm `display=next` von badhub zeigt dagegen die
nächsten Spiele (auch noch nicht gerufene) — er ist eine Anzeige, die
Meetingpoint-Seite ist zum Einchecken.

## Wann ist eine Partie spielbereit?

- **Alle Spieler** der Partie sind eingecheckt.
- Ist **Schiedsrichter muss für „spielbereit" eingecheckt sein** an (ab Werk
  aus), muss auch der eingeteilte Schiedsrichter eingecheckt sein — außer die
  Turnierleitung hat die Partie auf **Ohne Schiedsrichter** gesetzt.
<!-- pruef: "Ohne Schiedsrichter" in src-tauri/assets/tl.html -->
- Der Zähltafelbediener checkt nie ein.

Ein Check-in verfällt, wenn
- die Turnierleitung den Aufruf **zurücknimmt**,
- die Partie in eine **andere Halle** gerufen wird (in dieselbe Halle erneut
  gerufen: bleibt alles stehen, auch die Uhr),
- die Person in einem **anderen Spiel aufs Feld** kommt,
- BTP auf diesem Platz einen **anderen Spieler** einsetzt.

Kommt die Partie aufs Feld, ruht ihr Aufruf. Wird sie wieder vom Feld
genommen, ist sie nach wenigen Sekunden wieder gerufen — mit allen
Check-ins.

Aufrufe und Check-ins überstehen einen Neustart und ein Update des
Turnier-PCs.

## Schiedsrichter schon beim Aufruf

Mit Schiedsrichtern und eingeschalteter Schiedsrichter-Rotation bekommt eine
gerufene Partie gleich beim Aufruf **vorläufig** den nächsten freien
Schiedsrichter (nicht im Einsatz, nicht pausiert). Wird ein Schiedsrichter
frei, geht er an die älteste gerufene Partie ohne Schiedsrichter. In BTP
steht er erst, wenn die Partie aufs Feld kommt; vorher kann die Turnierleitung
in BTP oder von Hand jemand anderen einteilen — das hat Vorrang. Fehlen
Schiedsrichter, setzt die Turnierleitung einzelne Partien im ⋮-Menü auf
**Ohne Schiedsrichter**.

## Sicht der Turnierleitung

**TL-Web (Spielliste):** An jeder gerufenen Partie steht `🧍 2/4` (eingecheckt
von nötig) bzw. **✓ spielbereit**. Im ⋮-Menü unter **Meetingpoint** lassen sich
Personen von Hand ein- und auschecken. Oben schaltet **✓ nur spielbereite** die
Liste auf eingecheckte Partien um (je Gerät gemerkt).
<!-- pruef: "✓ nur spielbereite" in src-tauri/assets/tl.html -->

**Turnier-PC (In Vorbereitung):** Jede gerufene Partie zeigt je Person einen
Knopf (antippen = von Hand ein-/auschecken) und **spielbereit**, sobald alle
da sind.

## Nur spielbereite Partien aufs Feld

Häkchen **Automatische Feldvergabe nur für spielbereite Partien**:
<!-- pruef: "Automatische Feldvergabe nur für spielbereite Partien" in src/pages/SetupWizard.tsx -->

- Die Automatik vergibt Felder nur noch an spielbereite Partien — im
  Mehr-Hallen-Turnier nur an Felder der Halle, in die gerufen wurde.
- Spiele, die niemand zum Meetingpoint ruft, bekommen **kein** Feld mehr.
  Am besten zusammen mit der Aufruf-Automatik (unten) einsetzen.
- Von Hand lässt sich weiterhin jedes Spiel vergeben.
- Ein Wunschfeld hält sein Feld erst frei, wenn die Partie spielbereit ist.
- Steht ein Feld länger als eine Minute frei, ohne dass eine Partie bereit
  ist, zeigt das TL-Web oben einen Hinweis.

## Automatisch zum Meetingpoint rufen

**Partien automatisch zum Meetingpoint rufen**:
<!-- pruef: "Partien automatisch zum Meetingpoint rufen" in src/pages/SetupWizard.tsx -->

| Einstellung | Wirkung |
|---|---|
| **feste Anzahl je Halle** | Je Halle stehen immer so viele Partien gerufen bereit (ab Werk 2). |
| **nach Restzeit der laufenden Spiele** | Je Halle so viele, wie laut Prognose in den nächsten *X* Minuten (ab Werk 10) Felder frei werden, plus Puffer (ab Werk 1). Ein freies Feld zählt als „jetzt frei" — zu Turnierbeginn wird also gleich gerufen. Wird in *X* Minuten kein Feld frei, wird nicht gerufen. |
| **höchstens je Halle** | Obergrenze gerufener Partien (ab Werk 4). |
<!-- pruef: /fixed_per_hall: 2,\n            horizon_minutes: 10,\n            buffer: 1,\n            cap_per_hall: 4,/ in src-tauri/src/config.rs -->

- Gerufen wird in der Reihenfolge der Spielliste und in die Halle, die für
  das Spiel feststeht. Im Mehr-Hallen-Turnier ruft die Automatik keine
  Spiele ohne Halle.
- Übersprungen werden ausgenommene Spiele und Spiele mit Spielern, die gerade
  spielen oder noch Pause haben. Niemand wird zu zwei Partien zugleich gerufen.
- Die Automatik läuft auch, wenn die Felder von Hand vergeben werden. Die
  Pause der Automatik im TL-Web hält sie an. Sie nimmt nie selbst einen
  Aufruf zurück.
- Ein automatischer Aufruf wird angesagt wie ein Aufruf von Hand (wenn ein
  Ansage-Gerät angeschlossen ist).

## Wenn jemand nicht kommt

- **Warnung, wenn eine Partie nicht vollständig erscheint** (ab Werk aus, 5
  Minuten): Nach dieser Zeit wird die Marke an der Partie orange und nennt,
  wer fehlt.
- **Fehlende automatisch zum Meetingpoint ansagen (einmal)** (ab Werk aus, 5
  Minuten): „<Name>, bitte zum Meetingpoint in <Halle>." — genau einmal je
  Partie, auch über einen Neustart hinweg. Weitere Nachrufe wie gewohnt von
  Hand.
<!-- pruef: "Fehlende automatisch zum Meetingpoint ansagen (einmal)" in src/pages/SetupWizard.tsx -->

## Anwesenheit am Zähl-Tablet

Häkchen **Anwesenheit zusätzlich am Zähl-Tablet bestätigen lassen**: Steht die
Partie auf dem Feld, zeigt das Zähl-Tablet bei der Seitenwahl die Namen zum
Antippen („Anwesenheit bestätigen"). Im TL-Web zeigt die Feldkachel dann
„✓ 3/4 am Feld". Das ist nur eine Anzeige für die Turnierleitung — gezählt werden kann jederzeit, auch ohne Bestätigung.
<!-- pruef: "Anwesenheit bestätigen" in src-tauri/assets/tablet.html -->

## Grenzen

- Die Cloud-Adresse der Seite enthält die Kennung des Turniers. Wer sie
  kennt, kann Personen ein- und auschecken. Der Turnier-PC prüft jede
  Meldung (richtige Halle, gerufene Partie, vorhandener Platz); die
  Turnierleitung kann jeden Stand korrigieren, und von Hand vergeben geht
  immer.
- **Mit „nur spielbereite Partien" heißt das auch:** Wer die Adresse kennt,
  könnte eine Partie fälschlich „bereit" melden; die Automatik legt sie dann
  aufs Feld. Die Seite deshalb nur an den Meetingpoint-Geräten öffnen, die
  Adresse nicht aushängen, und bei Auffälligkeiten im TL-Web korrigieren.
- In der Cloud nimmt der Relay höchstens zehn Meldungen je Sekunde an;
  darüber meldet die Seite „Zu viele Meldungen – bitte kurz warten".
- Die Seite fragt alle zwei Sekunden nach; ein Check-in erscheint im TL-Web
  nach wenigen Sekunden.
