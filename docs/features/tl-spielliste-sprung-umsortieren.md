# Spiele umsortieren ohne langes Ziehen (TL-Web) — Spezifikation

> Status: **Entwurf** (via /idee: Brief → Grill → How-To → Review), 2026-08-26.
> Quelle: Feldtest-Rückmeldung vom 26.08.2026. Betroffene Crates: ausschließlich
> `src-tauri/assets/tl.html` und `src/io/` — **kein** Rust, **kein** Wire-Vertrag.
> ADR: [docs/adr/0050-verschiebe-modus-globales-einfuegeziel.md](adr/0050-verschiebe-modus-globales-einfuegeziel.md)
> (Ergänzung zu ADR 0026).

## Kontext / Problem

Die Turnierleitung sortiert wartende Spiele in der Spielliste der
Turnierleitungs-Weboberfläche um — bisher ausschließlich per Drag & Drop am
Griff (⠿) oder mit den Pfeiltasten ↑/↓ am fokussierten Griff.

Rückmeldung vom 26.08.2026:

> „Das Verschieben in Kombination mit dem Scrolling des Containers der Spiele in
> der Warteliste ist nach wie vor sehr hakelig und macht wenig Spaß. Ziel ist es
> ja, Spiele schnell und einfach umzusortieren." — Nachtrag: „Manchmal aber auch
> um 50/60 Spiele."

Zwei getrennte Ursachen stecken darin:

**Spur A — die Zieh-Geste selbst.** Das Auto-Scrollen hing an den
Zeiger-Ereignissen, der Zeiger fror am Panelrand ein, und die Liste lud während
eines Zugs nicht nach. **Erledigt in v0.9.272**, siehe
[spielliste-manuelle-reihenfolge.md](spielliste-manuelle-reihenfolge.md)
(Nachtrag 26.08.2026).

**Spur B — die Geste taugt für lange Wege grundsätzlich nicht.** Auch mit
perfektem Auto-Scrollen bleibt „Spiel Nr. 55 auf Position 3" eine mehrere
Sekunden lange, **gehaltene** Geste, bei der die ganze Liste unter dem Finger
durchläuft. Diese Spec behandelt ausschließlich Spur B.

Abgefragt wurde, welche Zielpositionen im Betrieb überhaupt vorkommen. Genannt:
**ganz nach vorn** (häufigster Fall), **genau zwischen zwei bestimmte Spiele**
(der teure Fall) und **um ein paar Plätze auf oder ab**. Ausdrücklich **nicht**
genannt: „ganz nach hinten".

Der dritte Fall ist heute schon gut bedienbar (kurzer Zug, Pfeiltasten) und
braucht nichts Neues.

**Für Fall 2 ist entscheidend:** Die Zielstelle wird **optisch** erkannt — „da,
zwischen diese beiden", erkennbar an Disziplin, Namen, Uhrzeit, Halle — nicht
über eine Spielnummer. Eine Eingabe „vor Nr. 217" ginge am Bedarf vorbei: Die
Nummer weiß man in dem Moment gerade nicht. Der Weg muss deshalb
**merken → in Ruhe scrollen → Zielstelle antippen** sein.

**Fall 1 hat sich im Grill als Auffindbarkeitsproblem herausgestellt.** Der
Befehl existiert seit ADR 0026 als Kebab-Eintrag „↑ Nach oben schieben"
(`tl.html:4468`), taucht aber in keiner Doku auf — die ⋮-Aufzählung in
[turnierleitung-web.md](turnierleitung-web.md) nennt ihn nicht. Der Nutzer
kannte ihn nicht. Ein Bedien-Feature, das niemand findet, ist fachlich dasselbe
wie ein fehlendes.

## Zielbild & Erfolgskriterien

**Zielbild.** Ein Spiel von Position 55 auf Position 3 zu bringen kostet: ⋮
öffnen, „⇅ Verschieben…", **normal** nach oben scrollen, Zielzeile antippen.
Kein Halten, kein Konflikt zwischen Ziehen und Scrollen. Der häufigste Fall
(„nach ganz vorn") kostet einen einzigen, **sichtbaren** Tipp an der Zeile.

**Erfolgskriterien.**

1. **Weich, aber der eigentliche Maßstab:** Nach dem nächsten Turnier meldet die
   Turnierleitung nicht mehr, dass Umsortieren hakelig ist. Bewusst so gewählt —
   für ein reines Bedien-Feature ist das die ehrliche Messgröße.
2. **Ohne Erklärung bedienbar:** Ein Turnierleiter ohne technischen Hintergrund
   findet „⇅ Verschieben…" im ⋮-Menü und versteht ohne Rückfrage, dass danach
   ein Tipp auf eine Zeile „hierhin" bedeutet.
3. **Der Weg nach vorn ist sichtbar:** „↑ nach oben" steht als Knopf an der
   Zeile, nicht mehr nur im Menü — und ist dokumentiert.

## Nicht-Ziele

- **Kein Ersatz für Drag & Drop.** Für kurze Wege bleibt der Zug am Griff der
  schnellere Weg; beide Wege stehen gleichberechtigt nebeneinander.
- **Keine Positions- oder Nummerneingabe** („an Position 12", „vor Nr. 217") —
  am tatsächlichen Bedarf vorbei, das Ziel wird optisch erkannt.
- **Kein „ganz nach hinten".** Im Betrieb nicht gebraucht. Folge: **hinter der
  letzten Zeile gibt es keine Einfügemarke** — und damit entfällt der komplette
  `before = null`-Sonderfall samt seiner Kappungs-Falle.
- **Keine regelbasierte Sortierung** („vor alle HE U15").
- **Kein Mehrfach-Verschieben** (mehrere Spiele in einem Zug).
- **Kein Weg in die Desktop-App** (`pages/PreparationPanel.tsx`). Dort gibt es
  Maus und großen Schirm, der Druck ist geringer. Bewusst zurückgestellt, nicht
  verworfen.
- **Keine Änderung an der Serverseite.** Weder ein neuer Tauri-Command noch ein
  neuer `TlAction`-Vertrag; es wird ausschließlich das bestehende
  `queue_reorder` mit zwei Match-IDs gesendet.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - `src-tauri/assets/tl.html` — `queueRow`, `wireQueue`, `renderQueue`,
    `queueReorder`, Kebab-Aufbau, CSS der Zeile, Modus-Band.
  - `src/io/queueGap.mjs` — **neu**, kanonische reine Logik der Zielauflösung.
  - `scripts/test-queue-gap.mjs` + ein Schritt in `.github/workflows/ci.yml` —
    **neu**.
  - **Unangetastet:** `src-tauri/src/**`, `relay/`, `relay-proto/`, `src/pages/`.
- **Architekturregeln (CLAUDE.md R1–R6):**
  - **R1** gewahrt — kein neuer Tauri-Command; TL-Web spricht wie bisher über
    `POST /tl/api/command` mit `TlAction::QueueReorder`.
  - **R2** gewahrt — BTP bleibt die Wahrheit. Die manuelle Reihenfolge
    überschreibt nur die *Sortierung*, keine Court→Match-Zuordnung; das Frontend
    übergibt ausschließlich zwei Match-IDs relativ zueinander.
  - **R3** bedacht, mit einer **Grenze**: Die Seite reist per `include_str!` in
    beiden Binaries (`src-tauri/src/tablet/assets.rs`, `relay/src/main.rs`).
    Cloud-TL-Geräte bekommen sie mit dem nächsten Relay-Deploy (automatisch bei
    jedem main-Merge), LAN-TL-Geräte erst mit dem nächsten **App-Release-Tag**.
    Bis dahin zeigen zwei TL-Geräte am selben Turnier verschiedene Oberflächen.
  - **R4** unberührt — TL-Geräte sind keine Tablets an Feldern.
  - **R5** unberührt — kein Ergebnispfad, `process_result` nicht beteiligt.
  - **R6** unberührt.
- **Konfiguration & Abwärtskompatibilität:** **Keine neuen Felder in
  `config.rs`**, also keine Migration und keine Frage der Lesbarkeit alter
  Konfigurationen. Tauri-`identifier` `de.badhub.btslight` und der Updater-Pfad
  `download/bts-light/` bleiben unangetastet.
- **Datenschutz:** Kein neues Datenfeld. `src/io/queueGap.mjs` arbeitet bewusst
  auf `[{ id, prep_call }]` — die Funktion sieht **nur Match-IDs**, keine
  Spielernamen und kein Geburtsjahr. Über die Leitung geht wie bisher nur
  `{ matchId, beforeMatchId }`.
- **Abhängigkeiten:** Keine. Keine neue Cargo- oder npm-Abhängigkeit, keine
  BTP-Protokolleigenheit, kein badhub-Endpunkt, kein nginx-Eingriff, kein
  Pi-Kiosk.

## Akzeptanzkriterien

**Sichtbarkeit des vorhandenen Befehls**

- [ ] An jeder umsortierbaren Wartelisten-Zeile steht ein sichtbarer Knopf, der
      das Spiel an den Anfang der umsortierbaren Liste setzt — ohne das ⋮-Menü
      zu öffnen.
- [ ] Der Knopf fehlt an einer Zeile, die bereits an der Spitze steht, und an
      einer gerufenen Zeile (`prep_call`) — dieselbe Bedingung wie heute
      (`kannHoch`).
- [ ] Die Wartelisten-Zeile trägt nach dem Umbau **höchstens fünf** direkte
      Flex-Kinder; die Budget-Rechnung im CSS-Kommentar ist neu geschrieben.
- [ ] Die ⋮-Menü-Aufzählung in `docs/turnierleitung-web.md` ist vollständig.

**Verschiebe-Modus — Betreten und Verlassen**

- [ ] Das ⋮-Menü jeder umsortierbaren Zeile enthält „⇅ Verschieben…".
- [ ] Nach dem Betreten steht oben ein Band, das die gemerkte Paarung nennt und
      einen „Abbrechen"-Knopf trägt.
- [ ] Das Betreten löscht eine bestehende Auswahl (`picked`); Aktionsband und
      Modus-Band stehen nie gleichzeitig.
- [ ] **Esc** beendet den Modus, ohne etwas zu senden.
- [ ] Wird das Panel „Spiele" zugeklappt, ausgeblendet oder das Profil
      gewechselt, endet der Modus.
- [ ] Verschwindet das gemerkte Spiel während des Modus aus der Liste (aufs Feld
      gelegt, gewertet, Walkover, Turnierwechsel), endet der Modus **mit einem
      Hinweis** — nicht stillschweigend.
- [ ] Der Modus überlebt den Sekundentakt-Redraw der Liste, aber **nicht** einen
      Seiten-Reload.

**Verschiebe-Modus — Ziel treffen**

- [ ] Während des Modus scrollt die Liste ganz normal (Wischen, Rad,
      Scrollbalken) — es gibt keine gehaltene Geste.
- [ ] Während des Modus lädt die Liste beim Scrollen weiter nach, sodass auch
      Zeilen jenseits der ersten 40 erreichbar sind.
- [ ] Jede zulässige Zielzeile trägt an der Oberkante eine sichtbare Marke, und
      **die ganze Zeile** ist das Ziel (Trefferfläche ≥ 56 px hoch).
- [ ] Keine Marke tragen: die gemerkte Zeile selbst, die Zeile direkt dahinter
      (dort einzufügen ändert nichts), gerufene Zeilen — und es gibt **keine**
      Marke hinter der letzten Zeile.
- [ ] Ein Tipp auf eine Zielzeile sendet `queue_reorder` mit
      `beforeMatchId = <ID der Zielzeile>` und beendet den Modus, sobald der
      Turnier-PC bestätigt hat.
- [ ] Im Modus sind Megafon, ⋮-Menü, Spielerlinks und die Wähler der Zeile
      stillgelegt; ein Tipp irgendwo auf der Zeile heißt immer „hierhin".
- [ ] Im Modus trägt keine Zeile einen Zieh-Griff.
- [ ] Der Modus ist ohne Zeigegerät bedienbar: Zielzeilen sind mit Tab
      erreichbar, Enter löst aus, Esc bricht ab.

**Fehler- und Grenzfälle**

- [ ] Der Modus endet **ausschließlich bei einem erfolgreich bestätigten Zug**
      — sowie über Esc, „Abbrechen", einen Panel-Wechsel oder ein verschwundenes
      Spiel. Schlägt das Senden fehl, **bleibt er stehen**, damit der Vorgang
      ohne erneutes Merken wiederholbar ist. Eine Regel, kein Sonderfall je
      Fehlerart.
- [ ] Ohne Verbindung zum Turnier-PC erscheint die bestehende Meldung „Keine
      Verbindung zum Turnier-PC — nichts wurde gesendet."
- [ ] Bei **401** („Gerät nicht mehr freigegeben") und **429** („zu viele
      TL-Geräte") erscheint die bestehende Meldung.
- [ ] Wird dasselbe Spiel innerhalb von 20 Sekunden zweimal vor dasselbe Ziel
      gesetzt (dazwischen woandershin), wirkt **auch der zweite Zug** — die
      Wiederholungserkennung des Vorgangsfensters greift hier nicht mehr.
- [ ] Bei aktivem Hallenfilter bedeutet eine Marke weiterhin „vor diese Zeile in
      der **globalen** Reihenfolge"; die Wirkung auf den manuellen Präfix ist in
      `docs/turnierleitung-web.md` erklärt (siehe ADR 0050).
- [ ] Hat der Turnier-PC die Liste gekappt (`queue_truncated`), gibt es Marken
      ausschließlich an tatsächlich gezeigten Zeilen.
- [ ] Drag & Drop und die Pfeiltasten am Griff funktionieren außerhalb des Modus
      unverändert.

## Tests

**TDD, Schritt 1 der Umsetzung.** `assets/tl.html` hat keinen Test-Harness; das
Projektmuster ist, reine Logik nach `src/io/*.mjs` auszulagern, dort zu testen
und in `tl.html` inline zu kopieren — genau wie `src/io/dragScroll.mjs` /
`scripts/test-drag-scroll.mjs` aus Spur A.

**`scripts/test-queue-gap.mjs`** prüft `src/io/queueGap.mjs`. Der teuerste
Fehler steht im Dateikopf: *eine Marke an der falschen Zeile verschiebt ein
Spiel woandershin, als die Turnierleitung getippt hat.* Fälle:

- Marke **nicht** an der gemerkten Zeile, **nicht** an ihrer direkten
  Nachfolgerin, **nicht** an gerufenen Zeilen, **nicht** hinter der letzten.
- Marke an allen übrigen Zeilen — einschließlich der allerersten („ganz nach
  vorn").
- Gemerktes Spiel verschwunden → Abbruchgrund `"verschwunden"`; Zielzeile
  verschwunden → `"ziel-weg"`; Ziel ohne Marke → `"kein-ziel"`.
- Ein-Zeilen-Liste, leere Liste, alle Zeilen gerufen.
- Unfug-Eingaben: `null`/`undefined`/`NaN`-IDs, doppelte IDs, `zeilen` kein
  Array — jeweils Abbruch statt Wurf.

**Eigener CI-Schritt** in `.github/workflows/ci.yml` (nach dem
`test-drag-scroll`-Schritt), mit einem Kommentar darüber, was ein Fehler hier
fachlich kostet. Kein Doppelpunkt im Schrittnamen, sonst bricht die CI.

**Weiterhin grün:** `node scripts/check-asset-syntax.mjs` (die Inline-Kopie muss
für sich allein gültiges ESM sein), `npm run build`, `cargo test`.

**Browser-Verifikation am echten Code** — kein CI-Test, aber der einzige Weg,
Trefferflächen und Modus-Ausstiege wirklich zu prüfen: `tl.html` über einen
lokalen Server laden, das Modul als Blob-Import instanzieren, eine Liste mit
60 Zeilen bauen und den Modus durchspielen (Vorgehen wie bei Spur A).

**Manueller Turnier-Testfall:** Ein Spiel von Position ~55 auf Position 3
bringen, ohne zu ziehen; danach prüfen, dass die Reihenfolge auch am Turnier-PC
und auf einem zweiten TL-Gerät so steht.

## Risiken & Rollback

| Risiko | Wirkung | Umgang |
|---|---|---|
| **Spur A ist noch nicht feldgetestet** | Möglich, dass das geschmeidige Ziehen den Schmerz allein nimmt und dieser Modus unnötig ist | Als Annahme benannt (unten). Die Entscheidung, trotzdem zu bauen, ist bewusst getroffen |
| **LAN-Geräte bekommen die Seite erst mit dem Release-Tag** | Zwei TL-Geräte am selben Turnier zeigen zeitweise verschiedene Oberflächen; Feldtest zunächst nur im Cloud-Modus | Als Grenze dokumentiert. Der Tag-Push kann nur durch einen Admin erfolgen |
| **Umbau des Zeilen-Budgets** berührt jede Wartelisten-Zeile | Layout-Regression auf schmalen Geräten | Eigener Umsetzungsschritt mit eigener Prüfung; die Budget-Rechnung wird im CSS-Kommentar mitgeschrieben |
| **Moduswechsel baut alle Zeilen neu** | Kurzes Flackern, ein offenes ⋮ schließt | Unkritisch — das ⋮ wird beim Einstieg ohnehin geschlossen |
| **Letzte Sekunde:** Das Spiel verschwindet zwischen Tipp und Antwort | Stiller No-Op wie heute | Bewusst akzeptiert. Die ehrliche Lösung wäre ein Ablehnungsgrund im Turnier-PC — das bräche „ohne Serveränderung" und die Versions-Schere |
| **Zwei TL-Geräte sortieren gleichzeitig** | Letzter Schreiber gewinnt | Unverändert gegenüber heute |

**Rollback:** Rein clientseitig, keine Config-Felder, keine Wire-Änderung. Eine
ältere Version zu installieren genügt; im Cloud-Betrieb genügt ein Relay-Deploy
des vorherigen Standes. Es bleiben keine Daten zurück, die eine ältere Version
nicht lesen könnte.

## Offene Fragen / Annahmen

**Annahmen** (alle bewusst, keine davon geprüft):

1. **Spur A ist gebaut, aber nicht feldgetestet.** Die Aussage „Spur A macht
   Spur B nicht überflüssig" ist begründet, aber nicht gemessen.
2. Die Turnierleitung weiß beim Öffnen des ⋮-Menüs bereits, welches Spiel sie
   verschieben will.
3. Wartelisten von 50–60 Spielen bleiben unter dem serverseitigen Deckel
   `QUEUE_LIMIT` = 120.
4. Praktisch sortiert immer nur ein TL-Gerät gleichzeitig.

**Offene Frage:** Ob der Feldtest von Spur A abgewartet wird, bevor dieser Modus
gebaut wird — eine Entscheidung des Nutzers, nicht der Spec.

## Betroffene Doku-Dateien

| Datei | Was |
|---|---|
| `docs/features/tl-spielliste-sprung-umsortieren.md` | diese Spec |
| `docs/adr/0050-verschiebe-modus-globales-einfuegeziel.md` | die Entscheidung zum Einfügeziel |
| `docs/turnierleitung-web.md` | Bedienung: Abschnitt „Spiele in der Warteliste umsortieren" erweitern; ⋮-Aufzählung vervollständigen; **Präfix-Wirkung bei Hallenfilter erklären** |
| `docs/features/spielliste-manuelle-reihenfolge.md` | Nachtrag: zweiter Bedienweg; `before = null` ist im Modus nicht erreichbar |
| `docs/changelog.md` | Nutzer-Sicht der veröffentlichten Version |
| `CLAUDE.md` | Tabellenzeile „Manuelle Spielreihenfolge" um `src/io/queueGap.mjs` ergänzen |
| `docs/roadmap.md` | Verweis auf diese Spec |

## Umsetzungs-Hinweise

*Erst nach Freigabe relevant. Ergebnis der How-To-Phase.*

**Zwei Entwurfsentscheidungen tragen den Rest:**

1. **Die ganze Zeile ist im Modus das Ziel; die Marke an der Oberkante ist die
   Optik.** Eigenständige Lücken-Elemente scheiden aus — `#queue-list` enthält
   laut ausdrücklicher Festlegung nur Zeilen, und `reconcileKeyed` entfernt
   jedes Kind ohne `data-reconKey`. Ein nur 12 px hoher Streifen als alleiniges
   Ziel wäre auf dem Tablet unbedienbar. Die Marke wird deshalb Bestandteil der
   Zeilen-HTML; `queueRow` liest den Modus-Merker direkt aus dem Modulraum, wie
   es das mit `openKebabMatchId` schon tut, und behält seine Signatur.
2. **Megafon, „↑ nach oben" und ⋮ kommen in einen `.row-actions`-Wrapper.**
   Sonst wäre der neue Knopf das siebte Flex-Kind und spränge das dokumentierte
   Budget (CSS-Kommentar `tl.html:553–568`). Mit dem Wrapper hat die Zeile
   **fünf** Kinder — eines weniger als heute, obwohl ein Knopf dazukommt. Das
   ist derselbe Umbau, den `.row-status` für die Marken bereits gemacht hat.

**Schritte, jeder für sich lauffähig:**

1. `src/io/queueGap.mjs` + `scripts/test-queue-gap.mjs` (TDD, rot → grün).
2. CI-Schritt in `.github/workflows/ci.yml`.
3. Inline-Kopie in `tl.html`, mit Verweis auf Modul **und** Testdatei.
4. **Vorgangskennung auf einen laufenden Zähler umstellen** (beide
   `queue-reorder`-Aufrufstellen). Eigenständig nützlich: Heute schluckt das
   20-Sekunden-Fenster einen wiederholten Zug auf dasselbe Ziel stillschweigend.
   Das ist ohne Schaden abschaltbar, weil `queue_reorder` idempotent ist —
   zweimal „M vor B" ergibt denselben Zustand.
5. `.row-actions`-Wrapper einführen, „↑ nach oben" aus dem ⋮ an die Zeile holen,
   Budget-Rechnung im CSS-Kommentar neu schreiben.
6. Modus-Zustand `moveMatchId` (Modulraum) und **eine** Funktion
   `endeVerschieben()` mit allen fünf Ausstiegen. Der Aufräum-Haken für „Panel
   weg" ist der vorhandene Frühausstieg in `renderQueue`, wo bereits
   `queueObserver` abgeräumt wird.
7. Modus-Optik: Band oben (Muster `.action-bar`), gedämpfte Quellzeile, Marken,
   stillgelegte Bedienelemente.
8. Klick- und Tastaturweg auf `loeseZiel` + `queueReorder`.
9. Doku, Version, Changelog.
10. **`code-reviewer`** — Pflicht laut CLAUDE.md.

**Kein `security-reviewer` nötig:** kein neuer User-Input über die Grenze, keine
Auth, kein Datei- oder URL-Handling.

**Version** gemeinsam in `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` und
`package.json` — die **Nummer erst beim Merge festlegen**. Diese Arbeit baut auf
dem Branch von Spur A (`tl-reorder-fluessig`) auf, weil beide `renderQueue` und
`queueRow` in derselben Datei anfassen.
