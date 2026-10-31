# 0065 — Ansage-Ton aus dem Rust-Kern (WASAPI über cpal), Gerät je Abspielvorgang neu aufgelöst

- **Status:** accepted
- **Datum:** 2026-10-31

## Kontext

Am Senior-Cup (31.10.2026) kam die Ansage nicht über die per Klinke angeschlossene
Hallenanlage, obwohl alle anderen Programme es taten. Die Desktop-App spielt Gong und
Azure-Stimme über **einen** dauerhaft gecachten `AudioContext` (`announcer.ts:131`), die
Ersatzstimme über `speechSynthesis` am Context vorbei. Beides lässt sich aus der App nicht auf
ein Gerät lenken.

Gemessen am 31.10.2026 (Entwickler-PC, Spec [ansage-ausgabegeraet](../features/ansage-ausgabegeraet.md)):

- WebView2/Chromium 156 nennt über `enumerateDevices()` **ohne Mikrofon-Freigabe weder
  Namen noch IDs** von Ausgabegeräten. `setSinkId` ist damit nicht gezielt nutzbar.
- Ein dauerhaft offener WASAPI-Strom stirbt beim Abziehen des Geräts und **bleibt still**,
  auch nach dem Wiederanstecken. Wird das Gerät **je Ton neu aufgelöst**, folgt die Ausgabe
  jedem Wechsel.
- WASAPI liefert Geräteliste, Namen und stabile Endpunkt-IDs ohne jede Freigabe. Ein per ID
  gewähltes Gerät wird benutzt, auch wenn Windows ein anderes als Standard führt.
- `RPC_E_CHANGED_MODE` beim Öffnen nach einem Wechsel (Sonde auf dem Hauptthread) ergab rund
  8 s Stille. Der Audio-Thread braucht deshalb eine feste COM-Initialisierung.

## Entscheidung

1. Gong, Azure-Stimme und Windows-Stimme gibt der **Kern** über `cpal` 0.18 (WASAPI) auf einem
   **eigenen Audio-Thread** aus. Das WebView rendert nur noch den Gong
   (`OfflineAudioContext` + `gongKlang.mjs`) und dekodiert Azure-MP3 (`decodeAudioData`).
2. Das Gerät wird **je Stück** aufgelöst: gewählte Geräte-ID (`Host::device_by_id`), sonst
   Windows-Standard.
   - Scheitert das Öffnen, wird neu aufgelöst, höchstens 3 Versuche.
   - Bei `RPC_E_CHANGED_MODE` wird der Audio-Thread neu aufgesetzt.
   - Reißt das Gerät mitten im Stück ab, spielt das Stück einmal von vorn.
3. Die Warteschlange bleibt in TypeScript (`enqueueAnnouncement`). Der Command kehrt erst nach
   dem Ende des Stücks zurück. TS schützt jeden Aufruf mit einer Frist.
4. Übergabe WebView → Kern als **PCM16 mono über Raw-IPC** (`x-rate`-Header, Deckel 8 MiB und
   60 s). Resampling im Kern linear.
5. Der alte WebView-Weg bleibt als letzter, **sichtbarer** Rückfall (Banner). Dazu kommt ein
   befristeter Notfall-Schalter „Alter Abspielweg".
6. `cpal` nur unter `cfg(windows)`. Unter Linux meldet der Kern `NichtUnterstuetzt`, und es
   bleibt beim alten Weg.
7. Gerätewahl je PC als Endpunkt-ID in `announce.output_device`. Sie wird nicht exportiert, nicht
   vererbt und ist gegen `save_config` geschützt.

## Alternativen

- **`setSinkId` + programmatische WebView2-Medienfreigabe:**
  - braucht `webview2-com` und `with_webview`
  - Mikrofon-Symbol und Datenschutz-Sperre können stören
  - PCs ohne Mikrofon sind ungeklärt
  - die Ersatzstimme bleibt unsteuerbar
  - Verworfen.
- **`setSinkId` + Gerätenamen aus Rust:** unmöglich, Chromium liefert ohne Freigabe keine IDs
  zum Abgleich.
- **Eigene WASAPI-Ausgabe über `windows` 0.62:**
  - Vorteile: liefert IDs, `AUTOCONVERTPCM` erspart das Resampling.
  - Nachteile: rund 300 Zeilen `unsafe`-COM, ungemessen.
  - Seit cpal 0.18 (gleiche `windows`-Version, `Device::id`) ohne Vorteil.
- **Azure als WAV anfordern:** achtfache Download- und Cache-Größe (`tts-cache/` ohne
  Aufräumen), zweiter Formatpfad neben den Ansage-Seiten.
- **MP3 im Kern dekodieren (`symphonia`):** neue Abhängigkeit (MPL-2.0) ohne zwingenden Grund.
- **Gong nach Rust portieren:** dritte Kopie des Klangs neben `gongKlang.mjs` und `ansage.html`.
- **Warteschlange in Rust:** doppelte Rückfall- und Generationslogik.

**Nachtrag Hörtest (31.10.2026):** Das Standardgerät wird als **konkreter
Endpunkt** aufgelöst (`IMMDeviceEnumerator::GetDefaultAudioEndpoint` →
`Host::device_by_id`). cpal 0.18 liefert bei `default_output_device()` das
virtuelle Standardgerät von Windows; darauf ist es nur noch Rückfall.

## Konsequenzen

- **Abhängigkeit und Build:** Neue Abhängigkeit `cpal` (Apache-2.0, RustAudio, MSRV 1.85), nur
  Windows; die Windows-Build-Zeit steigt etwas.
- **Lautstärkemixer:** Der Eintrag heißt „BTS Light" statt „Microsoft Edge WebView2", alte
  Zuordnungen wirken nicht mehr.
- **Raw-IPC:** wird erstmals im Repo genutzt.
- **Hardware-Verhalten:** ist in CI nicht testbar. Dafür gibt es Sonden (`#[ignore]`) und den
  Hörtest vor dem Merge.
