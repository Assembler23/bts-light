# Änderungsverlauf

Pro veröffentlichter Version die wesentlichen Änderungen. Die Versionen
werden über das Auto-Update (badhub.de) ausgeliefert; Tablet-Änderungen
erreichen den Cloud-Modus zusätzlich sofort über den Relay-Redeploy.

## v0.9.19

- **Code-Review-Fixes zur Info-Monitor-Zuweisung (v0.9.18).** Zwei
  Edge-Cases aus dem Review nachgezogen:
  - `read_assignments` migriert die alte v2-Datei jetzt **persistierend**
    nach v3 und schreibt das Ergebnis sofort auf Platte – Folge-Lesungen
    finden direkt v3 statt v2 erneut zu migrieren. Eine vorhandene aber
    **kaputte** v3-Datei (z.B. abgebrochener Schreibvorgang) ergibt
    bewusst eine leere Map statt auf v2 zurückzufallen; sonst hätte
    eine ältere v2 die jüngeren Info-Monitor-Zuweisungen überschrieben.
    Regressionstest in `monitor.rs`.
  - `monitor.html` prüft `redirectTo` **vor** `handleCommand`. Andersrum
    konnte ein anstehender `reload`-/`identify`-Command auf einer Seite
    feuern, die im selben Tick auf eine Info-HTML wegnavigiert –
    daraus resultierte ein Reload statt der Navigation.
- **Pi Zero 2 W: Chromium-Low-RAM-Warnung dauerhaft aus.** `setup-monitor.sh`
  setzt jetzt das `--no-memcheck`-Flag des Pi-OS-Chromium-Wrappers im
  Kiosk-Aufruf. Damit erscheint die "Less than 1 GB of RAM"-Splash auf
  Pi Zero 2 W nicht mehr; auf Geräten ≥ 1 GB ist das Flag ein No-Op.
  Heute live mit zwei Pi-Zero-2-W-Monitoren parallel verifiziert.

## v0.9.18

- **Info-Monitor-Zuweisung direkt aus dem Tool.** Die „Court-Monitore"-
  Seite hat ein erweitertes Dropdown: neben den Feldern (in den
  Mehr-Hallen-`optgroup`s) steht jetzt eine Sektion „Informationen" mit
  „Courtübersicht" und „In Vorbereitung". Wechseln zwischen Feld- und
  Info-Zuweisung passiert ohne SD-Karten-Editieren — der Pi merkt den
  Wechsel beim nächsten `/monitor/state`-Poll und navigiert sich selbst
  auf die richtige Seite. Auch der Rückweg (Info → Feld) klappt
  automatisch: die Info-Pages prüfen alle 30 s gegen `/monitor/state`,
  ob ihre Zuweisung sich geändert hat.
- **Datenmodell `MonitorTarget`** (Court | InfoOverview | InfoPreparation)
  ersetzt die reine CourtID-Zuweisung. Die Datei
  `monitor-assignments-v2.json` wird beim ersten Start nach
  `monitor-assignments-v3.json` migriert (jede CourtID → `Court`-Target);
  manuelles Eingreifen ist nicht nötig.

## v0.9.17

- **Info-Monitore: Court-Übersicht und In Vorbereitung.** Neben dem
  feld-bezogenen Court-Monitor (ein TV je Feld) liefert bts-light jetzt
  zwei Hallen-weite Info-Displays unter eigenen URLs aus —
  offline-fähig, direkt aus dem BTP-Snapshot, ohne Umweg über badhub.de:
  - `…/info/overview` zeigt **alle Felder** mit Status (frei, läuft,
    Behandlung, TL-Ruf), Paarung und Sätzen, bei Mehr-Hallen-Turnieren
    je Halle ein Abschnitt. Ideal für den TL-Tisch oder einen zentralen
    Eingangs-TV.
  - `…/info/preparation` zeigt die **gerufenen und eingeplanten Spiele**
    als Liste mit gold-Pille „In Vorbereitung", Halle und „vor X Min."
    pro Aufruf. Ideal als Meeting-Point-TV je Halle.
  Beide unterstützen `?halle=<Name>` (Hallen-Filter) und
  `?rotate=90|180|270` (Pivot-Monitor, dreht per CSS-Transform — keine
  OS-Anpassung am Pi nötig). Details:
  [docs/court-monitor.md → Info-Monitor](court-monitor.md).
- **`setup-monitor.sh` versteht Pi OS Lite.** Auf Lite installiert das
  Skript jetzt selbst den X-Stack (Xorg + matchbox-WM + Chromium),
  setzt Console-Autologin auf tty1 und richtet `.xinitrc` +
  `.bash_profile`-Hook so ein, dass beim Boot automatisch der Chromium-
  Kiosk startet. Auf Desktop bleibt der bisherige `.config/autostart`-
  Pfad. Non-interaktive Aufrufe (cloud-init, `curl | bash`) werden
  graceful unterstützt.

## v0.9.16

- **Hallen-Ansage für Spiele in Vorbereitung.** Im „In Vorbereitung"-Tab
  gibt es je gerufenem Spiel einen „Ansage"-Knopf: bts-light spielt dann
  eine gesprochene Ansage ab — Gong → „In Vorbereitung." → Disziplin →
  Paarung → „Bitte in *Halle X*." Nutzt die bestehende
  Ansage-Pipeline (Gong + Web Speech), Sprache aus den Ansage-
  Einstellungen oder automatisch (≥ Hälfte international ⇒ Englisch).
  `PreparationCandidate` trägt jetzt Disziplin und Einzel-Spielernamen
  inkl. Nationalitäten — Voraussetzung für die Ansage und Grundlage für
  die Auto-Sprachwahl. Der Knopf ist nur sichtbar, wenn die Ansagen
  aktiviert sind. Details: [docs/preparation.md](preparation.md),
  [docs/announcements.md](announcements.md).
- **Doku-Reorganisation.** Eigene Feature-Dokus für Spiele in Vorbereitung
  (`docs/preparation.md`) und für die Mehr-Hallen-Architektur als
  Gesamterzählung (`docs/multi-hall.md`); Querverweise in der
  `CLAUDE.md`-Datei-Map.

## v0.9.15

- **Court-Monitor: entschiedenes Match klar anzeigen — kein Geister-Satz.**
  Bei einem in zwei Sätzen entschiedenen Best-of-3 zeigte der Monitor noch
  eine leere dritte Satz-Spalte (0:0) als „laufenden Satz", als käme noch
  ein Satz. Jetzt: sobald das Tablet die Entscheidung meldet, rendert der
  Monitor nur die wirklich gespielten Sätze (etwaiger 0:0-Geister-Satz am
  Ende fällt weg), hebt je Satz das Gewinner-Team hell hervor und markiert
  die Sieger-Hälfte mit grünem Akzent und einer 🏆. Bei Aufgabe stammt der
  Sieger aus dem gespiegelten Tablet-Zustand (`retiredWinner`).
- **„In Vorbereitung" als Überschrift im Tablet-Panel.** Die Liste der
  gerufenen Spiele heißt jetzt „In Vorbereitung" statt „Aufgerufen" —
  konsistent zum Tab- und Liveticker-Namen.

## v0.9.14

- **Spiele „in Vorbereitung" aufrufen.** Neuer Tab „In Vorbereitung" im
  Tablet-Spielzettel: Die Turnierleitung wählt eingeplante Spiele aus und
  ruft sie in die Vorbereitung – bei Mehr-Hallen-Turnieren je Halle. Ein
  aufgerufenes Spiel erscheint auf der Aufruf-Anzeige des Livetickers
  (`/live?display=next`) hervorgehoben mit „vor X Min aufgerufen", damit
  die Spieler rechtzeitig in die richtige Halle gehen. Der Aufruf lässt
  sich zurücknehmen; kommt das Spiel aufs Feld, verschwindet er von
  selbst. BTP kennt keinen Vorbereitungs-Zustand – bts-light verwaltet
  ihn selbst, wie die Walkover-Vorschläge.

## v0.9.13

- **LAN und Cloud gleichzeitig.** Die Verbindungsart war bisher ein
  Entweder-oder. Für Zwei-Hallen-Turniere lässt sich jetzt **beides
  zusammen** aktivieren: die Haupthalle (mit bts-light + BTP) bindet ihre
  Tablets und Monitore lokal per LAN an, eine zweite Halle übers
  Cloud-Relay (Internet) — beides für dieselbe Turnier-Instanz. Im
  Einrichtungs-Assistenten sind LAN und Cloud nun zwei einzeln
  schaltbare Kacheln. Bei Doppelbetrieb zeigt der Tablet-Spielzettel je
  Feld beide QR-Codes (LAN und Cloud), die Court-Monitore-Seite beide
  Adressen, und die Geräteliste führt die Geräte beider Hallen zusammen.
  Reine LAN- oder reine Cloud-Turniere verhalten sich unverändert;
  bestehende Konfigurationen laden weiter.

## v0.9.12

- **Spielzettel: Zurück-Button im Setup war riesig.** Der „← Zurück ·
  Back"-Button im Aufstellungs-Assistenten füllte durch eine geerbte
  Flex-Regel die ganze Höhe des Fensters. Jetzt eine normal große
  Schaltfläche.

## v0.9.11

- **Court-Monitor: Spielernamen aus BTP exakt getrennt.** Der Monitor
  bezieht Vor- und Nachnamen jetzt direkt aus BTP, statt den Nachnamen am
  letzten Wort zu raten. Die Broadcast-Anzeige (Vorname klein, Nachname
  groß) stimmt damit auch bei mehrteiligen Nachnamen wie „van der Berg".

## v0.9.10

- **Installer legt die Firewall-Regel automatisch an.** Bei einer
  Neuinstallation richtet das Setup die eingehende Windows-Firewall-Regel
  für den Tablet-Server (Port 8088) selbst ein — die „Zugriff zulassen?"-
  Abfrage beim ersten Start entfällt. Es kommt einmalig eine
  Windows-Sicherheitsabfrage während der Installation. Greift nur bei der
  **interaktiven Installation**, nicht beim stillen Auto-Update — eine
  bestehende Installation bekommt die Regel also erst, wenn der Installer
  einmal von Hand ausgeführt wird.

## v0.9.9

- **Schließen beendet bts-light wirklich.** Das Fenster-Schließen-Kreuz
  beendet die App jetzt sauber, statt sie unsichtbar im Hintergrund
  weiterlaufen zu lassen — kein hängender Prozess mehr im Task-Manager.
  Läuft gerade ein Liveticker, fragt bts-light vorher zur Sicherheit
  nach. Für Hintergrundbetrieb das Fenster wie gewohnt minimieren.

## v0.9.8

- **Liveticker: Halle pro Feld im Push.** Der Liveticker-Push (`tset`)
  überträgt jetzt zu jedem Feld seine Halle — Grundlage für den nach
  Hallen getrennten Liveticker-Monitor auf badhub.de
  (`/live?display=monitor`). Noch keine sichtbare Änderung; die
  badhub-Seite folgt.

## v0.9.7

- **Mehr-Hallen-Unterstützung: Hallen sichtbar (Schritt 4–5/7).** Bei
  Turnieren in mehreren Hallen zeigt der Court-Monitor jetzt „Halle 2 ·
  Feld 6" statt nur des Feldnamens, das Tablet trägt dieselbe Bezeichnung.
  Die Felder-Übersicht, die QR-Code-Liste und die Geräte-Zuweisung im
  Dashboard sind nach Halle gruppiert. Ein-Hallen-Turniere bleiben
  unverändert — kein Hallen-Präfix, keine Gruppierung.

## v0.9.6

- **Mehr-Hallen-Unterstützung: Felder eindeutig per BTP-ID (Schritt 2–3/7).**
  bts-light unterscheidet Spielfelder jetzt über ihre stabile BTP-interne
  ID statt über den Feldnamen — durchgängig in Tablet-Server, Relay und
  Oberfläche. Damit verschmelzen bei Mehr-Hallen-Turnieren „Halle 1 ·
  Feld 1" und „Halle 2 · Feld 1" nicht mehr; alle Felder funktionieren
  unabhängig. Ein-Hallen-Turniere verhalten sich unverändert.
- **Einmalig nach diesem Update:** Die Court-Monitor-Geräte müssen ihren
  Feldern einmal neu zugewiesen werden (die alte Zuordnung hing am
  Feldnamen). Die Geräte erscheinen automatisch wieder in der Geräteliste.
  Tablets, die während des Updates geöffnet bleiben, einmal neu laden.

## v0.9.5

- **Tablet-Spielzettel: zwei Tabs.** Die Seite ist jetzt in „Übersicht"
  (Live-Stand aller Felder mit Tablet-Verbindung und Akku) und „QR-Codes"
  (Adressen zum Einrichten der Tablets) getrennt — übersichtlicher,
  gerade bei vielen Feldern.

## v0.9.4

- **Vorbereitung Mehr-Hallen-Unterstützung (Schritt 1/7).** bts-light liest
  jetzt die Standorte (Hallen) und die Feld-IDs aus BTP aus — Grundlage
  dafür, dass Turniere in mehreren Hallen künftig automatisch nach Halle
  getrennt angezeigt werden. Noch keine sichtbare Änderung; der Fahrplan
  steht in [roadmap.md](roadmap.md).
- **Diagnose-Log: Turnier-Topologie.** Das Log nennt bei jeder Änderung
  „N Hallen, M Felder, K Matches" — hilft bei Einrichtung und Fehlersuche.

## v0.9.3

- **Court-Monitor: Spielernamen im Broadcast-Stil.** Namen erscheinen
  jetzt zweizeilig — Vorname klein darüber, Nachname groß darunter, wie in
  Sport-Übertragungen. Lange Doppel-Namen bleiben dadurch aus der Distanz
  gut lesbar; die frühere Initialen-Kürzung entfällt. Details:
  [court-monitor.md](court-monitor.md).

## v0.9.2

- **Spielzettel: Zurück-Schritt im Match-Setup.** Der Aufstellungs-
  Assistent (Seitenwahl → Aufschlag → Annahme) hat ab Schritt 2 einen
  „← Zurück · Back"-Button. Eine falsch getippte Wahl lässt sich so
  korrigieren, ohne das Match neu zuweisen zu müssen.
- **Spielzettel: zweisprachige Beschriftung (DE/EN).** Titel und Hinweise
  des Setup-Assistenten erscheinen jetzt Deutsch und Englisch – für die
  wachsende Zahl internationaler Spieler:innen.
- Details: [tablet.md](tablet.md).

## v0.9.1

- **Court-Monitor: Spieldauer in der Kopfzeile.** Neben der Feldnummer
  zeigt der Monitor optional die laufende Spieldauer (Minuten, mit
  Stoppuhr-Symbol). Im Setup ein-/abschaltbar; sichtbar, sobald ein
  Tablet das Feld zählt.
- **Court-Monitor: Werbung im Leerlauf abschaltbar.** Neue Option
  „Werbung im Leerlauf anzeigen". Aus → ein freies Feld zeigt eine
  neutrale Leerlauf-Seite statt der Werbebilder.
- **Court-Monitor: lange Namen werden automatisch gekürzt.** Läuft ein
  Name über seine Spalte (häufig bei Doppeln mit langen internationalen
  Namen), kürzt der Monitor die Vornamen auf Initialen
  („Ajay Kumar Mandapati" → „A. K. Mandapati"); der Nachname bleibt voll.
- **Court-Monitor: Layout-Auswahl vorbereitet.** Das Anzeige-Layout ist
  jetzt im Setup wählbar (aktuell „A — Geteilt"); Grundlage für weitere
  Layouts. Abgeschlossene Sätze werden etwas größer dargestellt.
- Details: [court-monitor.md](court-monitor.md).

## v0.9.0

- **Court-Monitor: fester Name `bts-light.local` (mDNS).** Der Turnier-PC
  meldet sich im LAN-Modus unter dem festen Namen `bts-light.local` im
  Netz. Tablets und Court-Monitore erreichen ihn darüber, **ohne seine
  IP-Adresse zu kennen** – es braucht keine feste IP mehr, weder im
  Router noch am Laptop. Die Monitor-Adresse
  `http://bts-light.local:8088/monitor` ist damit in jedem Turnier-WLAN
  dieselbe – die Grundlage für ein Master-Image, das ohne Anpassung auf
  jedem Pi läuft. Details: [court-monitor.md](court-monitor.md).

## v0.8.2

- **Court-Monitor: Satzstand bleibt bei kurzem Tablet-Aussetzer stehen.**
  Schloss man am zählenden Tablet kurz den Browser, sprang der Monitor
  auf 0:0 und zeigte den Stand erst beim Wiederverbinden erneut. Ursache:
  ein erneutes Zuweisen desselben Matches (Tablet-Reconnect) setzte den
  gemerkten Satzstand zurück. Relay und LAN-Server halten jetzt den
  zuletzt bekannten Stand – zurückgesetzt wird nur bei echtem
  Match-Wechsel.
- Cloud-Monitor-Adresse korrigiert (`/bts-relay`-Pfad fehlte), Werbe-
  Upload-Limit am Server angehoben – beides bereits am Relay/Server
  ausgerollt.

## v0.8.1

- **Court-Monitor: stabile Geräte-ID per Pi-Seriennummer.** Der Pi-Kiosk
  übergibt jetzt die Hardware-Seriennummer als Geräte-ID. Damit lässt
  sich eine fertig eingerichtete SD-Karte beliebig auf weitere Pis
  klonen, ohne dass sich Geräte eine ID teilen – die Grundlage für ein
  „Master-Image" zur einfachen Verteilung. Anleitung:
  [pi-setup.md](pi-setup.md).

## v0.8.0

- **TV-Verwaltung für die Court-Monitore.** Monitore sind jetzt generische
  Geräte: Alle Raspberry Pis bekommen *dieselbe* Adresse (`…/monitor`) und
  zeigen beim Start einen Kopplungs-Code. Auf der neuen Seite
  **„Court-Monitore"** im Tool weist die Turnierleitung jedem Gerät ein
  Feld zu (jederzeit umstellbar), sieht den Online-Status und löst per
  Fernbefehl **„Identifizieren"** (Code groß einblenden) und **„Neu laden"**
  aus – in LAN und Cloud. Die feste Adresse `…/court/<Feld>/display`
  bleibt als Direkt-Variante erhalten. Details:
  [court-monitor.md](court-monitor.md).
- **Live-Vorschau der Anzeige-Optionen** im Court-Monitor-Setup –
  Disziplin/Runde/Spielnummer/Pausen-Timer wirken sofort sichtbar.
- Über-Dialog: Mitwirkende korrigiert (Tim Lehr; Philipp Hagemeister als
  „Visionär einer digitalen Turnierausrichtung").

## v0.7.0

- **Court-Monitor – TV-Anzeige am Spielfeld**: Pro Feld eine read-only
  Anzeige (Raspberry Pi, 32"–55"), die zwischen zwei Zuständen umschaltet:
  Werbung im Leerlauf, Match-Ansicht sobald ein Spiel aufs Feld kommt. Die
  Match-Ansicht („A — Geteilt") zeigt Spielernamen mit Landesflaggen, den
  Satzstand, die aufschlagende Mannschaft (eingefärbt) und einen
  Retro-Pausen-Countdown im Klappanzeigen-Stil. Werbebilder werden im Tool
  hochgeladen (ein gemeinsamer Satz für alle Felder); Wechsel-Intervall und
  Anzeige-Optionen sind einstellbar. Funktioniert im LAN- und im
  Cloud-Modus. Details: [court-monitor.md](court-monitor.md).

## v0.6.0

- **Sprachansagen für Feld-Aufrufe**: Wird in BTP ein Spiel auf ein Feld
  gezogen, sagt bts-light es über die PC-Lautsprecher an – Gong, Feld,
  Disziplin (Herren-/Dameneinzel, Herren-/Damendoppel, Mixed) und die
  Paarung. Deutsch, Englisch oder automatisch (Englisch, wenn mindestens
  die Hälfte der Spieler international ist); Stimmen und Tempo einstellbar.
  Details: [announcements.md](announcements.md).

## v0.5.0

- **Kampflose Wertung nach Aufgabe**: Gibt eine Mannschaft während eines
  Spiels auf und hat in derselben Disziplin noch weitere, ungespielte
  Spiele, blendet bts-light ein Fenster ein und schlägt vor, diese
  kampflos (Walkover) für den jeweiligen Gegner zu werten. Die
  Turnierleitung wählt die betroffenen Spiele aus und bestätigt – erst
  dann gehen sie mit `ScoreStatus = 1` nach BTP. Maßgeblich ist nur die
  Disziplin der Aufgabe; spielt ein Doppelpartner in einer anderen
  Disziplin mit anderem Partner, bleibt das unberührt.
- **Heartbeat**: bts-light meldet sich auch im Leerlauf alle 60 s beim
  Liveticker. So erkennt badhub.de ein laufendes Turnier zuverlässig als
  „live" – und kennzeichnet es als beendet, sobald bts-light geschlossen
  wird (kein Heartbeat mehr).
- **Versionsanzeige & Mitwirkende**: Fußzeile mit der installierten
  Version und ein „Über"-Dialog, der die Pioniere der BTS-Community
  würdigt – Philipp Hagemeister (Idee & Begründung), Tobias Lehr, letilo.

## v0.4.6

- **Kopier-Button** für die Tablet-Adressen in der Tablet-Spielzettel-
  Seite – die URL lässt sich jetzt in die Zwischenablage kopieren.
- Dieses Changelog angelegt.

## v0.4.5

- **Tablet-Übernahme mit laufendem Spielstand**: Das aktive Tablet
  spiegelt seinen Spielzustand laufend an den Server. Übernimmt ein
  anderes Gerät den Court, setzt es das laufende Spiel mit aktuellem
  Stand fort – statt bei 0:0 zu beginnen.
- Sieger-Wahl bei Aufgabe als große Buttons (vorher zu kleiner Text).

## v0.4.4

- **Spiel abbrechen / Aufgabe**: In der Behandlungspause beendet
  „Spiel abbrechen" das Match per Aufgabe – Teilstand wird übernommen,
  der Sieger manuell gewählt, das Ergebnis geht mit Status „retired"
  (`ScoreStatus = 2`) nach BTP.

## v0.4.3

- **Spieldauer** als MM:SS-Uhr in der Tablet-Kopfzeile.
- **Verletzungs-Button** (✚): unterbricht das Spiel, meldet es; das Feld
  wird in der bts-light-Felder-Übersicht hervorgehoben.
- **Turnierleitung-rufen-Button** (📣): Popup deutsch/englisch; Meldung
  erscheint app-weit in bts-light mit Feldnummer.
- **Tablet-Übernahme**: ein aktives Tablet pro Court; ein zweites Gerät
  zeigt „Feld wird bereits geschiedst" + Übernehmen.
- Zuvor (Zwischen-Deploys): Einzel-Court-Grafik-Fix (Name nicht doppelt),
  Ergebnis-Übermittlung mit automatischem Wiederholen bis zur Bestätigung.

## v0.4.2

- **Offizielle Pausen** (BWF): 60 s bei 11 Punkten, 120 s zwischen den
  Sätzen, je mit Countdown und „Weiterspielen".
- **Akkustand** der Tablets in der Felder-Übersicht (Android/Chrome).
- Moduswechsel LAN/Cloud greift sofort (Sync-Neustart beim Speichern).

## v0.4.1

- Oberflächen-Politur: Menü-/Button-Icons, Tooltips, modernere Optik.
- Cloud-Hinweis bei „Tablet-Spielzettel" für gesperrte Netze.

## v0.4.0

- **Cloud-Relay**: Tablets erreichen bts-light wahlweise direkt im LAN
  oder über einen Relay auf badhub.de. Der Cloud-Weg nutzt nur
  ausgehende Verbindungen und funktioniert auch hinter gesperrten
  Firmen-Firewalls. Umschaltbar im Setup. Details:
  [cloud-relay.md](cloud-relay.md).

## v0.1 – v0.3

Grundlagen: BTP-Anbindung (TP-Network-Protokoll), Badhub-Liveticker-Push,
Sync-Engine, Setup-Wizard und Dashboard, Auto-Update, digitaler
Tablet-Spielzettel im LAN, Diagnose-Logs, Single-Instance.
