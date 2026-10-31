# 0066 — Windows-Stimme (E3) im Kern, vorgezogen und gemeinsam für Turnier-PC und Ansage-Seiten

- **Status:** accepted
- **Datum:** 2026-10-31

## Kontext

Ohne Azure (aus, kein Schlüssel, kein Netz) spricht die Desktop-App heute über
`speechSynthesis`. Diese Ausgabe läuft am Audio-Weg der App vorbei und lässt sich keinem Gerät
zuordnen. Damit wirkt der Ausgabeweg aus [ADR 0065](0065-ansage-ton-aus-dem-kern-wasapi.md)
ohne Azure nicht. Ob am Senior-Cup Azure sprach, ist unbekannt.

[ansage-im-browser.md](../features/ansage-im-browser.md) plant die Windows-Stimme ohnehin als
Etappe E3 (WinRT/OneCore → WAV für die Ansage-Seiten).

Gemessen am 31.10.2026:
- Chromiums lokale Stimmen sind **genau die OneCore-Stimmen** von
  `Windows.Media.SpeechSynthesis.SpeechSynthesizer.AllVoices` (David, Katja, Stefan, George,
  Hazel, Susan, Hedda, Mark, Zira). SAPI5 kennt nur vier „Desktop"-Stimmen, die Chromium nicht
  nutzt.
- `SynthesizeTextToStreamAsync` liefert `audio/wav` in 8–44 ms (warm, 2–14 Wörter).

## Entscheidung

1. **Ein Synthese-Modul im Kern** (`ansage_ton/windows/winrt_stimme.rs`) erzeugt aus den bereits
   korrigierten Text-Segmenten PCM bzw. WAV. Es läuft in `spawn_blocking` (MTA), nie auf dem
   Audio-Thread.
2. **Zwei Abnehmer:**
   - der Turnier-PC: Abspielen über den Kern, Stufe 1 der Spec `ansage-ausgabegeraet`
   - die Ansage-Seiten: `ansage_vertonen` liefert ohne Azure `audio/wav`, E3-Kriterien 24–26,
     eigener PR.
3. **Stimmenzuordnung** von gespeicherter Chromium-`voiceURI` auf den WinRT-Namen: exakt →
   Teil vor „ - " → erste Stimme der Sprache. Gespeicherte Werte gelten ohne Migration. Die
   Liste in den Einstellungen kommt künftig aus WinRT.
4. **Reihenfolge:** Azure → OneCore (Kern) → `speechSynthesis` (nur noch Rückfall, mit Banner).
   **Keine fremdsprachige Ersatzstimme:** Fehlt die deutsche Stimme, übernimmt die Browser-Stimme.
5. **Rate:** `SpeakingRate = rate` (0,5–1,5), Abgleich nach Gehör.

## Alternativen

- **`speechSynthesis` behalten:** Das Ausgabegerät bleibt unsteuerbar, der Senior-Cup-Fall
  bliebe ohne Azure ungelöst.
- **SAPI5 (`ISpVoice`):** nur vier Desktop-Stimmen, nicht die von Chromium, die Stimme klänge
  anders.
- **E3 abwarten:** Stufe 1 wirkte dann nur mit Azure.
- **Englische Windows-Stimme als Ersatz, wenn die deutsche fehlt:** deutsche Namen und Felder
  würden unverständlich ausgesprochen.

## Konsequenzen

- **`windows`-Features:** neu sind `Media_SpeechSynthesis`, `Media_Core`, `Storage_Streams`,
  `Foundation` und `Win32_System_Com` (COM-Init des Audio-Threads).
- **Stimme:** Wer ohne Azure arbeitet, hört dieselbe Stimme über einen anderen Weg. Das Tempo
  kann leicht abweichen (Changelog).
- **E3 in `ansage-im-browser.md`:** gilt mit dieser Spec als vorgezogen, die Messliste dort ist
  teilweise beantwortet.
