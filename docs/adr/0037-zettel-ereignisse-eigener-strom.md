# 0037 — Zettel-Ereignisse: eigener Frame, eigener Store, eigene Datei

- **Status:** proposed
- **Datum:** 2026-08-19

Gehört zu [docs/features/schiedsrichterzettel-druck.md](../features/schiedsrichterzettel-druck.md).

## Kontext

Der Schiedsrichterzettel braucht mehr als der Punktverlauf hergibt: Karten, Verletzungen mit
Beginn und Ende, Unterbrechungen, Überstimmungen, Aufgabe, Disqualifikation — und die
Aufschlagfolge, die heute nirgends dauerhaft festgehalten wird.

Der naheliegende Weg wäre, den bestehenden Punktverlauf-Strom zu erweitern: ein Ereignis hängt
ohnehin an einer Position im Verlauf („gelbe Karte bei 11:8, nach Ballwechsel 19"), und der
Strom bringt Ordnung, Wiederherstellung (`RallySync`) und Cloud-Weg schon mit. Drei Kräfte
sprechen dagegen:

1. **`RallySync` ersetzt.** `TimelineStore::apply_sync` (`timeline.rs:208`) tauscht den
   Match-Eintrag vollständig aus, und `rally_sync` kommt bei Undo, Reconnect, Reload und
   Geräteübernahme — also ständig. Ereignisse, für die der **Host** die Wahrheit hält, dürfen
   davon nicht erfasst werden: Ein übernehmendes Ersatz-Tablet kennt die Karten des Vorgängers
   nicht und würde sie beim ersten Sync löschen.
2. **Ereignisse ohne Ballwechsel.** Eine Karte in der Satzpause oder vor dem ersten Aufschlag
   hat kein Trägerframe im Punktverlauf.
3. **Der Deckel ist ganzheitlich.** `MAX_TIMELINE_LEN` (8 KiB) wird im Relay hart geprüft: ein
   zu großer `RallySync` wird **komplett** verworfen, eine zu große `TimelineData` kommt beim
   Abrufer als 404 an. Ein Ereignis-Schwall würde damit nicht die Ereignisse kosten, sondern
   den **ganzen Punktverlauf**.

Dazu ein Datenschutz-Aspekt: `MatchTimeline` ist laut [ADR 0015](0015-punktverlauf-datei-je-turnier.md)
der Typ, der „genau in dieser Form später zu badhub wandert", und die Datei ist bewusst
personenbezugsfrei — die Begründung dort lautet „kein Personenbezug auf Platte, also keine
Lösch-Pflichten". Karten sind personenbezogene Sanktionsdaten; sie in denselben Typ zu legen,
würde diese Begründung kippen und dem Folge-Feature eine Falle vererben.

## Entscheidung

**Ein eigener Strom neben dem Punktverlauf, Join erst beim Rendern.**

- Neue Wire-Typen `MatchEvent`, `TabletMsg::MatchEvent`/`MatchEventSync`, die dazugehörigen
  `RelayFrame`-Varianten sowie `ScoresheetRequest`/`ScoresheetData` — **neben** dem
  Punktverlauf-Block, nicht darin.
- Neuer `SheetStore` (`tablet/sheet.rs`) mit eigener Datei `zettel/<slug>.json`, gebaut nach dem
  Muster von `TimelineStore` (`slugify` und Persistenz werden wiederverwendet, nicht kopiert).
- Eigenes DTO hinter einer eigenen, token-authentifizierten Route.
- **`MatchTimeline`, `TimelineSet`, `punktverlauf/<slug>.json`, die Graph-Route und
  `timelineSetSvg` bleiben unverändert.** Ein Golden-Test friert die Graph-Antwort ein.

Die Zettel-Projektion joint zur Laufzeit: `TimelineStore` (Ballwechsel) + `SheetStore`
(Ereignisse) + BTP-Snapshot (Namen) + `match_times` (Zeiten) + `officials` (Schiedsrichter).

**„Eine Quelle, zwei Projektionen" bleibt erfüllt:** ein Erfassungspunkt am Tablet, ein
Transportmuster ([ADR 0014](0014-punktverlauf-expliziter-rally-frame.md): expliziter Frame +
Komplett-Sync, verwerfen statt raten), zwei Lesesichten. Die Ordnung, die ein zweiter Strom
sonst rekonstruieren müsste, stellt der Anker `(set, after_n)` her — siehe
[ADR 0038](0038-ereignisse-append-only.md).

**Die Aufschlagfolge wird ein Ereignis, kein Feld.** `serve_start` trägt Aufschläger und
Empfänger. Damit muss `TimelineSet` nicht erweitert werden, die Byte-Gleichheit der Graph-Sicht
bleibt, und die Aussage der Punktverlauf-Spec „bewusst nicht vorgebaut" bleibt korrekt, statt
umgekehrt zu werden.

Dieser ADR **ergänzt ADR 0015 und bestätigt dessen Begründung**, statt sie zu revidieren: die
Punktverlauf-Datei bleibt personenbezugsfrei und badhub-tauglich, weil die Sanktionsdaten
woanders liegen.

## Alternativen

- **Feld `events` in `MatchTimeline`, beim Ausliefern abgestreift.** Verworfen: Die Trennung
  wäre **Serialisierungs-Disziplin statt Typ-Grenze** — eine künftige Stelle, die den Typ
  serialisiert, leakt Sanktionsdaten. Zusätzlich erbt der badhub-Push-Pfad die Falle.
- **Ereignisse im selben Frame-Strom** (`Rally` mit optionalen Feldern, `RallySync` trägt sie
  mit). Verworfen aus den drei Gründen oben; man müsste `apply_sync` zu einem Halb-Ersetzen
  umbauen und riskierte dabei den Punktverlauf.
- **Ersteaufschläger als Feld in `TimelineSet`.** Verworfen: bräche die Byte-Gleichheit der
  Graph-Sicht und damit die Verträglichkeit mit älteren Seiten, für einen Wert, der als Ereignis
  genauso gut aufgehoben ist.
- **Ereignisse gar nicht persistieren** (Zettel nur direkt nach Spielende). Verworfen: kein
  Nachdruck, und ein Absturz kostet das Protokoll.

## Folgen

- Zwei Stores mit zwei `finalize`-Pfaden. Die Aufrufstellen sind gezählt und in der Spec benannt;
  `confirm_walkover` finalisiert heute nicht einmal den Punktverlauf — das wird mitgezogen.
- Ein Match kann Ereignisse **ohne** Punktverlauf haben (Karte vor dem ersten Ballwechsel). Der
  Zettel zeigt sie dann ohne Raster; `has_timeline` bleibt davon unberührt.
- Sanktionsdaten hängen an einem eigenen Typ, einer eigenen Datei und einer eigenen Route —
  der Wächter-Test hat damit etwas **Strukturelles** zu prüfen statt einer Textregel.
- Eigene Deckel (`MAX_EVENTS_PER_MATCH`, `MAX_SHEET_LEN`) ohne Verhandlung über
  `MAX_TIMELINE_LEN`. Ein Ereignis-Schwall kann den Punktverlauf **strukturell** nicht mehr
  verdrängen.
- Negativ: zwei Wege zum Turnierwechsel, zwei Persistenzen, zwei Ingest-Filter — bewusst
  in Kauf genommen für die Typ-Grenze.
