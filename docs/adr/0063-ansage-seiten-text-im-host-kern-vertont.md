# 0063 — Ansage-Seiten: Text aus der Host-Oberfläche, Kern vertont, Seite spielt nur ab

- **Status:** accepted
- **Datum:** 2026-10-30

## Kontext

Ansagen sollen auf Geräten ohne Windows-Programm erklingen — Tablet, Handy,
Pi am TV (Spec [ansage-im-browser](../features/ansage-im-browser.md)). Die
gesamte Ansage-Logik lebt in TypeScript (`announcer.ts`): Satzbau,
Aussprache-Korrekturen samt Community-Wörterbuch, de/en-Wahl, Azure-SSML.
Der Azure-Schlüssel darf den Turnier-PC nicht verlassen, und eine offene
LAN-Route darf niemandem erlauben, beliebigen Text auf Azure-Kosten in die
Halle zu sprechen. Ansagen enthalten Spielernamen.

## Entscheidung

1. **Text in der Host-Oberfläche, Vertonung im Kern, Abspielen auf der
   Seite.** Die WebView des Turnier-PCs baut die Ansage mit demselben Code
   wie für den eigenen Lautsprecher (`prepare*` in `announcer.ts`,
   `jobZuAnsage`) und übergibt Text und SSML per Tauri-Command. Der Kern
   ruft Azure und legt einen fertigen **Ansage-Eintrag** ab. Die Seite
   `/ansage` liest nur (GET) und spielt Gong → Audio → Gerätestimme.
2. **Eigener Schalter** `announce.page_audio` („Audio für Ansage-Seiten
   erzeugen"), getrennt von `announce.enabled` (= am PC abspielen), Standard
   aus.
3. **Nur im RAM, 60 s.** Seiten-Audio geht nie in den `tts-cache/`
   (`azure_tts::synthesize_ohne_cache`), Deckel 32 Einträge / 6 MiB / 1 MiB
   je Audio.
4. **Genau einmal vertonen, nur mit Zuhörer.** Der Kern beansprucht jede
   Quelle (TL-Auftrag, Freitext) atomar für den Vertoner; eine Quelle, für
   deren Halle gerade keine Seite zuhört, wird verworfen — kein
   Azure-Aufruf, kein spätes Nachsprechen.
5. **Präsenz nur, wenn es klingt.** Eine Seite zählt als Ansage-Gerät ihrer
   Halle (TL-Warnung „niemand hört zu") nur mit freigeschaltetem Ton, einer
   Stimme (Host-Audio oder Gerät) und aktivem Feed.

## Alternativen

- **Ansagetext in Rust portieren:** eine zweite Kopie von Satzbau und
  Wörterbuch — driftet, eigenes Projekt.
- **Text auf der Seite bauen, Vertonen per Endpunkt:** bräuchte einen
  offenen Schreibweg; jeder im WLAN könnte Text auf Azure-Kosten sprechen.
- **Gerätestimme statt Host-Audio:** Pi-Chromium hat oft gar keine Stimme,
  Geräte klängen verschieden.
- **Live-Stream des PC-Tons:** große Infrastruktur, Latenz, nicht nötig.

## Konsequenzen

- Ohne laufende Desktop-Oberfläche entsteht nichts; der Feed meldet das
  (`renderer_weg`, 10 s ohne Lebenszeichen).
- Ohne Azure liefert der PC bis zur Offline-Stimme (Etappe E3) kein Audio;
  die Seite spricht mit der Gerätestimme oder gibt nur den Gong und meldet
  keine Präsenz.
- PC mit „Ansagen aktivieren" **und** Seite in derselben Halle klingen
  doppelt — die App warnt, verhindert es aber bewusst nicht.
