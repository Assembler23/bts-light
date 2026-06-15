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
| `name_overrides` | Phonetische Aussprache-Korrekturen, Liste `{ name, say }` (Default leer) |

## Aussprache-Korrekturen (`name_overrides`)

Spricht die TTS-Stimme einen Namen falsch (häufig bei vietnamesischen,
chinesischen, indischen Namen, weil die deutsche/englische Stimme Buchstaben
wie `ph`, `tr`, `x`, `zh`, `q` nach ihren eigenen Regeln liest), lässt sich pro
**Name oder Namensteil** eine **Ersatz-Schreibweise** hinterlegen, die die
Stimme besser trifft (z. B. `Nguyen` → `Nujen`).

- **Anwendung** (`src/io/announcer.ts`, `joinNames`): pro Spielername zuerst ein
  **exakter Voll-Name-Treffer**, sonst **Wort für Wort** — ein einmal
  eingetragener Nachname wirkt also für alle Spieler:innen mit diesem Namen.
  Vergleich case-insensitiv/getrimmt; Whitespace bleibt erhalten.
- **Reichweite:** Es ist **keine zusätzliche Sprache** — die Ansage bleibt
  de/en, nur die Aussprache einzelner Namen wird ersetzt. Läuft offline
  (kein externer Dienst).
- **Pflege:** Tabelle im Setup → Abschnitt *Ansagen* → *Aussprache-Korrekturen*.
  Knopf **„Häufige Namen laden"** fügt eine Startliste gängiger VN/CN/IN-
  Nachnamen mit deutscher Lautschrift ein (`src/io/nameOverrideSeed.ts`) — als
  **editierbare Startwerte** (Vietnamesisch ist tonal: brauchbar, nicht
  perfekt). ▶ je Zeile spielt die Aussprache zum Nachjustieren ab.
- **SSML/Phoneme** sind bewusst NICHT genutzt — Browser-`SpeechSynthesis`
  (WebView2/Chrome) ignoriert `<phoneme>`; nur die Ersatz-Schreibweise wirkt.

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
- `src/io/announcer.ts` — Gong + Sprachsynthese + Aussprache-Korrekturen (`applyOverride`, `playNameTest`).
- `src/io/nameOverrideSeed.ts` — Startliste häufiger Nachnamen (VN/CN/IN).
- `src/state/useAvailableVoices.ts` — System-Stimmen.
- `src/components/MatchAnnouncer.tsx` — Detektor (immer eingehängt).
- `src/pages/SetupWizard.tsx` — Einstellungen.
