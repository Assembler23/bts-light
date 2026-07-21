# Sprachansagen für Feld-Aufrufe

Wird in BTP ein Spiel auf ein Feld gezogen, spielt bts-light auf dem
Turnier-PC eine gesprochene Ansage ab: **Gong → „Feld X" → Disziplin
(+ Klasse) → Paarung → „Feld X"**. Deutsch oder Englisch, wählbare Stimmen,
einstellbares Tempo. Eingeführt in v0.6.0; Klassen-Ansage
(„Herreneinzel **A**") seit v0.9.145. Ist die Zähltafelbediener-Verwaltung
aktiv (ADR 0007), folgt am Ende zusätzlich „**Tabletbedienung: {Name}**"
(`scorekeeperNames` in `AnnounceMatchInput`, nur bei zugewiesenem Bediener) —
siehe [zaehltafelbediener.md](zaehltafelbediener.md).

## Funktionsweise

- **Technik:** Browser-Web-Speech-API (`speechSynthesis`) im WebView; der
  Gong wird per Web Audio API synthetisiert. Geräte-lokal, kein externer
  Dienst. Die Ansage spielt über die Lautsprecher des Turnier-PCs, auch
  wenn das Fenster ins Tray minimiert ist.
- **Auslöser:** Die immer eingehängte Komponente
  [`MatchAnnouncer`](../src/components/MatchAnnouncer.tsx) pollt alle 2 s
  `tablet_overview()` und merkt sich pro Feld die `match_id`. Der **erste
  Poll ist die Baseline** — bereits laufende Spiele werden nicht
  nachträglich angesagt. Danach löst jede neue `match_id` auf einem Feld
  eine Ansage aus. Eine Match-ID wird im 5-s-Fenster nicht doppelt
  angesagt.
- **Engine:** [`src/io/announcer.ts`](../src/io/announcer.ts) — portiert
  aus der Schwester-App badhub-tournament.

## Disziplin

Die Disziplin (Herren-/Dameneinzel, Herren-/Damendoppel, Mixed) kommt aus
dem BTP-**Event**, nicht aus dem Draw-Namen. Auflösungskette im Parser
([`btp/model.rs`](../src-tauri/src/btp/model.rs)):
`Match.DrawID → Draw.EventID → Event{GameTypeID, GenderID}`.

- `GameTypeID`: 1 = Einzel, 2 = Doppel.
- `GenderID`: 1 = Herren, 2 = Damen, 3 = Mixed.

Lässt sich das Event nicht auflösen, ist die Disziplin `Unknown` und wird
in der Ansage weggelassen.

### Klasse (seit v0.9.145)

Direkt hinter der Disziplin wird das **Klassen-Kürzel** angesagt
(„Herreneinzel **A**"). Es kommt aus `model::class_label`: bevorzugt aus dem
**Event-Namen** (trägt die Klasse auch in der Gruppenphase, wo Draws nur
„Gruppe 1…n" heißen), sonst aus dem **Draw-Namen** („HE A" in der
K.-o.-Phase). Bekannte Disziplin-Wörter werden entfernt; übrig bleiben darf
nur EIN kurzes Kürzel (≤ 4 Zeichen, z. B. „A", „B2", „U15") — **Gruppen-
oder Auslosungsnamen („Gruppe 3", „Hauptrunde") werden nie angesagt**
(Nutzer-Vorgabe vom Turnier 17.07.2026). Ohne erkennbares Kürzel bleibt die
Ansage wie bisher („Herreneinzel"). Durchgereicht wird das Kürzel als
`class_label` (CourtOverview, `MatchBrief.classLabel` für den Cloud-Slave,
`PreparationCandidate`) und als `className` in den Announcer-Eingaben.

## K.-o.-Runde (ab Viertelfinale)

Steht ein Spiel im **Viertelfinale, Halbfinale, Finale oder Spiel um Platz 3**,
wird die Runde **vor der Paarung** mitangesagt (z. B. „Feld 2. Herrendoppel.
**Halbfinale.** … gegen …"). **Frühere Runden, Gruppen und das Achtelfinale
werden NICHT angesagt.**

- Quelle: die rohe BTP-Runde `RoundName` (`btp/model.rs` `BtpMatch.round_name`),
  durchgereicht als `CourtOverview.round_name` (Rust + `types.ts`) bis zum
  Announcer (`AnnounceMatchInput.roundName`).
- Erkennung: `knockoutRoundLabel()` in [`announcer.ts`](../src/io/announcer.ts) —
  normalisiert (Punkte/Bindestriche/Leerzeichen) und matcht robust:
  `VF`/`Viertelfinale`/`QF`/`Quarterfinal`, `HF`/`Halbfinale`/`SF`/`Semifinal`,
  `Finale`/`Final`/`Endspiel`, `Spiel um Platz 3`/`Bronze`/`3rd`. Gruppen
  (`G1`, `Gruppe …`), `Achtelfinale`, `Runde/Round N`, Quali/Vorrunde → kein
  Label (→ keine Ansage). de/en gemäß Ansage-Sprache.

## Sprache: Deutsch / Englisch / Automatisch

Einstellbar im Setup unter „Sprachansagen":

- **Deutsch** / **Englisch** — feste Sprache.
- **Automatisch** — Englisch, sobald **mindestens die Hälfte** der Spieler
  auf dem Feld international ist, sonst Deutsch. International =
  Nationalität gesetzt und ≠ `GER`. Praktisch: Einzel ab 1 von 2, Doppel
  ab 2 von 4 ausländischen Spielern.

## Feld-Bezeichnung

Endet das BTP-Court-Label auf einer Zahl (`"1"`, `"Feld 2"`, `"Court 3"`),
wird „Feld <Zahlwort>" gesprochen (Zahl als Wort, sonst spricht der
Browser „Feld erste"). Bei frei benannten Feldern (`"Center Court"`) wird
das Label wörtlich gesprochen.

## Tonausgabe freischalten (WebView2)

Windows-WebView2 startet die Tonausgabe erst nach einer Nutzergeste. Der
**Test-Knopf** in den Einstellungen ist diese Geste; zusätzlich schaltet
ein einmaliger Klick irgendwo im Fenster das Audio frei. Empfehlung: vor
dem Turnier einmal die Test-Ansage drücken.

## Einstellungen (`AppConfig.announce`)

| Feld | Bedeutung |
|---|---|
| `enabled` | Ansagen an/aus (Default aus) |
| `language_mode` | `de` · `en` · `auto` |
| `voice_de` / `voice_en` | bevorzugte Stimme je Sprache (`voiceURI`), leer = Browser-Standard |
| `rate` | Sprech-Geschwindigkeit 0,5–1,5 (Default 0,8) |
| `gong` | Gong vor der Ansage (Default an) |
| `name_overrides` | Phonetische Aussprache-Korrekturen (Nutzer), Liste `{ name, say }` (Default leer) |
| `name_overrides_enabled` | Korrekturen anwenden (Basis-Wörterbuch + Nutzer)? Default `true` |
| `announce_hall` | Mehr-Hallen: nur Spiele dieser Halle (BTP-Location-Name) ansagen; leer = alle (Default). `MatchAnnouncer` filtert neue Feldbelegungen auf `court.location`. Siehe [multi-hall.md](multi-hall.md). |
| `saved_announcements` | Gespeicherte Ansage-Blöcke (wiederkehrende Freitext-Ansagen), Liste `string` (Default leer). |

> **UX:** Ist `azure_tts.enabled` aktiv, blendet [`AnnounceSettings`](../src/components/AnnounceSettings.tsx) die
> Standard-Stimmen-Auswahl (`voice_de`/`voice_en`) aus — Azure spricht die ganze Ansage, die lokale Stimme
> wäre wirkungslos (Fallback bei Fehler/offline greift weiterhin automatisch).

## Gong-Klang (Spielaufruf vs. Freitext/Info)

`playGong(ctx, kind)` in [`announcer.ts`](../src/io/announcer.ts) erzeugt zwei **deutlich verschiedene**
Gongs: **Spielaufruf** (`kind="match"`, Default) = tiefer, zweitöniger absteigender Sinus-Gong (A5 → D5);
**Freitext/Info** (`kind="info"`) = heller, dreitöniger aufsteigender Dreiklang (C5-E5-G5, Triangle). So ist
sofort hörbar, ob es ein Spielaufruf oder eine sonstige Durchsage ist.

**Timing (v0.9.155):** Der Gong wird auf der Audio-Uhr (`ctx.currentTime`) geplant, sein Ende aber am
**echten Audio-Ende** signalisiert — `gongFinished()` löst auf dem `onended` des zuletzt stoppenden
Oszillators auf (nicht auf einer festen Wall-Clock-`setTimeout`), plus `GONG_BREATH_MS` (150 ms) Atempause.
Startet der AudioContext in WebView2 verzögert, verschiebt sich das Ende real mit → die Sprache setzt nicht
mehr in den Nachklang ein (Tilo-Befund 19.07.). Ein Fallback-Timer kurz nach dem geplanten Ende verhindert,
dass die Ansage-Queue hängt, falls `onended` ausnahmsweise ausbleibt.

## Geteiltes Community-Wörterbuch (crowd-sourced)

Zusätzlich zum mitgelieferten Basis-Wörterbuch und der lokalen Nutzer-Tabelle
lädt bts-light ein **gemeinsames Aussprache-Wörterbuch** von badhub
(`GET /api/v1/pronunciations`):

- **Laden:** beim Start (kurz nach dem Config-Load) und danach alle 3 h, solange
  Internet da ist (`App.tsx`). Die Liste geht über `setSharedOverrides()` in
  [`announcer.ts`](../src/io/announcer.ts).
- **Offline:** Der Rust-Command `fetch_pronunciations` cached die Liste lokal
  (`pronunciations_cache.json` im App-Config-Verzeichnis) und liefert sie ohne
  Internet aus dem Cache → der reine LAN-Hallenbetrieb spricht weiter korrekt.
- **Priorität** in `buildOverrideMap`: Basis-Wörterbuch < **geteiltes
  Wörterbuch** < lokale Nutzer-Tabelle (eigene Korrekturen gewinnen immer).
- **IPA für Azure:** Einträge können zusätzlich ein `ipa`-Feld tragen. Im
  Azure-Pfad baut `buildIpaMap` daraus eine Map und `nameSsml` spricht den Namen
  über inline `<phoneme alphabet="ipa" ph="…">` (ganzer Name oder wortweise).
  Web Speech (offline) ignoriert `ipa` und nutzt `say`. Quelle: kuratiertes
  W3C-PLS-Lexikon (badhub).
- **`say`-Ersatzschreibweise auch bei Azure (seit v0.9.167):** Fehlt IPA, nutzt
  der Azure-Pfad jetzt ebenfalls die phonetische Ersatzschreibweise `say` (als
  gesprochenen Text), statt sie zu ignorieren. Die Rangfolge je Name/Wort regelt
  `resolveNameCorrection` (`src/io/nameCorrection.mjs`, node-testbar):
  **IPA (`<phoneme>`) → `say`-Text → `<lang>`-Erkennung**. Damit wirkt eine im
  Setup getippte Korrektur (z. B. „Chybych" → „Chübüch") auf **beiden** Stimmen
  — vorher war `say` auf dem Azure-Pfad totes Feld. Der ganze-Name-`<lang>`-
  Fallback greift unverändert nur, wenn dieser Name **keinen** Wort-Treffer hat
  (bessere Sprach-Erkennung als wortweise).
- **Sprach-Erkennung (`<lang>`-Pfad):** `detectNameLang` (`src/io/transliterate.ts`)
  erkennt die Herkunftssprache: markante CN/VN-Nachnamen + kuratierte Namenslisten
  `NAME_LANG_BASE` (es/fr/pl/tr/ms/in, generiert aus `data/name-lists/*.xml` via
  `scripts/gen-name-lang-base.mjs`). **Konfidenz:** nur bei eindeutiger Sprache wird
  `<lang xml:lang="…">` gesetzt (Locale-Map in `announcer.ts`), sonst deutscher
  Default — mehrdeutige Namen werden nicht „geraten". Nur cn/vn haben zusätzlich
  eine deutsche Umschrift (Web-Speech); die übrigen Sprachen wirken nur im
  Azure-`<lang>`-Pfad. Plan: `docs/features/name-pronunciation-plan.md`.
- **Manuelle Sprach-Korrektur:** `NameOverride.lang` (Nutzer-Tabelle) erzwingt je
  Name die Sprache, wenn die Erkennung daneben liegt. `buildLangOverrideMap` +
  `nameSsml` werten sie aus; Vorrang: Override (`"de"` = kein Tag, sonst
  `<lang>`) → kuratiertes IPA → `say`-Ersatzschreibweise → automatische
  Erkennung. Cache-Key in
  `azure_tts.rs` hasht das vollständige SSML (inkl. `<lang>`/`<phoneme>`), daher
  keine Kollision zwischen gleich geschriebenen Namen mit anderer Sprache.
- **Teilen (opt-in):** Schalter „Meine Korrekturen mit der Community teilen"
  (`announce.share_corrections`, Default aus). Beim Speichern werden die eigenen
  Einträge via Rust-Command `share_pronunciations`
  (`POST /api/v1/pronunciations`) gesendet. Qualitätsmodell: sofort live, Admin
  räumt nach. Server-Doku: badhub `docs/features/tts_pronunciations.md`.

## Verlauf, erneutes Abspielen & gespeicherte Blöcke

Auf der Ansagen-Seite ([`AnnouncePage.tsx`](../src/pages/AnnouncePage.tsx)):

- **Verlauf (letzte 10):** Jede **manuell** ausgelöste Ansage (Freitext + manuelle Feld-Ansage) wird in
  [`src/state/announceHistory.ts`](../src/state/announceHistory.ts) protokolliert (localStorage, max. 10,
  neueste zuerst). **Automatische Spielaufrufe landen NICHT im Verlauf.** Jeder Eintrag hat „Erneut" —
  Freitext wird neu über den Master verschickt (`publish_freetext`), eine Feld-Ansage lokal neu abgespielt
  (aus dem mitgespeicherten `CourtOverview`-Schnappschuss).
- **Gespeicherte Blöcke:** Aktuellen Freitext per „Als Block speichern" in `announce.saved_announcements`
  ablegen (dedupliziert, persistiert). Jeder Block lässt sich direkt ansagen (Halle wie im Freitext-Selektor,
  Master → Slaves), ins Textfeld laden oder löschen.

## Aussprache-Korrekturen (`name_overrides`)

Spricht die TTS-Stimme einen Namen falsch (die deutsche/englische Stimme liest
fremdsprachige Buchstaben nach ihren eigenen Regeln — betrifft viele Herkünfte:
asiatisch/indisch `ph tr x zh q`, französisch stille Endungen/`j`, spanisch
`j ll ñ z`, türkisch `ç ş ğ y`, polnisch `sz cz ł` …), lässt sich pro
**Name oder Namensteil** eine **Ersatz-Schreibweise** hinterlegen, die die
Stimme besser trifft (z. B. `Nguyen` → `Nujen`, `Lefebvre` → `Löfäwr`).

- **Basis-Wörterbuch** (`src/io/nameOverrideBase.ts`, `BASE_NAME_OVERRIDES`):
  mitgelieferte Liste häufiger fremdsprachiger Nachnamen (abgeleitet aus den
  häufigsten Namen der Badhub-Spieler-DB), wird **automatisch** angewendet. Die
  **Nutzer-Tabelle** (`name_overrides`) hat **Vorrang** (gleicher Schlüssel
  überschreibt die Basis). Schalter `name_overrides_enabled` schaltet beides ab.
- **Anwendung** (`src/io/announcer.ts`, `joinNames`): pro Spielername zuerst ein
  **exakter Voll-Name-Treffer**, sonst **Wort für Wort** — ein einmal
  eingetragener Nachname wirkt also für alle Spieler:innen mit diesem Namen.
- **Matching diakritik-/sonderzeichen-unabhängig** (`normalizeName`): NFD-Faltung
  + ı/İ/ø/ł/đ → „Nguyên"/„Nguyen", „Yıldız"/„Yildiz", „García"/„Garcia" treffen
  denselben Eintrag. Whitespace + Wort-Satzzeichen bleiben beim Vorlesen erhalten.
- **Reichweite:** Es ist **keine zusätzliche Sprache** — die Ansage bleibt
  de/en, nur die Aussprache einzelner Namen wird ersetzt. Läuft offline
  (kein externer Dienst).
- **Pflege:** Tabelle im Setup → Abschnitt *Ansagen* → *Aussprache-Korrekturen*.
  Das Basis-Wörterbuch wirkt automatisch; in der Tabelle pflegst du nur eigene
  Korrekturen/Ergänzungen (Vorrang). ▶ je Zeile spielt die Aussprache ab.
- **SSML/Phoneme** sind bewusst NICHT genutzt — Browser-`SpeechSynthesis`
  (WebView2/Chrome) ignoriert `<phoneme>`; nur die Ersatz-Schreibweise wirkt.

### Regelbasierte Umschrift (CN/VN) — `src/io/transliterate.ts`

Greift, wenn Wörterbuch UND Nutzer-Tabelle keinen Treffer haben, damit auch
NICHT gelistete chinesische/vietnamesische Namen besser klingen. Reihenfolge je
Wort in `applyOverride`: **Wörterbuch/Tabelle → Regel-Engine → unverändert**.

- **Erkennung** (`detectNameLang`): nur wenn ein Token ein **markanter** CN- bzw.
  VN-Nachname ist (Trigger-Listen; kurze/mehrdeutige wie „Le"/„Do"/„Ma" sind KEIN
  Trigger). Deutsche/andere Namen → keine Umschrift. Innerhalb eines erkannten
  Namens sind aggressive Regeln (v→w, x→s) gefahrlos.
- **Pinyin** (`pinyinToGerman`): Silben-Segmentierung (greedy Initial+Finale) →
  zh→dsch, ch→tsch, sh/x→sch, q/j→tsch/dsch, c→ts, z→ds, w→u, y→j; apikales i
  (zhi/chi/shi/zi/ci/si/ri)→„i"; nach j/q/x wird u→ü.
- **Vietnamesisch** (`vietnameseToGerman`): konsonant-fokussiert — tr→tsch, th→t,
  ph→f, ch→tsch, kh→ch, nh→nj (Anlaut)/n (Endung), ng/ngh→ng, gi→j, qu→ku, x→s,
  v→w; Endung -c/-ch→k. **„d" bleibt „d"** (ASCII verschluckt das đ; weiche
  d-Fälle wie „Duong"→„Juong" deckt das Wörterbuch ab).
- **Grenze:** Konsonanten zuverlässig; Vokale/Töne/Dialekt (südvietnamesisch,
  Wade-Giles) nur Näherung → Spezialfälle in der Nutzer-Tabelle (Vorrang).

## Vorbereitungs-Ansage (Spiele in Vorbereitung)

Neben der Feld-Ansage gibt es eine zweite Variante: aus dem
„In Vorbereitung"-Tab kann die Turnierleitung je gerufenem Spiel eine
**Hallen-Ansage** auslösen — Knopf „Ansage" neben dem Aufruf
([preparation.md](preparation.md)). Format:

> Gong → **„In Vorbereitung."** → Disziplin → Paarung („… gegen …") →
> **„Bitte in *Halle X*."** (entfällt bei Ein-Hallen-Turnieren)

Englisch entsprechend: „Preparation call." → Discipline → „… versus …" →
„Please report to *hall*."

Unterschiede zur Feld-Ansage:

- **Kein Feld** — die Ansage trägt keinen Court (das Spiel ist noch nicht
  auf einem Feld); statt des wiederholten Court-Worts steht am Ende die
  Halle, in die gerufen wurde.
- **Manueller Auslöser, kein Auto-Detektor.** Eine eigene `MatchAnnouncer`-
  Schwester gibt es nicht — der Knopf-Klick im Panel ist gleichzeitig die
  User-Geste, die WebView2 zum Entsperren der Tonausgabe braucht.
- **Sprach-Auflösung geteilt** mit der Feld-Ansage:
  `resolveAnnouncementLanguage(nationalities, mode)` in `announcer.ts` —
  Auto-Modus nutzt dieselbe Regel (≥ Hälfte international ⇒ Englisch).
- **`AnnounceConfig.enabled` gilt für beide.** Ist die Ansage global
  abgeschaltet, ist auch der „Ansage"-Knopf im Vorbereitungs-Tab
  ausgeblendet.

Engine: `playPreparationAnnouncement` / `buildPreparationSegments` in
[`src/io/announcer.ts`](../src/io/announcer.ts).

Eingeführt in v0.9.16.

### Zweiter/Dritter Aufruf je Partei (Plan 1)

Erscheint nur **eine** Partei, ruft die Turnierleitung gezielt die
**fehlende** Seite nach. Je gerufenem Spiel in der Vorbereitungs-Übersicht
gibt es dafür zwei kleine Knöpfe (einer je Partei, mit dem Nachnamen zur
Unterscheidung). Ansage-Format (an Tilos BTS angelehnt, `callStage` in
`AnnouncePreparationInput`):

> **„Zweiter Aufruf für: {nur die genannte Partei}."** → „Bitte in *Halle*."

Ein zweiter Druck auf dieselbe Partei macht daraus **„Dritter und letzter
Aufruf für: …"**. Der Zähler je (Spiel, Partei) lebt clientseitig im
Master-Fenster (kein Server-/Ticker-Zustand — wie Tilos Pass-Through-
Zweitaufruf). Es wird **nur die genannte Seite** angesagt (die andere ist
schon da). Englisch: „Second call for: …" / „Third and final call for: …".

**Auch am fernen Slave (Stufe 2, v0.9.154).** Derselbe Nachruf geht jetzt
vom Cloud-Slave der fernen Halle aus — und wird **lokal auf dem Slave-
Rechner** angesagt, direkt dort, wo die fehlende Partei steht (kein
Rückkanal zum Master nötig). Datenweg:

- Der Master pusht seine **aufgerufenen Spiele** (nur gerufene, noch
  ruf-bare Paarungen) periodisch als `HostFrame::Prepared` an den Relay –
  gebündelt, nur bei Änderung (Fingerabdruck in `push_prepared`,
  `relay_client.rs`; reine Liste über `build_prepared_list`). Ein leerer
  Push leert die Relay-Liste (kein Aufruf mehr offen).
- Der Relay hält sie je Namespace (`Namespace.prepared`) und gibt sie in
  `GET /{ns}/info/announce/state` **hallengefiltert** zurück
  (`AnnounceState.prepared`, gleiche Filterregel wie die Court-Matches).
- Der Slave holt sie über `cloud_announce_state` (`CloudPrepared`) und
  zeigt sie auf der Ansagen-Seite unter **„Aufgerufene Spiele"**. Die
  zwei Nachruf-Knöpfe je Partei rufen dieselbe
  `playPreparationAnnouncement`-Funktion mit `callStage` auf wie der
  Master; der `(Spiel, Partei)`-Zähler lebt clientseitig im Slave-Fenster.

Damit ist der Nachruf in beiden Rollen identisch – nur die Kandidatenliste
kommt am Slave aus der Cloud statt aus BTP. Siehe auch
[docs/multi-hall.md](multi-hall.md) und [docs/cloud-relay.md](cloud-relay.md).

## Bekannte Grenzen

- Der 2-s-Poll kann ein extrem kurz belegtes und sofort wieder geräumtes
  Feld verpassen — für reguläre Feld-Aufrufe unkritisch.
- Verfügbare Stimmen hängen vom Windows-System ab; ist die gewählte
  Stimme auf dem Rechner nicht vorhanden, nutzt der Browser seine
  Standardstimme.

## Beteiligte Dateien

- `src-tauri/src/btp/model.rs` — `Discipline`, Event-Parsing.
- `src-tauri/src/tablet/state.rs` — `CourtOverview` (`match_id`,
  `discipline`, Nationalitäten).
- `src-tauri/src/config.rs` — `AnnounceConfig`, `NameOverride`.
- `src/io/announcer.ts` — Gong + Sprachsynthese + Aussprache-Korrekturen (`normalizeName`-Faltung, `buildOverrideMap`, `applyOverride`, `playNameTest`).
- `src/io/nameOverrideBase.ts` — mitgeliefertes Basis-Wörterbuch (`BASE_NAME_OVERRIDES`).
- `src/io/transliterate.ts` — regelbasierte CN/VN-Umschrift (`detectNameLang`, `pinyinToGerman`, `vietnameseToGerman`).
- `src/io/azureAnnounce.ts` — baut die `AnnounceOptions.azure`-Option (nur wenn aktiv).
- `src-tauri/src/azure_tts.rs` + `commands::azure_tts_speak` — Azure-Synthese (Key im Backend) + Datei-Cache.

## Azure Neural TTS (hochwertige Cloud-Ansage, opt-in)

Ist `azure_tts.enabled`, wird die **ganze Ansage als ein SSML** (`buildAnnouncementSsml` /
`buildPreparationSsml` in `announcer.ts`) an Azure geschickt: deutscher/englischer Rahmen +
**jeder Name in seinem `<lang>`-Span** (`detectNameLang` → `zh-CN`/`vi-VN`), sodass die neuronale
Stimme die Namen **nativ** spricht. Ablauf:

- Frontend baut SSML → Tauri-Command **`azure_tts_speak`** (`commands.rs`) → `azure_tts::synthesize`
  (POST an `https://<region>.tts.speech.microsoft.com/cognitiveservices/v1`). **Key/Region bleiben im
  Backend** (aus `AppConfig.azure_tts`). Antwort = MP3 (Base64) → Web Audio spielt es nach dem Gong.
- **Cache:** MP3 je SSML-Hash unter `app_data_dir/tts-cache/` → Wiederholungen/„nochmal aufrufen"
  kosten kein Netz/Geld.
- **Fallback:** Azure aus / kein Key / Netzfehler → nahtlos die lokale Web-Speech-Ansage (mit Wörterbuch
  + Regel-Umschrift). Nie stumm, nie blockierend — aber **sichtbar**: jeder Azure→Web-Speech-Rückfall
  setzt über `reportAzureFallback` (`src/state/azureStatus.ts`) den App-weiten
  [`AzureFallbackBanner`](../src/components/AzureFallbackBanner.tsx) (quittierbar) und landet als
  `console.warn` im Diagnose-Log. Zusätzlich warnt `AnnounceSettings`, wenn der Schalter an ist,
  aber Key/Region fehlen (das Frontend-Gate `azureOption` liefert dann gar keine Azure-Option mehr).
- **Konfig** (`AppConfig.azure_tts`): `enabled`, `region`, `key`, `voice`. Einrichtung im Setup →
  *Ansagen*. Spielernamen werden XML-escaped in die SSML eingesetzt.
- **Vererbung an Cloud-Slaves** ([ADR 0003](adr/0003-azure-tts-vererbung-relay.md)): Der Master
  schickt seine Azure-Config (nur wenn aktiv **und** vollständig) huckepack im
  `HostFrame::Courts`-Push an den Relay; der Relay liefert sie im `AnnounceState`
  (`azureTts`-Feld) an den Cloud-Slave aus. Dort hält `AppState.inherited_azure` sie **nur im
  RAM** (nie in der `config.json`); `azure_tts_speak` wendet die Vorrangregel `effective_azure`
  an: vollständige lokale Config gewinnt, sonst die geerbte. Das Frontend bekommt bewusst **nur
  die Stimme** (`CloudAnnounce.azure_voice`) — der Key bleibt im Backend. Die Ansage-Einstellungen
  zeigen am Slave „Vom Master geerbt ✓". Ein Slave braucht damit **keine** Azure-Eingaben mehr;
  der Zwei-Hallen-Bug „Schalter an, Key fehlt → still Standardstimme" ist doppelt abgedeckt
  (Vererbung + Warnung/Banner).
- **Datenschutz:** Bei aktivem Azure-TTS werden die **Namen der gerufenen Paarung** zur Synthese an
  Azure (Region wählbar, z. B. West Europe/EU) gesendet — öffentliche Wettkampfdaten, opt-in über
  `enabled`. Ist Azure aus, verlässt nichts das Gerät.
- `src/state/useAvailableVoices.ts` — System-Stimmen.
- `src/components/MatchAnnouncer.tsx` — Detektor (immer eingehängt).
- `src/pages/SetupWizard.tsx` — Einstellungen.
