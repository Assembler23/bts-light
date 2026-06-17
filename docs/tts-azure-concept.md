# Konzept: Native Namensaussprache via Azure Neural TTS (vorab erzeugt + offline)

> **Status: Konzept/Spike — noch nicht gebaut.** Entscheidungsgrundlage für die
> hochwertige Ansage. Baut auf der bestehenden Ansage ([announcements.md](announcements.md)) auf.

## Ziel
Ausländische Spielernamen (v. a. chinesisch/vietnamesisch, aber auch alle anderen)
**muttersprachlich korrekt** aussprechen — nicht nur als deutsche Lautschrift-Näherung.
Gleichzeitig die **Offline-Tauglichkeit** der Halle erhalten (Verleih-Kit ohne Internet).

## Kernidee
1. **Pro Name die Sprache markieren** und von einer **mehrsprachigen Azure-Neural-Stimme**
   sprechen lassen (SSML `<lang xml:lang="zh-CN">Zhang Zhixin</lang>`). Die Sprach-Erkennung
   pro Name existiert bereits: `detectNameLang()` in `src/io/transliterate.ts` (zh/vn; erweiterbar).
2. **Offline durch Vorab-Generierung + Cache:** Audio wird erzeugt, **solange der PC Internet hat**
   (beim Sync/Turnierstart), lokal als Dateien gecacht und **während des Turniers offline abgespielt**.

## Architektur
- **Synthese im Rust-Backend** (`src-tauri`, vorhandener `reqwest`-HTTP-Client) gegen die
  **Azure Speech REST-API**:
  `POST https://<region>.tts.speech.microsoft.com/cognitiveservices/v1`,
  Header `Ocp-Apim-Subscription-Key`, Body = SSML, Antwort = Audio (z. B. MP3/Opus).
- **Eine** mehrsprachige Neural-Stimme für ALLES (Konsistenz), z. B.
  `de-DE-SeraphinaMultilingualNeural` / `de-DE-FlorianMultilingualNeural` — kann innerhalb
  einer Äußerung per `<lang>` die Sprache wechseln.
- **Cache zweistufig** (im App-Datenverzeichnis, z. B. `audio-cache/`):
  - **Feste Fragmente** (einmalig, bounded): „Feld 1…30"/Zahlen, Disziplinen (5), Runden
    (Viertelfinale/Halbfinale/Finale/Spiel um Platz 3), „gegen"/„versus", „und"/„and",
    „In Vorbereitung", „Bitte in <Halle>" (Hallennamen aus BTP).
  - **Namens-Clips**: je Spielername ein Clip (Sprache via `detectNameLang`), erzeugt aus der
    BTP-Spielerliste beim Sync; lazy für neu auftauchende Namen.
- **Ansage = Audiosegmente zusammensetzen** (Web Audio API, kleine Pausen dazwischen):
  Gong → Feld → Disziplin → (Runde) → TeamA-Namen → „gegen" → TeamB-Namen → Feld.
  Voll offline abspielbar, sobald die Clips im Cache sind.
- **Fallback (robust):** Fehlt ein Clip / kein Azure-Key / beim Erstlauf noch offline →
  nahtlos zurück auf die heutige **Web-Speech-Ansage** (mit Wörterbuch + Regel-Engine). Nie stumm.

## Generierungs-Timing
- Beim **Sync** (Namen + Hallen aus BTP bekannt) im Hintergrund erzeugen, solange Internet da ist.
- **Lazy**: ein im Spielbetrieb neu auftauchender Name wird beim ersten Mal erzeugt (falls online),
  sonst Fallback; danach gecacht.
- Cache überlebt App-Neustarts; Invalidierung nur bei Stimmen-/Versionswechsel.

## Konfiguration (`AppConfig`)
```
azure_tts: { enabled: bool, region: string, key: string, voice: string }
```
(Key/Region aus dem Azure-Portal; `enabled=false` → heutiges Verhalten.)

## Kosten
- Azure Speech **Free-Tier (F0): 0,5 Mio. Zeichen/Monat neural kostenlos**; danach Standard
  ~**15–16 $ / 1 Mio. Zeichen**. Ein Turnier = wenige zehntausend Zeichen → **Cent-Beträge**,
  durch den Cache zahlt man jeden Namen nur **einmal**.

## Offene Punkte / Voraussetzungen
- **Azure-Account + Speech-Ressource** (Key + Region) muss angelegt werden — **externe Voraussetzung**,
  ohne die nichts läuft. Region **West Europe** empfohlen (Latenz + DSGVO).
- **Datenschutz:** Spielernamen werden zur Synthese an Azure (EU) gesendet. Namen sind öffentliche
  Wettkampfdaten, aber es ist ein **neuer externer Datenfluss** → dokumentieren; nur Namen, keine
  weiteren Daten; EU-Region; opt-in über `enabled`.
- **Stimmenwahl:** vor Festlegung 2–3 mehrsprachige Stimmen gegenhören (Muster generieren).
- **Spike zuerst:** ein einzelner SSML-Call mit gemischter Sprache („Feld zwei. Herrendoppel.
  <lang zh-CN>Zhang Zhixin</lang> gegen <lang vi-VN>Pham Thi Hong Thu</lang>.") → Qualität/Aussprache
  prüfen, BEVOR Cache/Playback gebaut werden. Braucht den Azure-Key.

## Phasen
1. **Spike** (klein): Rust-Funktion `azure_tts_say(ssml) -> audio`, ein Testaufruf, Qualität prüfen
   (braucht Azure-Key). Stimmen vergleichen.
2. **Cache + Playback**: feste Fragmente + Namens-Clips erzeugen/cachen; Web-Audio-Sequencer; Fallback.
3. **Integration**: in den Ansage-Pfad (MatchAnnouncer/Vorbereitung/manuell), Config-UI, Doku.

## Bezug
- Sprach-Erkennung: `src/io/transliterate.ts` `detectNameLang` (zh/vn — für Azure auf zh-CN/vi-VN
  mappen; weitere Sprachen ergänzbar).
- Fallback-Ansage: `src/io/announcer.ts` (Web Speech + Wörterbuch + Regel-Engine) bleibt vollständig erhalten.
