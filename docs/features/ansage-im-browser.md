# Sprachansagen im Browser (Ansage-Seite) — Spezifikation

> Status: **Entwurf zur Freigabe 2026-10-29** (via /idee: Brief → Grill →
> How-To → Spec). Quelle: Nutzer-Anforderung vom 29.10.2026.
> Umsetzung: E1 v0.9.311 · **E1b v0.9.313** (Monitor-Modus + Pi-Autoplay).
> Betroffene Crates: `src-tauri`, `relay-proto`, `relay`; Frontend `src/`;
> `pi/`.
> ADRs: werden je Etappe angelegt (vier neue, Nummern beim Anlegen
> vergeben — Stand heute ab 0063), siehe [ADRs](#adrs).

## Kontext / Problem

> Wir wollen die Soundausgaben auf andere Geräte bringen, ich will dafür aber
> nicht extra über ein Windowsprogramm lösen, sondern über eine Website o.ä.

Sprachansagen erklingen heute nur dort, wo eine bts-light-Instanz läuft: am
Turnier-PC selbst oder an einem **Ansage-Slave** — einem zweiten
Windows-Rechner mit bts-light, per LAN (liest BTP selbst) oder per Cloud
(pollt `/{ns}/info/announce/state`). Für eine zweite Halle, einen
Meeting-Point oder einen Lautsprecher am Hallen-TV heißt das: Windows-Rechner
hinstellen, installieren, koppeln.

Dabei ist die Sprach-Engine (`src/io/announcer.ts`) schon Browser-Code, und
die Geräte, die ohnehin in der Halle hängen — Raspberry Pis an den TVs,
Zähl-Tablets, Handys der Helfer — haben einen Browser.

## Zielbild & Erfolgskriterien

Jedes Gerät mit Browser kann Ansagen einer Halle sprechen: Link oder QR
öffnen, Halle wählen, „Ton freischalten" — fertig. Der Pi am Hallen-TV sagt
zusätzlich zu seiner Monitor-Anzeige an, ohne Zusatzgerät. Im LAN und, bei
Bedarf, über die Cloud.

1. Ein Tablet oder Handy ist in **unter einer Minute** Ansage-Gerät einer
   Halle (QR scannen, Halle wählen, einmal tippen).
2. Alle Ansage-Geräte einer Halle **klingen gleich** — dieselbe Stimme, die
   der Turnier-PC erzeugt, unabhängig vom Gerät.
3. Die Turnierleitung sieht ehrlich, ob in einer Halle jemand zuhört; ein
   Gerät, das nicht sprechen kann, meldet sich **nicht** als Ansage-Gerät.
4. Der Azure-Schlüssel erreicht **nie** einen Browser oder das Relay-Feed.
5. Der bestehende Windows-Ansage-Slave funktioniert **unverändert** weiter.

## Grundentscheidungen

| Frage | Entscheidung | Warum |
|---|---|---|
| Wer baut den Ansagetext? | Die **Host-Oberfläche** (WebView des Turnier-PCs) baut Text/SSML wie heute; der **Rust-Kern vertont** und legt das Ergebnis als Ansage-Eintrag ab. Die Seite **spielt nur ab**. | Aussprache-Korrekturen, Community-Wörterbuch, de/en-Wahl, Azure-SSML leben ausschließlich in TS (`announcer.ts` u. a.). Ein Port nach Rust wäre eine zweite Kopie und ein eigenes Projekt; Text auf der Seite bräuchte einen offenen Vertonungs-Endpunkt. |
| Klang | Turnier-PC rendert Audio: **Azure**, wenn eingerichtet, sonst **Windows OneCore** (`Windows.Media.SpeechSynthesis`, ab E3). Gerätestimme (Web Speech) nur als Notlösung. | Gleicher Klang auf allen Geräten, auch auf dem Pi (Chromium ohne Sprachpaket hat keine Stimmen). |
| Schalter | Neuer Schalter **„Audio für Ansage-Seiten erzeugen"** (`announce.page_audio`, Standard **aus**), unabhängig von „Ansagen aktivieren" (`announce.enabled`, = am PC abspielen). | Der PC kann im Büro stumm bleiben und trotzdem die Hallen versorgen. |
| Doppelbetrieb | PC (oder Windows-Slave) **und** Ansage-Seite in derselben Halle ist ein Bedienfehler; die Desktop-Oberfläche **warnt**, verhindert ihn aber nicht. | Bewusst keine Automatik, die Geräte stummschaltet. |
| Erst-Ansage („neues Spiel auf Feld X") | Erkennung wandert vom Frontend (`MatchAnnouncer`) in den **Rust-Kern** (`TabletState`) und erzeugt einen Auftrag `court_assigned`. | Sonst könnte die Seite nur TL-Aufträge und Freitext sprechen; ein Auftragsstrom für alle Hörer. |
| Zugang | **LAN**: wie die Monitor-Seiten, offen. **Cloud**: eigener, **nur lesender Ansage-Schlüssel** (≠ `install_id`), rotierbar. | Die `install_id` ist zugleich Tablet-Schreibzugang; ein QR für Helfer-Handys darf sie nicht streuen. |
| Schreibwege | Die Seite **liest nur** fertige Einträge per GET. Kein Text-/SSML-Parameter, kein Vertonen auf Zuruf. | Sonst könnte jeder im WLAN beliebigen Text auf Azure-Kosten in die Halle sprechen. |
| Speicher | Seiten-Audio **nur im RAM**, Verfall **60 s** (Host und Relay). | Audio enthält Spielernamen; dieselbe Frist wie die Ansage-Aufträge. |
| Pi am TV | Ansage als **zuschaltbarer Modus der Monitor-/Kombi-Seiten**; `/ansage` als eigene Seite für Tablet/Handy. | Der TV hängt ohnehin in der Halle, oft am Lautsprecher. |

## Nicht-Ziele

- Kein Live-Audio-Stream des PC-Tons (WebRTC/Icecast).
- Keine Änderung oder Ablösung des Windows-Ansage-Slaves (LAN wie Cloud);
  der Cloud-Slave behält seinen Weg über `/{ns}/info/announce/state`.
- Kein Sprechen aus der Turnierleitungs-Seite (TL-Web) selbst.
- Der **Erstaufruf „In Vorbereitung rufen" aus TL-Web** bleibt ohne Ansage
  (wie heute; nur der Nachruf `AnnouncePrepCall` spricht).
- Keine Synchronität zwischen mehreren Geräten einer Halle — sie dürfen um
  den Abfrage-Takt versetzt klingen.
- Keine weitere TTS-Engine außer Azure, OneCore und Web Speech.
- Der bestehende Azure-Plattencache `tts-cache/` des PC-Abspielwegs bleibt
  unverändert (eigener Roadmap-Befund).
- Das bestehende Azure-Key-Leck unter `/{ns}/info/announce/state`
  (ADR 0003) wird **hier nicht** behoben — eigener Roadmap-Befund.

## Etappen

Jede Etappe ein eigener PR mit eigener Version.

| Etappe | Inhalt | Ergebnis für den Nutzer |
|---|---|---|
| **E1** | LAN-Seite `/ansage` + Azure-Audio für **TL-Aufträge, Freitext, Desktop-Aufrufknöpfe** | Tablet/Handy im LAN sagt alles außer der automatischen Erst-Ansage an |
| **E1b** | Ansage-Modus in den Monitor-/Kombi-Seiten, Pi-Autoplay | Pi am TV spricht mit |
| **E2** | Erst-Ansage im Kern (`court_assigned`) | Automatischer Feld-Aufruf auch auf Seiten |
| **E3** | Offline-Stimme Windows OneCore | Seiten sprechen ohne Azure/Internet mit PC-Stimme |
| **E4** | Cloud: Relay-Weg, Lese-Schlüssel, Präsenz | Ferne Halle / Geräte ohne LAN |

E3 beginnt erst nach der **Messung auf einem echten Turnier-PC** (siehe E3).

## Verhalten

### Gemeinsamer Ansage-Eintrag

Ein Eintrag ist eine **fertige** Ansage; die Seite erfindet nichts dazu.
Wire-Typen in `relay-proto` (bereits in E1 angelegt, damit E4 sie nutzt):

```text
AnsageEintrag {
  id: u64, hall: String, art: AnsageArt,
  courtId?: i64, matchId?: i64,      // für die Abspiel-Kontrolle
  createdAtMs: u64,
  gong?: "match" | "info",            // fehlt = kein Gong (announce.gong aus)
  lang: String, rate: f32,
  segmente: [String],                 // korrigierter Text, Fallback Web Speech
  audio?: { mime: "audio/mpeg" | "audio/wav", bytes: u32 }
}
AnsageArt = court_call | prep_call | officials | start_play | scorekeeper_call
          | freetext | desktop | court_assigned | (unbekannt → überspringen)
AnsageFeed {
  aktiv: bool,        // Host erzeugt gerade Ansagen für Seiten
  hostAudio: bool,    // Host kann vertonen (Azure | ab E3 OneCore)
  grund?: String,     // schalter_aus | slave | renderer_weg | turnier_pc_offline | unbekannt
  halls: [String], belegung: [{courtId, matchId}], eintraege: [AnsageEintrag]
}
```

Der Feed enthält **keine** Azure-Felder (Region, Schlüssel, Stimme).

### Wer vertont — genau einmal (E1)

- Eine neue Komponente `AnsageVertoner` in der Desktop-Oberfläche ist aktiv,
  wenn `page_audio` an ist und der Rechner **kein** Slave ist — unabhängig von
  `announce.enabled`.
- Sie holt im Sekundentakt neue Quellen (TL-Aufträge, Freitexte) vom Kern.
  Der Kern **beansprucht** jede Quelle beim Ausliefern atomar; eine Quelle
  wird nie zweimal ausgeliefert. Dieser Abruf zählt **nicht** als Präsenz.
- Die Komponente baut den Text mit **derselben** Logik wie der PC-Sprecher
  (`jobZuAnsage`, herausgelöst aus `AnnounceJobPlayer`) und übergibt ihn dem
  Kern zum Vertonen. Ein Auftrag wird **einmal** vertont, nicht je Halle; ein
  Freitext ohne Halle gilt für alle Hallen.
- Vertont wird nur, wenn in der Halle des Auftrags gerade eine Ansage-Seite
  zuhört (Freitext ohne Halle: irgendeine). Sonst wird die Quelle verworfen —
  keine Azure-Kosten, kein spätes Nachsprechen.
- Scheitert Azure, wird der Eintrag **ohne Audio** abgelegt (Seite fällt auf
  Segmente zurück) und eine Warnung geloggt.
- **Desktop-Aufrufknöpfe** (Felderübersicht „Aufrufen", Vorbereitungs-Aufruf
  und -Nachruf, SR-Ansage, Ansage-Seite der App) spielen wie heute am PC und
  legen bei `page_audio` zusätzlich einen Eintrag der Art `desktop` an.
- Der Vertoner meldet einen Herzschlag; ohne Herzschlag seit 10 s steht der
  Feed auf `aktiv: false, grund: "renderer_weg"`.

### Die Seite `/ansage` (E1)

- Erreichbar unter `http://<pc>:8088/ansage` und `https://<pc>:8443/ansage`.
  Link und QR je Halle stehen in den Ansage-Einstellungen der App.
- **Hallenwahl** aus der Liste `feed.halls`; ab zwei Hallen **Pflicht**
  (leere Halle hieße „alle" und damit fremde Aufrufe). Vorbelegung per
  `?halle=`; gemerkt im `localStorage` (jeder Zugriff in try/catch).
- **„Ton freischalten"**: einmal tippen (Browser-Pflicht). Läuft der
  Audio-Kontext bereits (Pi mit Autoplay-Flag, Android-Kiosk-App), entfällt
  der Knopf.
- Abfrage alle 1,5 s; Einträge in **strikter Warteschlange**, keine
  Überlappung. Die **erste** Abfrage ist Bestandsaufnahme — bereits
  vorhandene Einträge werden nicht gesprochen. Einträge älter als 60 s
  verfallen.
- **Nichts falsch sprechen:** Trägt ein Eintrag eine `matchId`, wird er nur
  gespielt, wenn das Feld laut letzter `belegung` **genau dieses Spiel**
  trägt. Der Server filtert solche Einträge zusätzlich beim Ausliefern.
- Abspielen: Gong **lokal** synthetisiert (wie heute) → Host-Audio → sonst
  Web Speech mit `segmente`, `lang`, `rate`.
- **Ohne Host-Audio und ohne Gerätestimme:** nur Gong, sichtbare Warnung
  „Dieses Gerät kann nicht sprechen", und das Gerät meldet sich **nicht** als
  Ansage-Gerät.
- **Präsenz:** Die Seite meldet sich nur als Ansage-Gerät ihrer Halle, wenn
  der Feed `aktiv` ist **und** sie sprechen kann **und** der Ton
  freigeschaltet ist. Die bestehende TL-Warnung „niemand hört zu" berücksichtigt
  Seiten automatisch.
- **Bildschirm:** Wake Lock nur im sicheren Kontext (HTTPS, also 8443 oder
  Cloud); sonst Hinweis „Bildschirm anlassen".
- Die Seite zeigt Status: verbunden / Halle / Stimme (Host-Audio, Gerät,
  keine) / letzte Ansage.

### Ansage-Modus der Monitor-Seiten (E1b)

- Die Monitor-, Übersichts- und Kombi-Seiten bekommen einen zuschaltbaren
  Modus `?ansage=<Halle>` mit demselben Ansage-Kern wie `/ansage`
  (gemeinsames Inline-Modul).
- Am TV gibt es keinen Freischalt-Knopf: Der Pi-Start setzt
  `--autoplay-policy=no-user-gesture-required`. Ohne freigegebenen Ton zeigt
  die Seite einen kleinen Hinweis und meldet keine Präsenz.
- Für bestehende Pis gilt das Flag erst nach Aktualisierung (Pi-Setup/Image).

Umsetzung (v0.9.313), Entscheidungen beim Bau:

- Der Kern (Abfrage, Warteschlange, Gong, Audio-Fristen, Gerätestimme,
  Präsenz) liegt in `src/io/ansageKern.mjs` und wird mit `welt`-Attrappen
  getestet; `ansage.html` und die drei Monitor-Seiten tragen dieselben
  Inline-Kopien (`scripts/test-ansage-kern.mjs`). Auf den Monitor-Seiten in
  einem eigenen `<script>`-Block, der ohne `?ansage` sofort endet.
- Die Halle aus der Adresse ist **fest**: Kennt das Turnier sie nicht, wird
  sie nicht verworfen (wie bei `/ansage`), sondern im Hinweis genannt.
- `?ansage=` wandert über **jede** Geräte-Umleitung mit (auch Werbung,
  Vorbereitung, Sieger, Zähltafel) und zählt beim Adressvergleich nicht —
  sonst verstummte der Pi beim ersten Zielwechsel oder lüde endlos neu.
- Cloud-Pfad (Basis ≠ `/`): kein einziger Abruf, nur der Hinweis. Ein 404
  auf den Feed (Slave-Brücke) bremst auf einen Versuch je Minute.
- Feed-Abruf mit 5-s-Frist und ohne Parallel-Abruf — gilt auch für
  `/ansage`; auf einer Monitor-Seite hätten hängende Abrufe sonst die
  Verbindungen der Anzeige selbst belegt.
- Eine Maus-/Tastatureingabe gibt den Ton notfalls auch ohne Flag frei.

### Erst-Ansage im Kern (E2)

- Der Kern vergleicht die Feldbelegung im Sync-Lauf **nach** der Besetzung
  von Schiedsrichtern und Zähltafelbedienung (`track_officials`,
  `track_scorekeepers`) und **nur am Master** (hinter dem Slave-Rücksprung).
- Der Stand liegt im `TabletState`, nicht in der `SyncEngine` — er überlebt
  das Speichern der Einstellungen.
- Regeln (aus `announceBaseline.mjs` übernommen und als Rust-Tests
  festgehalten):
  - Ein **leerer** erster Stand ist keine Bestandsaufnahme.
  - Der erste nicht-leere Stand ist Bestandsaufnahme; nichts wird angesagt.
  - Felder ohne Spiel sind ein gültiger Anfangsstand (Turniermorgen).
  - Neues `match_id` auf einem Feld → genau ein Auftrag `court_assigned`.
  - Dasselbe Spiel wird binnen 5 s nicht erneut angesagt.
  - **Turnierwechsel** setzt die Bestandsaufnahme zurück.
- Die Desktop-App spricht die Erst-Ansage künftig aus diesem Auftrag
  (`AnnounceJobPlayer`); `MatchAnnouncer` läuft nur noch am **LAN-Slave**.
- `court_assigned` wird über `/info/announce/jobs` nur mit `?auto=1`
  ausgeliefert — ein LAN-Slave sagt also nicht doppelt an. Ältere Slaves
  überspringen die unbekannte Art.
- Der Auftragsdeckel steigt von 50 auf 128 (Morgen-Burst der Feldvergabe).

### Offline-Stimme (E3)

- Vertonung über `Windows.Media.SpeechSynthesis` (WinRT/OneCore), Ausgabe WAV.
- Text = die korrigierten `segmente` (nicht die Azure-SSML).
- Stimmenwahl: `announce.voice_de`/`voice_en` (WebView-`voiceURI`) wird auf
  den WinRT-Stimmennamen abgebildet; Online-/Natural-Stimmen gibt es unter
  WinRT nicht → Sprach-Fallback.
- Reihenfolge: **Azure → OneCore → nur Segmente**.
- Linux-Build: OneCore fehlt; der Weg endet sauber bei Azure oder Segmenten.
- **Vor der Festlegung zu messen (echter Turnier-PC):** Stimmenliste WinRT
  vs. WebView2; Latenz kalt/warm (10 und 40 Wörter); WAV-Format und Größe;
  CPU parallel zum Sync; Betrieb mit App im Tray und gesperrtem Bildschirm;
  Rate nach Gehör; WinRT-SSML mit IPA; Windows 10 vs. 11; fehlendes deutsches
  Sprachpaket. Ergebnis fließt in den ADR der Etappe.

### Cloud (E4)

- **Ansage-Schlüssel** `announce.page_key`: zufällig erzeugt, nie gleich der
  `install_id`, nur lesend. Knopf **„Neuen Ansage-Link erzeugen"** macht den
  alten sofort ungültig. Wandert beim PC-Umzug im Identitäts-Bündel mit.
- Link: `https://badhub.de/bts-relay/ansage/<schlüssel>` (Monitor-Modus E1b
  analog).
- Der Turnier-PC überträgt Schlüssel und Einträge (inkl. Audio als Base64)
  über seine **bestehende, authentifizierte Host-Verbindung** — kein offener
  HTTP-Upload.
- Hochgeladen wird nur für Hallen, in denen gerade eine Cloud-Seite zuhört;
  das Relay meldet die Zuhörer-Hallen zurück. Damit gelten Vertoner-Gating und
  TL-Warnung auch für Cloud-Hallen.
- Relay hält Einträge im RAM, 60 s ab Empfang; Deckel 32 Einträge und 6 MiB
  je Namespace, 1 MiB je Audio (größere WAV gehen ohne Audio raus), 64
  Zuhörer.
- Hallenvergleich überall über eine gemeinsame Funktion
  (`trim` + Kleinschreibung, Unicode); die alte Slave-Route bleibt unberührt.
- **Kompatibilität:** Altes Relay + neue App: die App zeigt den Cloud-Link
  erst, wenn das Relay Ansage-Seiten kann, sonst „Relay unterstützt
  Ansage-Seiten noch nicht". Neues Relay + alte App oder rotierter Schlüssel
  oder PC offline: Seite zeigt „Link nicht aktiv — Turnier-PC offline, Link
  erneuert oder bts-light zu alt".

## Betroffene Komponenten / Architekturregeln

- **R1:** Neue Frontend-Funktionen ausschließlich über Tauri-Commands
  (`ansage_offene_quellen`, `ansage_vertonen`, `ansage_hoerer`,
  `ansage_link`; E3 `lokale_stimmen`).
- **R2:** Abspiel-Kontrolle gegen die BTP-Belegung; nichts erfindet
  Zuordnungen.
- **R4:** Einträge in der Cloud nur vom eingetragenen Host.
- **R6:** `install_id` bleibt Namespace; der Ansage-Schlüssel ist davon
  getrennt.
- Eine Textquelle: `announcer.ts` wird in „vorbereiten" und „abspielen"
  geteilt; `announceCourt.ts` liefert reine Builder; neues
  `src/io/announceJobs.ts` (`jobZuAnsage`) nutzen PC-Sprecher und Vertoner.
- Projekt-Muster Inline-Kopie: `src/io/gongKlang.mjs` und
  `src/io/ansageSeite.mjs` mit Inline-Kopie in `src-tauri/assets/ansage.html`
  (und E1b in den Monitor-Seiten) plus Drift-Tests.
- Neue Dateien (Arbeitsnamen): `src-tauri/src/tablet/ansage.rs`
  (`AnsageStore`), `src-tauri/assets/ansage.html`,
  `src/components/AnsageVertoner.tsx`, E3 `src-tauri/src/tts_lokal/`.
- `azure_tts::synthesize` bekommt einen Weg **ohne** Plattencache.
- E3: neue Features des vorhandenen `windows`-Crates
  (`Media_SpeechSynthesis`, `Storage_Streams`, `Foundation(_Collections)`);
  keine neue Abhängigkeit. E1–E4 sonst ohne neue Crates/npm-Pakete.
- Datenschutz: Ansagen enthalten dieselben Spielernamen, die Monitore zeigen;
  Audio nur im RAM, 60 s; keine neuen Personendaten.

## Akzeptanzkriterien

**E1**

1. Mit `page_audio` an, `enabled` aus und einer Seite in Halle A: Ein
   TL-Aufruf für ein Feld in Halle A erklingt auf der Seite mit Gong und
   Azure-Stimme; der PC bleibt stumm.
2. Derselbe Aufruf für Halle B erklingt auf der Seite in Halle A **nicht**.
3. Ein Freitext ohne Halle erklingt auf Seiten aller Hallen; er wird **einmal**
   vertont.
4. Ein Desktop-Knopf „Aufrufen" erklingt am PC (wenn `enabled`) **und** auf
   der Seite der Halle.
5. Steht beim Abspielen ein anderes Spiel auf dem Feld, schweigt die Seite
   für diesen Eintrag.
6. Eine Seite, die eine Minute offline war, spricht beim Wiederkommen nichts
   nach.
7. Ohne Zuhörer in einer Halle entsteht für deren Aufträge **kein**
   Azure-Aufruf.
8. Kein Feed (`/ansage/feed`) enthält — bei vollständig gesetzter Azure-Config
   — die Zeichenfolgen `azure`, `region` oder `key` als Feld.
9. Unter `/ansage*` existiert keine Route außer GET.
10. Audio ist nach 60 s unter `/ansage/audio/{id}` nicht mehr abrufbar (404)
    und wurde nie auf die Platte geschrieben.
11. Eine Seite ohne Gerätestimme, bei `hostAudio: false`: Gong + Warnung, und
    die TL-Seite meldet für die Halle „kein Ansage-Gerät".
12. Läuft in Halle A die App mit `enabled` **und** eine Seite, zeigt die App
    eine Doppelbetrieb-Warnung für Halle A.
13. Slave-Modus: kein Vertoner, Feed `aktiv: false, grund: "slave"`.
14. Ab zwei Hallen lässt sich die Seite nicht ohne Hallenwahl starten.
15. `page_audio` ist standardmäßig aus; eine alte `config.json` lädt.

**E1b**

16. Ein Pi mit `?ansage=Halle A` an der Kombi-Seite zeigt unverändert an und
    spricht die Ansagen von Halle A, ohne Tippen.
17. Ohne Autoplay-Freigabe: Hinweis auf der Seite, keine Präsenz.

**E2**

18. Kommt ein neues Spiel auf ein Feld, erklingt die Erst-Ansage (inkl.
    SR/Bedienung gemäß Schaltern) am PC (bei `enabled`) und auf den Seiten.
19. App-Start mit laufenden Spielen: keine Ansage (auch nicht, wenn der erste
    Snapshot leer war).
20. Einstellungen speichern während des Turniers: keine Ansage der laufenden
    Spiele.
21. Turnierwechsel ohne Neustart: keine Ansage aller Felder.
22. LAN-Slave neben neuem Master: genau eine Erst-Ansage am Slave.
23. Cloud-Slave verhält sich unverändert.

**E3**

24. Ohne Azure, unter Windows: Seiten erhalten WAV in der zur
    `voice_de`/`voice_en` passenden OneCore-Stimme.
25. Ohne passende Stimme: Fallback auf eine Stimme der Sprache; ohne jede
    Stimme: Eintrag ohne Audio, `hostAudio: false`.
26. Linux-Build kompiliert und fällt auf Segmente zurück.

**E4**

27. Über `…/bts-relay/ansage/<schlüssel>` funktioniert alles aus E1 (2–7,
    14) für eine Cloud-Seite.
28. `…/bts-relay/ansage/<install_id>` liefert 404.
29. Nach „Neuen Ansage-Link erzeugen" liefert der alte Link „Link nicht
    aktiv"; der neue funktioniert.
30. Der Cloud-Feed enthält nie Azure-Daten (wie 8), auch wenn das Relay
    Azure-Daten des Namespace kennt.
31. Nur die Host-Verbindung kann Einträge ablegen.
32. Eine Cloud-Seite zählt für die TL-Warnung als Ansage-Gerät ihrer Halle.
33. Altes Relay: App zeigt „Relay unterstützt Ansage-Seiten noch nicht";
    sonstiger Cloud-Betrieb unverändert.

## Tests

- **Rust `tablet/ansage.rs`/`state.rs`:** TTL, Deckel (Ältestes fliegt),
  Hallen-/`since`-Filter, Beanspruchung genau einmal (auch bei zwei Abrufen
  direkt hintereinander), Slave/aus → leer, kein Render ohne Zuhörer,
  veraltete `matchId` gefiltert, Präsenzregeln, `has_announce_listener` sieht
  Seiten, Hallenvergleich mit Umlauten und Groß/Klein.
- **Rust `server.rs`:** nur GET unter `/ansage*`; Feed ohne Azure-Felder
  (Kriterium 8); Audio-404 nach TTL.
- **Rust `azure_tts`/`config`:** Weg ohne Plattencache; `page_audio`-Default;
  alte Config lädt.
- **Rust E2:** Port aller Fälle aus `scripts/test-announce-baseline.mjs` +
  Turnierwechsel, Slave erzeugt nichts, Burst 26 Felder, `?auto=1`-Filter.
- **Rust E3:** `waehle_stimme`, `wav_info`, Rate-Abbildung, Linux-Stub;
  `#[ignore]`-Probe `tests/onecore_probe.rs`.
- **Rust E4:** Serde-Roundtrips der neuen Frames (unbekannte Art →
  überspringen), Schlüssel→Namespace, Rotation, `install_id` = 404, Feed ohne
  Azure-Daten, Deckel/TTL, nur Host legt ab, Zuhörer-Rückmeldung, Upload nur
  bei Zuhörer.
- **node:** `scripts/test-ansage-seite.mjs`, `scripts/test-gong-klang.mjs`
  (inkl. Drift gegen die Inline-Kopien), in `ci.yml` eingetragen.
- **Manuell je Etappe:** Tablet (Fire), Android-Handy, iPhone (Stummschalter!),
  Pi am TV (E1b), Cloud-Handy im Mobilfunknetz (E4).

## Risiken & Rollback

| Risiko | Gegenmaßnahme |
|---|---|
| WebView im Tray drosselt Timer → Vertoner rendert spät | Tauri-Event als Anstoß zusätzlich zum Takt; Latenz messen (Ziel ≤ 4 s) |
| Doppelte Azure-Kosten bei Doppelbetrieb | Warnung in der App (Kriterium 12) |
| iOS-Stummschalter dämpft Web Audio | Im Feldtest prüfen; Alternative `<audio>`-Element |
| Pi ohne Autoplay-Flag bleibt stumm | Hinweis auf der Seite, keine Präsenz; Pi-Setup-Doku |
| Android-System-WebView ohne `speechSynthesis` | Dort nur Host-Audio; ohne Azure (vor E3) Gong + Warnung |
| Relay-RAM durch Audio | Deckel je Namespace, Upload nur bei Zuhörern |
| nginx-CSP vor dem Relay (`default-src 'self'`) blockt Audio | Audio same-origin unter `/bts-relay/ansage/…`; vor E4-Rollout per `curl -I` prüfen |
| Schlüssel auf QR-Fotos/Verläufen | Nur lesend, rotierbar |

Rollback: E1/E1b/E3 durch Abschalten von `page_audio` (Standard aus) ohne
Wirkung auf den bisherigen Betrieb. E2 verändert die PC-Erst-Ansage — Rollback
nur per Versions-Rückgang; deshalb E2 mit eigenem Feldtest. E4: Relay-Routen
sind additiv; ohne Schlüssel keine Wirkung.

## Offene Fragen / Annahmen

- Annahme: Azure liefert MP3 (24 kHz, 48 kbit/s, ≈ 6 KB/s) — ein Aufruf
  ≈ 40–55 KB.
- Annahme: OneCore-WAV ≈ 32–48 KB/s; bestätigt erst die E3-Messung.
- Ob WinRT-SSML IPA-Phoneme unterstützt, klärt die E3-Messung; bis dahin
  ohne IPA (Qualität wie heute der Web-Speech-Weg).

## Betroffene Doku-Dateien

- `docs/announcements.md` — neuer Abschnitt „Ansage-Seite im Browser"
  (E1), Funktionsweise neu (E2), Engine-Kette (E3), Cloud + „Grenzen" (E4);
  doppelte Auftragstabelle bereinigen. Anker für Route, „Ton freischalten",
  „Audio für Ansage-Seiten erzeugen", „Neuen Ansage-Link erzeugen".
- `docs/oberflaechen.md` (Route `/ansage`), `docs/einstellungen.md`
  (Schalter, Standard aus), `docs/court-monitor.md` (E1b),
  `docs/pi-setup.md`/`docs/pi-master-image.md` (Autoplay-Flag),
  `docs/tablet-android-app.md`, `docs/cloud-relay.md` (E4),
  `docs/multi-hall.md`, `docs/master-slave.md` (E2).
- `scripts/check-handbuch-aktuell.mjs`: Regel für `ansage.html`.
- `CLAUDE.md`-Tabelle, `docs/changelog.md`, `docs/roadmap.md`,
  `docs/adr/README.md` (dabei fehlenden Eintrag 0056 nachtragen).

## ADRs

| Thema | Etappe |
|---|---|
| Text in der Host-Oberfläche, Kern vertont (RAM, 60 s), Seite spielt ab; `page_audio` getrennt von `enabled` | E1 |
| Erst-Ansage im `TabletState` als `court_assigned`; `MatchAnnouncer` nur am LAN-Slave | E2 |
| Offline-Stimme WinRT OneCore, WAV; Kette Azure → OneCore → Gerätestimme | E3 |
| Cloud-Zugang per Lese-Schlüssel, Route ohne Azure-Daten, Zuhörer-Rückmeldung (Bezug ADR 0003/0012) | E4 |

## Umsetzungs-Hinweise

Aus dem How-To (Code-Stand v0.9.295/308):

- **Desktop-Knöpfe** ohne Auftrag heute: `FieldOverviewPage.tsx` (Aufrufen),
  `PreparationPanel.tsx` (Vorbereitungs-Aufruf/-Nachruf), `AnnouncePage.tsx`,
  `OfficialsPanel.tsx`.
- **Beanspruchen** nach dem Muster `print_log().merken` (Zettel-Autodruck).
- **Text-Builder:** `announcer.ts` `playAnnouncement`/`playFreeText`/
  `playPreparationAnnouncement` → `prepare*` + `playPrepared`; den Switch aus
  `AnnounceJobPlayer.tsx` nach `src/io/announceJobs.ts`.
- **Route/Seite** nach dem Muster `render_anzeige_html`/`anzeige_page`
  (`__BASE__`), Asset per `include_str!` in `tablet/assets.rs` und `relay/`.
- **Präsenz:** zweite Karte `page_listeners` neben `announce_listeners`;
  `has_announce_listener` wertet beide (und ab E4 die Remote-Karte) aus.
- **Hallenvergleich** heute uneinheitlich (`to_lowercase` vs.
  `eq_ignore_ascii_case` vs. byte-genau) → `relay_proto::halle_passt`.
- **E2-Platz** im Sync: hinter dem Slave-Rücksprung, nach `track_officials`
  und `autodruck`; `hall_of_court` aus `tl.rs` in ein gemeinsames Modul.
- **E4-Relay:** Index Schlüssel→Namespace wie `tl_index` (gleiche
  Sperrreihenfolge), Aufräumen in `namespace_aufraeumen`, statische Routen vor
  `{ns}` wie `/pair/{code}`; Binärframes ignoriert das Relay → Base64 im
  Text-Frame.
- Reviews: `code-reviewer` je PR; `security-reviewer` Pflicht bei E1 (offene
  LAN-Route) und E4 (Schlüssel, Relay-Route).
