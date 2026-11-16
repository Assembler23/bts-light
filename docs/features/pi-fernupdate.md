# Pi-Fernupdate — Spezifikation

> Status: **abgestimmt 2026-11-16** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Idee vom 2026-11-16. Betroffen: `pi/`, `src-tauri`, `src`, `scripts/`, CI. Kein `relay`, kein `relay-proto`.
> ADR: [0102](../adr/0102-vertrauensmodell-pi-fernupdate.md) (Vertrauensmodell) ·
> [0103](../adr/0103-pi-paketmodell-und-rueckfall.md) (Paketmodell und Rückfall).

## Kontext / Problem

Die Monitor-Pis bekommen ihre Skripte (Kiosk-Skript, `bts-cec.sh`, Dienste,
Chromium-Startparameter) heute nur über ein neues Image. Jede Änderung heißt:
SD-Karte aus jedem Pi ziehen und einzeln neu bespielen. Die letzten drei
Image-Neubauten (CEC, Ausbau des Nur-Lese-Modus, TV fernschalten) waren reine
Skript-Änderungen aus `pi/setup-monitor.sh` — das Betriebssystem blieb gleich.

Die Anzeige-Seiten selbst kommen schon vom Turnier-PC. Es fehlt ein Weg, auch
die Hülle am Pi von dort zu aktualisieren.

Ausgangslage im Code:

- SSH ist im Image aus. Das heutige Image lädt keinen Code vom PC nach.
- Der Pi findet den PC ohne Identitätsprüfung: Jedes Gerät im Netz, das auf
  `:8088/health` antwortet, gilt als Turnier-PC. Der LAN-Server ist
  unverschlüsselt und ohne Anmeldung.
- `pi/setup-monitor.sh` ist ein interaktiver Installer mit eingebetteten
  Dateien (Heredocs), `apt` und `raspi-config` — kein Paket.
- Für `pi/` gibt es keinen Test und kein shellcheck in der CI.
- Die Sets werden nach Turnieren hart vom Strom genommen. Pi Zero 2 W und
  3 B+ laufen gemischt mit demselben Image. Die Pis haben keine Echtzeituhr.

## Zielbild & Erfolgskriterien

Die Turnierleitung sieht auf der Seite „Court-Monitore" je Pi den Stand der
Pi-Skripte. Gibt es einen neueren, drückt sie einen Knopf; der Pi holt das
Paket vom Turnier-PC, prüft es, schaltet um und startet neu. Funktioniert die
Anzeige danach nicht, fällt der Pi von allein auf den vorherigen Stand zurück.

Erfolgskriterien:

- Eine reine Skript-Änderung erreicht alle Pis eines Sets ohne Kartenziehen,
  mit einem Knopfdruck und ohne Erklärung.
- Kein Fernupdate und kein Stromausfall währenddessen macht einen Pi
  unbrauchbar.
- Ohne Knopfdruck ändert sich an keinem Pi etwas.
- Kein Internet nötig.

Ein neues Image braucht es **weiterhin** für: Betriebssystem-Wechsel, neue
Systempakete, Startparameter (`config.txt`/`cmdline.txt`), den Updater-Kern
selbst, neue systemd-Units und einen Schlüsselwechsel über die Reserve hinaus.

## Nicht-Ziele

- Ganzes Image aus der Ferne (zwei Systemhälften), `apt`-/Systempakete,
  Kernel, Firmware.
- `config.txt` und `cmdline.txt` — das Paket fasst die Boot-Partition nie an.
- Automatisches Update ohne Knopfdruck (auch nicht beim Pi-Start).
- Cloud-/Relay-Strecke, Slave-PC, TL-Web. Pis dort bleiben in dieser Version
  außen vor.
- Zurückstufen auf eine ältere Paketnummer.
- Kopplung zwischen Pi und PC, TLS, Anmeldung am LAN-Server.
- WLAN-Zugangsdaten verteilen, SSH, Fernwartung.
- Von Hand eingerichtete Pis und die Desktop-Variante: Sie erscheinen als
  „braucht neue Karte".
- Frei definierbare systemd-Units im Paket.

## Betroffene Komponenten / Architekturregeln / Daten

- **Komponenten:**
  - `pi/paket/` (neu): die Pi-Dateien als echte Dateien, einzige Quelle für
    Image **und** Paket.
  - `pi/kern/` (neu): der feste Updater-Kern, nur im Image.
  - `pi/schluessel/` (neu): öffentliche Schlüssel.
  - `pi/setup-monitor.sh`, `pi/build-image-docker.sh`,
    `pi/build-master-image.sh` (geändert).
  - `scripts/pi-paket.mjs` (neu) und vier neue Node-Tests.
  - `src-tauri/build.rs`, `src-tauri/src/tablet/pi_update.rs` (neu),
    `tablet/assets.rs`, `tablet/server.rs`, `tablet/state.rs`,
    `commands.rs`, `lib.rs`.
  - `src/api.ts`, `src/io/piUpdate.mjs` (neu),
    `src/pages/CourtMonitorPanel.tsx`.
  - `.github/workflows/ci.yml`, `release.yml`.
- **Architekturregeln:**
  - R1: Die Seite spricht nur über Tauri-Commands
    (`monitor_pi_update_status`, `monitor_pi_update_start`,
    `monitor_pi_update_abbrechen`).
  - R2: Die Warnung „auf dem Feld wird gespielt" verknüpft nur Vorhandenes
    (Monitor-Zuweisung und Felder-Übersicht); nichts wird erfunden.
  - R3: Bewusst nur der LAN-Server. Die Routen existieren weder im Relay noch
    in der Slave-Brücke.
  - R4–R6: nicht berührt.
- **Konfiguration & Abwärtskompatibilität:** Keine neuen Felder in
  `config.rs`. Der Zustand lebt nur im Arbeitsspeicher der App. `identifier`
  und Updater-Pfad bleiben unangetastet. Eine ältere App an einem neueren Pi
  antwortet auf die neue Route mit 404; der Pi tut dann nichts.
- **Datenschutz:** Keine personenbezogenen Daten. Übertragen werden
  Geräte-Kennung (`pi-<serial>`), Paketnummer, Image-Generation und
  Zustandswörter.
- **Abhängigkeiten:**
  - Keine neue Cargo- oder npm-Dependency (`ring` ist vorhanden, Node
    `crypto` signiert).
  - Am Pi: `openssl` (wird ausdrücklich in die Paketliste des Images
    aufgenommen), `sha256sum`, `tar`, `curl`.
  - GitHub: neues Secret `PI_PAKET_SIGNING_KEY` (Admin).
  - `build.rs` ruft künftig Node auf; Node ist in jedem Job vorhanden, der
    `src-tauri` baut.
- **Verhältnis zur Spec „Hotfix ohne Neustart":** unabhängig. Dort tauscht
  der PC seine Geräte-Seiten und signiert mit dem Updater-Schlüssel; hier
  tauscht der Pi seine Skripte und prüft mit einem eigenen Schlüsselpaar.

## Verhalten

### Paket

Drei Dateien, in die App eingebettet:

- `manifest` — Text, höchstens 1024 Byte:

  ```
  bts-pi-paket 1
  nummer 7
  image-min 1
  groesse 41984
  sha256 <64 Hex-Zeichen>
  ```

- `manifest.sig` — Ed25519-Signatur über das Manifest, genau 64 Byte.
- `paket.tar` — unkomprimiert, deterministisch gebaut, höchstens 2 MiB.

Die **Paketnummer** ist eine eigene Ganzzahl (`pi/paket/NUMMER`), nicht die
App-Version. `pi/paket.lock` hält Nummer und Prüfsumme fest; ändert sich der
Inhalt ohne Nummern-Sprung, ist die CI rot.

`image-min` nennt die Image-Generation, die das Paket mindestens braucht. Das
Image trägt seine Generation als Ganzzahl in `/etc/bts/image-generation`.

Nie Paketinhalt: `bts-monitor-url.txt`, `bts-cec-aus.txt`, WLAN-Daten,
Boot-Dateien, der Updater-Kern, die Schlüssel.

### Signatur und Schlüssel

- Eigenes Ed25519-Schlüsselpaar nur für Pi-Pakete. Das Image trägt **zwei**
  öffentliche Schlüssel: aktiv und Reserve.
- Signiert wird nur im Tag-Release aus dem Secret. Fehlt es oder passt es
  nicht zum aktiven öffentlichen Schlüssel, bricht der Release-Lauf vor dem
  Installer-Bau ab.
- Builds ohne Secret (lokal, PR-Läufe) signieren mit einem
  **Entwickler-Schlüssel**, der aus einem festen, dokumentierten Wert im
  Skript abgeleitet wird. Ein normales Image lehnt ihn ab. Nur ein eigens
  gebautes Test-Image (`PI_TESTSCHLUESSEL=1`, Dateiname
  `bts-light-pi-TEST.img.xz`) nimmt ihn an und meldet sich als Test-Image.
- Die App prüft ihr eingebettetes Paket beim Start. Trägt es nur die
  Entwickler-Signatur, zeigt die Seite „Entwickler-Stand, nur für
  Test-Images".

### Ablauf am Pi

Der Updater-Kern läuft als eigener Root-Dienst, unabhängig vom CEC-Dienst.

1. Er fragt den PC alle 10 s:
   `GET /pi-update?device=…&paket=…&image=…&kern=…&zustand=…&grund=…&gesperrt=…&test=…&start=…`
2. Der PC antwortet mit einer Textzeile, höchstens 128 Byte:
   `bts-pi-update 1 <angebot> <auftrag> <marke>` — ohne Auftrag
   `bts-pi-update 1 8 - 0`, mit Auftrag `bts-pi-update 1 8 laden <marke>`.
   Fehlt das Wort `bts-pi-update` oder kommt 404, tut der Pi nichts.
3. Bei einem Auftrag mit neuer Marke lädt der Pi **zuerst nur** Manifest und
   Signatur, prüft die Signatur gegen jeden Schlüssel im Kern und liest erst
   dann die Werte.
4. Er lehnt ab, wenn eine dieser Bedingungen verletzt ist: Nummer höher als
   die aktive; Nummer nicht gesperrt; `image-min` ≤ eigene Generation;
   Größe ≤ 2 MiB; freier Platz ≥ 3 × Größe + 5 MiB.
5. Er lädt das tar (Größendeckel, Zeitdeckel 120 s), vergleicht Länge und
   Prüfsumme und prüft die Inhaltsliste: nur Dateien und Verzeichnisse, kein
   führendes `/`, kein `..`, höchstens 200 Einträge.
6. Er entpackt in ein Nebenverzeichnis, setzt Besitz und Rechte selbst,
   prüft die Skripte mit `bash -n` und schaltet per atomarem
   Symlink-Wechsel um. Danach startet der Pi neu.
7. Nach dem Neustart läuft die **Probe** (siehe „Rückfall").

Die PC-Adresse nimmt der Kern aus der Datei des Kiosks, sonst aus einem
eigenen Merker (letzte Adresse mit gültiger Antwort), sonst über
`bts-light.local`. Die Adresse gilt als nicht vertrauenswürdig; Schutz
kommt allein aus Signatur und Nur-höher-Regel.

Der Pi meldet diese Zustände: `bereit`, `laedt`, `wendet-an`, `startet-neu`,
`probe`, `fehler`, `zurueckgefallen`. Und diese Gründe: `signatur`, `format`,
`nummer`, `image-alt`, `groesse`, `pruefsumme`, `download`, `platz`, `inhalt`,
`abgebrochen`, `gesundheit`. Fehler und Rückfall stehen in einer
Zustandsdatei am Pi und werden auch nach einem App-Neustart wieder gemeldet.

### Rückfall

Der Kern entscheidet allein, nur anhand laufender Prozesse:

- **Stufe A:** Die grafische Oberfläche läuft, und das Kiosk-Skript läuft
  mindestens 90 s mit derselben Prozessnummer. Frist: 300 s nach dem Start.
- **Stufe B:** Ein Chromium-Prozess läuft mindestens 60 s mit derselben
  Prozessnummer. Hat der Pi eine hinterlegte PC-Adresse, wird B erst
  Pflicht, wenn der Kern den PC mindestens 180 s am Stück erreicht.
- **Bestätigt** ist das Paket, wenn A und B bestehen. Dann räumt der Kern
  alle Versionen außer aktiv, vorher und Werksstand.
- **Zurückgefallen** wird, wenn A in der Frist scheitert, wenn B scheitert,
  obwohl der PC erreichbar ist, oder wenn der Pi dreimal in Folge startet,
  ohne Stufe A zu erreichen.
- Ein fehlender Turnier-PC löst **nie** einen Rückfall aus.
- Ein Stecker, der nach bestandener Stufe A gezogen wird, zählt nicht gegen
  das Paket.

Beim Rückfall sperrt der Kern zuerst die Nummer, schaltet dann auf die
Vorversion und startet neu. Die gesperrte Nummer spielt dieser Pi nie wieder
ein, auch nicht per Knopf. Die Sperre endet, sobald ein Paket mit höherer
Nummer bestätigt ist. Zeigt der aktive Verweis ins Leere, stellt der Kern auf
die Vorversion, sonst auf den Werksstand des Images.

Die vier Zeitwerte sind Startwerte und werden vor dem Image-Release am
echten Gerät gemessen und festgelegt.

### Anzeige und Bedienung in der App

Seite „Court-Monitore", je Pi neben den TV-Knöpfen:

| Lage | Bedingung | Anzeige |
|---|---|---|
| aktuell | Nummer gleich dem Angebot | „Paket 7 · aktuell" |
| Update verfügbar | Nummer kleiner, Image neu genug | Knopf „Update auf 8" |
| wird abgeholt | Auftrag erteilt, Pi noch `bereit` | Fortschrittstext |
| lädt / wendet an / startet neu | Meldung des Pi, oder Funkstille nach `startet-neu` innerhalb der Frist | Fortschrittstext |
| wird geprüft | `probe` | „neu gestartet, wird geprüft" |
| fehlgeschlagen | `fehler` | Grund in Klartext; Knopf bleibt (außer bei `image-alt`) |
| zurückgefallen | `zurueckgefallen`, oder gesperrte Nummer = Angebot | „Paket 8 lief nicht, Pi nutzt wieder 7"; kein Knopf |
| braucht neue Karte | Image zu alt, oder Pi im LAN gesehen, aber seit über 60 s ohne Meldung an `/pi-update` | Hinweis mit Verweis auf die Anleitung; kein Knopf |
| Pi neuer als dieser PC | Nummer größer als das Angebot | „bts-light aktualisieren"; kein Knopf |
| nicht am Haupt-PC | Pi nur über die Cloud-Liste gesehen | „Pi einmal im Netz des Haupt-PCs starten"; kein Knopf |
| keine Rückmeldung | Auftrag älter als 8 min ohne Abschluss | Hinweis „bitte nachsehen"; Knopf wieder da |

Ein Test-Image trägt zusätzlich ein Etikett. Am Slave-PC zeigt die Seite
keinen Knopf, sondern „Pi-Updates gibt es nur am Haupt-PC".

**Rückfrage vor jedem Update:** Läuft auf dem Feld, das ein Pi zeigt, ein
Spiel, warnt die App vor dem Einzel- und vor dem Sammel-Update, beim
Sammel-Update mit Anzahl („6 Pis nacheinander aktualisieren? Auf 4 Feldern
läuft ein Spiel; jeder Monitor ist etwa 2 Minuten dunkel."). Als Feld zählen
die Zuweisungen Feld, Zähltafel und Kombi; Info- und Werbe-Anzeigen zählen
nicht.

**„Alle Pis aktualisieren":** einer nach dem anderen. Der nächste Pi bekommt
den Auftrag erst, wenn der vorige die neue Nummer und `bereit` meldet.
Schlägt einer fehl, fällt zurück oder meldet sich nicht, stoppt der Lauf;
die Seite nennt den Pi mit Grund, die übrigen bleiben unberührt. Der Lauf
ist abbrechbar.

Ein Auftrag und ein laufender Sammel-Lauf überleben den App-Neustart nicht.
Die Pis melden ihren Stand danach selbst wieder.

## Akzeptanzkriterien

**Gutfall**

- [ ] Ein Pi mit Paket 7 an einer App mit Paket 8 zeigt „Update auf 8". Nach
      dem Knopfdruck meldet er innerhalb von 8 min Paket 8 und „aktuell",
      ohne dass jemand den Pi berührt.
- [ ] Ohne Knopfdruck fragt kein Pi je `/pi-update/paket.tar` ab (über 24 h
      geprüft).
- [ ] „Alle Pis aktualisieren" nennt in der Rückfrage die Zahl der Pis und
      der bespielten Felder. Zu jedem Zeitpunkt ist höchstens ein Pi im
      Update.
- [ ] Nach bestätigtem Update liegen am Pi höchstens drei Versionen.
- [ ] `bts-monitor-url.txt`, `bts-cec-aus.txt`, WLAN-Daten, `config.txt` und
      `cmdline.txt` sind nach dem Update bytegleich.

**Ablehnung am Pi** — jeweils: aktive Version unverändert, kein Neustart,
die App zeigt den Grund

- [ ] Falsche oder fehlende Signatur oder verändertes Manifest → `signatur`.
- [ ] Paket abgeschnitten oder Prüfsumme falsch → `pruefsumme`; größer als im
      Manifest oder über 2 MiB → `groesse`.
- [ ] Gleiche oder niedrigere Nummer → das tar wird nicht geladen.
- [ ] Image-Generation unter `image-min` → der Pi lehnt ab; die App zeigt
      schon vorher „braucht neue Karte" ohne Knopf.
- [ ] Ein tar mit Symlink, absolutem Pfad oder `..` wird nicht entpackt →
      `inhalt`.
- [ ] Zu wenig Speicherplatz → `platz`.
- [ ] Der PC verschwindet im Download → `download`; nach seiner Rückkehr
      funktioniert ein neuer Knopfdruck.
- [ ] Eine Antwort ohne das Wort `bts-pi-update` oder ein 404 löst nichts
      aus.
- [ ] Dieselbe Auftragsmarke wird nur einmal ausgeführt.

**Stromausfall**

- [ ] Nach Stromverlust in jeder Phase des Ablaufs (Download, Entpacken,
      nach dem Umbenennen, nach dem Schreiben der Probe, während des
      Umschaltens, im Neustart, in der Probe, im Rückfall, beim Aufräumen)
      startet der Pi in eine lauffähige Anzeige und meldet seinen wahren
      Stand.
- [ ] Stromverlust vor dem Umschalten sperrt die Nummer nicht.

**Rückfall**

- [ ] Ein Paket, dessen Kiosk-Skript sofort endet, führt ohne Eingriff
      spätestens nach dem dritten Start zur Vorversion. Die App zeigt
      „zurückgefallen".
- [ ] Dieselbe Nummer wird auf diesem Pi nicht erneut eingespielt; die
      nächsthöhere schon.
- [ ] Ein fehlender Turnier-PC nach dem Neustart löst keinen Rückfall aus.

**Umgebung**

- [ ] Pi am Slave-PC oder nur über die Cloud: Der Haupt-PC zeigt „nicht am
      Haupt-PC" mit Handlungshinweis; die Slave-App zeigt keinen Knopf.
- [ ] Karte mit altem Image: „braucht neue Karte", kein Knopf.
- [ ] Pi neuer als der PC: kein Auftrag, der Pi bleibt auf seinem Stand.
- [ ] Läuft auf dem Feld eines Pi ein Spiel, erscheint vor Einzel- und
      Sammel-Update eine Warnung.
- [ ] Schlägt beim Sammel-Update Pi 3 von 6 fehl, bleiben Pi 4–6 auf „Update
      verfügbar", und die Seite nennt Pi 3 mit Grund.
- [ ] Nach einem App-Neustart ist kein Auftrag offen; ein früherer Rückfall
      wird trotzdem wieder angezeigt.

**Bau und Schlüssel**

- [ ] Ein Entwickler-Build zeigt „Entwickler-Stand". Ein normales Image nimmt
      dessen Paket nicht an; ein Test-Image schon und ist als solches
      markiert.
- [ ] Ein normal gebautes Image enthält den Entwickler-Schlüssel nicht (der
      Image-Bau prüft das und bricht sonst ab).
- [ ] Ein Tag-Release ohne gültiges `PI_PAKET_SIGNING_KEY` bricht vor dem
      Installer-Bau ab.
- [ ] Ändert jemand `pi/paket/` ohne Nummern-Sprung, ist die CI rot.
- [ ] Zweimal gebaut ergibt das Paket dieselben Bytes, unter Windows wie
      unter Linux.
- [ ] Nach PR 1 verhält sich ein frisch gebautes Image wie vorher
      (Kopplungscode, CEC, Ansage-Ton, Hinweisseite ohne Adresse).

## Tests

**Rust (`tablet/pi_update.rs`)**

- `ohne_auftrag_nennt_die_antwort_nur_das_angebot`
- `ein_auftrag_reist_mit_marke_und_nur_an_frische_geraete`
- `ein_auftrag_verfaellt_nach_der_frist`
- `die_meldung_wird_auf_bekannte_zustaende_und_gruende_gelesen`
- `kennung_und_zahlen_werden_gedeckelt`
- `der_speicher_waechst_nicht_ueber_den_deckel`
- `lage_aktuell_verfuegbar_pi_neuer_image_zu_alt` (Tabellentest über alle
  Lagen)
- `funkstille_nach_startet_neu_ist_kein_fehler_bis_zur_frist`
- `zeitueberschreitung_gibt_den_knopf_wieder_frei`
- `zurueckgefallenes_paket_bekommt_keinen_auftrag`
- `alle_nacheinander_der_naechste_erst_nach_bestaetigung`
- `alle_stoppt_beim_ersten_fehlschlag_und_laesst_den_rest_unberuehrt`
- `das_eingebettete_paket_ist_in_sich_stimmig` (Länge, Prüfsumme, Signatur)
- `ein_entwickler_paket_wird_als_solches_erkannt`
- `das_paket_bleibt_unter_dem_deckel`

**Rust (`tablet/server.rs`)**

- `pi_update_antwortet_mit_magiewort`
- `pi_update_ohne_kennung_ist_400`
- `paket_route_liefert_die_eingebetteten_bytes`

**Node**

- `scripts/test-pi-quelle.mjs`: Jede Datei unter `pi/paket/` steht in der
  Inhaltsliste und umgekehrt; `setup-monitor.sh` enthält die verschobenen
  Heredocs nicht mehr und kopiert jede Zeile der Liste; keine fest
  eingebackenen Pfade im Kiosk-Skript.
- `scripts/test-pi-paket.mjs`: bytegleicher Bau, Rechte aus der Liste statt
  vom Dateisystem, `paket.lock` passt, Signatur prüft gegen den
  Entwickler-Schlüssel. Im Linux-Job zusätzlich die Gegenprobe mit
  `openssl pkeyutl -verify -rawin`.
- `scripts/test-pi-updater.mjs` (Linux-Job): startet den Kern gegen ein
  Wurzelverzeichnis `ROOT=<tmp>` mit einem Node-HTTP-Server als PC. Fälle:
  guter Durchlauf; falsche Signatur, verändertes Manifest, Signatur falscher
  Länge; Reserve-Schlüssel gilt; Entwickler-Schlüssel nur im Test-Image;
  gleiche und niedrigere Nummer; Image zu alt; Paket abgeschnitten, zu groß,
  über dem Deckel, falsche Prüfsumme; tar mit Symlink, absolutem Pfad, `..`;
  PC verschwindet im Download; kein Platz; **Abbruch nach jeder Phase
  hinterlässt einen startfähigen Stand**; Probe besteht; Probe scheitert mit
  Sperre; gesperrte Nummer bleibt gesperrt, höhere geht; drei Starts ohne
  Kiosk; Stecker nach Stufe A zählt nicht; ohne PC kein Rückfall; Verweis ins
  Leere; Antwort ohne Magiewort; gleiche Marke nur einmal.
- `scripts/test-pi-update.mjs`: Texte je Lage, Zählung der bespielten Felder
  (Feld, Tafel, Kombi; Info zählt nicht), Rückfrage-Text.
- `scripts/test-nicht-uebersetzen.mjs`: liest künftig
  `pi/paket/share/no-url.html`.

**CI:** `shellcheck` und `bash -n` über `pi/*.sh`, `pi/paket/bin/*.sh`,
`pi/kern/*.sh` im Linux-Job.

**Pflicht am echten Gerät vor dem Image-Release** (Zero 2 W und 3 B+):

1. `openssl`-Prüfung mit gültiger und falscher Signatur, falschem Schlüssel,
   leerer Datei.
2. Startzeit bis zu stabilem Chromium → die vier Probe-Zeiten und die
   8-Minuten-Frist der App festlegen.
3. Stecker ziehen in jeder Phase, je Gerätetyp mindestens dreimal je Phase.
4. Zwei kaputte Testpakete (Kiosk-Skript endet sofort; Chromium mit falschem
   Parameter) → Rückfall und Sperre.
5. Sammel-Update mit sechs Pis, einer davon ohne Strom → Stopp.
6. Haupt-PC und Slave-PC im selben Netz, ein Pi am Slave.
7. Ein Pi mit alter Karte daneben.
8. Dauer eines Updates Ende zu Ende (belegt den Text „etwa 2 Minuten").
9. Probestart des emuliert gebauten Images.

## Risiken & Rollback

- **Der Kern ist nie fernänderbar.** Ein Fehler darin kostet eine
  Kartenrunde. Deshalb klein halten, vollständig über die `ROOT`-Tests
  abdecken, vom security-reviewer prüfen lassen und erst nach den
  Gerätetests ausliefern.
- **Restrisiko falscher PC:** Ein fremder Rechner im Hallen-WLAN kann einen
  Pi einmal auf ein echtes, neueres Paket von uns heben und damit einen
  Neustart der Anzeige auslösen. Er kann keinen fremden Code einspielen,
  nichts zurückstufen und den Vorgang nicht wiederholen. Das entspricht dem
  heutigen „TV aus", das ebenfalls jeder im WLAN senden kann.
- **Prozessnamen als Gesundheitsmaß:** Benennt ein künftiges Paket das
  Kiosk-Skript um oder wechselt den Browser, fällt es zurück. Sicher, aber
  überraschend; steht in ADR 0103.
- **Schlüssel:** Geht der aktive verloren, signiert die Reserve weiter. Sind
  beide verloren oder offengelegt, braucht es neue Karten.
- **Test-Haken im Root-Skript** (`ROOT`, `BTS_TEST_*`) müssen im Echtbetrieb
  wirkungslos sein. Sie greifen nur, wenn `ROOT` gesetzt und nicht `/` ist.
- **`badhub` hat passwortloses sudo.** Die Trennung zwischen Root- und
  Benutzerbesitz schützt vor Versehen, nicht vor jemandem mit Tastatur am Pi.
- **Zwei bts-light-PCs im selben Netz:** Welchen PC ein Pi findet, bleibt
  Zufall wie bisher; das Update folgt diesem PC.
- **`build.rs` ruft Node:** Fehlt Node, muss die Fehlermeldung das klar
  sagen.
- **Rollback:** Am Pi durch den automatischen Rückfall. In der App durch die
  Vorversion — die Routen entfallen, der Pi bekommt 404 und tut nichts. Ein
  schlechtes Paket wird durch ein Paket mit höherer Nummer ersetzt.

## Offene Fragen / Annahmen

Annahmen dieser Spec, mit der Freigabe am 16.11.2026 bestätigt:

- **„Dienste" heißt Einstiegsskripte hinter festen Units.** Die Units selbst
  liegen im Image und zeigen auf den Symlink. Eine neue Unit braucht ein
  neues Image.
- **Die Sperre einer zurückgefallenen Nummer ist endgültig** bis zur
  nächsten Nummer. Es gibt keinen Knopf „trotzdem erneut versuchen".
- **Die Desktop-Variante** von `setup-monitor.sh` bekommt dasselbe
  Verzeichnis-Layout, aber keinen Updater und keine Generationsmarke.

Vor der Umsetzung zu messen (sonst keine offenen Fragen):

- Die `openssl`-Prüfung am echten Bookworm-Pi. Fällt sie durch, wird
  `minisign` ins Image aufgenommen; Format und Protokoll bleiben, nur
  Prüfbefehl und Signierer ändern sich.
- Die shellcheck-Befunde der drei Bestandsskripte (Umfang von PR 1).

## Betroffene Doku-Dateien

- `docs/court-monitor.md` — neuer Abschnitt „Pi-Skripte aktualisieren", mit
  `pruef`-Ankern auf die Knopftexte und die Route.
- `docs/oberflaechen.md` — neue Route und Knöpfe.
- `docs/pi-master-image.md` — Abschnitt „Updates" neu: wofür noch ein Image
  nötig ist, Image-Generation, Test-Image.
- `docs/pi-setup.md` — Einrichtung mit `pi/paket/`.
- `docs/release.md` — Secret, Abschnitt „Pi-Paket signieren",
  Schlüsselverlust und Reserve.
- `docs/logging.md` — Updater-Log `pi-<serial>-update`.
- `docs/roadmap.md`, `docs/changelog.md`.
- `CLAUDE.md` — neue Tabellenzeile „Pi-Fernupdate"; die Zeilen „Fernseher
  fernschalten" und „Ansage-Seite" auf die neuen Pfade.
- **Anker, die in PR 1 brechen:** elf `pruef`-Anker auf
  `pi/setup-monitor.sh` in `docs/court-monitor.md`, `docs/pi-setup.md` und
  `docs/pi-master-image.md` — umhängen auf `pi/paket/bin/bts-monitor.sh`
  bzw. `pi/paket/bin/bts-cec.sh`.

## Umsetzungs-Hinweise

### Layout am Pi

```
/opt/bts/kern/                 root, nur im Image: bts-update.sh, schluessel/*.pub.pem
/etc/bts/image-generation      Ganzzahl;  /etc/bts/kiosk-benutzer
/opt/bts/pakete/<nr>/          root, für badhub nur lesbar: bin/, share/, xinitrc
/opt/bts/aktiv  -> pakete/<nr> relativer Symlink
/opt/bts/vorher -> pakete/<nr>
/opt/bts/werk   -> pakete/<nr> Stand des Images, wird nie gelöscht
/var/lib/bts/update/           root 700: zustand, eingang/, log, pc
```

Feste Einstiegspunkte im Image: `bts-cec.service` startet
`/opt/bts/aktiv/bin/bts-root.sh`; `bts-update.service` startet
`/opt/bts/kern/bts-update.sh lauf` mit `Restart=always`; `~/.xinitrc` ist
der Einzeiler `exec /opt/bts/aktiv/xinitrc`.

Bisher eingebackene Werte ermittelt das Kiosk-Skript zur Laufzeit: den
Browser über `command -v`, die Hinweisseite relativ zum eigenen Ort.

### Reihenfolge beim Einspielen

1. Zustand `laeuft=<nr>` atomar schreiben (Nebendatei, `sync`, umbenennen,
   `sync`).
2. Manifest und Signatur laden, prüfen, Bedingungen prüfen.
3. tar laden, Länge und Prüfsumme prüfen.
4. Inhaltsliste prüfen, nach `pakete/.neu-<nr>/` entpacken, Besitz und
   Rechte setzen, `bash -n`, `sync`.
5. Umbenennen nach `pakete/<nr>`, `sync`.
6. Zustand `probe=<nr> vorher=<alt> versuche=0` atomar schreiben — **vor**
   dem Umschalten.
7. Symlink `aktiv` und `vorher` per `ln -sfn` auf eine Nebendatei und
   `mv -T` wechseln, `sync`.
8. `startet-neu` melden, Neustart.
9. Probe; bei Bestätigung aufräumen.

Invariante: `aktiv` zeigt immer auf ein vollständig geschriebenes
Verzeichnis.

| Strom weg … | Zustand beim nächsten Start | Reaktion des Kerns |
|---|---|---|
| in 1–3 | `laeuft=N`, Eingang halb | Eingang leeren, `abgebrochen` melden |
| in 4 | `.neu-N` halb | `.neu-*` löschen, `abgebrochen` |
| nach 5, vor 6 | `N/` vollständig, ohne Verweis | verwaiste Verzeichnisse löschen, `abgebrochen` |
| nach 6, vor 7 | `probe=N`, `aktiv` zeigt auf alt | Probe verwerfen, `N/` löschen, `abgebrochen`, keine Sperre |
| während 7 | alt oder neu (Umbenennen ist atomar) | wie Zeile davor oder danach |
| nach 7 oder im Neustart | `probe=N`, `aktiv` zeigt auf N | normale Probe |
| in der Probe | Zähler `versuche` | siehe „Rückfall" |
| im Rückfall | Sperre steht vor dem Zurückschalten | Rückfall zu Ende führen |
| beim Aufräumen | — | erneut aufräumen |

### App-Seite

- `tablet/pi_update.rs` nach dem Muster `tablet/tv.rs`: RAM-Store mit
  `MAX_GERAETE = 256`, Verdrängen des am längsten stummen Geräts,
  `geraet_bereinigt` für die Kennung, Zahlen als `u32`, Zustand und Grund
  über Lesefunktionen mit Rückfall „unbekannt". Frisch-Fenster 35 s,
  Vergessen nach 10 min.
- Die reine Funktion `lage(eintrag, angebot, jetzt)` ist die einzige Quelle
  der Anzeige-Zustände.
- Der Sammel-Lauf rückt träge vor — bei jeder Pi-Meldung und jeder
  Statusabfrage der Seite — über die reine Funktion `lauf_schritt`. Kein
  eigener Task.
- Routen neben `/pi-tv`: `/pi-update`, `/pi-update/manifest`,
  `/pi-update/manifest.sig`, `/pi-update/paket.tar`.
- „Braucht neue Karte" und „nicht am Haupt-PC" leitet der Command aus der
  vorhandenen Monitor-Geräteliste plus Store ab.
- Updater-Log am Pi: 200 Zeilen, Versand über das bestehende `/pi-log` mit
  der Kennung `pi-<serial>-update`, damit es das Kiosk-Log nicht
  überschreibt.

### Paketbau

`scripts/pi-paket.mjs` baut das tar selbst in Node (ustar, Reihenfolge und
Rechte aus `pi/paket/INHALT`, Zeitstempel 0, Besitzer 0), schreibt das
Manifest und signiert. `build.rs` ruft es nach `OUT_DIR` und meldet
`rerun-if-changed` für `pi/paket` und das Skript sowie
`rerun-if-env-changed` für das Secret. `tablet/assets.rs` bindet die drei
Dateien per `include_bytes!` ein.

### Drei PRs

**PR 1 — `pi/` in echte Dateien, ohne Verhaltensänderung.**
shellcheck-Befunde erheben und beheben; Heredocs wörtlich nach `pi/paket/`
verschieben (Nachweis per `diff` in der PR-Beschreibung); einzige inhaltliche
Änderung sind die zur Laufzeit ermittelten Werte; `setup-monitor.sh` kopiert
an die **alten** Zielorte und lädt `pi/` aus dem Repo-Archiv nach, wenn es
allein geladen wurde (die Anleitung in `docs/pi-setup.md` bleibt gültig);
`build-image-docker.sh` kopiert `pi/paket/` mit. Tests: `test-pi-quelle.mjs`,
shellcheck. Anker umhängen. Kein Versions-Bump. Vor dem Merge: Image aus dem
PR-Stand auf beiden Pi-Modellen starten. Review: code-reviewer.

**PR 2 — Paket, Signatur, Auslieferung, Zustand.**
ADR 0102/0103, `pi-paket.mjs`, `NUMMER`, `paket.lock`, öffentliche Schlüssel,
`build.rs`, `assets.rs`, `pi_update.rs`, Routen, Commands, Release-Riegel.
Tests: Rust-Liste oben, `test-pi-paket.mjs`. Versions-Bump ja, Changelog
„Vorbereitung, noch ohne Bedienung". Reviews: code-reviewer **und
security-reviewer** (offene Route, Eingabedeckel, Signaturkette, Secret im
Workflow). **Admin vorab:** zwei Schlüsselpaare erzeugen, öffentliche Teile
liefern, Secret `PI_PAKET_SIGNING_KEY` (nur aktiv) anlegen, Reserve offline
verwahren.

**PR 3 — Updater-Kern, Image, Oberfläche.**
`pi/kern/bts-update.sh` mit `start`, `takt`, `lauf`; neues Layout in
`setup-monitor.sh`; Selbsttest der Signaturprüfung und Schlüssel-Prüfung im
Image-Bau; Test-Image-Schalter; Seite „Court-Monitore". Tests:
`test-pi-updater.mjs`, `test-pi-update.mjs`. Versions-Bump ja. Reviews:
code-reviewer **und security-reviewer** (netzgetriebener Root-Pfad,
Entpacken, Test-Haken, Lesen von Benutzerdateien als root). Danach die neun
Gerätetests, dann das Image bauen und hochladen — das ist die letzte
Kartenrunde.
