# 0104 — Offline-Stimme: sherpa-onnx in der Programmdatei, Stimme als geprüfter Download, kein GPL-Code im Programm

- **Status:** accepted
- **Datum:** 2026-11-16

## Kontext

Fällt der Cloud-Sprachdienst aus, spricht heute die Windows-Stimme (ADR 0066) — verständlich,
aber ein hörbarer Bruch. Die Spec [ansage-offline-stimme](../features/ansage-offline-stimme.md)
verlangt eine neuronale deutsche Stimme, die **im Programm** rechnet: nichts zu installieren,
kein Python, kein Sprachserver.

Randbedingungen:

- **ADR 0100:** Die Programmdatei ist die ganze App. Eine DLL oder ein Datenordner neben der
  exe bräche den Kernwechsel durch Umbenennen.
- **Lizenz:** Das Repo ist öffentlich, hat keine LICENSE-Datei und enthält bewusst eingebettete
  Geheimnisse. GPL-Code, der in die exe gelinkt wird, stellte die ganze exe unter GPL-3.
- **Linux:** Der CI-Job `linux` muss ohne die Engine bauen und testen.

Recherche vom 16.11.2026 (an den Quellen geprüft, nichts gemessen):

- **sherpa-onnx 2.x** (Apache-2.0, Crates `sherpa-onnx`/`sherpa-onnx-sys`) linkt statisch und
  bringt ONNX Runtime (MIT) mit. Seit Version 2.0 sind espeak-ng und piper-phonemize entfernt;
  die Engine ist GPL-frei.
- Damit fehlt der Weg **Text → Phoneme** für Deutsch. Die Piper-Stimmen sind mit
  espeak-ng-Phonemen trainiert; espeak-ng samt Daten ist GPL-3.
- Das Crate `piper-rs` bindet espeak-ng ein, `piper-phonemize` ist GPL-3. Der fertige
  Piper-Windows-Build ist von 2023 und archiviert; der Nachfolger liefert nur Python-Pakete.
- Stimmen `thorsten` (low/medium/high) und `kerstin` sind **CC0**. `pavoque` ist
  nicht-kommerziell lizenziert, drei weitere sind ungeklärt.
- Ein freies deutsches Aussprache-Lexikon (voicegarden, 277.912 Einträge, CC-BY-SA 4.0 + MIT)
  existiert. Das eingebaute Lexikon-Frontend von sherpa-onnx **überspringt unbekannte Wörter**
  — für Eigennamen untauglich ohne eigenen Schätzer.
- Der Build von `sherpa-onnx-sys` lädt ein vorgebautes Archiv **ohne Prüfsumme**. Das
  Standard-Archiv ist gegen die statische C-Laufzeit gebaut, bts-light nutzt die dynamische.

## Entscheidung

1. **Engine:** sherpa-onnx 2.x, **statisch in die exe gelinkt**, als Abhängigkeit nur unter
   `[target.'cfg(windows)'.dependencies]`. Verwendet wird das Archiv
   `win-x64-static-MD` (dynamische C-Laufzeit).
2. **Lieferkette:** Die CI lädt das Engine-Archiv selbst, prüft es gegen eine im Repo
   festgehaltene SHA-256 und übergibt es per `SHERPA_ONNX_LIB_DIR`. Der Build lädt nichts
   ungeprüft nach.
3. **Hinter einem Trait:** Der Kern spricht die Engine nur über eine Schnittstelle „Segmente
   hinein, PCM heraus" an (Vorbild `winrt_stimme::pcm_fuer`). Tests laufen gegen eine
   Attrappe. Auf Nicht-Windows meldet die Schnittstelle „nicht unterstützt", wie die
   Windows-Stimme heute.
4. **Stimme:** `de_DE-thorsten-medium` (CC0, etwa 60 MB), **nicht in der exe**. Sie wird beim
   Einschalten einmalig ins App-Datenverzeichnis geladen.
5. **Download:** von einem Spiegel auf badhub.de (`download/bts-light/stimmen/`). Dateinamen,
   Größen und SHA-256 stehen als Konstanten **in der exe**. Vertrauensanker ist damit die
   über den signierten Updater gelieferte Programmdatei; eine eigene Signatur braucht es
   nicht. Eine neue Stimme kommt mit einer neuen App-Version.
6. **Kein GPL-Code in der exe — ohne Ausnahme.** `piper-rs`, `piper-phonemize` und espeak-ng
   werden nicht gelinkt. Die Archive `tts-models` von sherpa-onnx (enthalten espeak-ng-Daten)
   werden nicht gespiegelt.
7. **Text → Phoneme, bevorzugter Weg (V1):** freies Lexikon als Datendatei beim Modell, dazu
   ein eigener Schätzer für unbekannte Wörter und eine eigene Ausnahmeliste (Name → Phoneme),
   die an das vorhandene Aussprache-Wörterbuch anschließt. Die Phoneme gehen direkt in die
   Engine.
8. **Die Messung E0 entscheidet, ob V1 trägt.** Kriterium ist ein Hörvergleich derselben
   20 Ansagen (deutsche, polnische, vietnamesische, englische Namen) und der Befund, dass
   kein Wort verschluckt wird. Trägt V1 nicht:
   - **V2** (espeak-ng als kleiner Hilfsprozess im Stimmpaket, Austausch nur Text → Phoneme
     über Standard-Ein-/Ausgabe) kommt **nur nach ausdrücklicher Freigabe durch den Nutzer**
     in Frage — wegen der rechtlichen Restunsicherheit, der Verteilerpflichten beim Spiegeln
     und einer zweiten Programmdatei aus dem Netz.
   - Sonst entfallen die Abschnitte E2 und E3 der Spec. Ausfall-Erkennung und Vorab laden
     hängen nicht an der Engine.
9. **Nur Deutsch.** Englische Ansagen spricht die Stufe darunter (Windows-Stimme), nie die
   deutsche Offline-Stimme (Regel D9 aus ADR 0066).

## Alternativen

- **Piper als eigenes Programm (Sidecar) im Stimmpaket.** Verworfen: eigener cmake-Build des
  GPL-Nachfolgers, VC++-Laufzeit und DLLs nötig, ein ganzes GPL-Paket zu spiegeln, hoher
  Pflegeaufwand.
- **`ort` (ONNX Runtime direkt) mit eigenem Piper-Code.** Verworfen: löst das Phonem-Problem
  nicht und spart nur etwas Größe; `load-dynamic` bräuchte eine DLL (ADR 0100).
- **`piper-rs`.** Ausgeschlossen: espeak-ng wäre in der exe.
- **Supertonic 3** (Deutsch ohne Phonemizer, läuft in sherpa-onnx). Nicht gewählt: 122 MB,
  Lizenz mit Nutzungsauflagen und Vorbehalt des Lizenzgebers, Quell-Repo archiviert. Bleibt
  Vergleichskandidat im Hörtest.
- **Stimme in die exe einbetten.** Verworfen: +60 MB für jede Installation und jedes Update,
  auch für die, die das Feature nie einschalten.
- **Stimme direkt von HuggingFace laden.** Verworfen als Regelweg: Das Repo wird laufend
  umtrainiert; der Spiegel hält genau die geprüfte Fassung und hängt nicht an einem dritten
  Dienst. Quelle des Spiegels ist ein auf einen Commit festgelegter Stand.
- **Eigener Python-Sprachserver mit großem Modell** (Vorbild tlehr/bts). Verworfen:
  widerspricht „installieren und fertig".

## Konsequenzen

- Die exe wächst. Der Zuwachs ist ungemessen (Anhalt: die fertige Kommandozeilen-Sprachausgabe
  von sherpa-onnx hat 21 MB); E0 misst ihn. Jedes Update und jeder Kernwechsel trägt ihn mit.
- Die Windows-CI bekommt einen Schritt „Engine-Archiv laden, prüfen, cachen". Ein
  Versionssprung der Engine heißt: neue Prüfsumme eintragen.
- Der Release-Ablauf bekommt einen Handgriff: Stimmpaket einmalig auf den Spiegel legen
  (`docs/release.md`).
- Das Lexikon verlangt eine Quellenangabe (CC-BY-SA) in der Dokumentation.
- Unbekannte Namen können falsch betont klingen. Die Ausnahmeliste ist der Weg, das zu
  heilen; sie ist Pflegeaufwand.
- Rechenzeit, Speicher und Ladezeit auf einem schwachen Turnier-PC sind Annahmen
  (geschätzt 0,2–0,6 s für einen 4-s-Satz), bis E0 sie misst.

## Ergebnis der Messung

Offen — wird nach dem Windows-Lauf von `tests/offline_stimme_probe.rs` hier eingetragen
(exe-Zuwachs, Hörvergleich, Zeiten, gewählter Phonem-Weg).

### Vorlauf unter Linux (17.11.2026, Entwicklungsrechner, nicht maßgeblich)

Zweck: prüfen, dass der Weg trägt, bevor am Windows-PC gemessen wird. 24 Kerne, also deutlich
schneller als ein Turnier-PC.

- **Phoneme direkt in die Engine:** sherpa-onnx 2.0.1 nimmt Lautschrift über
  `GenerationConfig::phoneme_codepoints` an. Weg V1 ist damit technisch möglich, ohne espeak und
  ohne Umweg über eine Lexikon-Datei der Engine.
- **Zeiten** (20 Ansagen, 2–8 s Ton): Modell laden 0,37 s; erster Ton im Median 52 ms (1 Thread)
  bzw. 32 ms (2 Threads), höchstens 342 ms; Rechenzeit 6 % der Tonlänge (1 Thread).
- **Speicher:** Modell +78 MB; das Wiktionary-Lexikon als Tabelle +106 MB (zu viel für E2, ein
  kompaktes Format ist nötig). Dauerlauf 2.000 Ansagen: kein Fehler, der Speicher steigt in den
  ersten 250 Ansagen einmal um gut 100 MB und bleibt dann stehen.
- **Größe:** Das Testprogramm wächst unter Linux um 26,8 MB (Windows-Archiv: anderer Wert,
  misst der Windows-Lauf).
- **Lautschrift:** 0 Zeichen außerhalb des Zeichenvorrats der Stimme. Von den Namen der
  20 Ansagen werden knapp ein Drittel geschätzt (polnische und vietnamesische fast alle).
- **Nicht gemacht:** Vergleich mit espeak-Lautschrift (espeak darf nicht gelinkt werden; ein
  Vergleich bräuchte espeak als eigenes Programm auf dem Messrechner) und mit Supertonic 3.
- **Lexikon:** Die oben genannte Quelle „voicegarden" war nicht wiederzufinden; gemessen wurde mit
  dem deutschen Wiktionary-Lexikon aus `ipa-dict` (CC-BY-SA).
