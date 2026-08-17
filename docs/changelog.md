# Änderungsverlauf

Pro veröffentlichter Version die wesentlichen Änderungen. Die Versionen
werden über das Auto-Update (badhub.de) ausgeliefert; Tablet-Änderungen
erreichen den Cloud-Modus zusätzlich sofort über den Relay-Redeploy.

## v0.9.219

- **TL-Web: Render-Sparsamkeit** (Feldtest 17.08.2026, „Performance im
  Blick"). Bisher baute jede Zustandsänderung — auch ein einzelner
  Punktgewinn — die komplette Seite per `innerHTML` neu, samt aller
  Event-Listener. Jetzt zweistufig: (1) **Panel-Kurzschluss** — jedes
  Panel überspringt Neuaufbau und Verdrahtung, wenn sein HTML unverändert
  ist; ein Punktgewinn zeichnet nur noch das Felder-Panel. (2)
  **Keyed-Abgleich** für die zwei heißesten Listen: Feldkacheln und
  Spielzeilen werden je `court_id`/`match_id` abgeglichen — nur die
  geänderte Kachel/Zeile wird ersetzt und neu verdrahtet, der Rest bleibt
  samt offenem ⋯-Menü, Fokus und Scroll-Position stehen. Sekündlich
  tickende Zeittexte stehen nicht mehr im erzeugten HTML (sie hebelten
  den Abgleich aus) — `tickClocks()` füllt sie direkt nach jedem Render.
  Diagnose: `localStorage.tlRenderMessen = "1"` protokolliert die
  Render-Dauer in der Browser-Konsole. Nebenbei behoben: Die
  Spielzeilen-Griffe bekamen bei jedem Neuzeichnen doppelte
  Drag-Listener; neue Panels materialisieren wie gehabt, aber Listen
  ohne Änderung verlieren ihre Listener nicht mehr.

## v0.9.218

- **TL-Web: neues Panel „Anfangszeiten".** Der heutige Check-In-Zeitplan
  als Kachel (Feldtest 17.08.2026, Vorbild badhubs `…/zeitplan`-Seite):
  je Klasse Anfangszeit, Anmeldeschluss und der Check-In-Stand
  („12/16", grün wenn vollzählig) — nach Anfangszeit sortiert,
  durchgelaufene Schlüsse bleiben ausgegraut sichtbar. Bewusst nur
  Zähler, nie Spielernamen (die hat die Desktop-Check-In-Seite). Der
  Turnier-PC fragt badhub dafür höchstens minütlich (`/tl/stand`) über
  einen eigenen, vom Liveticker entkoppelten Tick; das Panel erscheint
  nur, wenn Check-In **und** TL-Web eingerichtet sind und badhub
  antwortet. Kurze Aussetzer leeren das Panel nicht; nach fünf Minuten
  ohne Abruf erscheint eine ⚠-Veraltet-Zeile. Die Zähler rechnen
  Abgemeldete heraus (wie die Desktop-Seite); ohne eigenen
  Anmeldeschluss gilt die Anfangszeit (wie die Ansage); Ablehnungen
  pausieren 30 Minuten statt minütlich anzuklopfen; neue Panels landen
  in Bestandsprofilen in der letzten Spalte statt in der Felder-Spalte;
  die Relay-Kürzungsleiter opfert das Panel notfalls, bevor der
  Cloud-Zustand zu groß würde.

## v0.9.217

- **TL-Web: Punktverlauf-Knopf ins ⋯-Menü der Feldkachel.** Der 📈-Knopf
  saß als letzte eigene Fläche in der Fußzeile der Kachel — jetzt steht
  er als „📈 Punktverlauf" im Menü, wie alle anderen Kachel-Aktionen
  (Feldtest 17.08.2026: weiter aufräumen). Verhalten unverändert: nur
  sichtbar, wenn ein Verlauf existiert; geladen wird erst beim Klick.
  Die Beendet-Liste behält ihren direkten 📈-Knopf.

## v0.9.216

- **TL-Web: Profil-Option „Aufrufe unbegrenzt".** Neues Häkchen im
  Anzeige-Abschnitt des Profil-Editors (Feldtest 17.08.2026): Der
  Aufruf-Knopf am Feld bleibt damit immer verfügbar — auch bei laufendem
  Spiel und über den dritten Aufruf hinaus („Erneut aufrufen"). Der
  Turnier-PC zählt die Stufe dann ehrlich weiter (4, 5, …); ab dem
  vierten Aufruf — und bei laufenden Spielen grundsätzlich — spricht das
  Ansage-Gerät die schlichte Feld-Ansage ohne Stufenwort statt noch
  einmal „Dritter und letzter Aufruf". Führt **kein** Profil die Option,
  hält der Turnier-PC den alten 3er-Deckel selbst — alles bleibt beim
  Alten; alte Browser-Profile ohne das Feld lesen sich als „aus"
  (`unlimitedCourtCalls`, `#[serde(default)]`). Der Aufruf-Knopf der
  Desktop-Felderübersicht folgt derselben Regel, und der Uhr-Chip
  („Letzter Aufruf") erlischt, sobald die Stufe gesprochen ist.

## v0.9.215

- **TL-Web: ⋮-Menü der Spielzeilen wird nicht mehr abgeschnitten.** Das
  Menü einer Zeile am unteren Panel-Rand ragte nicht über die Panel-Kante
  hinaus (die `overflow`-Klemmen der Panels schneiden auch hoch gestapelte
  Kinder — ein höherer z-index hätte nichts geändert). Es hängt jetzt wie
  das Kachel-Menü am Ansichtsfenster: `position: fixed` plus die geteilte
  Platzierungsfunktion (`platziereKebabMenue`), die unter dem Auslöser
  öffnet, wenn Platz ist, sonst darüber (Feldtest 17.08.2026). Offene
  Menüs folgen ihrem Auslöser beim Rollen und bei Größenwechseln nach
  (`folgeOffenenKebabs`); rollt er aus dem Sichtfenster, schließt das
  Menü. z-index jetzt über der Action-Bar, damit ein nach oben
  aufgeklapptes Menü nicht unter ihr verschwindet.

## v0.9.214

- **TL-Web-Spielliste: Symbolik statt Hinweistexte.** Die Texte „nicht
  bereit", „X spielt gerade", „pausiert noch bis …" und „ohne Halle" sind
  weg — sie machten die Liste unübersichtlich. Stattdessen: Wer gerade auf
  einem Feld steht, trägt einen **roten** Namens-Hintergrund
  (`title="spielt gerade"`); wer noch in der Mindestpause ist, einen
  **orangen**, daneben zählt eine **Eieruhr** die verbleibende Wartezeit
  sekündlich herunter. Ein Spiel ohne Hallenzuordnung erkennt man am
  fehlenden Hallen-Kürzel.
- **Jeder Name der Spielliste verlinkt auf die badhub-Spielerseite**
  (`badhub.de/spieler/<Nr>/live`, neuer Tab). Dafür reist die BTP-
  Lizenznummer (`MemberID`) jetzt in den Wartelisten-Einträgen des
  TL-Zustands mit — bewusste Datenschutz-Freigabe wie zuvor Nation und
  Verein: Die Nummer ist der öffentliche URL-Schlüssel genau dieser
  badhub-Seite und steht hier hinter dem Gerätezugang. Laufende und
  beendete Spiele bleiben ohne Lizenznummer (Wächter-Test angepasst).
  Die Namens-Färbung ordnet über `blocked.player_keys`
  (Lizenznummer statt Namensvergleich) — zwei gleichnamige Spieler einer
  Paarung färben nicht mehr gemeinsam.

## 0.9.213

- **Disziplin und Klassenkürzel jetzt auch im `tset`**: Der Liveticker liest
  `n` (= `draw_name + round_name`) und zeigte bei Gruppenturnieren deshalb
  nur „Gruppe 1 G1" — die Disziplin kam dort nie an. Sie ging bisher nur im
  `sched`-Kanal raus, der badhubs Spielerseite speist. Beide Felder sind
  optional serialisiert: ein Empfänger, der sie nicht kennt, sieht keinen
  Unterschied.

## v0.9.212

- **Startzeit-Prognose rechnet mit dem Live-Spielstand** (Spec
  [features/spielzeiten-prognose.md](features/spielzeiten-prognose.md),
  Etappe D): Die Restzeit belegter Felder kommt jetzt aus Satzstand,
  Zählsystem und dem gemessenen Eigentempo des laufenden Spiels — ein
  Spiel bei 14:6 im dritten Satz gibt sein Feld in der Simulation gleich
  frei, ein 0:0-Spiel hält es die volle erwartete Dauer. Ein möglicher
  Entscheidungssatz zählt mit seiner Wahrscheinlichkeit (aus Satzstand
  und Punktstärke: 15:5 + 10:6 vorn ⇒ dritter Satz fast ausgeschlossen,
  15:5 + 7:11 hinten ⇒ sehr wahrscheinlich, 13:13 unter Gleichstarken
  ⇒ etwa halber Satz). Alle Prognosen der Warteliste („dran ca. hh:mm")
  werden dadurch deutlich genauer. Felder ohne Live-Zählung behalten das
  bisherige Modell.
- **Neu in TL-Web:** Profil-Schalter „Restzeit laufender Spiele zeigen"
  (Standard: aus) — die Schätzung („~12 min Rest") in der Fußzeile jeder
  belegten Feldkachel. Cloud: Relay-Deploy läuft beim Merge automatisch;
  alte Hosts senden das Feld nicht, alte Seiten ignorieren es.

## v0.9.211

- **Disziplin, Klassenkürzel und Hallenfarbe im `sched`-Payload**: badhubs
  Spielerseite zeigte bisher nur „Gruppe 1 G1" — das ist `draw_name +
  round_name`, und `draw_name` ist bei Gruppenturnieren die
  Auslosungsgruppe, nicht die Klasse. Die Disziplin („HE") und das
  Klassenkürzel („A") lagen hier längst vor, wurden aber nie gesendet;
  badhub konnte sie deshalb nicht anzeigen, egal was es tat. Dazu die
  Farbmarke der Halle aus derselben Quelle wie im `tset`, damit Liveticker
  und Spielerseite dieselbe Marke zeigen.

## v0.9.210

- **Hallen-Farben** (Spec
  [features/hallen-farben.md](features/hallen-farben.md), ADR
  [0031](adr/0031-hallen-farben-eigener-config-store.md)/[0032](adr/0032-hallen-farben-deterministische-auto-palette.md)/[0033](adr/0033-hallen-farben-hex-auf-dem-draht.md)):
  Bei Mehr-Hallen-Turnieren trägt jede Halle eine Farbe als kleine Marke
  neben jeder Hallen-Nennung — Felderübersicht und Vorbereitungs-Panel der
  Desktop-App, TL-Web (Hallen-Filter, Feld-Gruppen, Wartelisten- und
  Beendet-Zeilen inkl. „·A", Spielort-Wähler), Monitor-/Übersichts-/
  Vorbereitungs-Seiten (LAN und Cloud) und der badhub-Push
  (`hall_color` in `tset`-Courts und -Aufrufen; die badhub-Anzeige folgt
  als eigener badhub-PR). Farben kommen automatisch aus einer kuratierten
  16-Ton-Palette (deterministisch; Grün/Violett der Feldzustände bleiben
  ausgespart, ein kräftiges Rot ist auf Nutzerwunsch dabei — die Marke ist
  nie der Zustands-Streifen) und lassen
  sich je Halle im Hallen-Editor der Felderübersicht übersteuern
  („Automatisch" setzt zurück). Kürzel/Name bleiben überall stehen —
  die Farbe ist nie einziger Informationsträger. Ein-Hallen-Turniere
  bleiben unverändert. Cloud: Relay-Deploy vor App-Release (optionale
  Felder, alte Gegenstellen degradieren farblos).
- TL-Web-Beendet-Zeilen nennen jetzt ihre Halle (Kürzel + Marke); die
  Vorbereitungs-Seite kann die Kopfzeile je `?halle=`-Filter einfärben.

## v0.9.209

- **Vollständiger Spielplan an badhub** (`sched`, Spec
  [features/spielplan-an-badhub.md](features/spielplan-an-badhub.md)):
  Neben dem `tset` geht jetzt höchstens minütlich ein zweiter Payload mit
  **allen** Spielen des Turniers an badhub — je Spiel angesetzte Zeit
  (BTP `PlannedTime` als Unix-ms), prognostizierte Startzeit aus der
  Spielzeiten-Messung, Warteschlangen-Position innerhalb der Halle sowie
  Halle und Feld. Damit zeigt badhubs Spielerseite
  `/spieler/{lizenz}/live` einem Teilnehmer seinen ganzen Turniertag statt
  nur des nächsten Spiels. Grund: der `tset` kappt bei 15 kommenden Spielen
  des gesamten Turniers — wessen Spiel weiter hinten liegt, tauchte dort
  nie auf.
- Der Liveticker bleibt unverändert: `sched` läuft getrennt und mit eigenem
  Fehlerpfad. Kennt badhub den Nachrichtentyp noch nicht (400/404), pausiert
  der Versand, statt jeden Zyklus anzuklopfen.

## v0.9.208

- **Automatische Hallen-Vorverteilung** (Spec
  [features/hallen-vorverteilung.md](features/hallen-vorverteilung.md),
  ADR [0029](adr/0029-hallen-vorverteilung-eigener-store.md)/[0030](adr/0030-halle-bindet-die-feldvergabe.md)):
  Bei Mehr-Hallen-Turnieren verteilt die Automatik die vordersten x Spiele
  der Warteliste auf die Hallen — im Verhältnis der entsperrten Felder,
  gemischt (2:1 → A, A, B, …) und fortlaufend nachgefüllt. Spieler sehen
  ihre Halle früh (Hallen-Kürzel „·A" in TL-Web, Hallen-Monitore, badhub
  `display=next&halle=…`). Bedienung im Kopf des Spiele-Panels (Schalter,
  x, „Auto-Hallen räumen"); Hand-Eingriff und Vorbereitungs-Aufruf
  übersteuern die Automatik; Tages-Halle und Vorverteilung schließen sich
  aus. Neuer turniergebundener Store `auto-halls.json`.
- **⚠ Verhaltensänderung (ADR 0030):** Eine gesetzte Halle (Regel, Hand
  oder Auto) **bindet** jetzt die automatische Feldvergabe — bisher waren
  Hand-Hallen reine Anzeige. Auto-vorverteilte Spiele werden ohne
  Vorbereitungs-Aufruf vergeben. Cloud: Relay-Deploy nötig (neue
  TL-Aktionen + Seite).

## v0.9.207

- **Satzpausen enden erst mit „Weiterspielen"** (Spec
  `spielzeiten-prognose` E9, ADR
  [0028](adr/0028-pause-haelt-bis-weiterspielen.md)): Das Tablet beendet
  die BWF-Pausen (60 s/120 s) nicht mehr automatisch bei 0 — nach Ablauf
  zählt das Overlay rot hoch („überzogen +0:37"), bis der Schiedsrichter
  weiterspielt; auch Reload/Übernahme behalten die Pause. Diagnose-Log
  meldet `break_overrun`.
- **Pausen in der TL-Sicht**: Die Feldkachel zeigt den Pausen-Countdown,
  nach Ablauf rot „überzogen +m:ss"; Behandlungspausen erscheinen erstmals
  („Behandlung seit …" — sie fielen bisher beim Parse heraus). Cloud-Seiten
  brauchen den Relay-Deploy (tablet.html + tl.html einkompiliert).

## v0.9.206

- **Startzeit-Prognose in TL-Web** (Spec
  [features/spielzeiten-prognose.md](features/spielzeiten-prognose.md),
  Bedienung [spielzeiten-prognose.md](spielzeiten-prognose.md)): Jedes
  wartende Spiel zeigt den voraussichtlichen Aufruf („🕐 14:32", „~" =
  noch ohne Messwerte, „gleich" = als Nächstes dran) — simuliert aus den
  Median-Spielzeiten je Klasse × Disziplin, Feldern, Reihenfolge,
  Hallen-Regeln, Spieler-Mindestpausen und 2 min Übergangspuffer. Neues
  Panel **„Spielzeiten"** mit Brutto/Netto/Anlaufzeit-Medianen je Gruppe;
  beendete Spiele tragen ihre Ist-Zeiten. Neuer SetupWizard-Abschnitt
  „Startzeit-Prognose" (an/aus, Startwert 25 min). Die Prognose bewegt die
  TL-Revision nicht (zeitabgeleitet); Cloud-Seiten brauchen den
  Relay-Deploy.
- **Spielzeiten-Messung, Etappe A** (Spec
  [features/spielzeiten-prognose.md](features/spielzeiten-prognose.md),
  ADR [0027](adr/0027-spielzeit-stempel-hostseitig.md)): Der Host misst je
  Match Bruttostart (erste Feldzuweisung), Nettostart (erster Punkt) und
  Spielende — persistiert turniergebunden in `match-times.json`
  (ADR-0022-Muster), immun gegen Feldwechsel und App-Neustart, Reset nur
  bei bestätigter Feldabnahme durch BTP (3 Polls).
- **BTP-`Duration` neustartfest und auf allen Pfaden.** Die Spieldauer
  kommt jetzt aus dem Zeiten-Store: Ein Tablet-Ergebnis nach App-Neustart
  mitten im Spiel sendet nicht mehr 0; das manuelle Backend-Ergebnis, die
  Disqualifikation und die TL-Web-Wertung senden erstmals eine echte
  Dauer. Walkover bleibt bewusst `Duration: 0` (kampflos wurde nicht
  gespielt). Details: [btp_protocol.md](btp_protocol.md).

## v0.9.204

- **Spielliste vereinfacht: hallenübergreifende Reihenfolge, eine Liste**
  (ADR [0026](adr/0026-spielliste-eine-globale-reihenfolge-eine-liste.md),
  löst [ADR 0023](adr/0023-manuelle-spielreihenfolge-praefix-je-halle.md)
  in der Hallen-Frage ab). Die manuelle Spielreihenfolge galt bisher **je
  Halle getrennt** — die Warteliste zerschnitt zusätzlich in
  Hallen-Gruppen und in vier Unterabschnitte („In Vorbereitung gerufen",
  „Spielbereit", „Noch nicht bereit", „Ohne Hallenzuordnung"). Beides war
  im Betrieb unpraktisch: Die Turnierleitung denkt die Spiele als eine
  Abfolge, nicht als Blöcke. Jetzt gibt es **eine globale Reihenfolge**
  und **ein Panel „Spiele"** (alle angesetzten Spiele, gerufene oben
  angepinnt, Rest nach manueller bzw. BTP-Reihenfolge); der bisherige
  Status steht als Abzeichen an der Zeile, die Halle als Kürzel. Alle
  Spiele werden gezeigt, mit Nachladen beim Scrollen. Die Feldkacheln
  bleiben nach Halle gruppiert — das bildet weiterhin die physische
  Realität ab.
- **Panels zuklappbar.** Zusätzlich zum Aus-/Einblenden lässt sich jedes
  Panel auf seine Kopfzeile reduzieren; „Schiedsrichter" und „Spiele"
  zeigen dabei den jeweils nächsten Eintrag im Kopf.
- **Mehrspalten-Layout.** 1 bis 3 Spalten je Profil, jedes Panel bekommt
  seine Spalte zugeordnet, Spaltenbreiten sind ziehbar. „Felder" ist damit
  ein Panel wie jedes andere; die bisherige Einstellung „Spielliste
  rechts/darunter" geht als Zwei-Spalten-Preset darin auf. Unter dem
  Tablet-Breakpoint werden Spalten immer gestapelt, unabhängig vom Profil.
- **Feldkachel und Spielzeile entschlackt.** Neues ⋯-Menü an der
  Feldkachel (einteilen, ansagen, Aufruf wiederholen, 2. Aufruf Partei
  A/B, Ergebnis eintragen); das ⋮-Menü der Spielzeile bekam „Nach oben
  schieben" und „Ergebnis eintragen" dazu.
- **Zweiter Aufruf am Feld — jetzt auch je Partei.** Bisher gab es am
  Feld nur einen ungeteilten Nachruf; jetzt lässt sich gezielt „2. Aufruf
  für Partei A" oder „für Partei B" auslösen (Roadmap-Wunsch vom
  17.07.2026 damit erledigt). Die Aufruf-Stufe zählt weiterhin einmal je
  Feld — ruft man nacheinander beide Parteien, zählt das als eine Runde.

## v0.9.203

- **Turnierleitungs-Oberfläche: Panels und Profile** (Spec
  [tl-web-panelsystem](features/tl-web-panelsystem.md), ADR
  [0024](adr/0024-tl-panel-profile-verwaltung-im-web.md) ·
  [0025](adr/0025-tl-panel-profile-transport-persistenz.md)). Die Seite war
  durch die vielen Einzelfeatures der letzten Wochen unübersichtlich
  geworden: neun fest verdrahtete Abschnitte, die sich nur zuklappen, aber
  nie wirklich abschalten ließen, fünf gewachsene Abzeichen-Farbfamilien
  und Wartelisten-Zeilen mit bis zu zehn Bedienelementen. Jetzt ist jeder
  Abschnitt ein **Panel** mit einheitlicher Kopfzeile — einzeln dauerhaft
  aus-/einblendbar, per eigenem Griff umsortierbar und über Trennstege in
  der Höhe frei verteilbar. Was davon wie aussieht, steckt in **benannten
  Profilen** („Tablet", „Wandmonitor"), die serverseitig liegen, an der
  Geräte-Identität hängen (also ein Neuladen und den Wechsel zwischen
  Hallennetz und Cloud überstehen) und turnierübergreifend erhalten
  bleiben. Sie ersetzen das alte, auf `localStorage` verstreute
  „Anzeige"-Klappmenü — ein Konfigurationsort statt drei. Dazu ein
  vereinheitlichtes **Abzeichen-System mit drei Dringlichkeitsstufen**
  (Info/Warnung/Alarm; „Matchball" bleibt als reine Info optisch klar vom
  Alarm-Rot „überfällig" getrennt) und **entschlackte Wartelisten-Zeilen**
  — Nachruf, Auto-Vergabe-Umschalter und Hallen-Wähler liegen jetzt hinter
  einem ⋮-Menü. Die Profil-Verwaltung läuft bewusst direkt in der
  Weboberfläche (zweite benannte Ausnahme von „kein Setup aus dem Web",
  begründet in ADR 0024) — reine Darstellungs-Präferenzen ohne
  Sicherheitsbezug. Bestehende Installationen aktualisieren ohne
  Migration: Ohne angelegtes Profil läuft die Seite auf einem eingebauten
  Standard mit den bisherigen Voreinstellungen.
- **Datenverlust-Pfad in der Konfiguration geschlossen** (im Zuge des
  Panelsystems gefunden). Bis hierher schrieb ausschließlich der
  Setup-Assistent die `config.json`; mit den Profilen kam ein zweiter
  Schreibweg aus der Weboberfläche dazu. Beide arbeiten jetzt auf
  **einem** gemeinsamen Konfigurationsstand — sonst hätte ein späteres
  Speichern im Setup-Assistenten die zwischenzeitlich in der
  Weboberfläche angelegten Profile kommentarlos überschrieben.

## v0.9.202

- **Schiedsrichter-Rückschreib-Race nach frischer Feldzuweisung behoben**
  (Live-Befund 14.08.2026, Zwei-Hallen-Turnier). Ein eigenständiges
  Schiedsrichter-`SENDUPDATE` ohne `CourtID`-Feld ließ BTP, wenn es kurz
  nach einer Feldzuweisung auf dasselbe Match traf, die eben erst
  angekommene Feldzuweisung wieder verlieren — zwei `SENDUPDATE`s zum
  selben Match in enger Folge brachten BTPs eigene Persistenz
  durcheinander. Ein erster Fix (feste 10-Sekunden-Karenzzeit) reichte
  nicht — der reale Abstand zwischen Feldzuweisung und Schiedsrichter-Write
  lag am Turnier teils bei 11–18 Sekunden. Der eigentliche Fix: Der
  Schiedsrichter-Abgleich schreibt jetzt bei **jedem** Write die aktuell
  bekannte `CourtID` mit — dann ist die Reihenfolge zweier Requests zum
  selben Match folgenlos, unabhängig vom zeitlichen Abstand. Dabei
  zusätzlich behoben: Werden mehrere Felder im selben Zyklus fertig,
  rücken ihre Schiedsrichter jetzt deterministisch nach Feldnummer
  sortiert ans Ende der Rotation statt in zufälliger Reihenfolge.
- **Schiedsrichter-Besetzung ging bei jedem Spielabschluss verloren,
  behoben** (Live-Befund 14.08.2026, Fortsetzung). Das Ergebnis-`SENDUPDATE`
  trug `Official1ID`/`Official2ID` gar nicht — BTP hat die Besetzung eines
  Matches dadurch bei jedem Ergebnis-Eintrag gelöscht, egal wie alt die
  Zuweisung war. Das erklärte drei Symptome auf einmal: Rotation rückte
  niemanden ans Ende, der Einsatz-Zähler zählte nicht hoch, beendete Spiele
  zeigten keine Schiedsrichter. Jeder Ergebnis-Schreibweg (Tablet, Desktop,
  TL-Web, Disqualifikation, Walkover, Nachschub-Queue) reassertiert jetzt
  die aktuell bekannte Besetzung im selben Request.
- **Spiele von der automatischen Feldvergabe ausnehmen** (Spec
  [feldvergabe-ausnahme](features/feldvergabe-ausnahme.md)). Die
  Turnierleitung kann ein einzelnes Spiel per Knopfdruck — in TL-Web und
  am Turnier-PC — temporär von der automatischen Feldvergabe ausnehmen,
  bis es manuell reaktiviert wird oder das Match endet. Manuelles
  Zuweisen bleibt für ein ausgenommenes Spiel jederzeit möglich; ein
  Badge markiert die Zeile in beiden Oberflächen. Der Zustand ist
  turniergebunden persistiert (eigene Datei, Muster ADR 0022) und
  überlebt damit einen Neustart des Turnier-PCs.

## v0.9.201

- **Beim Start werden nicht mehr alle laufenden Spiele angesagt.** Die
  ersten Abrufe der Feld-Ansage kommen, bevor der Sync-Lauf seinen ersten
  BTP-Schnappschuss hat — sie liefern **null Felder**. Dieser leere Stand
  galt als Baseline, und beim nächsten Abruf war damit jedes belegte Feld
  „frisch aufgerufen": Wer die App mitten im Turnier startete, hörte alle
  laufenden Spiele am Stück. Die Baseline gilt jetzt erst als gesetzt, wenn
  überhaupt Felder dabei sind; Felder **ohne** Spiel bleiben ein gültiger
  Anfangsstand, damit der erste Aufruf des Turniertags weiterhin angesagt
  wird. (Der Fall „Ansage-Halle im Betrieb umschalten" hat dieselbe
  Fehlerklasse und ist bewusst unverändert — siehe
  [announcements.md](announcements.md).)
- **Schiedsrichtermanagement** (Spec
  [schiedsrichter-management](features/schiedsrichter-management.md),
  ADR 0021/0022) — standardmäßig **aus**; Turniere ohne Schiedsrichter
  verhalten sich unverändert. Eingeschaltet unter Einstellungen →
  Schiedsrichter:
  - **Liste aus BTP.** Der `Officials`-Container wird gelesen; BTS Light
    pflegt nur die Zusatzdaten, die BTP nicht kennt — Rotationsreihenfolge,
    Pausen, Stammverein (BTP überträgt keinen), Sperrlisten und die
    feldweisen Schalter. Alles turniergebunden in
    `officials-state.json`, beim Turnierwechsel verworfen.
  - **Einteilung je Spiel** aus dem Client (neuer Menüpunkt
    „Schiedsrichter") und aus TL-Web, jederzeit änderbar, auch mitten im
    Spiel. Eine Zuweisung mit Konflikt (eigener Verein, gesperrter Verein,
    gesperrter Spieler) wird **ausgeführt** und nur gekennzeichnet — die
    Turnierleitung entscheidet.
  - **Automatische Rotation**, getrennt für Schiedsrichter und
    Aufschlagrichter und je Feld abschaltbar: Ein neu belegtes Feld bekommt
    den nächsten nicht pausierten, dienstfreien, konfliktfreien Official;
    nach dem Spiel rückt er ans Ende. Von Hand gelöste Zuweisungen füllt die
    Rotation **nicht** wieder auf.
  - **Feldweise Tabletbediener-Vergabe:** Auf Feldern, die der
    Schiedsrichter selbst bedient, verbraucht das Feld keinen Wartenden aus
    der Zähltafel-Schlange mehr (Default unverändert „alle Felder aktiv").
  - **Anzeige** am Schiri-Tablet (LAN, Cloud und ferne Halle), in der
    Spielübersicht und in TL-Web; **Ansage** von Schiedsrichter und
    Aufschlagrichter am Ende der Feld-Ansage, plus manueller Knopf für
    nachträgliche Zuweisungen.
  - **Rücksync nach BTP** bei jeder Änderung (eigenständiges Match-Update,
    Löschen als `0`, ohne `Status`-Feld) und zusätzlich eingebettet in das
    Zuweisungs-Update beim Ruf aufs Feld. Ein fehlgeschlagener Write wird im
    nächsten Sync-Zyklus wiederholt.
  - **Datenschutz:** Sperrlisten und Stammverein stehen nie im Zustand, den
    alle Turnierleitungs-Geräte bekommen — sie kommen nur auf gezielte, per
    Geräte-Token authentifizierte Anfrage. Zwei Wächter-Tests halten das
    fest.
  - **Cloud-Hinweis:** Die Schiedsrichter-Bedienung in TL-Web funktioniert
    über den Cloud-Weg erst nach dem Relay-Deploy; im Hallennetz sofort.

## v0.9.200

> **Nie veröffentlicht** — der Tag wurde nicht gesetzt. Die Änderungen sind
> in `main` und gehen mit v0.9.201 gemeinsam an die Installationen.

- **Cloud-Monitore und Cloud-Übersicht zeigen jetzt auch die Stände von
  LAN-Tablets.** Der Relay kannte Punktestand und Spielzustand bisher nur
  von Tablets, die selbst über die Cloud zählen — im `LanAndCloud`-
  Mischbetrieb (Tablets im Hallen-LAN, Anzeigen über badhub.de) blieben
  Cloud-Monitor, Court-Anzeige und Feld-Übersicht deshalb auf 0:0 stehen
  (Turnier-Befund 13.08.2026, Zwei-Hallen-Turnier). Der Host spiegelt jetzt
  jeden Feld-Stand (Satzliste + Tablet-Spielzustand mit Aufschlag/Pause) als
  `HostFrame::ScoreUpdate` an den Relay — nudge-getrieben plus 2-s-Sweep
  nach dem Zuweisungs-Push (fängt Relay-Neustarts, Court-Wechsel und
  BTP-Handeingaben ein), dedupliziert per Fingerabdruck. Der Relay übernimmt
  den Spiegel mit Stale-Schutz; leere Sätze überschreiben keinen Live-Stand
  eines zählenden Cloud-Tablets. Nebeneffekt: Ein Ersatz-Cloud-Tablet
  bekommt beim Verbinden den echten Stand statt 0:0 restauriert. Relay-Teil
  greift mit dem automatischen Relay-Deploy sofort; die App braucht dieses
  Update. Doku: `docs/court-monitor.md` („Score-Spiegel des Hosts").
- **Schiedsrichter: die ersten Schalter im Setup.** Der neue Abschnitt
  „Schiedsrichter" hat den globalen Schalter „Mit Schiedsrichtern spielen"
  und die beiden Rotations-Schalter (SR/AR). **Standardmäßig aus** — wer
  ohne Schiedsrichter arbeitet, merkt nichts. Die eigentliche Bedienung
  (Zuweisung je Spiel, Reihenfolge, Sperrlisten, Ansage) kommt mit den
  nächsten Schritten; der Hinweis im Abschnitt sagt das auch. Grundlage
  ist der BTP-Parser für die Schiedsrichterliste und die SR-/AR-Felder am
  Spiel. Spec `docs/features/schiedsrichter-management.md`, ADR 0021/0022.

## v0.9.199

- **Tote Cloud-Verbindungen werden schneller erkannt (Cluster-Hebel D).** Zwei
  passive Strecken fielen bisher erst spät auf. Ein **totes Tablet** belegte
  seinen Court-Slot bis zum ~30-s-Ping — jetzt pingt der Relay jedes Tablet
  alle 5 s und gibt den Slot bei 15 s Stille frei (wie beim Host). Und ein
  **half-open Master** (Netz weg ohne sauberes Trennen) reconnectet jetzt in
  ~15 s statt erst nach dem OS-TCP-Timeout (Minuten). Der Browser antwortet auf
  Protokoll-Ebene, also wird kein lebendes — auch kein in den Hintergrund
  gelegtes — Tablet fälschlich gedroppt, und ein kurzer WLAN-Hänger (< 15 s)
  ebenso wenig. Kein Bedienunterschied; nur schnelleres Aufräumen toter
  Verbindungen. Spec `docs/features/tote-verbindungen.md`, ADR 0020.

## v0.9.198

- **Ergebnisse gehen auch bei Cloud-Aussetzern und Neustarts nicht verloren.**
  Der Ergebnis-Weg ist jetzt rundum abgesichert (Cluster-Hebel B): Ein bereits
  erfolgreich übermitteltes Ergebnis, dessen Bestätigung im Netz verloren ging,
  ließ das Tablet früher **endlos** wiederholen (und konnte doppelt schreiben) —
  jetzt quittiert der Server einen solchen Wiederholungsversuch idempotent, das
  Tablet ist fertig. Ein hängender Sende-Versuch bricht nach ~12 s ab und
  wiederholt sofort (statt bis 20 s zu warten; der Relay wartet nur noch 8 s).
  Und die host-seitige Nachschub-Warteschlange liegt nun **auf Platte** — ein
  Neustart des Turnier-PCs verliert kein noch offenes Ergebnis mehr. Kein
  sichtbarer Bedienunterschied; nur zuverlässiger. Spec
  `docs/features/ergebnis-puffer.md`, ADR 0018.

## v0.9.197

- **Das zählende Tablet bleibt bei Verbindungsabrissen die Wahrheit.** Bei
  störanfälligem WLAN (viele Fremdgeräte in der Halle) kann ein Tablet kurz die
  Verbindung verlieren. Kehrt es im selben Spiel zurück und hat **niemand
  übernommen**, setzt es jetzt seinen lokalen Stand zuverlässig durch — statt
  ihn womöglich mit einem veralteten Server-Stand zu überschreiben. Hat dagegen
  ein **anderes** Tablet übernommen und weitergezählt, tritt das zurückkehrende
  Gerät zurück (kein Überschreiben). Und wurde das Spiel in der Zwischenzeit
  **per Hand fertig eingegeben**, überbügelt das Tablet dieses Ergebnis nicht
  mehr. Die Entscheidung trifft jetzt der Server anhand des „Feld-Halters"
  (statt eines geräte-lokalen Zählers) — deterministisch und im LAN wie Cloud.
  Zweiter Baustein des Robustheits-Pakets (Spec `docs/features/turnier-robustheit.md`,
  ADR 0017). Im Notfall per Einstellung `reconnect_legacy_rev` zur Laufzeit aufs
  alte Verhalten zurückschaltbar.

## v0.9.196

- **Spielstand erscheint auf dem TV nahezu sofort.** Court-Monitor und
  Feld-Übersicht bekommen den neuen Stand jetzt per WebSocket-„Nudge"
  angestoßen, statt bis zu 1 s (Monitor) bzw. 5 s (Übersicht) auf den nächsten
  Poll zu warten. Der Nudge trägt nur „Feld X geändert" — die Anzeige holt sich
  wie bisher den vollen Stand, es gibt also kein Flackern. Reißt die Verbindung
  ab (WLAN-Wechsel), übernimmt sofort ein schnelles Poll (250 ms) und der Push
  verbindet sich im Hintergrund neu — kein Stillstand. Gilt für LAN und Cloud
  (beide Hallen). Erster Baustein des Robustheits-Pakets „Echtzeit-Score-Strecke"
  (Spec `docs/features/turnier-robustheit.md`, ADR 0016).

## v0.9.195

- **Turnierlogo für den Check-In sparsamer übertragen.** Das Logo reist jetzt
  **einmalig beim Speichern** an den badhub-Check-In (über denselben
  `/api/checkin-branding`-Weg wie die Sponsoren) — statt es bisher alle 60 s
  base64 im Liveticker-Heartbeat mitzuschicken. Ändert der Operator das Logo,
  wird es einmal gesendet; sonst nichts. Sponsoren und Logo lösen unabhängig aus
  (das Häkchen die Sponsoren, das Speichern das Logo), sodass nicht bei jedem
  Sponsor-Toggle das ganze Logo mitreist. *Der eigentliche Verzicht auf das Logo
  im Heartbeat folgt in einem späteren Schritt, sobald die badhub-Version
  ausgerollt ist — bis dahin ändert sich für die Anzeige nichts.*

## v0.9.194

- **Sponsor-Werbebilder erscheinen auf den badhub-Check-In-Seiten.** Markiert
  man ein Werbebild mit dem Haken „Leiste", schiebt bts-light es jetzt auch an
  den Hallen-Check-In auf badhub (`POST /api/checkin-branding`, Bearer =
  Liveticker-Passwort — kein zweiter Zugang). So steht der Sponsor nicht nur in
  den Hallen-Anzeigen, sondern auch auf der Check-In-/Zeitplan-Seite der Spieler.
  Übertragen werden bis zu 4 Bilder als Base64, **nur beim Umschalten des
  Häkchens** (kein Heartbeat). Rein additiv: ohne badhub-Passwort passiert
  nichts, und solange badhub den Endpunkt noch nicht kennt, wird still nichts
  gesendet (kein Fehler). *Hinweis: sichtbar wird das erst, sobald die
  zugehörige badhub-Version ausgerollt ist.*

## v0.9.193

- **Cloud-Monitore: „Court-Übersicht" funktioniert jetzt.** Als letzter Schritt
  der Cloud-Info-Reihe (nach „In Vorbereitung" v0.9.191 und „Werbung" v0.9.192)
  liefert der Relay nun auch die **Feld-Übersicht** im Cloud aus: `overview.html`
  ist BASE-fähig, der Relay serviert `/{ns}/info/overview` und ihre Daten über
  `/{ns}/health` — denselben Endpunkt, den die Seite ohnehin pollt (LAN und
  Cloud teilen die Vertragsform). Weist man einem Cloud-Monitor „Court-Übersicht"
  zu, zeigt er jetzt alle Felder mit Spiel, Paarung, Satzstand und Aufruf-Uhr.
  Bewusst **weggelassen im Cloud**, weil im Relay nicht vorhanden: Aufschlag-
  Highlight sowie Verletzungs-/TL-Badges. **Siegerehrung** bleibt LAN-only (dem
  Relay fehlen die Ergebnisdaten). Damit ist die Cloud-Info-Monitor-Reihe
  abgeschlossen.

## v0.9.192

- **Cloud-Monitore: „Werbung" (Rotation) funktioniert jetzt.** Nach „In
  Vorbereitung" (v0.9.191) liefert der Relay nun auch die **Werbe-Rotation**
  im Cloud aus: `ad.html` ist BASE-fähig, der Relay serviert `/{ns}/info/ad`
  und die volle Bilderliste über `/{ns}/info/ad/state` (Indizes). Weist man
  einem Cloud-Monitor „Werbung" zu, rotiert er die hinterlegten Werbebilder.
  (Werbe-**Einzelbild** ist dateinamenbasiert und bleibt LAN-only; **Court-
  Übersicht** folgt, **Siegerehrung** bleibt LAN-only.)

## v0.9.191

- **Cloud-Monitore: „In Vorbereitung" funktioniert jetzt (+ Fundament für die
  übrigen Info-Sichten).** Wies man einem Court-Monitor im Cloud-Modus eine
  Nicht-Court-Sicht zu, blieb er auf dem bts-light-Logo hängen — der Relay
  transportierte nur die Feld-Zuweisung (CourtID) und verwarf alle anderen
  Ziele; die Info-Seiten waren zudem LAN-gebaut (absolute Pfade). Neu: das
  **vollständige Ziel** reist zum Relay (`MonitorControl.targets`), und die
  Seiten lösen ihren Basis-Pfad selbst auf (LAN wie Cloud). **„In Vorbereitung"**
  liefert der Relay jetzt komplett aus (Seite + Daten) und leitet dorthin um.
  **Werbung**, **Court-Übersicht** und **Siegerehrung** zeigen im Cloud vorerst
  weiter die Kopplungs-Seite (**nicht** 404) — der Relay leitet bewusst nur auf
  Sichten um, die er auch ausliefert. Werbung/Übersicht folgen; Siegerehrung
  bleibt LAN-only (dem Relay fehlen die Ergebnisdaten). Details:
  [features/cloud-info-monitor.md](features/cloud-info-monitor.md).

## v0.9.190

- **Sponsor-Leiste (Phase 2): auch im Cloud-Modus.** Die kleine Werbung neben
  dem Turnierlogo erscheint jetzt auch auf **Cloud-Monitoren** (im laufenden
  Spiel) und **Cloud-Tablets** (breite Geräte). Der bts-light-Host schickt die
  Leisten-Markierungen und das Turnierlogo mit dem (änderungs-gegateten)
  Monitor-Upload zum Relay; das Relay liefert sie über `/{ns}/info/logo` und
  `/{ns}/info/ad/state` aus. Feldübersicht/Vorbereitung bleiben vorerst
  LAN-only (Cloud-Info-Anzeigen sind noch nicht ausgebaut). Die badhub-Seiten
  (Check-in/Zeitplan) folgen in Phase 3.

## v0.9.189

- **Sponsor-Leiste (Phase 1): kleine Werbung neben dem Turnierlogo.** Jedes
  Werbebild kann im Setup („Court-Monitor" → Werbebild-Liste) mit dem Haken
  **„Leiste"** markiert werden. Die markierten Bilder erscheinen dann — neben
  dem **Turnierlogo** — klein in der **oberen Leiste** von **Feldübersicht**,
  **Vorbereitung**, **Court-Monitor** (nur im laufenden Spiel; der Werbe-
  Leerlauf bleibt Vollbild) und **Tablet-Spielzettel** (auf breiten Geräten).
  In der Regel 1–2 Sponsorbilder, kein Rotieren. Neuer Endpunkt `/info/logo`
  liefert das Turnierlogo an die Anzeigeseiten (bisher kannte es keine davon).
  Fehlt ein Bild/Logo, bleibt die Leiste ohne dieses Motiv (kurz cachebar,
  auch der Fehltreffer). *Cloud-Modus (Relay) und die badhub-Seiten
  (Check-in/Zeitplan) folgen in den nächsten Phasen.*

## v0.9.188

- **Turnierleitungssicht: Vereinsname/-logo je Gerät ein-/ausblendbar.** Im
  Anzeige-Menü (Kopfzeile) gibt es jetzt „Vereinsnamen zeigen" und
  „Vereinslogos zeigen" — wie bei den Nationen je Gerät und gespeichert.
  Voreinstellung ist die zentrale Setup-Wahl („Vereine anzeigen"); jedes Gerät
  darf sie für sich überstimmen. Darstellung überarbeitet: Der **Vereinsname**
  steht jetzt in einer eigenen Zeile **unter** dem Spielernamen (mit Wappen
  davor). Ist **nur das Logo** eingeschaltet (ohne Name), steht das Wappen
  kompakt **vor** dem Namen, direkt hinter der Nation.
- **Datensparsam: Logos werden gecacht.** Der Vereinslogo-Endpoint des
  Turnier-PCs (`/info/club-logo`) beantwortet jetzt auch den **Fehltreffer**
  (kein Logo) cachebar (1 h) — vorher fragte der Browser bei jedem Neuaufbau
  (Poll alle ~2 s) für jeden Verein ohne Logo erneut an. Treffer werden wie
  bisher lange gecacht (1 Tag); der Cloud-Resolver cacht ebenfalls. Zusätzlich
  merkt sich die Seite fehlende Wappen je Sitzung und lädt sie nicht neu.

## v0.9.187

- **Vereine anzeigen (optional, turnierweit).** In der Turnierleitungssicht
  und auf dem Tablet-Spielzettel lassen sich jetzt **Vereinsname** und
  **Vereinslogo** je Spieler einblenden — zwei getrennt schaltbare Optionen
  im Setup unter „Vereine anzeigen". Die Einstellung gilt **zentral für alle
  Geräte** (nicht je Gerät); Tablets übernehmen sie beim nächsten Neuladen
  bzw. mit der nächsten Paarung. Standardmäßig aus. Die Logos stammen aus dem
  badhub-Bestand (wie auf der Siegerliste); fehlt zu einem Verein ein Logo,
  bleibt es beim Namen. Im Hallennetz (LAN) holt der Turnier-PC die Logos
  (funktioniert ohne Internet am Anzeigegerät, unscharfes Namensmatching); im
  Cloud-Modus laden sie direkt über den öffentlichen badhub-Logo-Resolver
  (`/api/v1/club-logo`, exakter Vereinsname). Der Name wird in beiden Fällen
  angezeigt.

## v0.9.186

- **Check-In-Hauptfeld-Filter wirkt jetzt wirklich.** Der Filter aus
  v0.9.185 las die falsche Stelle (`Entry.StageID`, in echten Turnieren
  leer) und ließ deshalb an echten Daten alle Reservisten und
  Ausgeschlossenen auf der Liste. Er liest die Zuordnung jetzt aus dem
  `StageEntries`-Container von BTP — am laufenden Turnier bestätigt
  (z. B. HE-C: 31 → 26, die 1 Reserve und 4 Ausschließen fliegen raus).
  **Wer v0.9.185 nutzt, sollte auf diese Version aktualisieren** —
  vorher greift der Filter je nach Turnier gar nicht.
- **Court-Monitor: größere Schrift, mehr Fläche.** Namen (Einzel und
  Doppel) und Satzstand deutlich größer, die Ränder knapper — aus
  Hallendistanz auf großen TVs besser lesbar.

## v0.9.185

- **Check-In-Liste zeigt nur noch Hauptfeld-Meldungen.** Reservisten,
  Ausgeschlossene und reine Qualifikations-Teilnehmer erscheinen nicht mehr
  auf der Check-In-Seite; wer sich qualifiziert, taucht automatisch wieder
  auf. Vor der Auslosung ändert sich nichts — unplatzierte Meldungen bleiben
  vollständig. Gefiltert wird über den BTP-Stage-Typ, umbenannte Stages
  stören also nicht.

## v0.9.184

Fasst die nie einzeln veröffentlichten Stände v0.9.182/183 mit zusammen.

- **BVBB-Zugang aktualisiert.** Das Liveticker-Passwort des BVBB-Presets
  wurde badhub-seitig geändert; dieses Update bringt den neuen Zugang
  mit. **Wichtig für BVBB-Installationen:** Nach dem Update einmal die
  Einstellungen öffnen, „BVBB" erneut auswählen und speichern — die
  gespeicherte Konfiguration behält sonst das alte Passwort, und der
  Liveticker-Push bliebe abgelehnt.
- **Federball-Marker größer und realistischer.** Der Aufschlag-Marker
  auf der Tablet-Court-Grafik skaliert jetzt mit dem Bildschirm (bis
  doppelt so groß) und ist als echter Federball gezeichnet — fünf
  Federn, Bund, gewölbter Korken; aus Armlänge klar erkennbar.
- **Spiel in die andere Halle holen.** Die Turnierleitungs-Seite lässt
  beim Zuweisen und Umhängen jetzt auch Felder der anderen Halle zu —
  mit Sicherheitsabfrage („… gehört nach X — wirklich auf Feld Y in Z
  legen?") statt hartem Verbot. Manchmal muss ein Spiel bewusst
  umziehen, etwa wenn eine Halle voll läuft.

## v0.9.181

Das große Stück dieses Releases ist der **Punktverlauf-Graph**; dazu der
Federball als Aufschlag-Marker am Tablet. Cloud-Geräte (TL-Web, Tablets
über den Relay) haben beides schon über den Relay-Deploy — dieses Release
bringt es in die App und ins Hallennetz.

- **Punktverlauf-Graph pro Satz.** Zu jedem tablet-gezählten Spiel gibt es
  jetzt je Satz ein Liniendiagramm des Punktverlaufs (eine Linie je
  Partei, x = Ballwechsel, y = Punkte) — per Fingertipp/Klick als Overlay:
  am Tablet (auch offline), im TL-Web an Feldkachel und Beendet-Liste, im
  Desktop an Felderübersicht und Beendet-Tabelle. Bei laufenden Spielen
  wächst die Kurve live mit. Verläufe werden dauerhaft je Turnier
  gespeichert (ohne Spielernamen) und überstehen Neustarts, Undo,
  Tablet-Wechsel und Offline-Phasen; Zwischenstand-Einstiege, Aufgaben
  und nachträgliche Ergebnis-Korrekturen sind gekennzeichnet. Spiele ohne
  Tablet-Zählung (Papier) haben keinen Graphen — dort erscheint der
  Knopf gar nicht erst.
- **Federball statt gelbem Punkt.** Der Aufschlag-Marker auf der
  Court-Grafik des Tablets ist jetzt ein Federball, dessen Korken in
  Flugrichtung zum diagonal gegenüberliegenden Aufschlagfeld zeigt —
  der gelbe Kreis las sich wie ein Tennisball.

## v0.9.180

Nacharbeit aus dem lokalen Test des 10.08.: zwei Wünsche und zwei Befunde
der Turnierleitungs-Seite, dazu Komfort im Check-In. Der Flaggen-Fix
braucht diesen Release ausdrücklich — er liegt im Relay, das hierbei
automatisch mit deployt wird.

- **Flaggen zurück in der Cloud-TL-Sicht — und das Sekundenzucken ist weg.**
  Beides hatte dieselbe Ursache: Die Turnierleitungs-Seite hängt in der
  Cloud bewusst ohne Namespace in der Adresse und fand dort keine
  Flaggen-Route — jede Flagge lief in ein 404, der Rückfall auf das Kürzel
  ließ die Listen bei jedem Abruf sichtbar springen. Das Relay liefert die
  Länderflaggen jetzt auch namespace-los; zusätzlich merkt sich die Seite
  fehlgeschlagene Kürzel, sodass auch eine wirklich fehlende Flaggendatei
  nichts mehr springen lässt.
- **Spielliste per Trennsteg ziehbar.** Zwischen Feldern und Spielliste
  sitzt ein Griff: nebeneinander verschiebt er die Breite der Liste,
  gestapelt ihre Höhe — je Gerät und Anordnung gespeichert. Doppeltipp auf
  den Steg stellt die automatische, bedarfsgerechte Aufteilung wieder her;
  ein bloßer Tipp verstellt nichts.
- **Check-In: Links zur öffentlichen Seite.** Im Kopf des Check-In-Panels:
  „Check-In-Seite öffnen", „Link kopieren" (fürs Weitergeben per
  Messenger) und „Aushang (QR)" für den druckbaren Hallen-Aushang.
- **Check-In: Doppel stehen als Paar in einer Zeile** („Müller / Schmidt"),
  jede Hälfte mit eigenem Zustand und eigenen Knöpfen — ein- und
  ausgecheckt wird weiterhin einzeln. Meldet badhub noch keine
  Meldungs-Kennungen, bleiben die Zeilen wie bisher einzeln.

## v0.9.179

Nachmittags-Runde des 10.08.: Feinschliff der Turnierleitungs-Seite aus dem
Live-Test, dazu der Abgemeldet-Zustand im Check-In. Cloud-TL-Geräte hatten
alle Seiten-Änderungen bereits über den Relay-Deploy; dieses Release bringt
sie in die App (LAN-Weg) und liefert die App-seitigen Anteile.

- **Felder immer vollständig sichtbar.** Die Turnierleitungs-Seite rollt
  nicht mehr als Ganzes: Die Feldkacheln passen sich stufenweise an
  (kleinere Abstände, dann kleinere Schrift), nur die Spielliste rollt in
  ihrem eigenen Bereich — rechts wie im „Spielliste darunter"-Modus, der
  den Feldern jetzt bedarfsgerecht Platz gibt statt fester Prozente.
- **Ziehen funktioniert auf Tablets.** Statt Halten-und-Hoffen gibt es an
  jeder ziehbaren Zeile und Kachel einen Zieh-Griff (⠿) — dort startet das
  Ziehen sofort, Wischen daneben rollt normal. Am echten Android-Tablet
  bestätigt; der erste Anlauf über langes Drücken verlor das Wettrennen
  gegen die Scroll-Erkennung des Browsers und ist wieder ausgebaut.
- **Spielliste aufgeräumt:** Disziplin, Runde und Gruppe lassen sich im
  Anzeige-Menü einzeln ausblenden (je Gerät gespeichert).
- **Feld-Raster: Nummerierung wahlweise spaltenweise** (vertikal) — für
  Hallen, die ihre Felder spaltenweise zählen; Schlangen-Nummerierung
  wechselt dann je Spalte die Richtung.
- **Spielort: Klarheit statt stiller Grenze.** Gemessen am echten BTP:
  Die Schnittstelle verwirft Spielort-Änderungen (antwortet dabei sogar
  mit „Erfolg") — eine Rückschreibung des Hallen-Wählers ist unmöglich.
  Der Wähler sagt das jetzt selbst; der Messbefund steht im
  Protokoll-Dossier, das Messwerkzeug bleibt im Repo.
- **Check-In: Abgemeldete sichtbar.** Wer in badhub abgemeldet ist,
  erscheint in der Turnierleitungs-Sicht grau und durchgestrichen, zählt
  nicht mehr als fehlend und wird nicht mehr ausgerufen; Einchecken von
  Hand bleibt möglich und überschreibt die Abmeldung.

## v0.9.178

Fünf Ausbauten der Turnierleitungs-Oberfläche im Browser (TL-Web), alle am
Testturnier live geprüft. Cloud-Geräte sehen die neue Seite erst nach dem
Relay-Deploy.

- **Zähltafel-Warteschlange bedienen.** Die Warteschlange der
  Zähltafelbediener lässt sich jetzt auch aus dem Browser pflegen: vorziehen,
  entfernen, von Hand hinzufügen — dieselben Aktionen wie am Turnier-PC.
  Sichtbar nur, wenn die Zähltafel-Verwaltung eingeschaltet ist.
- **Beendete Spiele in der Ansicht.** Ein zugeklappter Abschnitt zeigt die
  letzten Ergebnisse, neueste zuerst; Aufgabe, kampflos und Disqualifikation
  tragen eine eigene Kennzeichnung — ein in BTP gewertetes 14:16, 15:10 sieht
  nicht mehr wie ein Fehler aus.
- **Satz- und Matchball an der Feldkachel.** Steht auf einem Feld der letzte
  Punkt eines Satzes oder des Matches an, bekommt die Kachel ein Abzeichen
  („Satzball" gelb, „Matchball" rot und pulsierend — das Feld wird gleich
  frei). Nur für die Turnierleitung, nie auf den Hallen-TVs.
- **Ergebnis-Dialog statt Freitext.** Ergebnisse werden in Satzfelder
  getippt — auch für Spiele, die nie auf einem Feld standen (jemand hat auf
  Papier abgerechnet), und als Korrektur direkt aus der Beendet-Liste, solange
  kein Folgespiel daran hängt. Die Eingabespalten sind mit den Teamnamen
  beschriftet, damit eine plausible, aber seitenvertauschte Korrektur nicht
  still den Sieger dreht; Ablehnungen des Turnier-PCs erscheinen im Dialog.
  Die Zugangs-Adresse der Seite hat jetzt einen Kopier-Knopf.
- **Felder wie in der Halle anordnen.** Je Halle lässt sich ein Raster
  hinterlegen (Spalten, Start-Ecke, Schlangen-Nummerierung) — Felderübersicht
  der App und TL-Web zeigen die Felder dann in der echten Hallen-Anordnung.
  In TL-Web zusätzlich je Gerät wählbar, ob die Spielliste rechts neben oder
  unter den Feldern steht. Details: [feld-raster](features/feld-raster.md),
  [turnierleitung-web.md](turnierleitung-web.md).

## v0.9.177

- **Check-In-Sicht für die Turnierleitung.** Der neue Punkt *Check-In* zeigt
  je Spielklasse, wer da ist und wer fehlt — schon vor der Auslosung, ohne
  jemanden am Anmeldetisch zu fragen. Spieler lassen sich von Hand auf
  anwesend setzen und wieder zurücknehmen; wer zurückgesetzt wurde, kann sich
  nicht selbst erneut eintragen, bis die Turnierleitung ihn wieder freigibt.
  Anfangszeit und Anmeldeschluss sind am Turniertag direkt hier änderbar, wenn
  ein Turnier in Verzug gerät.
- **Zwei Ansagen je Klasse, immer auf Knopfdruck.** „Noch N Minuten bis
  Anmeldeschluss" und die Namen der Fehlenden — bei vielen Fehlenden nur noch
  deren Anzahl, damit die Ansage nicht minutenlang läuft. Der Text entsteht
  aus dem aktuellen Stand, nicht aus dem, was zuletzt auf dem Bildschirm
  stand; niemand wird ausgerufen, der inzwischen eingecheckt hat.
- **Ohne Internet passiert nichts Schlimmes.** Ist badhub nicht erreichbar
  oder kennt es den Check-In noch nicht, steht dort ein ruhiger Hinweis statt
  einer Fehlermeldung — das Turnier läuft unverändert weiter. Der Punkt bleibt
  ausgegraut, solange der Check-In nicht eingerichtet ist. Details:
  [spieler-check-in.md](spieler-check-in.md).

## v0.9.176

- **Turnierleitung im Browser.** Felder vergeben, ohne am Turnier-PC zu
  stehen: Die neue Oberfläche läuft auf Tablet, Telefon oder einem zweiten
  Rechner — im Hallennetz und, wenn gewünscht, über das Internet. Mehrere
  Helfer arbeiten gleichzeitig; weist einer ein Feld zu, das ein anderer im
  selben Moment belegt, gewinnt genau einer und der andere erfährt, was dort
  jetzt steht. Spiele lassen sich per Antippen oder Ziehen auf Felder legen,
  umhängen und herunternehmen; Zeiten, Live-Spielstände, Aufrufe,
  Vorbereitungs-Aufrufe, Ergebnisse und kampflose Wertungen sind von dort
  bedienbar. Einzurichten unter *Turnierleitung*: freischalten, Namen
  eintragen, QR-Code scannen. Standardmäßig **aus** — Turniere ohne diese
  Oberfläche verhalten sich unverändert. Bedienung und Grenzen:
  [turnierleitung-web.md](turnierleitung-web.md).
- **Ansagen aus der Oberfläche.** Die Seite spricht nie selbst — sie steht
  im Zweifel im Büro. Sie beauftragt, gesprochen wird dort, wo die Anlage
  hängt: mit derselben Stimme, demselben Gong und denselben
  Aussprache-Korrekturen wie ein Aufruf vom Turnier-PC. Hört in der Zielhalle
  niemand zu, sagt die Seite das im Klartext.
- **Die Aufruf-Stufe zählt jetzt der Turnier-PC.** Zweiter und dritter
  Aufruf zeigen auf allen Geräten dieselbe Zahl — auch in der Desktop-App.
  Vorher zählte jede Oberfläche für sich.
- **Die Spielreihenfolge folgt endlich dem Turnierplan.** Zwei Fehler in der
  Auswertung des BTP-Mitschnitts hatten dafür gesorgt, dass Warteliste,
  automatische Feldvergabe **und der Liveticker** eine Reihenfolge zeigten,
  die im gedruckten Plan nirgends stand: Die angesetzte Zeit wurde nie
  gelesen (BTP liefert sie in einer Form, die der Parser nicht kannte), und
  die Ansetzungsreihenfolge innerhalb eines Zeitfensters war gar nicht
  bekannt. An einem echten Turnier gegengeprüft: Die neue Sortierung
  reproduziert die aus BTP exportierte Spielliste Position für Position. Der
  Liveticker zeigte die anstehenden Spiele zuvor sogar in umgekehrter
  Reihenfolge.
- **Ergebnisse korrigieren**, solange nichts daran hängt (Finale,
  Gruppenspiel). Sobald der Sieger im nächsten Spiel steht, wird mit
  Begründung abgelehnt: Was BTP beim Überschreiben mit dem Turnierbaum
  macht, ist noch nicht abschließend geklärt.
- **Ein Feld war nach dem ersten Ergebnis für immer blockiert.** Sobald auf
  einem Feld ein Spiel fertig wurde, stand es dauerhaft auf „wird geräumt"
  und nahm kein neues Spiel mehr an — weder von Hand noch über die
  Automatik. Ursache: Das beendete Spiel **behält** in BTP seine
  Feldangabe (Turnier-Doku „wo wurde gespielt", bewusst so seit Juli), und
  die Belegungsprüfung las das als „Feld besetzt". Sie fragt jetzt nach
  einem *laufenden* Spiel. Vor Doppelvergabe schützt weiterhin die
  Wartezeit der automatischen Vergabe.
- **Punkte zählen ohne Bremse.** Nach jedem Punkt waren die +1-Flächen drei
  Sekunden gesperrt; das war als Schutz gegen versehentliche Doppel-Taps
  gedacht und fühlte sich am Gerät zäh an. Jetzt 0,7 Sekunden — genug gegen
  den zitternden Finger, kurz genug für schnelle Ballwechsel.
- **Kein unmögliches Ergebnis mehr am Tablet.** In einem Turnier bis 15 mit
  Deckel 21 ließ sich über „Ergebnis eintragen" ein **27:25** speichern — und
  von dort ging es ungebremst nach BTP und in den Liveticker. Getippte
  Endstände werden jetzt gegen die Zählweise des Spiels geprüft, in der
  Oberfläche **und** am Turnier-PC. Bei Aufgabe, Kampflos und
  Disqualifikation bleibt der unfertige Satz erlaubt — dort bricht das Spiel
  ja mitten drin ab.
- **Der Spielort aus BTP wird gelesen.** Pflegst du im Turnierplan die
  Spalte „Spielort", steht die Halle jetzt an jedem wartenden Spiel — ohne
  dass jemand etwas eintragen muss. Damit greift auch der Hallenfilter des
  Livetickers (`display=next&halle=…`), der bislang leer blieb, sobald ein
  Turnier seine Aufrufe über BTP statt über bts-light machte.
- **Und wo BTP nichts liefert, setzt die Turnierleitung den Ort selbst.**
  Ein kleiner Hallen-Wähler an jeder Zeile der Warteliste, ohne das Spiel
  auf ein Feld zu legen; die Festlegung überlebt einen Neustart des
  Turnier-PCs. Einen Ort zu setzen gilt dabei nicht als Aufruf.
- **Flaggen in der Turnierleitung.** Ist die Nationen-Anzeige eingeschaltet,
  steht jetzt die Flagge neben dem Namen statt eines Kürzels — dieselben
  Bilder wie auf dem Court-Monitor.
- **Jedes Feld zeigt seinen Zustand als Farbe:** aufgerufen, überfällig (kein
  einziger Punkt nach der eingestellten Zeit — da muss jemand hin), im
  Spiel, beendet. Die Schwelle steht unter *Aufruf-Timer*, Standard fünf
  Minuten, und gilt für alle Geräte.
- **Besser lesbar in der Turnierleitung:** Jedes Spiel nennt seine Klasse als
  `HE-C`/`HD-D`, die Paarung steht mit dem „vs." in der Mitte und im Doppel
  die Partner untereinander, und die Felder bleiben stehen, während die
  Spielliste läuft — bei 120 wartenden Spielen war sonst kein Feld mehr zu
  sehen, auf das man etwas hätte ziehen können. Zwei Schalter je Gerät im
  Kopf: Spielnummer und Nationen ein- oder ausblenden.
- **Uhren gehen auch in ruhigen Phasen richtig.** Ändert sich am Turnier
  minutenlang nichts, antwortet der Turnier-PC nur noch mit „unverändert" —
  und dabei wurde die Zeit nicht mehr nachgezogen. Ein Tablet, dessen Uhr
  sich zwischendurch stellt (Funkzeit nach dem Aufwachen, Zeitumstellung),
  zeigte dann überall falsche Dauern, ausgerechnet dort, wo man auf „seit
  wann aufgerufen" schaut.

## v0.9.175

- **Hallen-Check-In, erste Stufe (Datenlieferung).** bts-light liest jetzt die
  **Meldeliste je Spielklasse** aus BTP und schickt sie an badhub — die
  Grundlage dafür, dass Spieler vor Beginn ihrer Klasse über eine Webseite
  selbst bestätigen können, dass sie in der Halle sind. Möglich wird das durch
  `Entry.EventID`: eine Meldung kennt ihre Klasse direkt und braucht dafür
  weder Auslosung noch Spielplan, die Liste steht also **vor der Auslosung**
  bereit. Einzurichten im Einrichtungs-Assistenten unter *Hallen-Check-In*:
  Häkchen setzen und die Adresse des Turniers bei turnier.de einfügen — die
  Kennung wird daraus automatisch herausgelesen.
- **Standardmäßig aus.** Ohne Häkchen und gültige Turnier-Kennung wird nichts
  gesendet; bestehende Installationen ändern ihr Verhalten nicht. Gesendet wird
  außerdem nur bei echter Änderung der Meldeliste (Nachmeldung, Abmeldung,
  korrigierter Name), nicht im Poll-Takt.
- **Stört den Liveticker nicht.** Der Check-In-Push läuft *nach* dem
  Liveticker-Push und mit eigenem Fehlerpfad. Kennt badhub das Feature noch
  nicht, pausiert bts-light 30 Minuten und versucht es dann erneut. Im
  Ansage-Slave-Modus wird grundsätzlich nichts gesendet — es schreibt genau ein
  Master.
- Sichtbar wird der Check-In für Spieler und Turnierleitung erst mit den
  folgenden Stufen (öffentliche Seite, Turnierleitungs-Sicht, Ansagen).
  Details: [spieler-check-in.md](spieler-check-in.md).

## v0.9.174

- **2./3. Aufruf mit Ansage aus der Spielübersicht.** Der „Aufrufen"-Knopf je
  Feld zählt bei wiederholtem Drücken hoch: 1× = normaler Aufruf, danach
  **„2. Aufruf" / „3. Aufruf"** — jeweils als vollständige Ansage (Gong, Feld,
  Disziplin, „Zweiter/Dritter und letzter Aufruf", Spieler). Die Stufe wird je
  Feld+Match geführt (neues Spiel auf dem Feld → beginnt wieder bei 1). Wirkt auf
  Standard- und Azure-Stimme. Der Knopf zeigt die nächste Stufe an.

## v0.9.173

- **Live-Spielstand wieder in der Spielübersicht.** Auf der Karte eines belegten
  Felds steht jetzt wieder der aktuelle Satzstand (inkl. laufendem Satz).
- **Aufruf-Timer verschwindet, sobald gespielt wird.** Der 1./2.-Aufruf-Timer
  zählt nur die Wartezeit, bis die Spieler ans Feld kommen — er wird jetzt
  ausgeblendet, sobald die ersten Punkte gefallen sind.
- **„Feld freigeben" nur noch als Mülleimer-Icon oben** (neben dem Schloss,
  v0.9.172): der frühere amber „Freigeben"-Knopf im Kartentext rutschte teils aus
  dem sichtbaren Bereich und ist jetzt entfernt.

## v0.9.172

- **„Feld freigeben" wieder griffbereit in der Spielübersicht.** Neben dem
  Sperren-Schloss jedes Felds gibt es jetzt ein **Papierkorb-Icon** (Tooltip
  „Feld freigeben"), das bei belegtem Feld immer sichtbar ist — auch wenn das
  Feld **gesperrt** ist (vorher steckte „Freigeben" nur im Kartentext eines
  belegten, *ungesperrten* Felds und wirkte dadurch verschwunden). Löst den
  bestehenden Sicherheits-Dialog aus; die eigentliche Freigabe (`free_court`,
  Schreibpfad nach BTP) ist unverändert.

## v0.9.171

- **Aussprache-Wörterbuch: Warm-up-Nachladen nach dem Start.** Ergänzend zum
  serverseitigen On-Load-Trigger (badhub stößt beim Turnier-Push die
  Generierung sofort an) lädt der Client das Wörterbuch nach dem Start in einem
  kurzen Warm-up (nach ~2/7/15 Min) statt erst beim nächsten 30-Min-Poll — so
  sind frisch generierte Aussprachen eines gerade geladenen Turniers binnen
  weniger Minuten am Feld. Danach normaler 30-Min-Takt.

## v0.9.170

- **Aussprache-Wörterbuch schneller aktualisiert.** Der Client lädt das geteilte
  Wörterbuch jetzt **alle 30 Min** statt alle 3 h (offline weiter aus dem Cache).
  Zusammen mit dem serverseitig auf **15 Min** verkürzten Generier-Takt und dem
  **5-Min**-Edge-Cache erreichen Auto-Aussprachen eines laufenden Turniers zeitnah
  die Ansage — statt erst Stunden später. Reine Intervall-Änderung, kein
  Verhaltenswechsel.

## v0.9.169

- **Pro-Disziplin-Stimmen werden an die ferne Halle vererbt.** Die am Master
  eingestellte Stimme je Disziplin (v0.9.168) kommt jetzt über den Relay auch
  bei einer fernen Cloud-Halle an — sie sagt dieselben Disziplinen mit denselben
  Stimmen an wie der Master. Umgesetzt entlang des bestehenden Azure-Vererbungs-
  Pfads (ADR 0003): `AzureTtsShare.discipline_voices` (Master → Relay → Slave)
  + `CloudAnnounce.azure_discipline_voices` fürs Slave-Frontend, alles serde-
  abwärtskompatibel. Betrifft die automatische Feld-Ansage der fernen Halle
  (`CloudAnnounceSlave`). **Relay-Redeploy nötig** (relay-proto geändert) — läuft
  automatisch beim Merge.

## v0.9.168

- **Azure-Stimme je Disziplin wählbar (optional).** In den Ansage-Einstellungen
  lässt sich bei aktivem Azure pro Disziplin eine eigene Stimme hinterlegen —
  z. B. Herreneinzel/-doppel mit der männlichen (Florian), Damen-Disziplinen mit
  der weiblichen Stimme (Seraphina); Mixed frei wählbar. Nichts gesetzt = die
  Standard-Stimme wie bisher (opt-in, kein Zwang). Beide Stimmen sind
  mehrsprachig → Aussprache-Korrekturen (IPA/`say`) wirken unverändert.
  Auswahl-Logik node-testbar (`src/io/disciplineVoice.mjs`). *Hinweis:* Die
  **Vererbung** dieser Pro-Disziplin-Stimmen an eine rein *geerbte* ferne
  Cloud-Halle folgt später — eine ferne Halle mit eigener Azure-Config nutzt ihre
  eigene Zuordnung, eine geerbte vorerst die eine Master-Stimme.

## v0.9.167

- **Phonetische Aussprache-Korrektur wirkt jetzt auch bei Azure.** Bislang
  griff die im Setup getippte Ersatzschreibweise (`say`, z. B. „Chybych" →
  „Chübüch") **nur** auf der Standard-Stimme; die hochwertige Azure-Stimme
  ignorierte sie und sprach den Rohnamen — bei ungewöhnlichen Namen bis zum
  Buchstabieren. Jetzt nutzt auch Azure die Ersatzschreibweise. Rangfolge je
  Name: IPA-Lautschrift (präzise) → Ersatzschreibweise → automatische
  Sprach-Erkennung. Eine einmal eingetragene Korrektur wirkt damit auf beiden
  Stimmen (und wird, wenn geteilt, an alle verteilt).

## v0.9.166

- **Bediener-Ansage auch in der fernen Halle (Cloud).** Ist die
  Zähltafelbediener-Verwaltung aktiv, sagt jetzt auch die **ferne Halle** über
  die Cloud „Tabletbedienung: {Name}" mit an — der zugewiesene Bediener wird
  dafür über den Relay mitgeschickt. Bisher nannte ihn nur der Master-PC.

## v0.9.165

- **Ansage „Tabletbedienung: {Name}" (Zähltafelbediener, Phase 1 Scheibe 3).**
  Ist die Zähltafelbediener-Verwaltung aktiv, nennt die Feld-Ansage am Ende den
  zugewiesenen Bediener — genau wie im Original-BTS. Gilt für die automatische
  Feld-Ansage und den manuellen „Ansagen"-Knopf, für Standard- und
  Azure-Stimme. Der reine pro-Feld-Hinweis (ohne aktivierte Verwaltung) wird
  weiterhin nicht angesagt.

## v0.9.164

- **Zähltafelbediener werden beim Feld-Aufruf zugewiesen (Phase 1, Scheibe 2).**
  Ist die Verwaltung aktiv, bekommt jedes belegte Feld automatisch einen
  Bediener aus der Warteschlange — **bevorzugt jemand, der zuletzt auf genau
  diesem Feld gespielt hat** (also der Verlierer des Vorspiels), sonst der
  Älteste in der Schlange. Der zugewiesene Bediener steht je Feld in der
  Spielübersicht („Bediener: …"); wird das Feld frei oder wechselt das Spiel,
  wird die Zuweisung wieder gelöst. (Ansage „Tabletbedienung: …" folgt als
  nächster Schritt.)

## v0.9.163

- **Zähltafelbediener-Warteschlange (wie im Original-BTS), Phase 1.** Neu unter
  Einstellungen → „Zähltafelbediener": Ist die Verwaltung aktiviert, kommt der
  **Verlierer** eines regulär beendeten Spiels automatisch in eine
  Warteschlange — er ist als nächster Zähltafelbediener dran. In der
  Spielübersicht zeigt „Nächste Zähltafelbediener" die Reihenfolge; du kannst
  vorziehen, entfernen und manuell jemanden hinzufügen. Walkover/Aufgabe
  erzeugen keinen Eintrag. (Automatische Zuweisung ans Feld, Ansage und
  BTP-Auscheck folgen in weiteren Schritten — siehe
  [docs/zaehltafelbediener.md](docs/zaehltafelbediener.md).)

## v0.9.162

- **Warnung, wenn eine ferne Halle offline geht.** Bisher meldete der Master
  nur, wenn sich eine ferne Halle **verbindet** (grüner Hinweis). Jetzt
  erscheint auch ein **amberfarbener Warn-Banner, wenn eine zuvor verbundene
  ferne Halle wegbricht** (z. B. nach einem PC-Wechsel oder Netzausfall) — er
  bleibt stehen, bis die Halle zurück ist oder man ihn wegklickt. So fällt ein
  Wegbrechen sofort auf, statt still zu passieren (Teil der Geräte-Übersicht
  aus [ADR 0006](adr/0006-master-identitaet-umziehen.md)).

## v0.9.161

- **Master-Identität auf einen neuen PC umziehen.** Muss der Turnier-PC
  gewechselt werden, lässt sich die Identität jetzt mitnehmen: unter Wartung →
  „Master-Identität umziehen" am alten PC **exportieren** (eine Datei mit der
  Kopplungs-Kennung + allen Einstellungen, ohne Passwörter), am neuen PC
  **importieren**. Danach bleiben alle gekoppelten Geräte — Tablets, Monitore,
  ferne Hallen — ohne Neu-Koppeln verbunden. Die Datei enthält den
  Kopplungs-Token und ist wie ein Passwort zu behandeln; es darf immer nur ein
  Master gleichzeitig laufen (Import fragt zur Sicherheit nach). Grundlage:
  [ADR 0006](adr/0006-master-identitaet-umziehen.md).

## v0.9.160

- **„In Vorbereitung"-Aufrufe erscheinen jetzt auch in BTP.** Ruft die
  Turnierleitung ein Spiel in die Vorbereitung, setzt bts-light das
  `Highlight`-Feld des Matches in BTP (wie das Original-BTS) — die Aufrufe
  sind so direkt im BTP-Planer sichtbar. Beim Ruf aufs Feld, bei Rücknahme
  oder Spielende wird die Markierung automatisch wieder entfernt. Der Abgleich
  läuft im Hintergrund und schreibt nur, wenn sich etwas geändert hat.

## v0.9.159

- **Disqualifikation aus der Turnierleitung.** Im „Ergebnis eintragen"-Dialog
  (Felderübersicht) gibt es jetzt einen Abschnitt „Disqualifikation": ein Team
  disqualifizieren, der Gegner gewinnt (BTP-ScoreStatus 3). Ein oben
  eingetippter Zwischenstand bleibt erhalten – anders als beim regulären
  Eintrag muss der Satz nicht zu Ende gespielt sein. Steht das Spiel noch auf
  einem Feld, wird es dabei freigegeben. Nutzt denselben abgesicherten
  BTP-Schreibweg (mit automatischem Nachreichen bei Verbindungsproblemen) wie
  die normale Ergebnis-Eingabe.

## v0.9.158

- **Pausenuhr verdeckt den Spielstand nicht mehr (TV).** Die Split-Flap-
  Pausenuhr (60-Sekunden-Pause, Satzpause, Behandlung) lag bisher vollflächig
  über der Anzeige. Jetzt ist sie ein kompakter, halbtransparenter **Banner
  am oberen Rand** – Satzstand und Namen bleiben darunter sichtbar.
- **TV im Leerlauf zeigt badhub.de.** Steht auf einem Feld kein Spiel, zeigt
  der Monitor weiterhin groß die Feldnummer und darunter jetzt prominent
  **badhub.de** – Orientierung in der Halle und dezente Werbung in einem.

## v0.9.157

- **Satz- und Matchball in der Felderübersicht.** Läuft an einem Feld ein
  Satz- oder Matchball, markiert die Turnierleitungs-Felderübersicht das
  betroffene Feld: gelbes Badge „Satzball", rotes pulsierendes „Matchball"
  (das Feld wird gleich frei). Das hilft beim Planen des nächsten Spiels.
  Rein informativ, nur in der bts-light-Oberfläche – die Hallen-TVs bleiben
  unverändert. Die Erkennung nutzt dasselbe Prinzip wie das Tablet
  (Führender einen Punkt vom Satzgewinn entfernt; Matchball, wenn damit der
  entscheidende Satz fällt).

## v0.9.156

- **„Zeit seit Aufruf" auf der großen Court-Übersicht (TV).** Die
  Multifeld-Anzeige (`overview.html`) zeigt jetzt je Feld, das aufgerufen
  wurde, aber noch nicht spielt, wie lange der Aufruf her ist – mit derselben
  Ampel wie die Einzelanzeige („1. Aufruf → 2. Aufruf → Letzter Aufruf", gelb/
  rot). Sobald das Spiel zählt, verschwindet der Hinweis wieder (der Satzstand
  ist dann der Status), damit das Board nicht in einer Farbwand endet. Nutzt
  den vorhandenen Aufruf-Zeitstempel; die Turnierleitung sieht so auf einen
  Blick, wo eine Partei zu lange auf sich warten lässt.

## v0.9.155

- **Gong überlappt nicht mehr das erste Wort der Ansage.** Der Aufruf-Gong
  wartet jetzt auf sein **echtes Ende** (statt auf eine feste Stoppuhr), bevor
  die Sprachausgabe einsetzt – plus eine kurze Atempause. Startet der Ton auf
  dem Ansage-Rechner minimal verzögert (kommt unter Windows vor), verschiebt
  sich der Sprechbeginn automatisch mit, statt in den Gong-Nachklang zu fallen.
  Betrifft Feld-Ansage, Freitext und die ferne-Halle-Ansage gleichermaßen.
  Das **Tablet** bleibt wie bisher ganz ohne Ton (reiner Spielzettel).

## v0.9.154

- **Zweiter und dritter Aufruf einer einzelnen Partei — jetzt auch in der
  fernen Halle.** Fehlt beim Aufruf nur eine Seite, ruft man gezielt diese
  Partei nach: „Zweiter Aufruf für …", beim nächsten Mal „Dritter und letzter
  Aufruf für …" (nur die genannten Namen, kurz gehalten). Das geht jetzt nicht
  nur an der Turnierleitung (Tab „In Vorbereitung"), sondern auch am
  **Slave-PC der fernen Halle**: dort zeigt die Ansagen-Seite unter
  „Aufgerufene Spiele" die in Vorbereitung gerufenen Spiele der eigenen Halle,
  und der Nachruf läuft **lokal auf dem Slave-Rechner** — genau da, wo die
  fehlende Partei steht. Die Aufruf-Liste schiebt der Master automatisch über
  die Cloud in die ferne Halle.

## v0.9.153

- **Ferne Halle per Telefon-Code koppeln.** Statt die lange install-ID
  abzutippen, erzeugt der Master-PC einen **8-stelligen Zahlen-Code** (im
  Setup unter „Telefon-Code"), den man der fernen Halle einfach durchsagt —
  dort eingegeben, verbindet sich der Slave automatisch mit dem Master. Der
  Code ist **1 Stunde gültig** (bequem beim Turnier-Aufbau).
- **Sprachansagen in der fernen Halle ohne eigene Azure-Einrichtung.** Ein
  Cloud-Slave übernimmt die Azure-Sprachkonfiguration automatisch vom Master
  (nur im Arbeitsspeicher); ein Hinweis-Banner zeigt, wenn stattdessen die
  Standardstimme genutzt wird.

## v0.9.152

- **Mitten im Spiel einsteigen und weiterzählen.** Findet sich erst während
  eines laufenden Spiels jemand zum Zählen, schaltet der Haken „Spiel läuft
  noch" im „Ergebnis eintragen"-Dialog den Übernahme-Modus ein: abgeschlossene
  Sätze und den aktuellen Satzstand eingeben, „Weiterzählen" führt durch die
  Aufstellung (Seite, Aufschläger, im Doppel Annehmer) — danach zählt das
  Tablet ab dem eingegebenen Stand ganz normal weiter. Die Aufschlagposition
  wird regelkonform aus dem Stand abgeleitet.

## v0.9.151

- **Endstand direkt am Tablet eintragen.** Hat niemand live am Tablet gezählt,
  lässt sich das Ergebnis jetzt am Tablet selbst eingeben: der offen sichtbare
  Knopf „Ergebnis eintragen …" öffnet einen Dialog für die Satzstände. Jeder
  Satz muss regulär zu Ende gespielt sein und es muss ein eindeutiger Sieger
  herauskommen; danach erscheint das gewohnte Match-Ende-Fenster zum
  Übermitteln. (Kampflos/Aufgabe laufen weiter über „Match beenden".)

## v0.9.150

- **Schnelleres Zählen am Tablet.** Der Punkt zählt jetzt beim Berühren der
  Fläche (statt erst beim Loslassen) – die Zähltafel reagiert spürbar direkter.
  Doppel-Auslösungen (langes Drücken, zwei Finger) sind abgesichert, der
  Doppel-Tap-Schutz bleibt.
- **Ergebnis aus der Turnierleitung eintragen.** Wurde ein Spiel nicht am
  Tablet abgeschlossen (z. B. vergessen oder Verbindungsabbruch), kann die
  Turnierleitung es jetzt selbst finalisieren: in der Spielübersicht auf einem
  belegten Feld „Ergebnis" wählen, Satzstände bestätigen/eintippen – das
  Ergebnis geht nach BTP, das Feld wird freigegeben. Der aktuelle Stand ist
  vorbelegt; jeder Satz muss regulär zu Ende gespielt sein (Kampflos/Aufgabe
  laufen weiter über den Aufgabe-Dialog am Tablet).

## v0.9.149

- **Tablet-Spielzettel: helles Design und größere Schrift (Tilo-Feedback).**
  Die Zähloberfläche ist jetzt hell statt dunkel – so bleibt sie auch bei
  niedriger Display-Helligkeit gut lesbar (schont den Akku, weil niemand mehr
  hochdrehen muss) und ist ohne Lesebrille besser zu erkennen. Satzstand,
  Spielernamen und Bedientexte durchgängig größer; das Spielfeld mit dem
  Aufschläger-Punkt bleibt gut sichtbar. Die Optik orientiert sich am
  erprobten Umpire-Panel (hell, kräftige Team-Farben).
- **Feldnummer prominent, auch vor Spielstart.** Solange dem Tablet kein
  Spiel zugewiesen ist, steht die Feldnummer jetzt groß und zentral auf dem
  Bildschirm – beim Verteilen der Tablets ist damit sofort erkennbar, welches
  Gerät an welchem Feld hängt. Auch im laufenden Spiel ist die Feldnummer in
  der Kopfzeile deutlicher.

## v0.9.148

- **Stabilitäts-Release nach dem Zwei-Hallen-Turnier (Cluster A):** alle fünf
  Befunde aus der Log-Auswertung des Turnier-Wochenendes behoben.
- **BTP-Aussetzer lösen keinen Massen-Reset mehr aus.** Liefert BTP einen
  Abruf lang einen leeren Turnier-Stand (beim Turnier 2× passiert: alle Felder
  freigegeben, Ticker leer), wird er verworfen und erst nach Bestätigung im
  Folge-Abruf übernommen; das Dashboard zeigt eine orangene Warnung.
- **Altes Ergebnis erscheint nicht mehr unter neuem Spiel.** Punktestand und
  Spielzustand tragen jetzt die Match-Kennung — ein nach Funkloch/Schlafmodus
  im alten Spiel hängendes Tablet kann die Anzeige des Felds nicht mehr mit
  dem alten Stand überschreiben (in App **und** Cloud-Relay geprüft).
- **Toter Master blockiert die Cloud-Halle nicht mehr.** Nach einem
  Netzwechsel hielt eine tote Verbindung den Cloud-Zugang bis zu 17 Minuten —
  jetzt wird sie nach 15 Sekunden Stille abgelöst (Relay-seitig bereits aktiv).
- **Liegengebliebene Ergebnisse werden automatisch nachgereicht.** Schlägt das
  Schreiben eines Ergebnisses nach BTP fehl, versucht es die App alle 30 s
  erneut — mit Schutzregeln: manuell nachgetragene Ergebnisse werden nie
  überschrieben, Spieler werden nur binnen 5 Minuten nach Spielende
  ausgecheckt.
- **Betrieb:** Empfehlung „Bildschirm-Schlaf am Tablet aus" (140
  Schlaf-Reconnects an einem Turniertag) und öffentlicher DNS (1.1.1.1) für
  den Turnier-PC in der Doku; Ergebnis-Logzeilen zeigen wieder immer den
  Feldnamen.

## v0.9.147

- **BTP bekommt Feld, Spieldauer und Spielende (Tilo-Feedback 18.07.2026).** Beim
  Tablet-Ergebnis behält das Match jetzt seine **echte CourtID** (vorher `0` → die Feld-Info
  verschwand in BTP; die Freigabe übernimmt allein der Courts-Block), die **`Duration`** wird
  aus dem Aufruf-Zeitstempel in ganzen Minuten berechnet (vorher immer 0), und je Spieler wird
  das **Spielende** gesetzt: `LastTimeOnCourt` (lokale Uhrzeit) + `CheckedIn: false` — Tilos
  Original-Mechanismus, die Spieler sind damit sofort wieder einplanbar.
  Details: [btp_protocol.md](btp_protocol.md).
- **Tablet-Reconnect verliert keine Punkte mehr (Turnier-Befund 18.07.2026).** Zwei Ursachen
  behoben: (1) Jedes Tablet sendet jetzt eine **persistente Geräte-Kennung** — meldet sich
  dasselbe Gerät nach einem Netz-Aussetzer zurück, löst es seine tote Session nahtlos ab statt
  fälschlich „ein anderes Tablet hat übernommen" zu sehen (LAN + Cloud). (2) Eine
  **Stand-Revision** im gespeicherten Spielstand sorgt beim Reconnect für „**neuer gewinnt**":
  Der während des Aussetzers veraltete Server-Stand überbügelt nicht mehr die offline
  weitergezählten Punkte; das Tablet behält seinen Stand und der Liveticker zieht nach.
  Cloud-Tablets brauchen dafür den Relay-Deploy + einmal Seite neu laden.
  Details: [tablet.md](tablet.md), [cloud-relay.md](cloud-relay.md).

## v0.9.146

- **Ferne Halle: Tablets & TVs über die Slave-IP — ohne Extra-Rechner.** Der Cloud-Ansage-Slave
  betreibt jetzt selbst eine **Brücke** auf `:8088`: Tilos Court-Monitor-Pis finden den Slave-PC
  per Subnetz-Scan und werden auf den Court-Monitor des Masters umgeleitet (die separate
  Notbrücke vom Turnier entfällt). Zusätzlich gibt es **`Slave-IP/felder`** als Feld-Auswahlseite
  fürs Tablet — genau wie in der Master-Halle; jedes Feld öffnet die Cloud-Tablet-Seite des
  Masters. Der Slave meldet sich zudem per mDNS als `bts-light.local`. Ergebnisse fließen
  unverändert direkt über die Cloud ins Master-BTP (Weg A). Details: [multi-hall.md](multi-hall.md).

## v0.9.145

- **TV-Anzeige: Spielernamen deutlich größer.** Auf der Einzelfeld-Anzeige (Court-Monitor)
  waren die Namen aus Hallendistanz zu klein (Turnier-Feedback): Nachname jetzt 13 statt
  7 vmin (Doppel 8 statt 5.4), Vorname entsprechend; der laufende Satzstand gibt dafür
  etwas ab (13 statt 15 vmin). Erreicht Cloud-TVs mit dem Relay-Deploy, LAN-Monitore mit
  diesem Release.
- **Ansage nennt jetzt die Klasse: „Herreneinzel A".** Direkt hinter der Disziplin wird das
  Klassen-Kürzel (A, B, C, … / U15 …) mitangesagt — auf Master, Cloud-Slave und bei
  Vorbereitungs-Aufrufen. Es wird aus dem BTP-Event-Namen (auch in der Gruppenphase) bzw.
  dem Draw-Namen („HE A") extrahiert; **Gruppen-Namen werden nie angesagt**. Ohne
  erkennbares Kürzel bleibt die Ansage unverändert.
- **BTP beendet Spiele wieder automatisch (Regression seit v0.9.103).** Tablet-Ergebnisse
  kamen zwar in BTP an, aber das Spiel blieb dort offen — Sieger musste je Match manuell
  gewählt und gespeichert werden (Live-Befund Zwei-Hallen-Turnier 17.07.2026). Ursache:
  v0.9.103 hatte das `Status`-Feld nicht nur aus der Feldzuweisung (dort richtig), sondern
  versehentlich auch aus dem **Ergebnis**-`SENDUPDATE` entfernt. `Status` steht wieder im
  Ergebnis (wie im Original-BTS); zusätzlich werden Ergebnis und Feldfreigabe jetzt in
  **einem** Request geschrieben — der frühere zweite „nackte" Freigabe-Request konnte das
  Ergebnis wieder entwerten. *(Vor dem Release am echten BTP gegenprüfen: Spiel schließt
  automatisch, Feld wird frei, Spieler-Check-in bleibt bei Feldzuweisungen unangetastet.)*

## v0.9.144

- **Tablets & TVs in der fernen Halle (Weg A / Direkt-Cloud).** Ein Zwei-Hallen-Turnier,
  bei dem **beide** Hallen Tablets **und** TVs haben, aber die Turnierleitung/Feldvergabe nur in
  Halle A sitzt, geht jetzt ohne Telefon: Die Geräte der fernen Halle verbinden sich **direkt über
  die Cloud** mit dem Master; die Ergebnisse fließen zurück ins **Master-BTP**. Der Slave-PC sagt
  weiterhin nur an. Auf dem **Dashboard** des Slaves erscheint neu **„Geräte dieser Halle
  anschließen"** — zuerst die **Hallen-Auswahl** (der Cloud-Slave hat kein BTP und bekommt die
  Hallennamen jetzt über die Cloud), danach je Feld ein scannbarer **Tablet-QR** und der
  **Monitor-Link** für den TV. Die gewählte Halle steuert zugleich die Ansage.
- **Warnung „sagt alle Hallen an".** Der **Master** warnt jetzt deutlich auf dem Dashboard, wenn bei
  einem Mehr-Hallen-Turnier keine Ansage-Halle gewählt ist (sonst sagt er auch die Spiele der fernen
  Halle mit an). *(Voraussetzung Rollout: aktualisierter Relay auf badhub — neues `hall`-Feld in der
  Feldliste.)* Architektur-Entscheid + Betriebs-Voraussetzungen: [ADR 0002](adr/0002-ferne-halle-direkt-cloud-geraete.md).

## v0.9.143

- **Master/Slave-Einrichtungshilfe (zwei Hallen über Cloud).** In den Einstellungen führt ein
  Schritt-für-Schritt-Assistent durch die Cloud-Kopplung: Der **Master** zeigt seinen
  **Kopplungs-Code** (mit „Kopieren"); die **ferne Halle** schaltet **„Ansage-Slave-Modus"** ein, trägt
  den Code ein und wählt ihre Halle. Der Slave-Schalter ist **immer in den Einstellungen** verfügbar —
  eine ferne Halle hat kein BTP und kann ein Mehr-Hallen-Turnier nicht selbst erkennen.
- **„Ferne Halle online?" in der Kopfzeile.** Neben dem Internet-Status zeigt der Master jetzt, ob die
  ferne Halle (Cloud-Ansage-Slave) verbunden ist (grün/rot je Halle). Fällt dort kurz das Internet aus,
  springt die Anzeige auf rot und nach dem Reconnect automatisch zurück auf grün.

## v0.9.142

- **Mehr-Hallen über Cloud — Ansage in der fernen Halle (B1a).** Sind die Hallen **nicht im selben
  Netz** (km entfernt, getrennte LTE-Router), kann ein zweiter Rechner als **Cloud-Ansage-Slave** laufen:
  Er holt die Matches **seiner** Halle + Freitext-Ansagen über den Cloud-Relay vom Master (statt aus BTP)
  und sagt sie lokal an. Einrichtung: im **Ansage-Slave-Modus** den **Kopplungs-Code des Masters**
  eintragen (der Code steht beim Master in den Einstellungen). Leer = klassischer LAN-Slave wie bisher.
  *(Voraussetzung: aktualisierter Relay auf badhub.)*

## v0.9.141

- **Geräte-Abdeckung auf dem Dashboard.** Zwei neue Balken zeigen auf einen Blick, ob jedes Feld
  versorgt ist: **Tablets X/Y Felder** und **Monitore (TV) X/Y Felder** (Einzel- + Kombi-Anzeige;
  mit Hinweis „N in Kombi" und „M offline"). Voll = grün, unvollständig = gelb. So sieht man sofort,
  welchem Feld noch ein Tablet oder TV fehlt.

## v0.9.140

- **Manuelle Sprach-Korrektur je Name.** In der Aussprache-Tabelle lässt sich pro Eintrag die
  **Sprache erzwingen** (Auto / Deutsch / Chinesisch / Vietnamesisch / Spanisch / Französisch /
  Polnisch / Türkisch / Malaiisch / Indisch), falls die automatische Erkennung mal danebenliegt.
  „Deutsch" erzwingt den deutschen Default (kein `<lang>`). Vorrang im Azure-Pfad:
  Sprach-Override → kuratiertes IPA → automatische `<lang>`-Erkennung.

## v0.9.139

- **Mehr Sprachen nativ (Azure `<lang>`).** Die Namens-Spracherkennung deckt jetzt neben
  Chinesisch/Vietnamesisch auch **Spanisch, Französisch, Polnisch, Türkisch, Malaiisch und
  indische Namen** ab (kuratierte Namenslisten, ~2.600 Einträge, mitgeliefert). Bei aktiver
  Azure-Stimme wird jeder erkannte Name in seiner Sprache nativ gesprochen
  (`<lang xml:lang="…">`). **Mehrdeutige Namen bleiben deutsch** (kein Raten). Wenig Datenlast,
  kein Netz-Lookup. Reihenfolge weiter: kuratiertes IPA → `<lang>` → deutsch.

## v0.9.138

- **Präzise Aussprache über IPA (Azure).** Das gemeinsame Wörterbuch trägt jetzt zusätzlich IPA-Phoneme.
  Ist die hochwertige Azure-Stimme aktiv, spricht sie Namen über inline `<phoneme alphabet="ipa">` exakt
  aus (z. B. „Wang" → ˈvaŋ) — gespeist aus einem kuratierten Lexikon (Start: ~765 Namen de-DE). Der
  Offline-/Web-Speech-Pfad nutzt weiter die `say`-Lautschrift. Vorrang unverändert: eigene > Community > Basis.

## v0.9.137

- **Gemeinsames Aussprache-Wörterbuch (Community).** bts-light lädt jetzt ein zentrales, von allen
  Nutzern gepflegtes Aussprache-Wörterbuch von badhub (beim Start + alle 3 h) und **cached es offline**
  für den LAN-Hallenbetrieb. So sprechen fremdsprachige Namen über alle Turniere hinweg korrekt, ohne dass
  jeder sie selbst pflegen muss. Priorität: eigene Korrekturen > Community > mitgeliefertes Basis-Wörterbuch.
- **Eigene Korrekturen teilen (opt-in).** Schalter „Meine Korrekturen mit der Community teilen" in den
  Ansage-Einstellungen — beim Speichern werden die eigenen Einträge zur gemeinsamen Datenbank beigetragen.

## v0.9.136

- **Freitext-Gong klar unterscheidbar.** Der Gong für Freitext-/Info-Ansagen ist jetzt ein heller,
  dreitöniger perlender Dreiklang (C-Dur aufsteigend, weicher Triangle-Klang) statt nur derselben zwei
  Töne wie der Spielaufruf in umgekehrter Reihenfolge. Der **Spielaufruf** bleibt der tiefe, zweitönige
  absteigende Gong — beide sind nun auf Anhieb auseinanderzuhalten.

## v0.9.135

- **Verlauf der letzten 10 Ansagen + erneut abspielen.** Auf der Ansagen-Seite werden die zuletzt
  **manuell** ausgelösten Ansagen (Freitext + manuelle Feld-Ansage) protokolliert — jede lässt sich mit
  einem Klick **erneut abspielen**. Automatische Spielaufrufe erscheinen nicht im Verlauf.
- **Gespeicherte Ansage-Blöcke.** Wiederkehrende Ansagen (z. B. „Siegerehrung in 10 Minuten") lassen sich
  per **„Als Block speichern"** ablegen und jederzeit per Knopfdruck ansagen (Halle wählbar, Master →
  Slaves), ins Textfeld laden oder löschen.
- **Azure-Stimme aktiv → Standard-Stimme ausgeblendet.** Ist die hochwertige Azure-Stimme an, ist die
  Standard-Stimmenauswahl deaktiviert (sie hätte keinen Effekt) — der Offline-/Fehler-Fallback greift
  weiterhin automatisch.

## v0.9.134

- **Startseite ist jetzt ein Turnier-Dashboard.** Die Status-Seite zeigt oben den **Turniernamen** und
  Kennzahlen-Kacheln (**Konkurrenzen, Spieler, Spiele, Felder, Laufend, Hallen**) sowie einen
  **Fortschrittsbalken** „Abgeschlossene Spiele X/Y" — Überblick über das ganze Turnier auf einen Blick
  (sobald der Liveticker läuft).
- **Ansage-Halle direkt auf dem Dashboard.** Bei einem Mehr-Hallen-Turnier lässt sich „Dieser PC sagt an:
  alle/nur Halle X" direkt auf der Startseite umstellen — **wird sofort gespeichert**, kein Scrollen ans
  Ende der Einstellungen mehr.
- **Wartung als eigener Menüpunkt.** Update-Prüfung, Logs und Versionsanzeige sind vom Dashboard in den
  neuen Menüpunkt **„Wartung"** (unter Einstellungen) gewandert.

## v0.9.133

- **Ansage-Einstellungen jetzt auf der Seite „Ansagen".** Alle Detail-Einstellungen (Sprache, Stimmen,
  Tempo, Gong, Aussprache-Korrekturen, **Azure**, Halle) sind von den Einstellungen auf die **Ansagen**-
  Seite gewandert (Abschnitt „Ansage-Einstellungen" mit eigenem Speichern). In den **Einstellungen** gibt
  es für Ansagen nur noch den **An/Aus-Schalter**.
- **Eigener Gong für Freitext-Ansagen.** Freitext-Ansagen nutzen jetzt einen **aufsteigenden** Gong
  (statt des absteigenden Spielaufruf-Gongs) — so hört man sofort, dass es **kein Spielaufruf** ist.

## v0.9.132

- **Spielübersicht tabellarisch (BTS-Stil).** Oben die Felder, darunter zwei Tabellen: **Nicht zugewiesene
  Spiele** (per Drag&Drop oder Klick aufs Feld vergeben; mit Spalten #, Spiel = Zeit/Klasse/Runde,
  Spieler, **Halle** – die durch die Disziplin→Halle-Regel vorgegebene Halle wird angezeigt) und
  **Abgeschlossene Spiele** (Feld, #, Spiel, Spieler mit fett markiertem Sieger, Schiedsrichter, Ergebnis
  = Sätze). Neuer Befehl `finished_matches`. Die **Schiedsrichter**-Spalte zeigt vorerst den
  **Tabletbediener** (bei laufenden Feldern; je abgeschlossenem Spiel folgt der echte Schiedsrichter mit
  dem späteren Schiri-Modul).

## v0.9.131

- **Freitext-Ansage.** Auf der Seite „Ansagen" gibt es ein Textfeld: Text eintippen, Halle wählen
  (oder „alle Hallen") → wird angesagt (Gong + Stimme wie eingestellt, Azure falls aktiv). Der **Master**
  legt den Text ab; **Slaves** holen ihn vom Master und sagen ihre Halle an — so kommt eine Freitext-
  Ansage „für Halle B" auch dorthin, ohne Audio über die Leitung (nur der kurze Text).
- **Ansage-Einstellungen gebündelt.** Der Einstellungs-Abschnitt heißt jetzt „Ansagen" und ist EIN
  Modul: ein Schalter aktiviert/deaktiviert alles (auch Freitext); darunter liegen Sprache, Stimmen,
  Tempo, Gong, Halle, Aussprache-Korrekturen **und Azure**.

## v0.9.130

- **Ansage-Slave-Modus (Mehr-Hallen, Phase 2).** Neuer Schalter „Ansage-Slave-Modus": macht aus einem
  zweiten bts-light-Rechner einen reinen **Ansage-Rechner** für die andere Halle. Er liest nur BTP und
  sagt die unter „Sprachansagen" gewählte Halle **selbst** an (eigene Azure-Stimme, kein Audio über die
  Leitung) — **kein** Liveticker-Push, **keine** Auto-Feldvergabe, **kein** Tablet-Server/mDNS/Relay.
  Damit kollidiert er nicht mit dem Master. Es gibt genau **einen Master** (mit der BTP-Steuerung);
  beliebig viele Slaves dürfen mitlaufen, jeder für seine Halle. Voraussetzung: der Slave-Rechner
  erreicht den BTP-Rechner im selben Netz (LAN/WLAN).

## v0.9.129

- **Mehr-Hallen: Disziplinen je Halle (Vergabe-Constraint, Phase 1b).** Neue Einstellung „Disziplinen je
  Halle" (bei ≥2 Hallen): lege fest, in welcher Halle eine Disziplin/Klasse gespielt wird. Spiele dürfen
  dann **nur in ihre Halle** vergeben werden — **manuell wie automatisch** (Hard-Block). Zwei Ebenen:
  „Alle HE" als **Kategorie-Standard**, einzelne Auslosungen (z. B. „HE A") **überschreiben** ihn (z. B.
  HE A/B in Halle 1, HE C/D in Halle 2). Ohne Eintrag: keine Einschränkung. In der Spielübersicht werden
  nicht erlaubte Felder fürs gewählte Spiel ausgegraut; eine Vergabe dorthin wird mit Hinweis abgewiesen.

## v0.9.128

- **Mehr-Hallen: Ansagen je Halle (Phase 1).** Neue Einstellung „Ansagen nur für Halle X" (Sprachansagen).
  Ist sie gesetzt, sagt dieser PC **nur Spiele dieser Halle** an — so hört in einem 2-Hallen-Setup jede
  Halle nur ihre eigenen Ansagen (z. B. zwei eigenständige Steuer-PCs, je Halle einer). Leer = alle Hallen
  (Einzelhallen-Turniere unverändert). Sobald BTP ≥2 Hallen meldet, erscheint auf der Status-Seite eine
  **Infobox**, die direkt zur Einstellung führt. Fundament für das Ansage-Gerät (Slave) in Phase 2/3.

## v0.9.127

- **Vereinslogos auf einheitlichem weißen Chip.** Logos kommen mit sehr unterschiedlichen Hintergründen
  (oft weißes JPG, manchmal transparentes PNG) — als rohe Rechtecke auf dunklem Grund wirkte das unruhig.
  Jetzt sitzt jedes Logo auf einem einheitlichen weißen, abgerundeten, quadratischen Chip
  (`object-fit:contain`) → konsistentes, ruhiges Bild unabhängig von der Quelle.

## v0.9.126

- **Vereinslogos jetzt verbandsübergreifend.** Umgestellt vom verbands-/geogebundenen `clubfinder` auf den
  neuen öffentlichen `GET /api/v1/club-logos` (alle Landesverbände in einem Aufruf). Damit bekommen auch
  **Teilnehmer aus anderen LVs** ihr Logo — wichtig bei überregionalen Turnieren. Kein `t=`-Slug mehr
  nötig (funktioniert, sobald die badhub-URL gesetzt ist).

## v0.9.125

- **Vereinslogos: richtiger (key-freier) Badhub-Endpoint.** Der zuvor genutzte
  `/api/v1/federations/{slug}/clubs` verlangt einen API-Key (→ 401, keine Logos). Umgestellt auf den
  öffentlichen `GET /api/v1/clubfinder?fed={slug}&limit=200`, der Vereinsname + `logo_url` ohne Key
  liefert. Damit erscheinen die Logos jetzt tatsächlich.
- **Vereinslogos größer** dargestellt (≈1.8× der Vereins-Schrift), damit sie gut erkennbar sind.

## v0.9.124

- **Vereinslogos auf dem Sieger-Monitor.** Neben dem Vereinsnamen erscheint jetzt — sofern vorhanden —
  das **Vereinslogo aus Badhub**. Der Turnier-PC holt einmalig die Vereinsliste des Verbands
  (`/api/v1/federations/{slug}/clubs`, Slug aus der `live_url`), matcht den BTP-Vereinsnamen (exakt, mit
  konservativer Locker-Variante ohne Ortszusatz) und liefert das Logo über einen lokalen Endpoint
  `/info/club-logo` aus — funktioniert damit auch auf reinen LAN-TVs ohne eigenes Internet. **Gibt es
  kein Logo** (kein Treffer, Verein ohne Logo, oder offline) → es wird **gar kein Logo** angezeigt, nur
  der Name. Logos werden gecacht; Bild-Abruf ist auf die badhub-Origin beschränkt (SSRF-sicher).

## v0.9.123

- **Sieger-Monitor: Header + Footer wirklich randlos über die volle Breite.** Header- und Footer-Leiste
  bekommen zusätzlich `align-self: stretch` + `width: 100%` + `box-sizing: border-box` — damit spannen
  sie sicher über die gesamte Breite (kein zentrierter Kasten mehr), unabhängig von der Flex-Ausrichtung.

## v0.9.122

- **Einzel-Monitore: Namen einzeilig + Verein größer.** Im Einzel-Modus steht der ganze Name jetzt in
  EINER Zeile (Vorname + Nachname) und wird per `fitSolo()` über die volle Breite gezogen — nutzt den
  Platz maximal. Der Verein ist deutlich größer (5.5vmin) und etwas heller.
- **Footer + Header über die volle Breite.** Layout auf Flex-Spalte umgestellt (wie `overview.html`):
  Header oben, `main` füllt den Rest, Footer unten — alle randlos über die gesamte Breite statt als
  zentrierter Kasten.

## v0.9.121

- **Siegerehrung als eigener Menüpunkt.** Die Disziplin-Steuerung der Siegerehrung ist von „Monitore"
  in einen eigenen Menüpunkt **„Siegerehrung"** (Pokal-Icon) gewandert — übersichtlicher getrennt. Die
  TV-Zuweisung („ganzes Podium" / „nur Platz 1/2/3") bleibt unter „Monitore".
- **Einzel-Monitore nutzen die volle Breite.** Statt fixer `vmin`-Schriftgrößen skaliert `fitSolo()` die
  Namen nach dem Layout dynamisch auf ~94 % der Bildschirmbreite (kurze Namen durch die Höhe begrenzt,
  lange schrumpfen exakt auf die Breite). Damit sind die Namen auf den Einzel-TVs deutlich größer und
  besser lesbar; bei zwei dritten Plätzen wird automatisch passend herunterskaliert. Re-Fit bei
  Fenster-/Bildschirmänderung.

## v0.9.120

- **Sieger-Einzel-TVs: bessere Darstellung + Footer-Fix.** Im Solo-Modus (`?only=1|2|3`) wird die
  Disziplin-Leiste (Footer) nicht mehr vom Inhalt überlappt/halb abgeschnitten — der Einzel-Bereich
  bekommt `overflow:hidden`, sodass der Footer immer voll sichtbar bleibt. Die Medaille ist kleiner
  (14vmin statt 22) und damit weniger top-lastig; dafür sind die Namen größer (Einzel 13vmin, Doppel
  8vmin). **Namen werden im Einzel-Modus ausgeschrieben** (kein Mittelnamen-Kürzen, da ein TV nur einen
  Platz zeigt → viel Platz). Der Sonderfall „zwei dritte Plätze auf einem TV" (`?only=3` ohne Spiel um
  Platz 3) skaliert automatisch kompakter (`multi`-Modus), damit beide Paare samt Footer sicher passen.
- **Footer zeigt Turniername + Disziplin.** Die Disziplin-Leiste hat jetzt etwas mehr Platz (16vh) und
  zwei Zeilen: oben klein/gedämpft der **Turniername**, darunter groß/gelb die **Disziplin** (z. B.
  „MD U17 C"). Gilt für Voll-Podest und Einzel-TVs. Der Turniername kommt aus dem BTP-Snapshot
  (`/info/winners/state` liefert ihn jetzt mit).

## v0.9.119

- **Sieger-Einzel-TVs (ein Platz pro Monitor) auch zweizeilig + größer.** Der Solo-Modus (`?only=1|2|3`,
  drei TVs vor dem Podest) nutzt jetzt dieselbe Vorname-/Nachname-Darstellung wie das Voll-Podest und ist
  deutlich größer (Einzel 12vmin, Doppel 7.5vmin) — optimal lesbar, da ein TV nur einen Platz zeigt.

## v0.9.118

- **Sieger-Monitor: zweizeilige Namen (Vorname / Nachname) + Mittelnamen gekürzt.** Statt eines langen,
  krumm umbrechenden Namens steht jetzt der **Vorname** (kleiner) über dem **Nachnamen** (groß) — das
  erlaubt eine größere, ruhigere Darstellung, gerade bei Doppeln. Mehrere Vornamen werden gekürzt
  („Melina Sabrina" → „Melina S."). Mehrteilige Nachnamen bleiben korrekt (BTP `Firstname`/`Lastname`
  werden dafür getrennt mitgeschickt; z. B. „Nguyen Duc" bleibt zusammen). Doppel etwas kompakter,
  damit die vier Zeilen passen.

## v0.9.117

- **Sieger-Monitor: Podest nutzt die volle Breite + noch größer.** Säulen breiter (31vw statt 26),
  weniger Seitenrand (1.5vw statt 5) und größere Schrift/Medaillen/Podest-Zahlen (Namen 7/5.6/5vmin,
  Zahlen 9vmin) — füllt den Bildschirm und ist aus der Distanz noch besser lesbar.

## v0.9.116

- **Sieger-Monitor: größere Namen.** Auf dem Voll-Podest sind die Spielernamen (und Vereine) jetzt
  deutlich größer (Gold 6vmin, Silber 4.8, Bronze 4.6) — bessere Lesbarkeit aus der Distanz/fürs Publikum.

## v0.9.115

- **Fix Sieger-Monitor: kein Flackern/„ständiger Reload" mehr.** Die Podium-Anzeige (`winners.html`) baute
  bei jedem 2-s-Poll den ganzen Bildschirm neu auf, auch wenn sich nichts geändert hat → sichtbares
  Flackern auf dem TV. Jetzt wird nur noch bei **tatsächlicher Änderung** neu gezeichnet (Signatur-Vergleich).
- **Fix Sieger-Steuerliste: stabile Reihenfolge.** Die Disziplinen kamen aus einer HashMap und wurden nur
  nach `finished_at` sortiert (das BTP nicht liefert → immer leer) → die Liste „wackelte" bei jedem Poll.
  Jetzt deterministisch nach `draw_id` sortiert.

## v0.9.114

- **Sieger-Monitor / Siegerehrung.** Neue Info-Anzeige, die das **Podium (1./2./3.) mit Verein** einer
  ausgespielten Disziplin zeigt — als klassisches Siegerpodest (Silber–Gold–Bronze), Disziplinname groß
  im Footer (gut für Fotos). **Gesteuert aus bts-light** (Seite *Court-Monitore → Siegerehrung*): der
  Operator wählt live, welche Disziplin erscheint — **keine Rotation**. Sieger = Gewinner des K.o.-Finals
  (Gruppen sind nur Qualifikation); ist „Spiel um Platz 3" nicht ausgetragen, werden **beide
  Halbfinal-Verlierer** als 3. Platz gezeigt.
- **Drei-Monitor-Aufbau möglich:** je ein TV vor Platz 1/2/3 (`?only=1|2|3`), oder ganzes Podium auf einem
  Bildschirm. Im Pi-Launcher und in der Geräte-Zuweisung wählbar. Verein wird neu aus BTP gelesen
  (`Player.ClubID` → Vereinsname).
- **Fix:** BTP-Matches mit `Winner=0` („noch kein Sieger") gelten nicht mehr fälschlich als beendet.

## v0.9.113

- **Fix Spieler-Status rot/gelb nach Spielende.** Ein gerade beendetes Spiel ließ beim Belegen eines
  anderen Felds fälschlich freie Spieler als „aktiv" (rot) erscheinen. Behoben und mit Regressions-Tests
  abgesichert.

## v0.9.112

- **Hochwertige Ansage über Azure Neural TTS (opt-in).** Statt der lokalen Stimme kann die ganze Ansage
  von einer **neuronalen Azure-Stimme** gesprochen werden, die asiatische/internationale Namen **nativ**
  ausspricht (SSML-Sprachtag pro Name via `detectNameLang` → zh-CN/vi-VN). Stimme wählbar
  (Seraphina/Florian, mehrsprachig). Einrichtung in *Einstellungen → Ansagen → „Hochwertige Stimme über
  Azure"* (Region + Key + Stimme).
- **Robust:** Key bleibt im Backend (Rust-Command `azure_tts_speak`), Ergebnis wird je Ansage **gecacht**
  (kein Netz/Geld bei Wiederholung), und bei Fehler/offline greift **automatisch die lokale
  Web-Speech-Ansage** als Fallback — nie stumm. Braucht Internet in der Halle.

## v0.9.111

- **Aussprache: regelbasierte Umschrift für chinesische & vietnamesische Namen.** Auch NICHT im
  Wörterbuch gelistete Namen werden jetzt besser gelesen: eine Engine (`src/io/transliterate.ts`)
  schreibt Pinyin (zh→dsch, x→sch, q→tsch, apikales i→i, j/q/x+u→ü …) und Vietnamesisch (tr→tsch,
  th→t, ph→f, kh→ch, nh→nj, Endung -c→k …) in deutsche Lautschrift um. Beispiele: „Zhang Zhixin"→
  „Dschang Dschi-schin", „Xu Yinsong"→„Schü In-ssong", „Pham Thi Hong Thu"→„Fam Ti Hong Tu".
- **Sicher:** greift NUR bei Namen, die per **markantem chinesischem/vietnamesischem Nachnamen** erkannt
  werden (deutsche/andere Namen bleiben unverändert). Reihenfolge je Wort: Wörterbuch/Tabelle → Engine →
  unverändert. Über denselben An/Aus-Schalter steuerbar.
- **Ehrliche Grenze:** Konsonanten sitzen zuverlässig; Vokale/Töne/Dialekt (z. B. südvietnamesisch,
  taiwanesisches Wade-Giles) bleiben Näherung — Feinschliff über die Nutzer-Tabelle (Vorrang).

## v0.9.110

- **Aussprache-Basis-Wörterbuch erweitert (Vornamen + mehr).** Zusätzlich zu den Nachnamen jetzt
  gängige **internationale Vornamen** (vietnamesisch, chinesisch, indisch, türkisch), die eine deutsche
  Stimme falsch liest (z. B. „Duc"→Dück, „Quang"→Kwang, „Can"→Dschan, „Arjun"→Ardschun) + Çoban→Tschoban.
  Vornamen werden ja mitgesprochen (BTP liefert „Vorname Nachname"). Insgesamt nun 130 Einträge.
  Hinweis: Die häufigsten Vornamen in den Ligen sind deutsch (werden korrekt gelesen); fremdsprachige
  Vornamen sind ein Long-Tail — abgedeckt sind die gängigen, Spezialfälle über die Nutzer-Tabelle.

## v0.9.109

- **K.-o.-Runde in der Feld-Ansage (ab Viertelfinale).** Vor der Paarung wird jetzt die Runde
  mitangesagt — **Viertelfinale, Halbfinale, Finale, Spiel um Platz 3** (z. B. „Feld 2. Herrendoppel.
  Halbfinale. … gegen …"). Frühere Runden, Gruppen und das Achtelfinale werden **nicht** angesagt.
  Erkennung aus der BTP-Runde (`RoundName`), robust gegen Schreibweisen (VF/HF/Finale, Voll-Namen,
  de/en). Die rohe Runde wird dafür als `CourtOverview.round_name` durchgereicht.

## v0.9.108

- **Aussprache: mitgeliefertes Basis-Wörterbuch + An/Aus-Schalter.** Häufige internationale Nachnamen
  (abgeleitet aus den häufigsten Namen der Badhub-Spieler-DB; VN/CN/IN/FR/ES/TR/PL) werden jetzt
  **automatisch** korrekt(er) ausgesprochen — ohne Pflege. Eigene Einträge in der Tabelle haben
  **Vorrang**. Neuer Schalter „Aussprache-Korrekturen anwenden" (Default an) schaltet alles ab/an.
- **Robusteres Matching (diakritik-/sonderzeichen-unabhängig).** „Nguyên"/„Nguyen", „Yıldız"/„Yildiz",
  „García"/„Garcia" treffen denselben Eintrag (NFD-Faltung + ı/ø/ł/đ). Der „Häufige Namen laden"-Knopf
  entfällt (das Basis-Wörterbuch wirkt automatisch).
- Ehrlich: Die Lautschrift sind **Näherungen** (keine verifizierte Aussprache-DB) — gut für häufige
  Namen, per ▶-Test und eigener Tabelle nachjustierbar.

## v0.9.107

- **Aussprache-Korrekturen für die Ansage.** Spricht die Stimme einen Namen falsch, lässt sich pro
  **Name oder Namensteil** eine **Ersatz-Schreibweise** hinterlegen (z. B. „Nguyen" → „Nujen",
  „Lefebvre" → „Löfäwr"). Ein Nachname reicht einmal und wirkt für alle Spieler:innen mit diesem Namen.
  Pflege im Setup → *Ansagen* → *Aussprache-Korrekturen*, mit ▶-Test je Zeile und Knopf
  **„Häufige Namen laden"** — Startliste gängiger Nachnamen vieler Herkünfte (vietnamesisch, chinesisch,
  indisch, französisch, spanisch, türkisch, polnisch) mit deutscher Lautschrift. Läuft offline;
  keine zusätzliche Ansage-Sprache, nur korrektere Aussprache.

## v0.9.106

- **Relay-Log persistent + ohne Sonderrechte lesbar.** Der Cloud-Relay schreibt sein Log jetzt
  zusätzlich in eine **täglich rotierende Datei** unter `storage/relay-logs/bts-relay.log.YYYY-MM-DD`
  (Pfad per `RELAY_LOG_DIR` in der systemd-Unit). Der `badhub`-User liest sie direkt per SFTP/SSH —
  kein journalctl-Recht nötig. Loglevel auf INFO begrenzt (kein Verbindungs-Spam).
- **Relay: StateRestore-Diagnose.** Beim (Neu-)Verbinden/Übernehmen eines Tablets protokolliert der
  Relay explizit, ob ein gespeicherter Spielstand wiederhergestellt wurde oder das Feld bei 0:0 startet —
  genau die offene Frage vom 14.06. (Ersatz-Tablet sprang auf 0:0).
- **Tablet crash-fest geloggt.** Unbehandelte JS-Fehler (`window.onerror`) und Promise-Rejections landen
  jetzt im Tablet-Log und werden sofort + beim nächsten Boot hochgeladen (Buffer von 300 auf 500 erhöht).
- **Court-Monitore loggen.** combo/overview/monitor erfassen JS-Fehler + Schlüsselereignisse
  („keine Daten", Deassign, Offline-Wechsel) und schicken sie best-effort an den Turnier-PC
  (`/pi-log` → lokal + Cloud, Datei `mon-<device>.log`). Deckt u. a. die Kombi-„keine Daten"-Klasse ab.

> **Server-Schritt einmalig** (wegen neuer Unit-Env): `sudo cp ops/bts-relay.service /etc/systemd/system/`
> dann `sudo systemctl daemon-reload && sudo systemctl restart bts-relay`.

## v0.9.105

- **Kombi-Anzeige: Satz-Sieger deutlich hinterlegt.** Der gewonnene Satz steht jetzt
  als **grüner Block** statt nur weiß-auf-grau — aus der Ferne sofort als Sieger
  erkennbar (Feld-Wunsch). Gilt für die übereinander- wie die nebeneinander-Variante.
- **Kombi-Anzeige: Pausen-Countdown am betroffenen Feld.** Läuft an einem Feld eine
  Pause, zeigt dessen Band die Restzeit (`Pause`/`Satzpause` + `m:ss`, `Behandlung`
  ohne Countdown) — direkt „an der Seite, wo die Pause ist". Server-zeit-relativ
  gerechnet (die Pi braucht keine synchrone Uhr).
- **Tablet: Aufschläger/Annehmer nach jedem Satz neu (Doppel/Mixed).** Endet ein Satz
  und das Match läuft weiter, fragt das Tablet nach der Satzpause „**Neuer Satz — wer
  schlägt auf?**" — beschränkt auf das Gewinnerteam des letzten Satzes, danach die
  Annehmer-Wahl. Aufschläger/Annehmer können je Satz wechseln; bis zur Bestätigung
  bleibt die Zähltafel gesperrt. Einzel läuft unverändert automatisch weiter.

## v0.9.104

- **Aktive Halle (Tages-Halle) für Mehr-Hallen-Turniere.** Bei Turnieren, bei denen
  an einem Tag nur in EINER Halle gespielt wird (z. B. eine BTP-Datei für zwei Tage),
  musste man bisher jedes Spiel manuell „in Vorbereitung" rufen, damit die Auto-
  Feldvergabe greift. Neu: in den Einstellungen → „Automatische Feldvergabe" trägt man
  die **aktive Halle** ein (BTP-Hallenname) — dann vergibt bts-light automatisch nur auf
  die Felder dieser Halle, **ohne** Aufruf-Pflicht (die Ansage folgt dann automatisch).
  Leer = alle Hallen (Mehr-Hallen wie bisher mit Aufruf). Im Ein-Hallen-Turnier wird der
  Wert ignoriert; ein unbekannter Hallenname wird geloggt und fällt sicher zurück.

## v0.9.103

- **Fix: bts-light setzte BTP-Spieler fälschlich auf „nicht spielbereit" (rot→gelb).**
  Unser `SENDUPDATE` schrieb `Status: 0` in den Match-Knoten — sowohl bei jeder
  Feldzuweisung (Auto + manuell) als auch beim Ergebnis. `Match.Status` ist in BTP
  aber ein **Bitfeld mit den Check-in-Bits der Spieler**; hart auf 0 zu setzen hat
  sie als nicht eingecheckt markiert. Wir schreiben das `Status`-Feld jetzt **gar
  nicht mehr** (BTP behält seinen Stand — wie Tilos BTS). Stabilisiert voraussichtlich
  auch die automatische Ansage/Feldvergabe. *(Bitte am echten BTP gegenprüfen.)*

## v0.9.102

- **Fix: Kombi-/Übersichts-Monitore zeigten zwischendurch „keine Daten".** Ursache
  war ein nicht-atomares Schreiben der Monitor-Zuweisungsdatei: las ein Monitor-Poll
  sie genau während eines Schreibens (z. B. beim Zuweisen), kam unvollständiges JSON →
  leere Zuweisung → der Monitor navigierte auf die leere Einzel-Seite, bis man „Neu
  laden" drückte. Zuweisungen werden jetzt **atomar** geschrieben (temp + rename), und
  die Monitore **entprellen** ein leeres Zuweisungs-Ergebnis (erst nach mehreren Polls).
- **Akku-Anzeige der Tablets zurück (über Fully Kiosk).** Da die Web-Battery-API über
  HTTP nicht verfügbar ist, liest das Tablet den Akku jetzt über das **Fully-Kiosk-JS-
  Interface** (`fully.getBatteryLevel()`/`isPlugged()`), Fallback Web-API. Voraussetzung:
  in Fully Kiosk **„JavaScript Interface aktivieren"**.
- **Pi-HDMI: Bild auch bei gleichzeitigem Einschalten von Pi und TV.** Das Setup setzt
  jetzt `hdmi_force_hotplug=1` — der Pi gibt immer ein HDMI-Signal aus, auch wenn der TV
  beim Booten noch nicht bereit war (vorher half nur ein Pi-Neustart).

## v0.9.101

- **Fix: Spielstand bleibt nach Tablet-Crash/-Tausch erhalten (kein 0:0 mehr).**
  Bisher bekam ein neu oder ersatzweise verbundenes Tablet den gespeicherten
  Spielstand nur über den „Übernehmen"-Pfad — bei einem echten Crash war das Feld
  aber sofort frei, sodass das Ersatz-Tablet ein frisches 0:0 begann (im Feld-Test
  bestätigt: `state_restore` kam nie). Jetzt sendet der Server (LAN **und** Cloud-
  Relay) den gespeicherten Stand auch beim **normalen Verbinden** — das Tablet
  übernimmt ihn, sofern die Match-ID passt, sonst gilt das frisch zugewiesene
  Match. Nach einem übermittelten Ergebnis wird der gespiegelte Stand verworfen
  (kein Wiederaufleben eines beendeten Spiels).
- **Fix: Feld-Ansagen laufen strikt nacheinander.** Wurden zwei Spiele kurz
  hintereinander auf Felder gezogen, startete der Gong der zweiten Ansage, während
  die erste noch sprach. Alle Ansagen (Feld, Vorbereitung, manuell) laufen jetzt
  durch **eine globale Warteschlange** und warten aufs **Sprechende**, bevor die
  nächste (mit Gong) beginnt.

## v0.9.100

- **Auto-Feldvergabe spielt den Zeitplan ab + prüft Spieler-Verfügbarkeit.**
  Die automatische Feldvergabe belegt freie Felder jetzt in der Reihenfolge der
  **BTP-Ansetzung** (`PlannedTime`, von oben nach unten) statt nur nach
  Spielnummer; manuell „in Vorbereitung" gerufene Spiele bleiben Vorrang, ohne
  Ansetzung gilt wie bisher die Spielnummer. Ein Spiel wird **übersprungen**,
  wenn einer seiner Spieler **gerade auf einem anderen Feld spielt** oder noch
  in seiner **Pause** ist – dann rückt das nächste Spiel nach. Spieler-Identität
  über Lizenznummer (Name als Fallback), wirkt auch über Disziplinen hinweg; ein
  Spieler kann nie auf zwei gleichzeitig frei werdende Felder kommen.
- **Pausenzeit aus BTP.** Die Mindest-Pause wird aus **BTP-Setting 1303**
  gelesen (wie der Turniername aus 1001). In den Einstellungen → „Automatische
  Feldvergabe" lässt sich „Pause nach Spielende (Min.)" als **Override** setzen
  (0 = BTP-Wert übernehmen). Die Vorbereitungs-/Kandidatenliste ist konsistent
  ebenfalls nach Ansetzung sortiert.

## v0.9.99

- **Vertikale Kombi: größere Namen + sichtbarer Aufschlag-Punkt.** Namen jetzt
  5.6vh (war 4.8), Flaggen entsprechend größer. Der gelbe Aufschlag-Punkt beim
  aufschlagenden Spieler ist im Vertikal-Modus deutlich größer (3vh) — er war
  schon verdrahtet (gleiche Logik wie Einzelmonitor/horizontale Kombi), nur
  neben den großen Zahlen kaum sichtbar; erscheint, sobald das Tablet die
  Aufschlag-Info meldet.

## v0.9.98

- **Vertikale Kombi: Spielstand als große Zahlen-Spalte.** Statt „21 : 19"
  nebeneinander stehen die Satzzahlen jetzt **untereinander** zwischen den
  Namen — Team 1 oben, Team 2 darunter — und **deutlich größer** (15vh):
  Name/Name · 21 · 18 · Name/Name. Gewinn-/Laufend-Färbung wie gehabt.

## v0.9.97

- **Kombi-Anzeige: Option „Felder nebeneinander" (vertikal).** Neuer Schalter in
  den Court-Monitor-Einstellungen: statt zwei Felder über­einander (horizontale
  Trennung) werden sie **nebeneinander** gezeigt — je Feld ein Hochformat-
  Scoreboard (Team 1 oben, Spielstand als Satz-Paare mittig, Team 2 unten). So
  mappt ein TV zwischen zwei Feldern räumlich auf links/rechts. Technisch hängt
  der Schalter `&dir=v` an die Kombi-URL (`combo.html` rendert das Layout).
  Globaler Schalter (gilt für alle Kombi-Anzeigen).

## v0.9.96

- **Kombi-Anzeige: Namen noch größer** (Feld-Test 2026-06-13): `--name-size`
  jetzt 1 Feld 10vh · 2 Felder 6.5vh · 3 Felder 4.3vh (war 8/5.5/3.8).

## v0.9.95

- **Kombi-Anzeige: Spielernamen größer/lesbar.** Die Namen standen fix auf
  3.2vh und wirkten neben den großen Satz-Zahlen winzig. Sie skalieren jetzt –
  wie die Zahlen – nach Feldzahl (`--name-size`: 1 Feld 8vh · 2 Felder 5.5vh ·
  3 Felder 3.8vh), ohne bei Doppeln/3 Feldern überzulaufen. Zahlen unverändert.

## v0.9.94

- **Felder-Lobby als Tablet-Startseite (`/felder`).** Statt das Tablet fest auf
  `…/court/<id>` zu starten, gibt es jetzt eine Start-Übersicht aller Felder:
  ein Tipp auf ein Feld beginnt das Zählen. Belegte Felder (ein Tablet zählt
  sie schon) sind als „belegt" + Paarung markiert; ein Tipp führt auf die
  bestehende „Feld belegt – übernehmen?"-Abfrage. **Doppelbelegung bleibt
  ausgeschlossen** (serverseitige `CourtOccupied`-Sperre unverändert). Die Lobby
  pollt `/courts` (jetzt inkl. `occupied` + Paarung) alle ~3 s. Empfohlene
  Tablet-Start-URL daher `http://<PC-IP>:8088/felder`.
- **Fix: Identifizieren/Neu-laden funktionierten nach einem bts-light-Neustart
  erst nach mehreren Klicks.** Die Fernbefehl-`id` zählte im RAM hoch und
  startete nach jedem Neustart wieder bei 1, während die Monitore die zuletzt
  gesehene `id` im `localStorage` über den Neustart hinweg behielten → kleinere
  `id` = „schon erledigt". Die `id` ist jetzt **zeitstempel-basiert** (`now_ms`)
  und damit über Neustarts hinweg monoton steigend.
- **Diagnose Akkustand:** Das Tablet loggt jetzt beim Start `battery_env`
  (`getBattery` vorhanden? `secureContext`?). Hintergrund: `navigator.getBattery()`
  braucht in modernem Chromium HTTPS — über HTTP-LAN ist die Akku-Anzeige daher
  oft nicht verfügbar (kein Code-Fehler, Plattform-Einschränkung).

## v0.9.93

- **Fix: „Identifizieren" wirkt jetzt auch in Court-Übersicht und Kombi-
  Anzeige.** Bisher zeigte der gelbe Code-Overlay nur in der Einzelfeld-
  Ansicht (`monitor.html`); in `overview.html`/`combo.html` passierte beim
  Klick auf „Identifizieren" nichts. Ursache: beide pollten zwar bereits
  `/monitor/state` (für den Reassignment-Check), werteten den darin
  enthaltenen `command` aber nicht aus. Jetzt behandeln sie den Fernbefehl
  mit derselben id-basierten Logik wie `monitor.html` (Identifizieren + Neu
  laden) und blenden den Geräte-Code groß auf gelbem Grund ein. Greift,
  sobald der PC aktualisiert ist und die Pis die Seite neu laden.

## v0.9.92

- **Turnierlogo für den badhub-Liveticker.** In den Einstellungen
  (Abschnitt „Liveticker-Ziel") lässt sich ein **Turnierlogo hochladen**
  (PNG/JPG/WEBP/GIF/SVG, max. 2 MB) inkl. optionaler Hintergrundfarbe für
  transparente Logos. bts-light schickt es als Base64 im vollen `tset`-Event
  mit (`tournament_logo`/`_mime`/`_background_color`) — badhubs vorhandenes
  `#live-logo`-Element zeigt es dann oben auf **badhub.de/live** an, genau wie
  beim Original-BTS. **Hintergrund:** BTP liefert kein Logo (verifiziert in
  BTS- und bts-light-Code), deshalb der Upload. Ohne Logo wird nichts gesendet
  (Felder mit `skip_serializing_if`), badhub blendet das Element dann aus.

## v0.9.91

- **Punkt-Cooldown am Zähltablett (Doppel-Eingabe-Schutz).** Nach einem Punkt
  sind die +1-Flächen **3 s gesperrt** (sichtbar gedimmt) — verhindert
  versehentliche Doppel-Taps/Doppelpunkte (Punkte fallen ohnehin nicht im
  Sekundentakt). **Undo** hebt die Sperre sofort auf (Korrektur ohne Warten).
  Dauer als Konstante `SCORE_COOLDOWN_MS` leicht anpassbar.

## v0.9.90

- **Fix: Court-Übersicht je Halle flackerte / ging „offline" (Redirect-Loop).**
  Seit der Per-Halle-Zuweisung (v0.9.82) hat das Monitor-Ziel ein `?halle=…`;
  `overview.html`/`preparation.html` verglichen das Server-Ziel aber naiv gegen
  `location.pathname` (ohne Query) → der Vergleich schlug **immer** an → die
  Seite navigierte im Sekundentakt neu (Flackern), aktualisierte keine
  Ergebnisse und fiel durch das Dauer-Neuladen auf **„offline"**. Jetzt
  Vergleich über Pfad **+ Query** (ohne `device`/`rotate`/`hallSeconds`), wie in
  `ad.html`/`combo.html`. Greift, sobald der PC aktualisiert ist (die Pis laden
  `overview.html` vom PC).

## v0.9.89

- **Pi-Logs einheitlich über den PC (statt direkt in die Cloud).** Pi-Court-
  Monitore posten ihr Log jetzt – wie die Tablets – an den Turnier-PC
  (`/pi-log` im LAN, plain HTTP); der PC legt es lokal ab und leitet es an die
  Cloud weiter. Vorteil: **nur der PC braucht Internet**, weniger LTE-Daten, und
  **kein TLS/keine Pi-Uhr** nötig — der bisherige Direkt-HTTPS-Upload scheiterte
  bei fehlender Pi-RTC (falsche Uhr) still. Pi-Seite: `pi/shared-startbrowser.sh`
  (wirkt erst nach Neu-Flashen der Karten). Doku: `docs/logging.md`.

## v0.9.88

- **Internet-/Uplink-Status in der Kopfzeile.** Neben „BTS-Netzwerk" zeigt
  bts-light jetzt „Internet" (grün) bzw. „Kein Internet" (rot) — ein kurzer
  HEAD auf badhub.de alle 30 s. So sieht man, ob der LTE-/Uplink aktiv ist (=
  Voraussetzung für Cloud-Logs + Liveticker-Push). Der Carriername (z. B.
  Vodafone) ist vom PC aus nicht ermittelbar.

## v0.9.87

- **TV-Launcher bietet auch die Online-Anzeige (badhub.de).** Das Auswahl-Menü
  zeigt jetzt zwei Gruppen: **Lokal** (bts-light) **und Online** (öffentlicher
  badhub-Liveticker je Halle, `…/live?t=…&display=monitor&halle=<Halle>`, etwas
  andere Darstellung). So lässt sich am TV per Fernbedienung auch die
  Online-Ansicht je Halle wählen. Der Link kommt aus dem konfigurierten Verband.

## v0.9.86

- **TV-Launcher — kurze URLs statt langer `?halle=`-Eingabe.** An einem Smart-TV
  reicht jetzt die **kurze** Adresse `bts-light.local:8088` (= Auswahl-Menü, auch
  unter `/tv`): per **Fernbedienung (Pfeiltasten + OK)** „Alle Hallen", je Halle
  ein Button oder „Nächste Spiele" wählen — kein `?halle=` mehr tippen.
  Direkt-Kurzpfade: `…/alle`, `…/h/1`, `…/h/2` (n-te Halle), `…/next`. Die
  bisherige Debug-Landing liegt jetzt unter `/status`.

## v0.9.85

- **Aufgabe: Disziplin-Kaskade jetzt optional (Verletzung abgefragt).** Bisher
  löste **jede** Aufgabe automatisch einen Walkover-Vorschlag für die restlichen
  Spiele der Disziplin aus. Der Match-beenden-Dialog fragt jetzt: **„Aufgabe –
  nur dieses Spiel"** (nur dieses Spiel zählt) oder **„Verletzung – auch
  Folgespiele der Disziplin"** (dann erst der Walkover-Vorschlag für die
  Folgespiele). Durchgeschleift bis BTP (`cascadeWalkover`-Flag,
  abwärtskompatibel). „Spiel abbrechen" in der Behandlungspause beendet nur
  dieses Spiel.

## v0.9.84

- **Court-Übersicht: Hallenname in der Kopfzeile + Unten-Abschnitt behoben.**
  Der Hallenname (bei Rotation mit „1 / N") steht jetzt **hinter „Court-Übersicht"
  in der Kopfzeile** statt in einer eigenen Zeile — spart Platz. Außerdem wurden
  unten Kacheln abgeschnitten: Ursache war eine hartcodierte Kopfzeilenhöhe
  (`calc(100% - 7vh)`); jetzt füllt der Inhalt per Flex exakt den Rest → nichts
  läuft mehr aus dem Bild.
- **Court-Monitore: Online-Link je Halle.** Bei mehreren Hallen gibt es unter
  „Court-Übersicht (Hallen-Display)" jetzt auch je Halle einen **öffentlichen
  Online-Link** (`…/live?…&display=monitor&halle=<Halle>`), zusätzlich zur
  Gesamt-Online-Ansicht und den lokalen Links.

## v0.9.83

- **Status-Seite: „Anzeigen im Browser" erst nach Start + Hallen-Buttons.** Die
  Buttons (Liveticker, Hallen-Monitor, Nächste Spiele) sind jetzt **deaktiviert,
  bis der Liveticker gestartet ist** (vorher konnte man ins Leere klicken, ohne
  BTP-Verbindung). Nach dem Start kennt bts-light die Hallen aus der Turnierdatei
  und blendet bei **mehreren Hallen je Halle einen lokalen Hallen-Monitor-Button**
  ein (öffnet die Court-Übersicht dieser Halle).

## v0.9.82

- **Pi je Halle zuweisen (Court-Übersicht).** Im Zuweisungs-Dropdown eines
  Court-Monitors erscheinen ab 2 Hallen unter „Informationen" automatisch
  „Court-Übersicht – alle Hallen" **und** je Halle „Court-Übersicht – Halle X".
  Der Pi wird dann fest auf `…/info/overview?halle=<Halle>` umgeleitet — kein
  URL-Tippen am Pi. Technisch: `MonitorTarget::InfoOverview` trägt jetzt eine
  optionale Halle (abwärtskompatibel, alte Zuweisungen bleiben gültig).

## v0.9.81

- **Court-Übersicht-Links automatisch in der Court-Monitore-Seite.** Neue
  Sektion „Court-Übersicht (Hallen-Display)": zeigt den **Online-Liveticker**
  (öffentlich, aus dem konfigurierten Verband) und die **lokale Übersicht**.
  Sind **mehrere Hallen** im Turnier, erscheint **automatisch je Halle** ein
  fertiger Link (`…/info/overview?halle=<Halle>`) zum Kopieren auf den jeweiligen
  Hallen-TV. „Öffnen" zeigt die Vorschau am PC (localhost). `open_external`
  erlaubt dafür jetzt zusätzlich lokale `http://`-Links (Loopback/`bts-light.local`).

## v0.9.80

- **Court-Übersicht: Auto-Rotation bei mehreren Hallen.** Erkennt der Monitor
  mehrere Hallen und ist **kein** `?halle=` gesetzt, zeigt er jede Halle
  nacheinander **im Vollbild** (statt alle gestapelt zu quetschen) — Kopf mit
  Hallenname + „1 / N", Intervall via `?hallSeconds=<n>` (Default 12). Mit
  `?halle=<Name>` bleibt ein Monitor fest bei einer Halle (empfohlen bei 12
  Feldern/Halle → ein TV pro Halle, 4×3-Raster). Doku: court-monitor.md.

## v0.9.79

- **Court-Übersicht: Doppel-Darstellung wie der Hallen-Monitor.** Bei Doppeln
  stehen die zwei Partner jetzt **untereinander** (je eigene Flaggen-Spalte,
  volle Namen statt abgeschnitten), Satzstand mittig rechts — vorher quetschten
  sich beide Namen in eine Zeile und wurden abgeschnitten. Zudem **kein
  Unten-Überlauf** mehr: das Kachel-Grid teilt die Höhe strikt (`minmax(0,1fr)`)
  und clippt im Notfall, statt aus dem Bild zu laufen.

## v0.9.78

- **Kopfzeile zeigt „BTS-Netzwerk" statt nur WLAN.** Die Anzeige sagt jetzt, ob
  der PC im **lokalen BTS-Netz** hängt — erkannt am `btsaccess`-WLAN **oder** an
  einer IP im BTS-Subnetz `192.168.16.x` (also **auch am LAN-Kabel**, nicht nur
  WLAN). Grün „BTS-Netzwerk", wenn verbunden; sonst grau „Kein BTS-Netz
  (\<WLAN-Name>)". Hintergrund: das WLAN kann auch ein anderes sein, und Tablets
  laufen ggf. über die Cloud — entscheidend ist das lokale Netz, über das
  LAN-Tablets/Pi-Monitore den PC erreichen.

## v0.9.77

- **Fix: kein aufblitzendes cmd-Fenster mehr.** Die WLAN-Anzeige (v0.9.76)
  startete alle 15 s `netsh` ohne `CREATE_NO_WINDOW` → unter Windows blitzte bei
  jedem Poll kurz ein Konsolenfenster auf, besonders auffällig **ohne** WLAN
  (langsameres `netsh`). Der Aufruf läuft jetzt fensterlos im Hintergrund.

## v0.9.76

- **WLAN-Anzeige in der Kopfzeile.** Neben dem Liveticker-Status zeigt bts-light
  jetzt, mit welchem **WLAN** der Turnier-PC verbunden ist — **grün**, wenn es
  das erwartete Netz `btsaccess` ist, sonst neutral mit Klarname (bzw. „Kein
  WLAN" am LAN-Kabel). So sieht man auf einen Blick, ob der PC im richtigen Netz
  hängt. SSID wird plattformabhängig ausgelesen (Windows: `netsh`), alle 15 s,
  mit Deadline gegen hängende WLAN-Dienste.

## v0.9.75

- **Court-Monitor-Code eindeutig (Pi-„PI00"-Kollision behoben).** Mehrere
  Raspberry-Pi-Monitore zeigten beim „Identifizieren" denselben Kopplungs-Code
  „PI00", weil alle Pi-Seriennummern mit demselben Präfix (`00000000…`)
  beginnen und der Code aus den **ersten** vier Zeichen gebildet wurde. Der Code
  nutzt jetzt die **letzten** vier alphanumerischen Zeichen der Geräte-ID →
  jeder Pi ist eindeutig unterscheidbar. **Kein Re-Flash nötig** — der Code wird
  am PC/Relay berechnet; Update + Relay-Redeploy genügen. (Die Geräte-IDs waren
  schon vorher eindeutig, nur die Anzeige nicht.)

## v0.9.74

- **„Match beenden" ab 0:0 — mit Dialog für Aufgabe oder Kampflos.** Der
  Beenden-Button am Tablet ist jetzt **ab Spielbeginn (0:0)** verfügbar (vorher
  erst ab dem 2. Satz) und bewusst **dezent** gestaltet. Ein Tippen öffnet eine
  zweisprachige Rückfrage („Spiel beenden? · End the match?") mit **Aufgabe
  (Verletzung) · Retirement** und **Kampflos · Walkover**; „Regulär beenden"
  erscheint nur, wenn schon Sätze gespielt wurden. Der Status geht nach BTP
  (`ScoreStatus` 2 = Aufgabe, **1 = Kampflos**, Kampflos ohne Sätze). Sieger wird
  danach im Match-Ende-Overlay gewählt. Aufgabe und Kampflos schließen sich aus.

## v0.9.73

- **Tablet-Diagnoselog wird gesammelt (PC + Cloud).** Tablets schicken ihr Log
  (Verbindung, Match, Punkte, Karten, Reconnects) alle ~5 min an den bts-light-
  Server → liegt beim Turnier-PC unter „Logs öffnen" als
  `tablet-logs/court-N.log` (auch **offline**). Hat der PC Internet, wird es
  zusätzlich an die badhub-Cloud weitergeleitet (`api/tablet_log.php`) → fern
  auswertbar. **5×-Tap-Diagnose** triggert zuverlässiger (ganzer Verbindungs-
  Bereich tippbar statt nur der winzige Punkt). (Cloud-Modus-Tablet: Server-Empfang
  über den Relay folgt noch.)

## v0.9.72

- **Schiri-Modus: Spielende-Ansage lesbar.** Wie bei der Satzpause (v0.9.71)
  verdeckte am Match-Ende die „beendet"-Überlagerung (Sieger + Übermitteln/
  Wieder-öffnen) die Ansage-Leiste. Im Schiri-Modus steht die Spielende-Ansage
  („Spiel. Das Spiel gewinnt … {Satzstände}.") jetzt **direkt auf der beendet-
  Überlagerung**.

## v0.9.71

- **Schiri-Modus: Ansage in der Pause lesbar.** Beim Satzende verdeckten
  Countdown + „Weiterspielen/Korrektur"-Buttons der Pausen-Überlagerung die
  Ansage-Leiste. Im Schiri-Modus steht der Ansagetext (z. B. „Satz. Den ersten
  Satz gewinnt … Bitte die Seiten wechseln.") jetzt **direkt auf der Pausen-
  Überlagerung** – gut lesbar zum Vorlesen.

## v0.9.70

- **Fix: Tablet zeigte nach Reconnect ein bereits entferntes Spiel.** Wurde ein
  Spiel vom Feld genommen, während die Tablet-WebSocket nach langer Inaktivität
  „still" tot war, behielt das Tablet das alte Spiel auch nach dem automatischen
  Reconnect – der Server unterdrückte das `match_cleared`, weil der „noch nichts
  gesendet"-Zustand und „kein Match" beide als `None` galten (Dedup `None==None`).
  Jetzt feuert der erste Push pro Verbindung immer (Sentinel) → leeres Feld
  meldet sofort `match_cleared`. (Nur LAN; Cloud war korrekt.)

## v0.9.69

- **Schiri-Modus am Zähltablett (Deutsch).** Hinter dem PIN aktivierbar
  (⚙ → „Schiri-Modus: an"): eine **immer sichtbare Ansage-Leiste** zeigt den
  vorzulesenden Text (Eröffnung, Stand mit Aufschlägerstand zuerst, „N beide",
  „Aufschlagwechsel …", 11-Pause, Satzende+Seitenwechsel, Satzbeginn, Spielende;
  Satz-/Matchball-Badge). Dazu **Karten/Verwarnungen** je Spieler: Gelb
  (Verwarnung), Rot (Fehler → Gegner bekommt +1), Schwarz (Disqualifikation) –
  mit Ansagetext, **nur lokal** protokolliert (Chips). Reine Anzeige, kein
  Eingriff in die Zähl-Logik. Doku: `docs/umpire-mode.md`. (Für Vereins-/
  Verleih-Turniere; Bundesliga läuft über das Original-BTS.)

## v0.9.68

- **Tablet-Einstellungs-PIN in der Oberfläche setzbar.** Der PIN fürs ⚙-Menü
  am Zähltablett (Feldwechsel ohne QR) lässt sich jetzt direkt in den
  Einstellungen unter **„Tablet-Verbindung"** eingeben (nur Ziffern, Default
  „0000") – kein Bearbeiten der `config.json` mehr nötig.

## v0.9.67

- **Feldwechsel ohne QR jetzt auch im Cloud-Modus.** Das PIN-Menü am Tablet
  (v0.9.66) konnte die Feld-Liste bisher nur im LAN laden. Jetzt pusht der Host
  die vollständige Feld-Liste an den Relay (`HostFrame::Courts`), der sie unter
  `/{ns}/courts` ausliefert – der Feldwechsel funktioniert damit in LAN **und**
  Cloud identisch. (Greift im Cloud-Modus über den Relay-Redeploy.)

## v0.9.66

- **PIN-Einstellungsmenü am Zähltablett – Feldwechsel ohne QR.** Ein Zahnrad ⚙
  im Tablet-Header öffnet (nach PIN) ein Menü: **Feld wechseln** zeigt die
  Feld-Liste (BTP-Feldname inkl. Halle) und schaltet das Tablet auf ein anderes
  Feld um, **ohne einen QR-Code zu scannen**; dazu **Vollbild ein/aus**. PIN in
  `config.json` (`tablet_settings_pin`, Default „0000", nur Ziffern, ohne
  Neustart wirksam) – reiner Bedien-Schutz. Neuer Server-Endpoint `GET /courts`.
  Die echte Kiosk-Sperre (kein Internet, Android-Buttons aus, Exit-PIN) macht ein
  Kiosk-Browser – Anleitung in `docs/tablet-kiosk.md` (Allowlist deckt bts-light
  und Tilos BTS ab). Cloud-Modus: Feldwechsel-Liste noch offen.

## v0.9.65

- **Court-Monitor zeigt nach der Satzpause sofort 0:0.** Nach dem ersten Satz
  klebte der TV am alten Satzstand (z. B. 21:7) und sprang erst beim ersten
  Punkt des neuen Satzes auf 0:0. Ursache: Der LAN-Server ließ den laufenden
  0:0-Satz weg, sobald schon ein Satz gespielt war (gedacht gegen einen
  0:0-„Geistersatz" nach Spielende). Jetzt wird 0:0 nur noch weggelassen, wenn
  die abgeschlossenen Sätze das Match **bereits entscheiden** (echtes Spielende),
  nicht **zwischen** den Sätzen. Gilt für Monitor, Kombi-Anzeige, Übersicht und
  Liveticker; LAN- und Cloud-Pfad identisch. (Der Cloud-Monitor über den Relay
  war nicht betroffen.)

## v0.9.64

- **Monitor-Online-Status flackert nicht mehr.** Der Server stufte einen Monitor
  schon nach 6 s ohne Poll als offline ein – ein kurzer WLAN-Zucker (im Hallen-/
  Verleih-WLAN normal) ließ den Online-Punkt damit hin- und herspringen
  (`MONITOR_ONLINE_WINDOW_MS` 6 s → **20 s**). Ein wirklich totes Gerät fällt
  weiterhin nach 20 s raus.
- **Feldnummer groß auf der Leerlauf-Seite.** Wenn kein Spiel läuft und keine
  Werbung kommt, zeigte der Monitor nur Turniername + „Kein Spiel auf diesem
  Feld". Jetzt steht die **Feldnummer groß** dazwischen – man erkennt sofort,
  welches Feld der Bildschirm zeigt.

## v0.9.63

- **Court-Monitor-Leerlauf: „badhub.de" groß als Werbung.** Die Wortmarke füllt
  jetzt fast die ganze TV-Breite (an der Viewport-Breite skaliert), gut lesbar in
  hellem Weiß; „BTS light" deutlich kleiner darunter, das Federball-Logo etwas
  zurückgenommen. Greift im Cloud-Modus über den Relay-Redeploy.
- **Pi-Kiosk-Launcher stabiler (kein Flackern mehr).** Der gemeinsame
  `pi/shared-startbrowser.sh` beendete bei einem *einzelnen* WLAN-Aussetzer sofort
  den Kiosk (Desktop taucht auf, dann Neustart). Jetzt **Hysterese**: erst nach
  mehreren erfolglosen Runden (≈30 s) beenden, und die gemerkte bts-light-IP wird
  bei kurzen Blips nicht mehr verworfen. Der Kiosk läuft bei Wacklern einfach durch.

## v0.9.62

- **Court-Monitor: Logo & Symbole schrift-unabhängig.** Die Leerlauf-Anzeige
  nutzte das 🏸-Emoji als Logo; auf Raspberry Pi OS (keine Emoji-Schrift) blieb
  das Kästchen leer. Jetzt **Inline-SVG-Federball** → rendert auf Pi, Handy und
  Windows gleich. Ebenso die Emojis 📢 (Aufruf-Chip) und ⏱ (Spieldauer) im
  Monitor entfernt (Klartext genügt). Greift im Cloud-Modus über den Relay-
  Redeploy (monitor.html jetzt in dessen Deploy-Triggern).

## v0.9.61

- **„Offline ausblenden" in der Court-Monitore-Verwaltung.** Ein Umschalter
  blendet offline gemeldete Monitore aus der Liste aus — übrig bleiben nur die
  aktuell laufenden. Reiner Ansichtsfilter: Zuweisungen bleiben erhalten, ein
  wieder pollender Pi taucht automatisch erneut auf. Hilft, wenn sich über den
  Turniertag alte/neu-geflashte Geräte ansammeln.

## v0.9.60

- **„Nochmal aufrufen" je Feld.** In der Spielübersicht hat jedes belegte Feld
  jetzt einen Megafon-Button „Aufrufen", der die Feld-Ansage (Gong + Feld +
  Disziplin + Paarung) erneut abspielt – praktisch, wenn die Spieler nicht kommen.
  Sichtbar, wenn Ansagen aktiviert sind. (Ansage-Logik mit der Ansagen-Seite
  geteilt, eine Quelle.)

## v0.9.59

- **Spielübersicht als Board.** Statt links/rechts jetzt: oben der Pool der
  spielbereiten Spiele (ziehbar), darunter die **Felder als Spalten** mit
  Ampel-Kopf (grün frei / gelb belegt / rot gesperrt), Aufruf-Uhr und
  Freigeben/Sperren je Spalte. Übersichtlicher bei vielen Feldern; bei ≥2 Hallen
  nach Halle gruppiert + Hallen-Filter. Drag&Drop und Klick-Auswahl bleiben.
  Beim Zuweisen wird geprüft, dass das Spiel noch spielbereit ist.

## v0.9.58

- **Mehr-Hallen-Komfort.** Bei Turnieren mit ≥2 Hallen:
  - **Hallen-Filter** („Alle | Halle 1 | Halle 2 …") auf der Tablet- und der
    Court-Monitore-Seite – zeigt nur die gewählte Halle.
  - **Halle je Court-Monitor wählbar** (Dropdown „Halle: automatisch / Halle …"):
    überschreibt die aus dem Feld abgeleitete Halle. So lassen sich auch Geräte
    ohne Feld (Info-/Werbe-/Kombi-Monitore, noch unzugewiesene Pis) einer Halle
    zuordnen. Persistiert in `monitor-halls.json`.
  - **Tablet-Übersicht je Halle** mit Kurz-Zusammenfassung „X/Y Tablets
    verbunden" in der Hallen-Überschrift.
  - Geräte ohne Feld-Halle erscheinen weiterhin sauber gruppiert; ein leerer
    Hallen-Filter zeigt einen Hinweis statt einer leeren Liste.

## v0.9.57

- **Sicherheitsabfrage beim Feld-Freigeben.** „Freigeben" fragt jetzt erst nach
  („Feld wird in BTP zurückgezogen, Halle+Feld am Spiel entfernt; läuft ein
  Spiel, wird der laufende Spielstand verworfen") und muss mit „Freigeben"
  bestätigt werden. Verhindert versehentliches Zurückziehen eines laufenden
  Spiels. Die angezeigten Spiel-Infos kommen aus dem Live-Stand des Felds.

## v0.9.56

- **Automatische Feldvergabe.** Optional (Einstellungen → „Automatische
  Feldvergabe"): bts-light belegt freie, nicht gesperrte Felder automatisch mit
  dem nächsten spielbereiten Spiel und schreibt das nach BTP — sobald ein Feld
  **lange genug frei** ist (einstellbare Wartezeit, verhindert Belegen in der
  kurzen Lücke zwischen Spielen; 0 = sofort).
  - Reihenfolge wie in der Vorbereitung (gerufen zuerst, dann Spielnummer).
  - **Mehr-Hallen-sicher:** Im Mehr-Hallen-Turnier werden nur Spiele verteilt,
    die für die jeweilige Halle „in Vorbereitung" gerufen wurden — kein Risiko,
    ein Spiel in die falsche Halle zu legen.
  - **Keine Doppelvergabe:** ein bereits (auch zyklusübergreifend) vergebenes
    Spiel/Feld wird erst nach BTP-Bestätigung wieder berücksichtigt.

## v0.9.55

- **Aufruf-Timer jetzt auch im Cloud-Modus auf dem Court-Monitor.** Der Aufruf-
  Timer (hochzählende Uhr + 1./2./3.-Aufruf-Chip) erscheint nun auch auf Pis, die
  über den Relay (LTE/Verleih-Set) angebunden sind — gleiche Anzeige wie im LAN.
  Der **1.-Aufruf-Zeitpunkt wird autoritativ vom Host** mitgeschickt (gleiche
  Quelle wie die Spielübersicht), bleibt also über Reconnects stabil und ist je
  Turnier frisch; die Schwellen kommen über die Monitor-Konfiguration mit.

## v0.9.54

- **Aufruf-Timer jetzt auch auf dem Court-Monitor.** Steht ein Spiel auf dem
  Feld, zeigt der TV in der Kopfzeile eine hochzählende Uhr + Aufruf-Chip
  („📢 m:ss · 1. Aufruf → 2. Aufruf → Letzter Aufruf", grün→gelb→rot, pulsierend).
  Rechnet relativ zur Server-Zeit (Pi-Uhr oft nicht synchron). Schwellen wie
  bei der Spielübersicht aus **Einstellungen → Aufruf-Timer**.
- *Gilt zunächst für den LAN-Pfad* (Pi am Hallen-WLAN / `bts-light.local`); im
  Cloud-Modus folgt der Timer separat.

## v0.9.53

- **Zählweise aus BTP übernommen.** bts-light liest jetzt das in BTP eingestellte
  Spielsystem (`ScoringFormats`, je `Stage` zugeordnet, Draw → `StageID` → Stage)
  und gibt es ans Zähltablett weiter — statt fest „3×21". Daraus ergeben sich
  **Satzgewinn, Cap und die Intervall-Pause** korrekt je Format:
  - `3×21` → Satz bis 21, Cap 30, Intervall-Pause bei 11.
  - `3×15 (21)` → Satz bis 15, **Cap 21**, **Intervall-Pause bei 8** (auch der
    Seitenwechsel im Entscheidungssatz).
  - 11er-Sätze (Cap 11/15/13) entsprechend; unbekannte Formate fallen sicher
    auf 3×21 zurück.
- **Diagnose-Log:** die erkannten Zählweisen werden bei Turnier-Wechsel ins Log
  geschrieben (ohne Spielernamen), zur Kontrolle gegen BTP.
- *Bekannte Grenze:* ein abweichender **Entscheidungssatz** (`LastSetType`, z. B.
  Decider zu 11 statt 21) wird noch nicht gesondert ausgewertet — alle Sätze nutzen
  das reguläre Format. Folgt bei Bedarf.

## v0.9.52

- **Aufruf-Timer (1./2./3. Aufruf).** Der Aufruf aufs Feld ist der 1. Aufruf;
  bts-light zeigt je belegtem Feld eine **hochzählende Uhr** und meldet ab den
  eingestellten Minuten den **2.** und **3./letzten** Aufruf als fällig
  (grün → gelb → rot). Schwellen einstellbar in den **Einstellungen → Aufruf-Timer**
  (unter den Ansagen). Anzeige in **Spielübersicht** und **Ansagen**-Seite.
  Der Zeitpunkt wird serverseitig je Feld festgehalten (überlebt
  Seitenwechsel/Neuladen); wechselt das Spiel auf dem Feld, läuft die Uhr neu.
  *Court-Monitor-Anzeige folgt separat (eigener Datenpfad).*

## v0.9.51

- **Neue, durchgängige Navigation.** Statt Dashboard-„Hub" mit Zurück-Button gibt
  es jetzt eine **immer sichtbare Seitenleiste** (Status · Spielübersicht · Tablets ·
  Ansagen · Monitore · Einstellungen) — von jedem Bereich direkt in jeden anderen,
  ohne Zurück. Oben eine **feste Kopfzeile** mit Verband, Live-Status-Punkt und
  Start/Stoppen (von überall erreichbar).
- **Feature-abhängige Menüpunkte.** „Ansagen" und „Monitore" sind immer sichtbar,
  aber **ausgegraut**, solange sie nicht aktiviert sind; ein Klick führt direkt in
  den passenden **Einstellungen**-Abschnitt. Nach dem Aktivieren wird der Punkt
  sofort nutzbar (kein Neustart).
- **Einstellungen als Dauer-Seite.** Der Einrichtungs-Assistent ist jetzt auch
  jederzeit über die Seitenleiste erreichbar (mit kurzer „Gespeichert"-Bestätigung);
  der geführte Assistent erscheint nur noch bei der Erst-Einrichtung.
- **Neu: Ansagen-Seite.** Manuelle Feld-Ansage je laufendem Spiel + Test-Ansage
  (Grundlage für den künftigen Aufruf-Timer / 2.+3. Aufruf).

## v0.9.50

- **Spiele per Drag-and-Drop aufs Feld ziehen.** In der Spielübersicht lassen
  sich Spiele jetzt direkt auf ein freies (grünes) Feld ziehen (Klick-Auswahl
  bleibt als Alternative).
- **„Auf Feld"-Liste.** Bereits zugewiesene Spiele verschwinden nicht mehr aus
  der linken Liste, sondern erscheinen farblich markiert (gelb) mit Feldnummer.
- **Freigeben entfernt Halle+Feld am Match in BTP.** Beim Freigeben wird jetzt
  nicht nur die Court-Verknüpfung gelöst, sondern auch `Match.CourtID` gelöscht
  (`court_id=0`) — Halle und Feld verschwinden so aus den BTP-Match-Eigenschaften.
  Zuweisen setzt `Match.CourtID` zusätzlich konsistent mit (Vorbild Original-BTS).
  Technik: `proto.rs court_assign_request` (Courts- + Matches-Block in einem
  SENDUPDATE, ohne Ergebnis), `match_planning()`-Lookup.

## v0.9.49

- **Feldsteuerung: Spielübersicht + Feldvergabe (schreibt nach BTP).** Neue Seite
  „Spielübersicht" (Dashboard → Button): links die spielbereiten Spiele, rechts
  die Felder als **Ampel** — grün=frei, gelb=belegt, rot=gesperrt. Spiel wählen +
  freies Feld anklicken → **Match auf Feld zuweisen**; belegtes Feld → **freigeben**;
  je Feld ein **Sperren**-Umschalter (gesperrte Felder werden nicht belegt;
  bts-light-seitig, in der Config persistiert).
- **Bidirektional:** Zuweisen schreibt via `SENDUPDATE`-Courts-Block nach BTP
  (Vorbild: Original-BTS); umgekehrt wird eine in BTP gesetzte Zuweisung weiter
  gelesen. Die aktuelle Belegung kommt immer aus dem BTP-Snapshot (eine Wahrheit).
  Voraussetzung: in BTP müssen Netzwerk-Edits aktiv sein.
- Technik: `proto.rs courts_update_request` + `write_courts_to_btp`, Commands
  `assign_court`/`free_court`/`set_court_locked`; `locked_courts` in Config + State.

## v0.9.48

- **Einbettcode nur noch an einer Stelle.** Die „Website-Einbettung"-Karte vom
  Dashboard entfernt — der Einbettcode wird jetzt ausschließlich über die
  „Code"-Buttons je Verband im Setup-Wizard gepflegt (eine Quelle, kein
  Doppel-Pflegen). `EmbedCodeCard` entfällt; Snippet lebt zentral in
  `embedSnippet.ts`.
- **Einheitliche Kartenbreite** im Liveticker-Ziel: alle Preset-Karten füllen
  jetzt die volle Breite (`ChoiceCard` w-full), statt sich an die Textlänge
  anzupassen.

## v0.9.47

- **Einbettcode = kompakte „Jetzt live"-Box (WordPress-sicher).** Der
  Copy-Button liefert jetzt den Einzeiler
  `<script src="https://badhub.de/embed/badge.php" data-key="…"></script>`
  (statt des vollen iFrames) — die kompakte Box erscheint nur bei laufendem
  Turnier und verlinkt zum Liveticker.
- **Einbettcode je Verband im Setup-Wizard.** Hinter jeder LV-Preset-Karte ein
  „Code"-Button, der den fertigen Einbettcode des jeweiligen Verbands kopiert
  (kein Umweg übers Dashboard). Gemeinsamer Helper `embedSnippet.ts`,
  Dashboard-Karte nutzt denselben Snippet.

## v0.9.46

- **5 weitere Landesverbände als Preset.** Der Setup-Wizard bietet neben BVBB
  jetzt auch **BVRP, HBV, BBV, BWBV, NBV** als Ein-Klick-Ziel (eigene
  Liveticker-Adresse + Push-Token je Verband, einheitlicher Karten-Look).
- **Website-Einbettung mit Copy-Button.** Neue Dashboard-Karte
  „Website-Einbettung": zeigt den fertigen iFrame-Code für die Verbands-Website
  (WordPress) passend zum konfigurierten Turnier (`badhub.de/embed/live.php?t=…`,
  mit Auto-Höhe per postMessage) und kopiert ihn per Klick.
- **Hinweis für eigene Turniere.** Im manuellen Setup („Anderes Turnier") eine
  Infobox: für eine eigene Liveticker-Adresse vorab an info@badhub.de wenden.

## v0.9.45

- **Schnellere Selbstheilung nach Netzausfall.** Der Server-Timeout für tote
  Tablet-Verbindungen von 30 s auf **10 s** verkürzt. Da das jetzt kürzer ist
  als der Tablet-Watchdog (15 s), ist das Feld nach einem Router-/WLAN-Ausfall
  serverseitig schon frei, **bevor** sich das Tablet neu meldet – das „Feld
  wird bereits geschiedst"-Overlay erscheint dann gar nicht mehr und das
  Tablet belegt das Feld direkt selbst neu (kein manuelles „Übernehmen"). Auf
  gesunder Verbindung unkritisch: der Protokoll-Ping hält `last_seen` alle
  ~2 s frisch.

## v0.9.44

- **Zähltafelbediener-Hinweis auf dem Tablet-Spielzettel (Teil 2).** Bei der
  Seitenwahl zeigt das Tablet jetzt direkt, wer voraussichtlich die Zähltafel
  bedient: das Verlierer-Team des zuletzt auf diesem Feld beendeten Spiels
  („🧮 Zähltafel / Scoreboard: …"). `MatchBrief` trägt dafür ein neues Feld
  `scorekeeper` (vom Server aus `TabletState::scorekeeper`, LAN + Cloud),
  `#[serde(default)]` für Abwärtskompatibilität. Ergänzt Teil 1 (Übersicht in
  bts-light, v0.9.39). Kein Vorspiel auf dem Feld → kein Hinweis.
- **Pi-Court-Monitore: „German / English"-Übersetzungs-Pille unterdrückt.**
  Der Chromium-Kiosk läuft jetzt mit `--lang=de-DE`/`--accept-lang` und
  `--disable-features=Translate,TranslateUI` – Seite (deutsch) und UI-Sprache
  stimmen überein, sodass Chromium keinen Übersetzen-Hinweis mehr oben rechts
  einblendet. Wirkt nach erneutem `setup-monitor.sh` + Pi-Neustart.

## v0.9.43

- **TV-Anzeige verliert nach einem Netzausfall nicht mehr den Spielstand.**
  Sprang der TV nach einem kurzen Router-/Netzausfall auf 0:0 zurück (obwohl
  das Tablet weiterzählte) und kam nicht wieder, lag das an gleich mehreren
  Schwachstellen im Live-Score-Pfad. Behoben:
  - **Sticky Score:** Liveticker-Push und Felder-Übersicht vertrauten dem
    Tablet-Stand nur bei *offener* WebSocket-Verbindung – ein kurzer
    Aussetzer warf sie auf BTPs 0:0 zurück. Jetzt zählt der zuletzt
    gemeldete Stand für dasselbe Match unabhängig vom Verbindungsstatus
    (wie schon beim Feldmonitor); `verbunden` ist nur noch der Online-Indikator.
  - **Persistenz:** Der laufende Satzstand wird je Feld in `live-scores.json`
    gesichert und beim Start wieder geladen. Ein App-Neustart (Absturz,
    Standby) wirft den TV damit nicht mehr auf 0:0, bis das Tablet zurück ist.
    Atomar geschrieben (Temp-Datei + Rename), Schreiber serialisiert.
  - **Tote Verbindungen freigeben:** Bricht der Router weg, schickt der
    Browser oft kein „Close" – die Verbindung hing serverseitig und hielt das
    Feld „belegt", sodass das zurückkehrende Tablet ausgesperrt blieb. Der
    Server erkennt jetzt stille Verbindungen (Protokoll-Ping; >30 s ohne
    Lebenszeichen) und gibt das Feld frei.
  - **Selbstheilender Reconnect:** Hört das Tablet beim Wiederanmelden „Feld
    belegt", versucht es sich (wenn es das laufende Match hält) automatisch
    alle 4 s neu anzumelden und re-pusht nach erfolgreicher Übernahme sofort
    seinen Stand – ohne manuelles „Übernehmen". Ein echt fremdes Tablet
    behält das Feld; dann entscheidet weiter der Mensch.

## v0.9.42

- **Einzel- und Kombi-Anzeige einheitlich.** Drei Angleichungen:
  - Aufschlag-Punkt steht jetzt auf beiden Ansichten **vor der Flagge**
    (Punkt → Flagge → Name); vorher saß er auf der Einzel-Ansicht hinter
    dem Namen.
  - Flaggen einheitlich groß: feste Box + `object-fit:cover` auch auf der
    Kombi-Anzeige (vorher variable Breite je Seitenverhältnis).
  - Einzel-Ansicht hebt abgeschlossene Sätze jetzt auch **während des
    laufenden Spiels** den Satzsieger hell (weiß) hervor — wie die
    Kombi-Anzeige; vorher erst nach Spielende. Bei Aufgabe weiterhin keine
    Satz-Hervorhebung (letzter Satz unvollständig).

## v0.9.41

- **Einzel-Court-Ansicht: Aufschlag-Punkt spieler-genau im Doppel.** Auf
  dem Einzel-Feldmonitor (`monitor.html`) saß der gelbe Aufschlag-Punkt im
  Doppel/Mixed noch auf Team-Ebene (bei beiden Spielern). Jetzt steht er
  beim **konkret aufschlagenden Spieler** — dieselbe BWF-Logik wie auf der
  Kombi-Anzeige. Nutzt das vom Tablet berechnete `serving:{team,index}`;
  altes Tablet ohne die Info → Punkt beim ersten Spieler des Teams. Einzel
  unverändert.

## v0.9.40

- **Tablet-Auto-Reconnect (Heartbeat).** Das Tablet verbindet sich jetzt
  selbstständig neu, wenn der Server/Router kurz weg war — kein manuelles
  Seite-neu-Laden mehr nötig. Ein Watchdog (alle 5 s) sendet ein Ping und
  erkennt **tote Verbindungen auch dann, wenn der Browser kein `onclose`
  liefert** (Router weg → nur Stille): kam >15 s nichts vom Server, gilt
  die Verbindung als tot und wird neu aufgebaut. Backoff auf max. 5 s
  verkürzt (vorher 30 s). Der Watchdog ist der **einzige** Reconnect-
  Treiber (keine doppelten Sockets mehr).
  - `TabletMsg::Ping` / `ServerMsg::Pong` (relay-proto); LAN-Server
    *(server.rs)* und Cloud-Relay *(relay/main.rs)* antworten je sofort
    mit Pong.
- **Kombi-Anzeige: Feldnummer hervorgehoben.** Die Feldnummer am
  Bandanfang steht jetzt größer und als gelbes Badge (dunkler Text auf
  gelbem Block) — aus der Ferne sofort erkennbar.

## v0.9.39

- **Zähltafelbediener (Teil 1: bts-light-Übersicht).** bts-light merkt
  sich jetzt je Feld den **Verlierer des zuletzt dort beendeten Spiels**
  — das ist der voraussichtliche Zähltafelbediener fürs nächste Spiel.
  In der „Tablet-Spielzettel"-Übersicht steht er beim Feld mit
  Tablet-Symbol. Da BTP beendete Spiele nicht zuverlässig dem Feld
  zugeordnet behält, **trackt der Sync-Loop den Übergang OnCourt→Finished
  selbst** (kein Verlass auf BTP, keine externe DB — In-Memory pro Feld).
  - `TabletState.scorekeeper_by_court` + `SyncEngine.track_scorekeepers`
    (vergleicht zyklisch, welches Spiel ein Feld verlassen hat).
  - `CourtOverview.scorekeeper` (Verlierer-Namen), in TabletPanel angezeigt.
  - Teil 2 (Hinweis direkt auf dem Tablet-Spielzettel bei der Seitenwahl)
    folgt separat.

## v0.9.38

- **Aufschlag-Indikator spieler-genau im Doppel/Mixed.** Der gelbe Punkt
  steht jetzt beim **konkret aufschlagenden Spieler** (nicht mehr nur beim
  Team) und wechselt regelkonform: Bei geradem Punktestand des
  aufschlagenden Teams serviert der Spieler im rechten Aufschlagfeld, bei
  ungeradem der im linken; bei Side-out wechselt das Team. Das Tablet
  berechnet den Aufschläger (es kennt Positionen + Spieler-IDs) und legt
  `serving: {team, index}` in den `court_state`; `CourtOverview` trägt
  `serving_team` + `serving_player`, `combo.html` setzt den Punkt bei der
  richtigen Namens-Zeile. Einzel: Punkt beim einzigen Spieler. Alte
  Tablet-Stände ohne die Info → Team-Level-Fallback.

## v0.9.37

- **Fix: kein „Geistersatz" mehr nach Spielende.** Nach dem Match-Ende
  setzt das Tablet den laufenden Satz auf 0:0 zurück; `handle_score`
  hängte diesen leeren Satz an die Satzliste → in Kombi-/Übersicht-/
  Liveticker-Anzeige erschien ein zusätzlicher leerer Satz. Ein 0:0-Satz
  wird jetzt nicht mehr angehängt, wenn bereits Sätze gespielt sind
  (der allererste 0:0-Satz bleibt).
- **Fix: Monitor synct nach Netzwerk-Unterbrechung wieder.** Fiel der
  bts-light-Rechner kurz offline (Router/WLAN) und die Tablets zählten
  weiter, blieb der Kombi-Monitor nach dem Reconnect auf dem alten
  Stand. Das Tablet pusht jetzt beim Wiederverbinden (`ws.onopen`)
  sofort seinen aktuellen Satzstand + Spielzustand (Aufschlag/Pause) an
  den Server — Monitore + Liveticker holen damit den weitergezählten
  Stand vom Tablet zurück.
- **Kombi-Anzeige: Aufschlag-Indikator.** Vor dem aufschlagenden Team
  steht jetzt ein gelber Punkt (abgeleitet aus dem Tablet-Spielzustand:
  servingSide + teamOnSide). Zeigt auf einen Blick, welches Team
  aufschlägt; wechselt beim Aufschlagwechsel. `CourtOverview` trägt dazu
  ein `serving_team`-Feld (1/2/none).

## v0.9.36

- **Kombi-Anzeige: Ergebnis-Zahlen viel größer + ruhiger.** Die Satz-
  Zahlen skalieren jetzt mit der Feldzahl und nutzen die Bandhöhe aus
  (1 Feld ~30vh, 2 ~19vh, 3 ~13vh) — auf Distanz klar lesbar. Der
  „läuft"-Status (Punkt + Text) ist entfernt (redundant, kostete Platz);
  der laufende Satz wird nur noch farblich (gelb) markiert, **ohne
  Unterstrich**. Frei/Pause/TL/Behandlung bleiben als Status sichtbar.
- **Tablet: Zurück zur Aufstellung bei 0:0.** Wenn nach der Seiten-/
  Aufschlagwahl versehentlich zu schnell getippt wurde, führt der
  ↩-Button bei 0:0 (noch kein Punkt) zurück zur Aufstellung statt ins
  Leere. Das Button-Label wechselt dann zu „↩ Aufstellung ändern".

## v0.9.35

- **Fix: Auto-Update-Versionssprung repariert.** Ab v0.9.32 hatte der
  Versions-Bump (`package.json`/`tauri.conf.json`/`Cargo.toml`) nicht
  gegriffen — alle Builds v0.9.32–v0.9.34 trugen intern noch **0.9.31**.
  Folge: `latest.json` meldete eine neue Versionsnummer (aus dem Tag),
  der Installer war aber intern 0.9.31 → der Windows-Updater installierte
  faktisch wieder 0.9.31 und blieb in einer Update-Schleife. Mit v0.9.35
  stimmen Tag und interne Version wieder überein; das Update greift und
  bringt **alle** Fixes/Features aus v0.9.27–v0.9.35 auf einmal.
- **CI: Releases werden serialisiert** (`concurrency`-Group), damit nie
  zwei Publish-Jobs parallel ins Auto-Update-Verzeichnis schreiben und
  eine inkonsistente `latest.json` hinterlassen.

(Inhaltlich enthält 0.9.35 alle Änderungen seit 0.9.31: finishManually-
Push, Geräteliste sortiert/gruppiert, offline-Geräte entfernen.)

## v0.9.34

- **Offline-Geräte aus der Liste entfernen (X).** Offline-Monitore haben
  jetzt ein **X** zum Entfernen aus der „Court-Monitore"-Liste (vergisst
  den Live-Eintrag + löscht eine eventuelle Zuweisung). **Online-Geräte
  haben kein X** und werden auch server-seitig abgelehnt — sie kämen eh
  beim nächsten Poll zurück und sollen ihre Zuweisung nicht verlieren.
  Neuer Command `forget_monitor_device` (prüft `is_monitor_online`).

## v0.9.33

- **Fix: TV zeigt nach manuellem „Match beenden" den Endstand.**
  `finishManually()` pushte den finalen Stand nicht an Server/TV (wie
  zuvor schon `reopen()` nicht) → der Court-Monitor hing auf dem letzten
  Live-Stand. Ruft jetzt `sendScoreUpdate()` (Code-Review-Finding).
- **Court-Monitore-Übersicht: sortiert, gruppiert, offline unten.** Die
  Geräteliste in „Court-Monitore" ist jetzt aufgeräumt:
  - **Online-Geräte oben, offline darunter** unter einer „offline"-
    Trennlinie (ausgegraut) — keine Bereinigung nötig, störende
    Altgeräte rutschen nach unten.
  - Bei **mehreren Hallen** nach Halle gruppiert (Zwischenüberschrift).
  - Sortierung: **Felder zuerst (Feld 1 oben, dann 2, 3 …), dann
    Kombi-Felder, dann Info-/Werbe-TVs, dann unzugewiesene.**

## v0.9.32

- **Pausen-Countdown auf Tablet und TV synchron.** Das Tablet setzte
  `endsAt` mit seiner eigenen Uhr; der TV rechnet (seit v0.9.29) gegen
  die Server-Uhr → bei abweichenden Geräteuhren liefen die Countdowns
  5–6 s auseinander. Das Tablet holt jetzt per `/health` (neues Feld
  `serverNowMs`) seinen Uhr-Offset zum Server und setzt/zählt die Pause
  in **Server-Zeit** (`serverNow()`). Damit zeigen Tablet und TV
  denselben Wert. Offset wird beim Start und alle 30 s aktualisiert;
  ohne Verbindung Fallback auf die lokale Uhr.
- **Kombi-Anzeige lesbarer.** Die Satz-Zahlen sind deutlich größer
  (7vh, fett) und der laufende Satz stärker hervorgehoben (Glow). Im
  Doppel stehen die beiden Spieler eines Teams jetzt **untereinander**
  (A1 / A2) statt nebeneinander, **mit Flagge** je Spieler.
- **Court-Übersicht (`/info/overview`) zeigt jetzt Spielstände.** Je Feld
  beide Teams mit **Flagge**, Name(n) und **Satzstand** (gewonnene Sätze
  hervorgehoben, laufender Satz gelb) — vorher nur Teams + Status.
- **Court-Übersicht: dynamische Kachelgröße.** Das Feld-Raster passt die
  Spaltenzahl an die Feldanzahl an (1→1, 2→2, 3-4→2, 5-6→3 … bis 4) und
  füllt die Bildschirmhöhe (gleich hohe Zeilen). Bei wenigen Feldern
  (z. B. 4) große, bildschirmfüllende Kacheln statt kleiner Boxen oben.

## v0.9.31

- **Fix: TV übernimmt den Stand nach „Match wieder öffnen".** `reopen()`
  pushte den wiederhergestellten Stand nicht an den Server → der
  Court-Monitor hing auf dem alten beendeten Stand (zeigte z. B. 0:0 im
  laufenden Satz statt 20:17, und die alten Satz-Zahlen). `reopen()` ruft
  jetzt `sendScoreUpdate()` (wie `undo()`), der Server ersetzt die
  Satzliste, der TV zeigt beim nächsten 1-s-Poll den korrigierten Stand.
- **Neu: Korrektur direkt aus der Pause.** Im Pausen-Overlay (11er-/
  Satzpause) gibt es jetzt einen Button „↩ Korrektur — letzter Punkt
  zurück": bricht die Pause ab und nimmt den auslösenden Punkt zurück
  (z. B. wenn der Ball wiederholt werden muss und die Pause zu früh kam).
  Erscheint nur, wenn ein Punkt zum Zurücknehmen vorhanden ist.

## v0.9.30

- **Fix: „Match wieder öffnen" stellt den echten Stand auch nach einem
  Tablet-Reload her.** Die Undo-/Reopen-History wurde bewusst nicht
  persistiert. Endete ein Match automatisch (gewinnender Punkt) und das
  Tablet wurde danach neu geladen / reconnectete, war die History weg —
  `reopen()` konnte den letzten Stand (z. B. 20:1) nicht zurückholen und
  zeigte einen leeren `currentSet` (0:0) als zusätzlichen Satz. Die
  History wird jetzt mit in `localStorage` gesichert (auf 50 Snapshots
  gecappt) und beim Laden wiederhergestellt. „Match wieder öffnen" bringt
  damit den korrekten Stand + die korrekten Seiten zurück, und Korrektur
  per Undo funktioniert auch nach Pause/Reload (vorher war Undo bei
  leerer History gesperrt).

## v0.9.29

- **KRITISCHER Fix: Punkte landen nach „Match wieder öffnen" nicht mehr
  beim falschen Gegner.** `snapshot()`/`restoreSnapshot()` im Tablet-
  Spielzettel speicherten `teamOnSide` (welches Team auf welcher Seite
  steht) nicht. `swapSides()` (Satzende + Mid-Game-Switch bei 11 im
  Decider) flippt diese Zuordnung aber. Beim Undo/Wiederöffnen über eine
  solche Grenze blieb `teamOnSide` auf dem geflippten Stand, während
  `positions`/`currentSet`/`setsCompleted` zurückgesetzt wurden → die
  Team↔Seite-Zuordnung war gespiegelt und getippte Punkte gingen an den
  **falschen Gegner**. Jetzt wird `teamOnSide` (und `intervalDoneThisGame`)
  mit im Snapshot gesichert und korrekt wiederhergestellt. Alte, in
  localStorage liegende Snapshots ohne das Feld bleiben lesbar.
- **Fix: Pausen-Countdown + Match-Uhr auf dem TV stimmen wieder.** Der
  Court-Monitor (Pi) rechnete Pausen-Restzeit und Spieldauer mit seiner
  **eigenen** Uhr (`Date.now()`) gegen ein absolutes `endsAt`/`startedAt`
  vom Tablet. Pi Zero hat keine RTC und oft keine NTP-Synchronisation im
  Turnier-WLAN → die Uhr driftet, der Countdown war z. B. **+1 Minute**
  zu hoch (Tablet 1 min → TV 2 min). `MonitorState` trägt jetzt
  `serverNowMs` (Server-Zeit beim Poll); `monitor.html` rechnet relativ
  dazu statt zur Pi-Uhr. Fallback auf `Date.now()` bei alten Frames.

## v0.9.28

- **Kombi-Monitor Code-Review-Fixes (v0.9.27).**
  - `/combo/state` cappt die Felderzahl jetzt serverseitig auf **3** und
    entfernt **Duplikate** — eine manuell gebaute URL `?courts=1,1,1,…`
    kann das Band-Layout nicht mehr unleserlich machen.
  - `combo.html::setVal` vereinfacht (toter Parameter entfernt) +
    Fallback `0` statt `"undefined"` in der Satz-Zelle bei
    abweichendem Schema.
- **Chromium-Übersetzungsleiste auf den Pi-Monitoren aus.** Der
  Kiosk-Aufruf in `pi/setup-monitor.sh` bekommt
  `--disable-features=Translate --disable-translate` — damit erscheint
  die „German / English / Diese Seite übersetzen?"-Leiste oben rechts
  nicht mehr.

## v0.9.27

- **Kombi-Court-Monitor: bis zu 3 Felder auf einem Bildschirm.** Ein
  großer TV kann jetzt die Live-Spielstände von 2–3 Feldern gleichzeitig
  zeigen — als horizontale Bänder untereinander, je Feld Feldname,
  Disziplin, Status (läuft/Pause/TL/frei), beide Teams (Doppel-tauglich)
  und Satzstand mit hervorgehobenem laufendem Satz. So deckt man mit
  wenigen großen Bildschirmen viele Felder ab statt ein TV pro Feld.
  - Neue `MonitorTarget`-Variante `CourtCombo { court_ids }`
    (Wire-Form `{"kind":"court_combo","court_ids":[1,2,3]}`).
  - Neue Anzeige-Seite `combo.html` + Routen `/combo` und
    `/combo/state?courts=1,2,3` (filtert die Felder-Übersicht auf die
    gewählten CourtIDs, Reihenfolge = Band-Reihenfolge). 1-s-Poll,
    Pivot (`?rotate=`), Heartbeat wie die anderen Info-Seiten.
  - Zuweisung über einen **Kombi-Dialog** im „Court-Monitore"-Bereich:
    Dropdown-Eintrag „Felder wählen…" → Modal mit Feld-Checkboxen
    (2–3, Auswahl-Reihenfolge nummeriert). Aktive Kombi wird im
    Dropdown angezeigt.
  - Cloud-Modus: wie Info/Ad LAN-only (CourtCombo hat keine einzelne
    `court_id`, wird im Relay-Filter ausgeschlossen).

## v0.9.26

- **Schnellere Umstellung weg von Info-/Werbe-Anzeigen.** Ein Pi auf
  einer Info- oder Werbe-Seite (Courtübersicht, In Vorbereitung,
  Werbung) prüfte bisher nur **alle 30 s**, ob seine Zuweisung sich
  geändert hat — beim Umschalten zurück auf ein Feld (oder ein anderes
  Target) dauerte es entsprechend lang. Im LAN ist dieser Check ein
  winziger HTTP-GET; das Intervall ist jetzt auf **1 s** gesenkt
  (`overview.html`, `preparation.html`, `ad.html`) — gleich schnell wie
  `monitor.html`. Damit wirkt **jede** Umstellung im LAN binnen ~1 s,
  egal aus welcher Anzeige heraus.

## v0.9.25

- **Werbebilder mit Anzeigenamen.** In den Einstellungen → Werbebilder
  hat jedes Bild jetzt ein freies Textfeld für seinen Anzeigenamen
  (z. B. „Sommerfest 2026", „Sponsor Hauptbruecke"). Der Name wird in
  einer separaten JSON-Datei (`court-ad-labels.json`) persistiert und
  taucht in der „Werbung"-Sektion des Court-Monitor-Dropdowns statt
  des kryptischen `ad-1234567890.jpg` auf. Bilder ohne Label fallen
  auf den Dateinamen zurück. Beim Löschen eines Bilds wird der
  zugehörige Label-Eintrag mit aufgeräumt.
- **Tauri-Command `list_court_ads` ändert Rückgabetyp** von `Vec<String>`
  auf `Vec<CourtAd>` (`{file, label}`). Frontend nutzt jetzt `CourtAd[]`
  überall. Neuer Command `set_court_ad_label` zum Speichern.
- **MonitorTarget bleibt referenziert über `file`** (nicht Label) — eine
  Umbenennung in der UI bricht keine bestehenden Pi-Zuweisungen.

## v0.9.24

- **Default-Anzeige (Logo) übernimmt das App-Header-Design.** Statt des
  Badhub-Federball-PNGs zeigt der Pi jetzt das **gleiche Icon wie die
  bts-light-App selbst** (Dashboard-Header): Federball-Emoji 🏸 in einem
  dunklen Rounded-Square mit Schatten. Darunter Wordmark „badhub.de",
  darunter klein „BTS light". Dieselbe Atem-Animation wie vorher.
- **`fonts-noto-color-emoji` in `setup-monitor.sh`.** Pi OS Lite hat
  standardmäßig nur Mono-Schriften — ohne diese Font würde das Emoji
  als leeres Kästchen rendern. Wird beim ersten Setup-Lauf
  automatisch mit installiert. Auf Pis, die schon laufen, einmalig
  manuell nachziehen: `sudo apt-get install -y fonts-noto-color-emoji`
  und Chromium reloaden.
- **Unbenutztes Logo-PNG + Route entfernt** (`/assets/badhub-logo.png`,
  `BADHUB_LOGO_PNG`, `src-tauri/assets/badhub-logo.png`) — wurde nur in
  v0.9.23 kurz gebraucht und ist jetzt durch das Emoji-Design abgelöst.

## v0.9.23

- **Default-Anzeige für unzugewiesene Pis: Badhub-Logo Vollbild.**
  Statt der bisherigen Kopplungs-Karte mit großem Code zeigt ein Pi,
  der noch keinem Feld/Info-Target zugewiesen ist, jetzt das
  Badhub-Logo zentriert mit „badhub.de"-Wordmark darunter und einer
  sanften Atem-Animation. Sieht im Verleih-Set wie „läuft" aus, nicht
  wie „eingerichtet aber nichts darauf". Logo (PNG, 4 kB) ist in die
  bts-light-Binary eingebettet, neue Route `/assets/badhub-logo.png`.
- **„Identifizieren" zeigt jetzt den Device-Code Vollbild.** Der bisherige
  Identify-Overlay-Code (gelb, blinkend) bleibt — aber jetzt die einzige
  Stelle, an der der Code groß sichtbar wird. Operator klickt „Identifi-
  zieren" im Tool, der entsprechende Pi blendet seinen Code für 10 s
  (vorher 6 s) ein. Damit ist die Pi→Code-Zuordnung sauber bedienbar
  ohne den Code immer am TV anzuzeigen.

## v0.9.22

- **Online-Status auf Info-Pages korrigiert.** Der Pi auf einer
  Info-Page (Court-Übersicht, In Vorbereitung, Werbung) wurde in der
  „Court-Monitore"-Liste bisher als **offline** angezeigt, obwohl er
  problemlos läuft. Grund: `record_monitor_poll` lief nur in
  `/monitor/state`, das von Info-Pages aber nur alle 30 s gepollt wurde
  (Reassignment-Check) — der Server hat den Pi 24 von 30 s nicht
  gesehen, das Online-Fenster ist aber nur 6 s. Beim Entfernen oder
  Wechseln der Zuweisung dauerte es entsprechend lang, bis der Pi
  wieder als online angezeigt wurde.
- **Fix:** Die Info-State-Endpoints (`/info/ad/state`,
  `/info/preparation/state`, `/health`) akzeptieren jetzt einen
  optionalen `?device=<id>`-Query-Param. Wenn der gesetzt ist, zählt
  jeder dieser Polls als Lebenszeichen — der Pi gilt durchgehend als
  online. `ad.html`, `overview.html`, `preparation.html` schicken die
  Geräte-ID jetzt mit.
- **`ad.html` pollt schneller (5 s statt 60 s).** Neue Werbebilder
  erscheinen damit auch ohne Reboot/Reassignment auf dem Pi — und der
  schnellere Poll trägt direkt zum Online-Heartbeat bei.

## v0.9.21

- **Code-Review-Fixes zum Werbe-Target (v0.9.20).**
  - `read_assignments` parsed v3 jetzt **pro Eintrag** mit
    `serde_json::Value`-Zwischenstufe statt das ganze Map auf einmal.
    Schutz vor Datenverlust bei Downgrade: bisher hätte ein User, der
    eine Werbe-Zuweisung gesetzt hat und dann auf v0.9.18/v0.9.19
    zurückrollt, **alle** Court-Zuweisungen verloren (ein einziger
    unbekannter Eintrag → Map-Parse failed → leere Map). Jetzt: nur die
    unbekannten Einträge fallen weg, bekannte bleiben. Regressionstest
    in `monitor.rs`.
  - `ad.html`: `applyState` hat ein Dirty-Tracking — der 60-s-Pool-Poll
    triggert nicht mehr unnötig Cross-Fade auf das gleiche Bild und
    resettet auch nicht das Rotations-Intervall. Im `single`-Modus
    wird `showImage` nur bei tatsächlichem File-Wechsel gerufen.
  - `ad.html`, `overview.html`, `preparation.html`: bei
    Re-Assignment-Navigation (z. B. Pi wechselt von einem Info-Target
    zu einem anderen) wird der `?rotate=…`-Pivot-Param mitgenommen.
    Bisher ging die Rotations-Einstellung jedesmal verloren.

## v0.9.20

- **Werbe-Target im Court-Monitor-Dropdown.** Pis lassen sich jetzt
  nicht nur Feldern oder Info-Displays zuweisen, sondern auch direkt
  einer Werbe-Anzeige. Im „Court-Monitore"-Dropdown gibt es eine
  dritte Sektion „Werbung" mit zwei Modi:
  - **Rotierend:** alle hinterlegten Werbebilder im Wechsel, Intervall
    aus den Court-Monitor-Einstellungen (`ad_interval_s`).
  - **Einzelbild:** ein bestimmtes Werbebild Vollbild, dauerhaft.
  Wenn keine Werbebilder hinterlegt sind, ist die ganze Sektion
  ausgegraut. Neue Anzeige-Seite `assets/ad.html` mit Cross-Fade-
  Animation; Bilderpool wird alle 60 s frisch geholt, sodass das
  Hochladen neuer Bilder ohne Neustart wirkt.
- **`MonitorTarget` erweitert** um die Varianten `AdRotation` und
  `AdSingle { file }` (Wire-Form
  `{"kind":"ad_rotation"}` und `{"kind":"ad_single","file":"…"}`). Damit
  ist der Enum nicht mehr `Copy` — wo bisher `.copied()` reichte, ist es
  jetzt `.cloned()` (zwei Stellen angepasst, sonst transparent).
  `redirect_path()` liefert für Ad-Targets Pfad+Query
  (z. B. `/info/ad?mode=single&file=…`).
- **Reassignment-robust für Ad-Single.** Wechselt der Operator das
  Einzelbild eines Pis von `a.png` auf `b.png`, vergleicht `ad.html`
  beim 30-s-Poll den vollen Pfad+Query (nicht nur `pathname`) und
  navigiert auf das neue Bild. Kein Reload-Loop, kein Hängenbleiben
  auf dem alten Bild.

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
