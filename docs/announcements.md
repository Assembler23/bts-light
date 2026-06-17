# Sprachansagen für Feld-Aufrufe

Wird in BTP ein Spiel auf ein Feld gezogen, spielt bts-light auf dem
Turnier-PC eine gesprochene Ansage ab: **Gong → „Feld X" → Disziplin →
Paarung → „Feld X"**. Deutsch oder Englisch, wählbare Stimmen,
einstellbares Tempo. Eingeführt in v0.6.0.

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
  + Regel-Umschrift). Nie stumm, nie blockierend.
- **Konfig** (`AppConfig.azure_tts`): `enabled`, `region`, `key`, `voice`. Einrichtung im Setup →
  *Ansagen*. Spielernamen werden XML-escaped in die SSML eingesetzt.
- **Datenschutz:** Bei aktivem Azure-TTS werden die **Namen der gerufenen Paarung** zur Synthese an
  Azure (Region wählbar, z. B. West Europe/EU) gesendet — öffentliche Wettkampfdaten, opt-in über
  `enabled`. Ist Azure aus, verlässt nichts das Gerät.
- `src/state/useAvailableVoices.ts` — System-Stimmen.
- `src/components/MatchAnnouncer.tsx` — Detektor (immer eingehängt).
- `src/pages/SetupWizard.tsx` — Einstellungen.
