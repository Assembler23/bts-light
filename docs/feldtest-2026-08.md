# Feldtest v0.9.268 — was am nächsten Turnier zu prüfen ist

> Stand 24.08.2026. Deckt alles ab, was seit **v0.9.258** dazugekommen ist und
> noch nie an einem echten Turnier lief. Ergebnisse bitte unten eintragen —
> die Datei ist auch das Protokoll.

## Bevor es losgeht

1. **App auf v0.9.268 bringen.** Sie ist seit 24.08. veröffentlicht und
   `Latest`; das Auto-Update zieht sie beim Start. Kontrolle: Titelleiste oder
   Einstellungen. **Ohne dieses Update ist die Hälfte der Punkte unten gar
   nicht sichtbar** — die Turnierleitungs-Oberfläche zeigt neue Bedienelemente
   nur, wenn der Turnier-PC sie kennt.
2. **`install_id` notieren.** Sie ist der Schlüssel zu den hochgeladenen
   Diagnose-Logs — auf dem Server liegt je Installation genau eine Datei, nach
   ihr benannt. Ohne sie findet man hinterher nichts wieder.

   **Wo sie steht:** Setup-Assistent → Abschnitt für die Kopplung ferner
   Hallen, dort als **„Kopplungs-Code"** (der lange, nicht der achtstellige
   Telefon-Code), mit Kopieren-Knopf daneben. In der Oberfläche heißt sie
   nirgends `install_id` — es ist derselbe Wert (R6: Kennung, Relay-Namespace
   und Log-Zuordnung sind eins). Auf einem **Slave** wird der Abschnitt nicht
   angezeigt.

   Ohne App: Feld `install_id` in
   `%APPDATA%\de.badhub.btslight\config.json` — nur lesen, und Änderungen
   nur bei beendeter App.

   Sie gilt **pro Installation**, nicht pro Turnier: einmal notiert, gilt sie
   für alle weiteren Turniere auf diesem Rechner.
3. **Nicht** die `config.json` bei laufender App von Hand ändern — die
   laufende App überschreibt externe Änderungen mit ihrem Speicherstand.

**Wichtig zur Bewertung:** Die Punkte A1–A4 haben eine gemeinsame Eigenschaft
— sie sind erst dann bestanden, wenn sie ein **Speichern der Einstellungen**
überleben. Genau dort saßen zwei der Fehler vom 22.08. (jeder Merker, der nur
in der Übertragung lebt, wird beim Speichern zurückgesetzt). Also: erst
einstellen, dann im Setup irgendetwas speichern, **dann** prüfen.

---

## A · Am Schreibtisch, vor dem ersten Spiel

Diese Punkte brauchen kein laufendes Spiel und sollten vorab durch sein.

### A1 · Feld sperren (v0.9.258)

| Schritt | Erwartung |
|---|---|
| TL-Web → ⋯ an einer **freien** Feldkachel → „🔒 Feld sperren" | Feld wird als gesperrt markiert; die Automatik legt nichts mehr darauf |
| Dasselbe an einer **belegten** Kachel | Der Text sagt „Feld **nach diesem Spiel** sperren"; das laufende Spiel zählt ungestört zu Ende |
| ⋯ → „🔓 Feld freigeben" | **Rückfrage** erscheint (bewusst nur in dieser Richtung — ein zweites Gerät sieht nur „gesperrt", nicht warum) |
| Sperren, dann in den Einstellungen **irgendetwas speichern** | Die Sperre ist **noch da**. Vorher ging sie dabei verloren — das ist der eigentliche Fix |

**Mehr-Hallen zusätzlich:** Sperrt man das *letzte offene* Feld einer Halle,
verlieren die dorthin vorverteilten Spiele ihre Hallenbindung und bekommen
wieder Felder in der anderen Halle. Ohne das stünde die Halle still, ohne dass
jemand den Grund sähe.

### A2 · Wunschfeld (v0.9.262)

| Schritt | Erwartung |
|---|---|
| ⋯ an einem wartenden Spiel → Feld wählen | Marke **⌖ Feldname** erscheint an dem Spiel |
| Das Spiel ist **spielbereit** und das Wunschfeld wird frei | Es bekommt genau dieses Feld — kein anderes Spiel darf dazwischen |
| Das Spiel ist **noch nicht** spielbereit (Spieler stecken in der Pflichtpause) und das Feld wird frei | Das Feld geht an ein **anderes** Spiel |

Der dritte Fall ist der wichtige. Er sieht wie ein Fehler aus, ist aber die
bewusste Entscheidung (ADR 0046): Ein Finale, dessen Spieler noch im
Halbfinale stehen, darf das Hauptfeld nicht eine Dreiviertelstunde leer halten.

Ein Wunschfeld auf ein **gesperrtes** Feld muss der Turnier-PC ablehnen.

### A3 · Schiedsrichter-Ansagen (v0.9.263–265)

Am Tablet: **⚙ → PIN (Standard `0000`) → „Schiri-Modus: an"**, dann ein Spiel
durchspielen und **mitlesen**. Das ist reines Vorlesen — dafür braucht es kein
Turnier, nur ein Tablet und zehn Minuten.

- Bei **15er-Zählweise**: Die Pause heißt **nicht** mehr „Pause bei 11 Punkten".
- Der dritte Satz heißt **„Entscheidungssatz"**, nicht „Dritter Satz".
- Eröffnung: „… und zu meiner **Linken** …".
- Nach **jeder** Pause kommt die Freigabe („Spiel!"), nicht nur nach mancher.
- Nach einem Satzausgleich kommt der **Zwischenstand**.
- Disqualifikation: „… disqualifiziert wegen **grober Unsportlichkeit**".
  (Vorher stand hier „unsportlichen Verhaltens" — falsch; die offizielle
  DBV-Fassung lautet anders. Das ist der Punkt, den ich zweimal korrigieren
  musste.)

### A4 · Pausenzeit und Wartezeit (v0.9.258 ff.)

| Schritt | Erwartung |
|---|---|
| Pausenzeit ändern | Sie gilt **nur für Spiele, die danach enden**. Bereits beendete behalten ihre alte Pause |
| Einstellungen speichern, **ohne** die Wartezeit zu ändern | Bei den Spielern passiert **nichts**. Vorher wurde die Pause dabei auf alle übertragen — das war der „Bug: Dringend" |
| Ein Spiel, dessen Spieler noch in der Pause sind, aufs Feld schieben | Geht — mit **Sicherheitsfrage**, die die Namen und das Ende der Pause nennt |
| **Kombination**: pausierendes Spiel auf ein Feld in der **falschen Halle**, für das außerdem ein **Wunschfeld** gesetzt ist | **Alle** Bedenken stehen zusammen in *einer* Rückfrage, nicht drei hintereinander |

Der Kombinationsfall ist der, den du ausdrücklich sehen wolltest. Bricht man
ab, bleibt die Auswahl bestehen — vermutlich war nur das Zielfeld daneben.

---

## B · Im laufenden Betrieb

### B1 · Warnung „Ergebnis fehlt" (v0.9.259)

**Auslösen:** Ein Spiel auf einen entschiedenen Stand bringen (zwei Sätze
gewonnen) und das Ergebnis **nicht** absenden.

Nach **einer Minute** (Vorgabe, einstellbar) muss die Turnierleitung es
sehen — als Marke an der Feldkachel **und** im Störungsband oben.

**Genauso wichtig: kein Fehlalarm.** Ein Ergebnis, das schon abgeschickt ist
und nur noch in der BTP-Warteschlange steckt, darf **nicht** gemeldet werden.
Das ist der häufigste Weg, sich so eine Warnung zu ruinieren — sie würde nach
jedem Spiel kurz aufblinken und nach einer Stunde ignoriert.

### B2 · Halle von Hand setzen (v0.9.260) — nur Mehr-Hallen

Ein Spiel hat automatisch Halle A bekommen. Von Hand **Halle B** setzen.

Die Automatik muss es danach nach B vergeben. Vorher legte die
Handeingabe das Spiel still: Die automatische Bindung an A blieb bestehen, die
Handeingabe wurde ignoriert, und das Spiel bekam gar kein Feld mehr.

### B3 · Anzeigen: Datenlast, Uhr, Aufholen (v0.9.261)

Nichts davon muss ausgelöst werden — nur beobachten:

- **Pausen-Countdown** auf einem Gerät mit **falsch gestellter Uhr**: Er muss
  trotzdem stimmen (er rechnet mit der Server-Uhr).
- **Nach einem Verbindungsbruch** (Netz kurz weg, WLAN-Wechsel): Der Monitor
  holt auf, ohne dass jemand die Seite neu lädt.

### B4 · Tablet erkennt eine veraltete Fassung (v0.9.266)

**Dieser Test läuft von selbst ab, wenn du die App aktualisierst**, während
Tablets offen sind:

- Tablet **ohne** Spiel auf dem Feld → lädt sich **von selbst** neu.
- Tablet **mit** laufendem Spiel → springt **nicht**, zeigt oben nur den
  Hinweis „Neue Fassung verfügbar" mit dem Knopf „Jetzt laden".

Der zweite Fall ist der, der schiefgehen darf und nicht darf: Mitten im Zählen
den Bildschirm springen zu lassen wäre der falsche Moment.

### B5 · Fernbefehl „⟳ Tablets" (v0.9.268)

TL-Web, Kopfzeile rechts, neben „Profile". Rückfrage bestätigen.

| Erwartung | |
|---|---|
| **Alle** Zähltablets laden neu | auch die mit laufendem Spiel |
| Der Spielstand ist danach **wieder da** | er liegt auf dem Gerät und beim Turnier-PC |
| Der Knopf ist **unsichtbar**, wenn der Turnier-PC älter als v0.9.268 ist | Feature-Erkennung — kein toter Knopf |

**Zwei Dinge, die kein Fehler sind:** Ein Tablet, dessen geladene Seite älter
als dieses Update ist, kennt den Befehl noch nicht und reagiert nicht — das
holt nur B4 ab. Und ein Gerät ohne nutzbaren Browser-Speicher (Kiosk mit
gesperrtem Speicher) springt mitten im Spiel bewusst **nicht**, sondern zeigt
den Hinweis; dort überlebte der Stand das Neuladen nicht.

---

## C · Passiv — nichts tun, hinterher nachsehen

### C1 · Stillstands-Wächter der Anzeigen (v0.9.255)

Nicht auslösbar, und das ist der Punkt. Beide Anzeige-Seiten prüfen sich alle
zehn Sekunden selbst: Kam noch eine Antwort? Wurde noch ein Stand übernommen?

Bleibt eines länger als eine Minute aus, schreiben sie es ins Log und laden es
**sofort** hoch — samt der Unterscheidung, ob gar nichts mehr ankommt (Netz,
Gerät) oder ob die Seite die Antworten verwirft.

**Der Test ist also:** Hängt ein Monitor wie am 22.08., **muss es diesmal eine
Spur geben**. Bleibt es beim „hängt öfters mal" ohne Log-Eintrag, hat der
Wächter versagt und das ist der wichtigste Befund des Tages.

### C2 · Was hinterher zu sichern ist

- Die **Diagnose-Logs** der Anzeigen und Tablets (5× auf den Verbindungspunkt
  oben rechts öffnet am Tablet das Diagnose-Fenster).
- Die `install_id` — ohne sie sind die hochgeladenen Logs nicht zuzuordnen.
- Bei jedem Vorfall: **Feldnummer, Uhrzeit, was auf dem Bildschirm stand**.
  Die Uhrzeit ist das, was die Log-Suche trägt.

---

## Ergebnisse

| Punkt | Ergebnis | Bemerkung |
|---|---|---|
| A1 Feld sperren | offen | |
| A2 Wunschfeld | offen | |
| A3 Schiri-Ansagen | offen | |
| A4 Pausen-/Wartezeit | offen | |
| B1 Warnung „Ergebnis fehlt" | offen | |
| B2 Halle von Hand | offen | |
| B3 Anzeigen | offen | |
| B4 Versionsabgleich | offen | |
| B5 Fernbefehl | offen | |
| C1 Stillstands-Wächter | offen | |
