# Ausgabegerät für Ansagen — Spezifikation

> Status: **Freigegeben 31.10.2026, umgesetzt** (PR 1 v0.9.318 #416, PR 1b v0.9.320,
> PR 2 v0.9.321) (Spec-Pipeline von Hand: Brief → Grill →
> Messung → How-To → Review, siehe [spec-pipeline-konzept.md](../spec-pipeline-konzept.md)).
> Betroffenes Repo: **nur bts-light**. ADRs: [0065](../adr/0065-ansage-ton-aus-dem-kern-wasapi.md)
> (Ausgabeweg), [0066](../adr/0066-windows-stimme-e3-im-kern.md) (Windows-Stimme).
> Übernimmt Etappe **E3** aus [ansage-im-browser.md](ansage-im-browser.md) (vorgezogen).
> Gemessen 31.10.2026 am Entwickler-PC (WebView2/Edge 156, cpal 0.16 und 0.18.2, WinRT).

## Kontext / Problem

**Senior-Cup, 31.10.2026 (Prod):** Die Hallenanlage hing per Klinke am Notebook. Alle anderen
Programme kamen über die Anlage, bts-light nicht. Ohne Klinke liefen die Ansagen über die
Notebook-Lautsprecher. Laut Turnierleitung war das **nicht das erste Mal**. Die Turnierleitung sah
in der App nicht, wohin die Ansage ging, und konnte nichts einstellen.

Heute (`src/io/announcer.ts`):
- Gong und Azure-Stimme laufen über **einen** `AudioContext`, der beim ersten Ton angelegt und nie
  neu aufgebaut wird.
- Ohne Azure spricht `speechSynthesis` (Windows-Stimme) **am AudioContext vorbei**. Wohin, lässt
  sich nicht steuern.

**Gemessen:**
- Ein dauerhaft offener Ausgabestrom stirbt beim Abziehen des Geräts und **bleibt still**, auch
  wenn das Gerät zurückkommt. Das ist das Muster des gecachten AudioContext und passt zum
  Senior-Cup-Bild.
- Wird das Gerät **je Ton neu aufgelöst**, folgt die Ausgabe jedem Wechsel.
- Chromium (WebView2) nennt **ohne Mikrofon-Freigabe keine Ausgabegeräte**, weder Namen noch IDs.
  Eine Gerätewahl im WebView (`setSinkId`) ist damit ohne Freigabe nicht machbar.

## Zielbild & Erfolgskriterien

Die Ansage des Turnier-PCs (Gong und Stimme) kommt zuverlässig über das Gerät der Hallenanlage, und
die Turnierleitung sieht, worüber sie läuft.

**Erfolg:**
- Am Notebook mit Klinke **und** USB-Box gilt: Gerät einstecken oder abziehen **zwischen** zwei
  Ansagen → die nächste Ansage kommt über das richtige Gerät, ohne Neustart und ohne Klick.
- Ein gezielt gewähltes Gerät wird benutzt, auch wenn Windows ein anderes als Standard führt.
- Fällt es weg, kommt die Ansage über den Windows-Standard, ein Banner sagt es, und das Banner
  verschwindet von selbst, wenn das Gerät zurück ist.
- Weder Gerätefehler noch eine fehlende Windows-Stimme machen die Ansage still, ohne dass ein Banner
  es sagt. Die Ansage-Kette blockiert nie.

## Nicht-Ziele

- Keine Gerätewahl auf der Ansage-Seite `/ansage` (Handy, Tablet, Pi). Dort wählt man am Gerät.
- Keine dauerhafte Marke in der Kopfzeile. Anzeige nur auf der Seite **Ansagen**, Banner nur bei
  Problemen.
- Keine Lautstärke je Gerät, keine zwei Ausgänge gleichzeitig.
- Die App **stellt nichts in Windows um** (Lautstärkemixer, Standardgerät, Sprachausgabe). Sie liest
  nur.
- Kein Umhängen **mitten** in einer Ansage auf ein zurückgekehrtes Gerät (die nächste folgt).
- Schlafende Bluetooth-Boxen, die die ersten Silben verschlucken, sind kein Ziel. Sie werden im
  Handbuch erwähnt.
- Kein Linux-Audio. Unter Linux bleibt der heutige Weg.

## Entscheidungen (Grill, Messung, How-To, 31.10.2026)

| Nr | Entscheidung |
|---|---|
| D1 | **Ton komplett aus dem Rust-Kern** (WASAPI über `cpal` 0.18). Das WebView rechnet nur noch: Es rendert den Gong und dekodiert Azure-MP3, gibt aber nichts aus. ADR 0065. |
| D2 | **Gerät je Ansage-Stück neu auflösen** (Gong, Stimme). Öffnen scheitert → neu auflösen, bis 3 Versuche. |
| D3 | **Windows-Stimme (E3 aus `ansage-im-browser`) vorgezogen in Stufe 1.** WinRT/OneCore erzeugt WAV im Kern. Gemessen sind es dieselben Stimmen wie Chromiums lokale. ADR 0066. |
| D4 | Die Warteschlange bleibt in TS (`enqueueAnnouncement`). Rust spielt ein Stück und kehrt nach dessen Ende zurück. |
| D5 | Der alte WebView-Weg bleibt als **letzter, sichtbarer Rückfall**. |
| D6 | Anzeige ohne Wahl: „Windows-Standard (<Gerät>)" + Hinweis auf den Lautstärkemixer. „Läuft über <Gerät>" nur bei gezielter Wahl. |
| D7 | Gerätewahl **je PC** als WASAPI-Geräte-ID. Wandert nicht im Identitäts-Bündel, wird nicht vom Master vererbt und ist vor Setup-Assistent und Import geschützt. |
| D8 | Gerät fällt **mitten** in einer Ansage weg → das Stück **von vorn** auf dem neuen Gerät, höchstens einmal je Stück. Den Gong lässt man dann weg. |
| D9 | Deutsche Windows-Stimme fehlt → **keine** fremdsprachige Ersatzstimme, stattdessen Browser-Stimme (alter Weg) + Banner. |
| D10 | Notfall-Schalter „Alter Abspielweg", **befristet** auf drei Versionen nach PR 1. |
| D11 | Gong per `OfflineAudioContext` aus `gongKlang.mjs` (eine Klangquelle). Azure bleibt MP3 und wird im WebView dekodiert. Anzeige per Abfrage, keine Windows-Benachrichtigung. |
| D12 | Lieferung in **PR 1** (Stufe 1), **PR 1b** (E3 für Ansage-Seiten) und **PR 2** (Gerätewahl). |

## Betroffene Komponenten / Architekturregeln / Daten

- **R1:** Frontend ↔ Kern nur über Tauri-Commands. Audio geht als PCM16 mono über **Raw-IPC**
  (`invoke(cmd, Uint8Array, {headers:{"x-rate":…}})`, Rust `tauri::ipc::Request::body()`).
- **Neues Rust-Modul** `src-tauri/src/ansage_ton/`:
  - `mod.rs`: API, `TonFehler`, `Abspielbericht`, Stubs für `not(windows)`
  - `pcm.rs`, `stimme.rs`: rein, beide Plattformen, unit-getestet
  - `windows/ausgabe.rs`: eigener Audio-Thread, cpal
  - `windows/winrt_stimme.rs`: `SpeechSynthesizer`
- **Neue Commands:**
  - PR 1: `ansage_pcm_spielen`, `ansage_stimme_spielen`, `ansage_abbrechen`, `ansage_ausgabe`, `ansage_stimmen`
  - PR 2: `ansage_geraete`, `ansage_ausgabe_setzen`
- **Frontend:**
  - `announcer.ts`: Gong, Azure, Sprache, `playNameTest` und Abbruch laufen über den Kern; der alte Weg bleibt als Rückfall.
  - neu: `src/io/tonWeg.mjs` (rein: Rückfall-Kette, Fristen), `state/ansageAusgabe.ts`, `components/AnsageAusgabeBanner.tsx`
  - geändert: `AnnouncePage.tsx` (Anzeige, PR 2 Auswahl), `AnnounceSettings.tsx` (Stimmenliste aus Windows, Notfall-Schalter), `AzureFallbackBanner.tsx` (Text).
- **Konfiguration:**
  - PR 1: `announce.legacy_output: bool` (Standard `false`)
  - PR 2: `announce.output_device: String` (leer = Windows-Standard) und `announce.output_device_name: String` (nur Anzeige bei fehlendem Gerät)
  - alles mit `#[serde(default)]`, alte Konfigurationen bleiben gültig.
- **Abhängigkeiten:**
  - `cpal = "0.18"` **nur** unter `[target.'cfg(windows)'.dependencies]`. Lizenz Apache-2.0, nutzt dieselbe `windows`-Version 0.62. Der Linux-CI-Job hat kein ALSA, deshalb ist cpal dort ausgeschlossen.
  - `windows`-Features neu: `Media_SpeechSynthesis`, `Media_Core`, `Storage_Streams`, `Foundation`, `Win32_System_Com` (COM-Init des Audio-Threads).
- **Datenschutz:** Gerätenamen (Bluetooth-Geräte tragen oft Personennamen) kommen **nie** ins
  Diagnose-Log (R6, `log_upload.rs`). Geloggt werden nur Ereignisarten.

## Verhalten im Detail

### Abspielen (PR 1)

1. **Ablauf einer Ansage:** Gong (wenn eingeschaltet) → 150 ms Atempause → Stimme, wie heute in
   `playPrepared`.
   - **Gong:** einmal je Art per `OfflineAudioContext` + `gongPlanen` zu PCM16 gerendert und
     gecacht, dann `ansage_pcm_spielen`.
   - **Azure-Stimme:** MP3 wie heute aus `azure_tts_speak`, im WebView per `decodeAudioData` zu PCM16
     (24 kHz) dekodiert, dann `ansage_pcm_spielen`.
   - **Ohne Azure oder bei Azure-Fehler:** `ansage_stimme_spielen {segmente, lang, voice, rate}`. Der
     Kern synthetisiert je Segment über WinRT, setzt 300 ms Stille dazwischen und spielt alles als
     **ein** Stück.
2. **Abspielen eines Stücks im Kern** (eigener Thread mit fester COM-Initialisierung):
   1. Gerät auflösen: in PR 2 erst das gewählte (`device_by_id`), sonst der Windows-Standard.
   2. Mix-Format holen, linear auf die Geräte-Rate resampeln, auf die Kanäle verteilen.
   3. Vorn 20 ms Stille, hinten 150 ms Nachlauf.
   4. Abspielen. „Fertig" wird erst gemeldet, wenn alles samt Nachlauf geschrieben ist.
   5. Strom schließen.
3. **Öffnen scheitert:** neu auflösen, 300 ms warten, höchstens 3 Versuche.
   - Bei `RPC_E_CHANGED_MODE` (gemessen, 31.10.: ~8 s still) wird der **Audio-Thread neu
     aufgesetzt**, statt nur zu wiederholen.
   - Danach Rückfall (s. u.).
4. **Abriss mitten im Stück:** Gerät neu auflösen und das Stück **von vorn** spielen, höchstens
   einmal. Beim Gong wird der Rest weggelassen.
5. **Frist in TS:** Jeder Kern-Aufruf läuft gegen eine Frist (`gongResolveRace`).
   - PCM: Dauer + 2000 ms.
   - Stimme: 2000 + Zeichen·140 + 1000 ms.
   - Abgelaufen → `ansage_abbrechen`, Banner, die Kette läuft weiter.
6. **Abbruch** (`cancelAnnouncements`, z. B. Ansagen ausgeschaltet): Der Kern erhöht eine Generation,
   stoppt den laufenden Strom, und der wartende Aufruf kehrt mit `abgebrochen: true` zurück.
   Wartende ältere Stücke verfallen.

### Rückfall-Kette (rein, `tonWeg.mjs`)

| Stück | 1. | 2. | 3. |
|---|---|---|---|
| Gong | Kern | alter WebView-Gong | weglassen |
| Sprache mit Azure | Azure über Kern | Windows-Stimme über Kern (+ Azure-Banner wie heute) | Browser-Stimme (alter Weg) + Ausgabe-Banner |
| Sprache ohne Azure | Windows-Stimme über Kern | Browser-Stimme + Ausgabe-Banner | – (Frist schützt die Kette) |

- Unter Linux meldet der Kern `NichtUnterstuetzt`, dann alter Weg **ohne** Banner.
- **Notfall-Schalter** `legacy_output` an → nur noch der alte Weg (heutiges Verhalten).

### Anzeige und Banner

- **Seite Ansagen, neben dem Test-Knopf:**
  - ohne Wahl: **„Ausgabe: Windows-Standard (<Gerät>)"**
  - darunter der Hinweis: „Kommt die Ansage aus dem falschen Lautsprecher? In Windows unter
    *Lautstärkemixer* bei **BTS Light** das Ausgabegerät auf *Standard* stellen."
  - Abfrage alle 3 s, nur solange die Seite sichtbar ist.
- **Banner** (eigener Store, löst sich selbst auf, wenn das nächste Stück wieder ohne Rückfall läuft
  bzw. das Gerät zurück ist):
  - „Kein Ausgabegerät gefunden – Ansage über das Programmfenster versucht"
  - „Keine Windows-Stimme für Deutsch installiert – Sprachpaket in den Windows-Einstellungen
    nachinstallieren"
  - „Ansage über das Programmfenster (Rückfall)" bei sonstigem Kern-Fehler
  - PR 2: „<Gerät> nicht verfügbar – Ansage läuft über Windows-Standard (<Standard>)"
- `AzureFallbackBanner`: „… die Windows-Stimme übernimmt".

### Stimmen

- **Liste in den Einstellungen** aus `ansage_stimmen()` (WinRT `AllVoices`: Name, Sprache).
  Rückfall: Browser-Liste.
- **Gespeicherte Werte** (`voice_de`/`voice_en`, heute Chromium-`voiceURI` wie „Microsoft Katja -
  German (Germany)") gelten ohne Migration. Zuordnung: exakt → Teil vor „ - " → erste Stimme der
  Sprache → `KeineStimme`.
- **Rate:** `SpeakingRate = rate` (0,5–1,5).
- **Namens-Test** (`playNameTest`) läuft über denselben Weg wie die echte Ansage.

### Gerätewahl (PR 2)

- **Auswahl** auf der Seite Ansagen: „Ansage über: [Windows-Standard | <Geräte…>]".
  - Gespeichert wird die Geräte-ID (z. B. `wasapi:{0.0.0.00000000}.{16f9a587-…}`) und der Name zur
    Anzeige.
  - Setzen nur über `ansage_ausgabe_setzen`, nie über `save_config`.
- **Anzeige:**
  - gewählt und verfügbar → „Ausgabe: <Gerät>"
  - fehlt → „<Gerät> nicht verfügbar – Ansage läuft über Windows-Standard (<Standard>)" + Banner.
  - Kommt das Gerät zurück, verschwindet das Banner (Abfrage 5 s), und die nächste Ansage läuft
    wieder darüber.
- **Schutz:**
  - `identity_bundle` leert das Feld.
  - `apply_imported_identity` behält den Wert des PCs.
  - `keep_host_managed_fields` schützt es.
  - `AzureTtsShare` und Ansage-Slaves vererben es nicht.

### PR 1b: Ansage-Seiten ohne Azure

`ansage_vertonen` liefert ohne Azure OneCore-WAV (`audio/wav`) statt „kein Audio".
Umgesetzt in v0.9.320, dabei (Review):
- `hostAudio` im Feed ist jetzt „Azure **oder** Windows-Stimme vorhanden" — sonst meldet
  sich eine Seite ohne Gerätestimme (Pi) nie als Zuhörer, und es wird gar nicht vertont.
- WAV (16 kHz mono ≈ 32 KB/s, Doppel-Ansage bei Tempo 0,8 ≈ 720 KB) ist ~12× so groß wie
  Azure-MP3: Deckel je Audio 1 → **3 MiB**, je Ablage 6 → **32 MiB**; über dem Gesamtdeckel
  verlieren die ältesten Einträge nur ihr Audio statt ganz zu verschwinden; zu großes Audio
  wird geloggt.
- Bekannt: Zwei sehr schnell nacheinander ausgelöste Desktop-Ansagen können auf den Seiten
  in vertauschter Reihenfolge ankommen (Nummer wird nach der Synthese vergeben; mit Azure
  schon vorher so).
Das erfüllt die E3-Kriterien 24–26 aus [ansage-im-browser.md](ansage-im-browser.md).

### Rückwärtskompatibilität

- **Alte Konfiguration:** ohne die neuen Felder bedeutet das Windows-Standard, neuer Weg, kein
  Notfall-Schalter.
- **Stimmen:** gespeicherte Stimmen gelten weiter. Klang ohne Azure praktisch gleich (gleiche
  OneCore-Stimmen), Rate nach Gehör abgeglichen.
- **Lautstärkemixer:** Der Eintrag heißt jetzt „BTS Light" statt „Microsoft Edge WebView2". Eine alte
  WebView-Zuordnung wirkt nicht mehr.

## Akzeptanzkriterien

**PR 1**
1. Test-Ansage mit Azure: Gong und Stimme kommen über das Windows-Standardgerät; die Seite zeigt
   „Ausgabe: Windows-Standard (<Gerät>)".
2. Azure aus: Gong + Windows-Stimme (gewählte bzw. Standardstimme der Sprache), über den Kern.
3. Zwischen zwei Ansagen USB-Box bzw. Klinke einstecken → die nächste Ansage kommt darüber; abziehen
   → die nächste über das neue Standardgerät. Kein Neustart, kein Klick.
4. Gerät **während** einer Ansage abziehen → das Stück beginnt auf dem neuen Gerät von vorn
   (höchstens einmal); die Kette läuft weiter.
5. `RPC_E_CHANGED_MODE` beim Öffnen → Audio-Thread neu, das Stück spielt; spätestens das nächste
   Stück ist hörbar (Sonde).
6. Kein Ausgabegerät aktiv → Banner „Kein Ausgabegerät gefunden …"; die Kette blockiert nicht (fünf
   Ansagen nacheinander kehren alle zurück).
7. Keine deutsche Windows-Stimme → Browser-Stimme + Banner „Keine Windows-Stimme für Deutsch …";
   **nie** eine englische Stimme für einen deutschen Text.
8. Fünf Ansagen schnell nacheinander → strikt nacheinander, 150 ms nach dem Gong; keine
   Überlappung zwischen Kern- und Rückfall-Ton.
9. Ansagen während der Wiedergabe abschalten → sofort still, keine Nachzügler.
10. Kern hängt (Sonde: künstlich verzögert) → nach der Frist Abbruch, Banner, die nächste Ansage
    spielt.
11. Notfall-Schalter an → Verhalten wie v0.9.317 (AudioContext und `speechSynthesis`).
12. Linux-Build: kompiliert ohne ALSA; Ansage über den alten Weg ohne Banner.
13. `ansage_pcm_spielen` lehnt `x-rate` außerhalb 8000–96000, Bodies über 8 MiB und ungerade Längen
    ab (Unit-Test).
14. Diagnose-Log enthält nach einem Gerätewechsel **keinen** Gerätenamen.
15. Namens-Test in den Einstellungen klingt und läuft wie die echte Ansage.
16. App im Tray und Bildschirm gesperrt → die Ansage kommt.

**PR 1b**

17. Ansage-Seite ohne Azure spielt OneCore-WAV; Kriterien 24–26 aus `ansage-im-browser.md` erfüllt.

**PR 2**

18. Gerät gewählt, Windows-Standard ist ein anderes → Ansage über das gewählte.
19. Gewähltes Gerät abziehen → Ansage über den Standard + Banner mit Gerätenamen; wieder anstecken
    → Banner verschwindet innerhalb von 5 s, die nächste Ansage läuft über das gewählte Gerät.
20. Einstellungen speichern (Setup-Assistent, Ansage-Einstellungen) ändert die Gerätewahl **nicht**
    (Test `keep_host_managed_fields`).
21. Identitäts-Bündel exportieren → ohne Gerätewahl; importieren → die Gerätewahl des Ziel-PCs
    bleibt (Tests).
22. Ansage-Slave mit geerbtem Azure übernimmt **keine** Gerätewahl vom Master (Test).
23. Alte `config.json` ohne die Felder lädt → Windows-Standard (Serde-Test).

## Tests

- **Rust-Unit (CI):**
  - `pcm.rs`: Byte-Parsing, WAV-Chunks, Resampling (Länge, Gleichanteil, Nulldurchgänge),
    Kanalverteilung, Rand-Stille, 60-s-Deckel
  - `stimme.rs`: Zuordnung, Rate-Grenzen, Segment-Fuge
  - Command-Prüfungen
  - PR 2: Serde, Identitäts-Bündel, Import, `keep_host_managed_fields`, Vererbung
  - `not(windows)`-Stub
- **Node (CI):** `scripts/test-ton-weg.mjs` (Rückfall-Kette, Fristen, Linux ohne Banner,
  Generation). `test-gong-timing.mjs` und `test-gong-klang.mjs` bleiben unverändert grün.
- **Sonden (`#[ignore]`, von Hand):** `tests/ansage_ton_probe.rs` (Wechsel, Abriss,
  `RPC_E_CHANGED_MODE`, gewählte ID), WinRT-Stimmen und Latenz.
- **Hörtest vor Merge:** Kriterien 1–11, 15, 16 (und PR 2: 18, 19) am Notebook mit Klinke und
  USB-Box, Protokoll in den PR.

## Risiken & Rollback

| Risiko | Gegenmaßnahme |
|---|---|
| COM-Konflikt (gemessen) | eigener Thread, Neuaufsetzen bei `RPC_E_CHANGED_MODE`, WinRT nur in `spawn_blocking`, TS-Frist |
| Spielt fehlerfrei, aber auf dem falschen Gerät | Anzeige + Hinweis; Notfall-Schalter (D10) |
| Erste Silben abgeschnitten | 20 ms Vorlauf; Gong vorn |
| Lineares Resampling klingt rau | Hörtest; kubische Hermite-Interpolation als Ersatz ohne neue Crate |
| Windows-Stimme klingt anders | gleiche Stimmen, Rate nach Gehör, Changelog |
| CI ohne Audiogerät / Linux ohne ALSA | Hardware-Tests `#[ignore]`; cpal nur `cfg(windows)` |
| Missbrauch über Raw-IPC | Deckel + Unit-Test |

**Rollback:**
1. Automatischer Rückfall (sichtbar).
2. Notfall-Schalter ohne Update.
3. Updater auf die Vorversion.

## Hörtest PR 1 (31.10.2026, Entwickler-PC, USB-Box + Laptop-Lautsprecher)

- **Ergebnis:** Ansage auf der Box; Box zwischen zwei Ansagen abgezogen →
  Laptop; wieder angesteckt → Box; mitten in der Ansage abgezogen → Log
  „Ausgabestrom abgerissen" + „Stück beginnt von vorn", Ansage auf dem Laptop
  von vorn. Kein Banner, kein Öffnen-Fehler.
- **Befund 1 — konkretes statt virtuelles Standardgerät:** cpal 0.18 gibt bei
  `default_output_device()` das **virtuelle** „Standard-Ausgabe"-Gerät von
  Windows (`ActivateAudioInterfaceAsync`), nicht den Endpunkt. Der Kern löst
  deshalb das Standardgerät selbst auf (`IMMDeviceEnumerator::
  GetDefaultAudioEndpoint(eRender, eConsole)` → Endpunkt-ID →
  `Host::device_by_id`); das virtuelle Gerät bleibt nur Rückfall.
  `windows`-Feature `Win32_Media_Audio`.
- **Befund 2 — gestörtes Gerät in Windows:** Nach vielem Ab- und Anstecken
  bekam die Box eine neue Endpunkt-ID, und Windows lieferte für sie
  `AUDCLNT_E_UNSUPPORTED_FORMAT` schon bei `GetMixFormat` — für **jedes**
  Programm (auch der Browser blieb stumm). Erst Aus-/Einschalten der Box
  heilte es. Die App zeigte das Rückfall-Banner; hörbar war nichts, weil auch
  der Rückfall auf dasselbe Gerät spielt. Das Log nennt seither bei `intern`
  die cpal-Fehlerart und den Windows-Text (ohne Gerätenamen).
  Handbuch-Hinweis: Kommt auch aus dem Browser nichts, liegt es am Gerät.

## Hörtest Klinke (31.10.2026, Entwickler-PC)

- Die Klinke erscheint als **eigener Endpunkt** „Headphones (2- Intel® Smart
  Sound …)" neben „Lautsprecher (…)"; Windows macht sie beim Einstecken zum
  Standard.
- v0.9.318: Klinken-Box → Klinke gezogen → Laptop → eingesteckt → Klinken-Box;
  mitten in der Ansage gezogen → von vorn auf dem Laptop (Log). Bestanden.
- **Aber:** Auch v0.9.317 folgte an diesem PC dem Klinkenwechsel. Der
  Senior-Cup-Fehler ist hier **nicht nachstellbar**; seine Ursache (Treiber
  des Notebooks oder feste Zuordnung im Lautstärkemixer) bleibt unbewiesen.
  Der Test am Senior-Cup-Notebook steht aus.

## Offene Fragen / Annahmen

- **Gemessen nur am Entwickler-PC** (Intel Smart Sound + USB-Box). Ein Realtek-Notebook mit
  getrennten Endpunkten für Klinke und Lautsprecher (Senior-Cup-Fall) ist nicht gemessen. Das holt
  der Hörtest vor dem Merge nach, möglichst am Senior-Cup-Notebook.
- **Senior-Cup-Ursache unbewiesen** (Azure oder Browser-Stimme, Mixer-Zuordnung oder toter Strom).
  Der neue Weg deckt alle drei ab.
- Rate-Gleichheit zwischen Chromium und WinRT nach Gehör, nicht gemessen.
- **Notfall-Schalter:** Entfernung drei Versionen nach PR 1, Termin in der Roadmap.

## Betroffene Doku-Dateien

| Datei | PR |
|---|---|
| `docs/announcements.md`: Ausgabeweg, Anzeige, Windows-Stimme, Banner, „Ansage kommt aus dem falschen Lautsprecher", Sprachpaket, Notfall-Schalter; PR 2 Gerätewahl je PC | 1, 2 |
| `docs/oberflaechen.md`: Seite Ansagen, `pruef`-Anker für „Ausgabe:", „Windows-Standard", „Ansage über", „Alter Abspielweg", Bannertexte | 1, 2 |
| `docs/features/ansage-im-browser.md`: § E3 → Verweis hierher | 1, 1b |
| `CLAUDE.md`: Tabellenzeile „Sprachansagen"/neue Zeile Ausgabegerät | 1 |
| `docs/logging.md`: keine Gerätenamen | 1 |
| `docs/adr/0065`, `0066` → accepted; README | 1 |
| `docs/changelog.md`, `docs/roadmap.md` | 1, 1b, 2 |
| `.github/workflows/ci.yml`: Schritt „Ansage-Tonweg (JS)" | 1 |

## Umsetzungs-Hinweise (aus dem How-To)

- **Code-Stellen:**
  - `getAudioContext` `announcer.ts:131`
  - `playPrepared` `:1035`
  - `enqueueAnnouncement` `:192`
  - `speakSegment` `:246`
  - `playNameTest` `:1104`
  - `cancelAnnouncements` `:1117`
  - `gongPlanen` in `gongKlang.mjs` läuft unverändert auf `OfflineAudioContext`.
- **Kern-Stellen:**
  - `azure_tts_speak` `commands.rs:936`
  - `ansage_vertonen` `:3931` (PR 1b, Zeile ~3951)
  - `keep_host_managed_fields` `:354`
  - `identity_bundle` `:475`
  - `apply_imported_identity` `:499`
  - Plattform-Muster: `print/mod.rs`
- **cpal 0.18:** `description().name()`, `sample_rate()` liefert `u32`, `build_output_stream` nimmt
  `StreamConfig` by value, `Device::id()`/`Host::device_by_id`. Sonde unter
  `docs/features/_intake/ansage-ausgabegeraet/` beschrieben.
- **WinRT:** `SynthesizeTextToStreamAsync` → `audio/wav`, 8–44 ms (14 Wörter 282 KB). Stimmen
  identisch mit Chromiums lokalen (David, Katja, Stefan, George, Hazel, Susan, Hedda, Mark, Zira).
- **PR-Schnitt:**
  - PR 1, Schritte: Cargo → `pcm.rs` → `stimme.rs` → `mod.rs` → Ausgabe-Thread → WinRT → Commands →
    `tonWeg.mjs` → `announcer.ts` → `api.ts` → Seite/Einstellungen/Banner → Doku/ADRs.
  - PR 1b: `ansage_vertonen`.
  - PR 2: Config + Schutz → `device_by_id` → Commands → UI → Banner.
