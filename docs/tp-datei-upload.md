# Turnierdatei an badhub senden

Noch am Turniertag kann badhub.de einen **vorläufigen Stand** des Turniers
zeigen: Auslosungen, Gruppentabellen, KO-Bäume und die Spielliste — ergänzt um
die Zwischenstände und Ergebnisse aus dem Liveticker. Die Quelle dafür ist die
**Turnierdatei von BTP** (die `.TP`-Datei, mit der der Tournament Planner
gerade arbeitet). bts-light liest sie, ohne sie zu verändern, und schickt sie
auf Knopfdruck an badhub.

Nach dem Turnier ersetzt der gewohnte Endimport bei badhub den vorläufigen
Stand durch den endgültigen — danach lässt sich die Datei für dieses Turnier
nicht mehr hochladen.

## Was du vorher wissen solltest

**Alles aus der Datei wird sichtbar.** badhub zeigt, was in der Datei steht —
auch Auslosungen, die du vielleicht noch nicht veröffentlichen wolltest. Der
Knopf ist die Freigabe: Sende die Datei erst, wenn die Auslosungen stehen
dürfen. Platzierungen und Medaillen gibt es vorläufig nicht; die Turnierseite
öffnet auf dem Reiter „Spiele“ und trägt einen Hinweis mit dem Stand der Datei
und des Livetickers.

**Ergebnisse aus dem Liveticker bleiben erhalten.** Hat der Liveticker ein
Spiel schon fertig, die Datei aber noch nicht, zeigt badhub das Ergebnis aus
dem Liveticker. Ein Upload löscht nie ein Liveticker-Ergebnis.

**Die Datei enthält persönliche Daten** der Spielerinnen und Spieler
(Anschriften, Telefonnummern, Geburtsdaten). badhub bewahrt sie nur so lange
auf, bis die Daten übernommen sind — spätestens 14 Tage —, außerhalb des
Webs und ohne sie zu veröffentlichen. Veröffentlicht werden nur Name, Verein,
Nationalität, Auslosung und Ergebnis.

## Einrichten: die Datei wählen

Einstellungen → Abschnitt **„TP-Datei (Turnierdatenbank)“**, direkt hinter der
BTP-Verbindung.
<!-- pruef: "TP-Datei (Turnierdatenbank)" in src/pages/SetupWizard.tsx -->

1. **TP-Datei wählen** öffnet den Dateidialog. Wähle die Datei, die in BTP
   geöffnet ist — meist unter *Dokumente* oder dort, wo du das Turnier
   angelegt hast; die Endung ist `.TP`.
   <!-- pruef: "TP-Datei wählen" in src/pages/SetupWizard.tsx -->
2. Neben dem Pfad steht das Urteil: **„Turnierdatei erkannt, 3,6 MB“** ist gut.
   „Das ist keine TP-Datei“ heißt: falsche Datei (etwa eine Sicherung oder ein
   Export). „Größer als 32 MB“: zu groß für den Upload. In beiden Fällen wird
   nichts gespeichert.
   <!-- pruef: "Turnierdatei erkannt, " in src-tauri/src/badhub/tp_upload.rs -->
3. Unten **Speichern**.

Die Datei ist an die **Turnier-Kennung** aus „1 · Liveticker-Ziel“ gebunden,
die beim Wählen eingetragen war. Änderst du die Kennung später — etwa fürs
nächste Turnier —, bleibt der Pfad stehen, aber Setup und Status sagen **„Datei
neu wählen“**, bis du die Datei erneut gewählt hast. So landet nie die Datei
des einen Turniers unter der Kennung eines anderen.
<!-- pruef: "Datei neu wählen" in src/pages/SetupWizard.tsx -->

**Die Kennung steht auch in der Datei.** BTP trägt die turnier.de-Kennung in
die Turnierdatei ein, und bts-light liest sie beim Wählen mit:

- Ist unter „1 · Liveticker-Ziel“ noch keine Kennung eingetragen, übernimmt
  bts-light sie aus der Datei und sagt **„Turnier-Kennung aus der Datei
  übernommen“** — du musst sie nicht von turnier.de abtippen.
  <!-- pruef: "Turnier-Kennung aus der Datei übernommen" in src/pages/SetupWizard.tsx -->
- Passt die Kennung, steht neben dem Urteil **„Turnier-Kennung passt“**.
  <!-- pruef: "Turnier-Kennung passt" in src/pages/SetupWizard.tsx -->
- Gehört die Datei zu einem **anderen** Turnier als die eingetragene Kennung,
  wird sie abgelehnt („… gehört zu Turnier …“) und nichts gespeichert. Prüfe
  dann die Kennung oben — oder du hast die Datei vom Vorjahr erwischt. Eine
  andere Kennung übernimmt bts-light bewusst nicht von selbst: Sie steuert
  auch den Liveticker, und der würde sonst still auf ein fremdes Turnier
  zeigen.
  <!-- pruef: "gehört zu Turnier" in src/pages/SetupWizard.tsx -->

Eine Datei ohne Kennung (Turnier nicht mit turnier.de verknüpft) lässt sich
nur wählen, wenn oben eine Kennung eingetragen ist; ob sie passt, prüft dann
badhub beim Senden.

Läuft bts-light **nicht auf dem BTP-Rechner**, steht im Abschnitt ein Hinweis:
Die Datei liegt dort, wo BTP läuft; der Upload von einem anderen Rechner
(Netzlaufwerk) ist möglich, aber nicht empfohlen — Netzlaufwerke liefern eine
gerade geschriebene Datei nicht immer vollständig.

Im **Ansage-Slave-Modus** gibt es den Abschnitt nicht: Nur der Master lädt
hoch ([Master und Slave](master-slave.md)).

## Senden: der Knopf im Status-Fenster

Status-Fenster → Abschnitt **„Turnierdaten an badhub“** → **TP-Datei an badhub
senden**.
<!-- pruef: "TP-Datei an badhub senden" in src/pages/Dashboard.tsx -->

Der Knopf ist aus, solange keine Datei gewählt ist oder die Kennung nicht mehr
passt; daneben steht dann der Sprung ins Setup. Der Knopf hängt **nicht** daran,
ob der Liveticker läuft — du kannst die Datei auch vor dem Start senden, zum
Beispiel am Vorabend nach der Auslosung.

Nach dem Klick zeigt die Zeile neben dem Knopf den Stand:

| Anzeige | Bedeutung |
|---|---|
| **läuft …** | bts-light wartet kurz, bis BTP nichts schreibt, liest die Datei zweimal und vergleicht, dann wird gesendet. Dauert einige Sekunden. |
| **angenommen, wird verarbeitet** | badhub hat die Datei; der Import läuft dort. bts-light fragt bis zu zwei Minuten nach. |
| **übernommen, 312 Spiele** | Fertig — die Turnierseite zeigt den neuen Stand (bis zu eine Minute Verzögerung durch den Seiten-Cache). |
| **unverändert seit 10:05** | Die Datei ist dieselbe wie beim letzten erfolgreichen Upload. Nichts zu tun. |
| **angenommen, Verarbeitung dauert noch — Stand beim nächsten Klick** | badhub braucht länger als zwei Minuten. Später noch einmal klicken; der Knopf ist wieder frei. |
| **BTP ist gerade beschäftigt, bitte gleich noch einmal** | BTP schrieb länger als 20 Sekunden in die Datei (z. B. viele Eingaben auf einmal). Kurz warten, erneut klicken. |
| **Datei ändert sich gerade, bitte gleich noch einmal** | Zwischen den beiden Lesevorgängen hat sich die Datei verändert. Erneut klicken. |
| **TP-Datei nicht gefunden: Köpi-Cup 2026.TP — im Setup neu wählen** | Datei verschoben, umbenannt oder Laufwerk getrennt. |
| **„Köpi-Cup 2026.TP“ gehört zu Turnier 99F55FAB…, eingestellt ist 0EA5FD86…** | Die Datei nennt eine andere Turnier-Kennung als das Setup — BTP hat inzwischen eine andere Datei geöffnet, oder die Kennung oben ist falsch. Nichts wurde gesendet. |
| **Turnier bereits endgültig importiert** | Der Endimport ist schon gelaufen; die Datei lässt sich für dieses Turnier nicht mehr senden. |
| **zu schnell hintereinander, in 37 s wieder möglich** | badhub nimmt je Turnier höchstens eine neue Datei pro Minute an. |
| **Verbandskürzel ‹…› passt nicht zum badhub-Zugang** | Die Liveticker-Adresse in „1 · Liveticker-Ziel“ gehört zu einem anderen Verband als dein badhub-Passwort. Adresse prüfen — nicht das Passwort. |
| **badhub kennt den Upload noch nicht** | Das badhub, gegen das du sendest, hat die Funktion noch nicht (oder das Testsystem sperrt sie). Kein Fehler in bts-light. |
| **badhub nicht erreichbar** | Kein Internet oder badhub antwortet nicht. |

<!-- pruef: "BTP ist gerade beschäftigt" in src-tauri/src/commands.rs -->
<!-- pruef: "badhub kennt den Upload noch nicht" in src-tauri/src/badhub/tp_upload.rs -->
<!-- pruef: "Turnier bereits endgültig importiert" in src-tauri/src/badhub/tp_upload.rs -->
<!-- pruef: "Datei ändert sich gerade, bitte gleich noch einmal" in src-tauri/src/commands.rs -->
<!-- pruef: "unverändert seit " in src-tauri/src/badhub/tp_upload.rs -->
<!-- pruef: "gehört zu Turnier" in src-tauri/src/commands.rs -->

Ein zweiter Klick, während „läuft …“ steht, tut nichts. Der Stand der Zeile
geht beim Neustart von bts-light verloren — badhub kennt ihn weiter.

## Was während des Sendens in BTP passiert

Damit die Kopie keinen halb geschriebenen Stand erwischt, hält bts-light seine
**eigenen** Schreibzugriffe auf BTP (Ergebnisse von den Tablets, Feldvergaben,
Schiedsrichter) kurz zurück: Es wartet, bis kein eigener Schreibzugriff mehr
läuft, lässt drei Sekunden Ruhe und liest dann zweimal mit zwei Sekunden
Abstand. Ein Ergebnis, das genau in diesen Sekunden ankommt, wird danach
geschrieben — nichts geht verloren, es dauert nur ein paar Sekunden länger.

Was bts-light **nicht** sieht, sind Eingaben der Turnierleitung direkt in BTP.
Deshalb der doppelte Lesevorgang: Ändert sich die Datei dazwischen, wird nicht
gesendet. Und badhub prüft die Datei noch einmal (lesbar? plausibel viele
Spiele?), bevor es sie übernimmt.

## Automatisch senden

Statt jedes Mal zu klicken, kann bts-light die Datei **von selbst** senden —
aber nur, solange der Liveticker läuft, und nur, wenn sich die Datei seit dem
letzten Senden geändert hat.

Einstellungen → Abschnitt „TP-Datei (Turnierdatenbank)“ → Häkchen
**Automatisch senden, solange der Liveticker läuft**, darunter der Takt in
Minuten (Voreinstellung 15, mindestens 5). Dann **Speichern**.
<!-- pruef: "Automatisch senden, solange der Liveticker läuft" in src/pages/SetupWizard.tsx -->
<!-- pruef: /TP_TAKT_STANDARD_MINUTEN: u16 = 15/ in src-tauri/src/config.rs -->
<!-- pruef: /TP_TAKT_MIN_MINUTEN: u16 = 5/ in src-tauri/src/config.rs -->

**Was dann passiert.** Mit dem Start des Livetickers beginnt die Automatik,
die erste Prüfung kommt nach etwa einer Minute. Bei jeder Prüfung liest
bts-light die Datei kurz ein und vergleicht sie mit dem zuletzt gesendeten
Stand. Ist sie gleich, passiert nichts — BTP merkt davon nichts, badhub wird
nicht angefragt. Hat sie sich geändert, läuft dasselbe wie beim Knopf
(kurzes Zurückhalten der eigenen Schreibzugriffe, zweimal lesen, senden).
Stoppst du den Liveticker, endet auch die Automatik; am Vorabend sendest du
weiter mit dem Knopf.

**Das heißt auch:** Mit eingeschalteter Automatik erscheint jede Änderung in
BTP auf badhub, sobald der Takt sie erwischt — auch eine Auslosung, die du
gerade erst gemacht hast. Schalte die Automatik erst ein, wenn das in Ordnung
ist, oder mach die Auslosung, bevor du den Liveticker startest.

**Die Zeile im Status-Fenster** unter dem Knopf zeigt den Stand, zum Beispiel
„Automatik: alle 15 min · nächste Prüfung 14:47 · zuletzt 14:32
(automatisch): übernommen, 312 Spiele“. Ist der Liveticker aus, steht dort
**„Automatik startet mit dem Liveticker“**.
<!-- pruef: "Automatik startet mit dem Liveticker" in src/pages/Dashboard.tsx -->

**Wenn etwas nicht klappt.** Vorübergehende Störungen (BTP gerade
beschäftigt, Datei in Bewegung, badhub nicht erreichbar) versucht die
Automatik nach anderthalb Minuten noch einmal, höchstens zweimal
hintereinander, dann wieder im normalen Takt; bei „badhub bittet um Geduld“
nach der Zeit, die badhub nennt. Die Zeile nennt den Grund und den nächsten
Versuch. Kennt dein badhub den Upload noch nicht, wartet die Automatik eine
halbe Stunde.
<!-- pruef: "badhub bittet um Geduld" in src-tauri/src/badhub/tp_upload.rs -->
<!-- pruef: "nächster Versuch" in src-tauri/src/badhub/tp_takt.rs -->

Bei Fehlern, die ein neuer Versuch nicht heilt, **hält die Automatik an** und
die Zeile wird gelb: **„Automatik angehalten: …“** mit dem Grund aus der
Tabelle oben — etwa „Turnier bereits endgültig importiert“, ein falsches
Verbandskürzel oder Passwort, eine verschobene Datei (nach drei Fehlversuchen),
eine Datei, die zu einem anderen Turnier gehört (BTP hat eine andere Datei
geöffnet), oder die Kennung hat sich geändert („Datei neu wählen“). Daneben
steht der Sprung ins Setup.
<!-- pruef: "Automatik angehalten" in src/pages/Dashboard.tsx -->

Die Automatik läuft wieder an, sobald du im Setup die Ursache behebst und
speicherst — also die Datei neu wählst, die Turnier-Kennung, die
Liveticker-Adresse, die badhub-Adresse oder das Passwort änderst — oder sobald
ein Klick auf **TP-Datei an badhub senden** gelingt. Das Häkchen aus- und
wieder einzuschalten löst den Halt **nicht**. Nach einem Neustart von bts-light
beginnt die Automatik frisch: Die Zeile sagt „noch nichts gesendet“, der erste
Lauf kommt eine Minute nach dem Start des Livetickers.

## Weiterlesen

- [Einstellungen — TP-Datei (Turnierdatenbank)](einstellungen.md#tp-datei-turnierdatenbank)
- [Master und Slave](master-slave.md) — nur der Master lädt hoch
- Technik: Spec [features/tp-datei-upload.md](features/tp-datei-upload.md),
  [ADR 0060](adr/0060-upload-sperrt-btp-writes-schreib-gate.md),
  [BTP-Protokoll — Schreib-Gate](btp_protocol.md#schreib-gate-fuer-den-tp-upload-adr-0060);
  Automatik: Spec [features/tp-takt-upload.md](features/tp-takt-upload.md),
  [ADR 0061](adr/0061-takt-upload-lebenszyklus-schnellhash-anhalteregel.md)
