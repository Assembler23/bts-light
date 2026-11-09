# Spec: Ansage in Teilen vertonen (Zeichen sparen)

Stand: **freigegeben 09.11.2026**, umgesetzt in v0.9.366. Baut auf
[ansage-elevenlabs.md](ansage-elevenlabs.md) (E8 Zeichenzähler, E9
Seiten-Cache) auf.

## Ziel

Eine Ansage besteht aus festen Teilen, die sich im Turnier ständig
wiederholen („Feld 3." · „Herrendoppel O40." · „Viertelfinale." ·
„gegen …" · „Feld 3."), und aus den Paarungen, die je Team mehrmals
vorkommen. Heute kostet jede Ansage beim Anbieter den vollen Text, weil der
Cache nur **ganze** Ansagen kennt. Vertont man jeden Teil einzeln und
cacht ihn, bezahlt man jeden Teil nur einmal. Schätzung aus dem Köpi-Cup
(1.180 Ansagen à ≈ 170 Zeichen): **50–60 % weniger Zeichen**.

Warum das jetzt tragbar ist (im Gegensatz zum verworfenen Baukasten in
E9): Geschnitten wird **nur an den 350-ms-Blockpausen**, die seit v0.9.335
ohnehin zwischen den Teilen stehen. Die Klebestelle fällt in eine Pause, die
man sowieso hört.

## Entscheidungen

| # | Frage | Entscheidung |
|---|---|---|
| T1 | Wie einschalten? | Häkchen **„Ansage in Teilen vertonen (spart Zeichen)"** im Cloud-Stimmen-Block der Seite Ansagen, **Standard aus**. Gilt für den gewählten Dienst, wirkt sofort wie die Dienstwahl. Ein Schalter für alle Dienste (man testet ohnehin einen Dienst zur Zeit). |
| T2 | Wo wird geteilt? | **Im Kern**, an einer Stelle (`TtsZiel`), damit PC-Ansage, Ansage-Seiten und Slaves gleich arbeiten. Das Frontend schickt weiter ein SSML-Dokument. |
| T3 | Woran wird geteilt? | An `<break time="350ms"/>` (Blockpause aus `ssmlRahmen.mjs`). Jeder Teil wird wieder ein vollständiges `<speak><voice>…</voice></speak>`; ein durch den Schnitt offenes oder geschlossenes `<prosody>` wird ergänzt, damit Tempo und `<lang>`-Regeln je Teil stimmen. Fremdsprachige Namen bleiben im Teil. |
| T4 | Wie zusammensetzen? | Jeder Teil kommt als **PCM 24 kHz mono 16 bit** (Azure `riff-24khz-16bit-mono-pcm`, ElevenLabs `pcm_24000`, OpenAI `pcm`, Gemini WAV 24 kHz). Stille am Anfang/Ende jedes Teils wird bis auf einen kleinen Rand abgeschnitten, dazwischen kommen genau 350 ms Stille. Ergebnis: eine WAV. Kein MP3-Decoder nötig. |
| T5 | Cache | Je Teil, über die vorhandenen Caches: PC-Weg Platte (`tts-cache/`, wie heute), Ansage-Seiten nur Arbeitsspeicher (E9, nie Platte). Der Cache-Schlüssel enthält das Format, damit sich PCM- und MP3-Einträge nie vermischen. |
| T6 | Anfragen | Fehlende Teile werden **gleichzeitig** angefragt (sonst addiert sich die Wartezeit). Scheitert ein Teil, scheitert die Ansage wie heute (Ersatzstimme), es wird nichts Halbes gespielt. |
| T7 | Zähler | Eine Ansage zählt einmal: Zeichen = Summe der neu vertonten Teile; „aus Cache", wenn alle Teile aus dem Cache kamen. |
| T8 | Slaves | Der Schalter reist in der Share mit (`AzureTtsShare.teile`, Standard aus), Slaves teilen wie der Master. |

## Bekannte Grenzen (bewusst)

- **Stimme schwankt je Aufruf**, vor allem ElevenLabs v3: „Feld 3" kann etwas
  anders klingen als die Namen danach. Darum Standard aus und Hörtest.
- **Mehr Anfragen** (bis ~8 je Ansage beim ersten Mal): Gemini-Gratis-Stufe
  reicht dafür nicht.
- WAV statt MP3: größere Cache-Dateien (≈ 48 KB je Sekunde) — bei Teilen aus
  wenigen Sekunden unkritisch.

## Akzeptanzkriterien

1. Schalter aus: Verhalten und Audio Byte für Byte wie bisher.
2. Schalter an: Eine Ansage mit sechs Teilen ergibt beim zweiten Mal mit
   anderen Namen nur Anfragen für die Namen-Teile (Test mit Zähler bzw.
   Mock).
3. Teilen eines echten Ansage-SSML (mit `<lang>` und Tempo) ergibt Teile,
   die jeweils gültig verschachtelt sind (`<prosody>` nie um `<lang>`),
   und deren Text zusammen den ganzen Text ergibt (Test).
4. Zusammengesetzte WAV: 24 kHz, Pausen exakt 350 ms zwischen den Teilen,
   Randstille gekürzt (Test mit synthetischem PCM).
5. Ansage-Seiten schreiben auch mit Schalter kein Audio auf die Platte.
6. Zähler: Zeichen nur für neu vertonte Teile (Test).
7. Ein alter Slave ohne das Feld liest die Share weiter (serde-Default).

## Nicht im Umfang

- Vorab-Vertonen aller Felder/Disziplinen beim Turnierstart.
- Teilen innerhalb eines Blocks (z. B. Namen einzeln).
- Änderungen am Web-Speech-/Windows-Stimmen-Weg.
