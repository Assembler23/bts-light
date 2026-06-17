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
2. **Annahme aktualisiert (2026-06-17): in der Halle ist verlässlich Internet da.** → **On-Demand,
   ganze Ansage am Stück** synthetisieren (natürlichste Satzmelodie, einfachster Bau). Vorab-Generierung
   + Offline-Cache (s. u.) wird damit **optional** (nur als Offline-Härtung später).

## Architektur (On-Demand, primär)
- **Synthese im Rust-Backend** (`src-tauri`, vorhandener `reqwest`-HTTP-Client) gegen die
  **Azure Speech REST-API**:
  `POST https://<region>.tts.speech.microsoft.com/cognitiveservices/v1`,
  Header `Ocp-Apim-Subscription-Key`, Body = SSML, Antwort = Audio (MP3/Opus).
- **Eine** mehrsprachige Neural-Stimme für die ganze Ansage (Konsistenz + natürliche Prosodie), z. B.
  `de-DE-SeraphinaMultilingualNeural` / `de-DE-FlorianMultilingualNeural` — wechselt innerhalb der
  Äußerung per `<lang>` die Sprache.
- **Ganze Ansage als EIN SSML** bauen (statt Fragmente): Feld → Disziplin → (Runde) → TeamA →
  „gegen" → TeamB; Spielernamen je in `<lang>` ihrer erkannten Sprache. Azure liefert ein Audio →
  Web Audio spielt es nach dem Gong ab. Durchgehende Satzmelodie = menschlicher.
- **Latenz**: Request bei Ansage-Auslösung feuern, **während der ~1,2 s Gong läuft** lädt das Audio →
  praktisch keine spürbare Verzögerung.
- **Cache** (im App-Datenverzeichnis): Audio je **Ansage-Text-Hash** ablegen → „nochmal aufrufen"
  und identische Ansagen kosten nichts/kein Netz; Invalidierung bei Stimmen-/Versionswechsel.
- **Fallback (robust):** kein Azure-Key / Netz weg / API-Fehler → nahtlos die heutige
  **Web-Speech-Ansage** (Wörterbuch + Regel-Engine). Nie stumm, nie blockierend.

## Optional später: Offline-Härtung (nur falls Hallen-Internet doch wackelt)
Vorab-Generierung beim Sync + zweistufiger Cache (feste Fragmente „Feld N"/Disziplin/Runde/„gegen" +
Namens-Clips) → Audiosegmente zusammensetzen. Mehr Bau, aber voll offline. **Erst wenn nötig.**

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

## Phasen (On-Demand-Variante)
1. **Spike** (klein): Rust-Funktion `azure_tts_say(ssml) -> audio`, ein Testaufruf mit gemischter
   Sprache, Qualität + 2–3 Stimmen gegenhören (**braucht Azure-Key**).
2. **Integration**: ganze Ansage als SSML (Namen in `<lang>`), Request beim Auslösen, Audio nach Gong
   abspielen (Web Audio), Cache je Text-Hash, **Fallback** auf Web Speech.
3. **Config-UI** (`azure_tts`: enabled/region/key/voice) + Doku + Datenschutz-Hinweis.
4. *(optional, später)* Offline-Härtung (Vorab-Generierung + Cache), nur falls Hallen-Internet wackelt.

## Bezug
- Sprach-Erkennung: `src/io/transliterate.ts` `detectNameLang` (zh/vn — für Azure auf zh-CN/vi-VN
  mappen; weitere Sprachen ergänzbar).
- Fallback-Ansage: `src/io/announcer.ts` (Web Speech + Wörterbuch + Regel-Engine) bleibt vollständig erhalten.
