# Tablet-Stand robust — Spezifikation

> Status: **abgestimmt 2026-11-11** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Kommunikations-Audit vom 10.11.2026 (Paket 1, siehe [roadmap.md](../roadmap.md)).
> Betroffene Crates: src-tauri (inkl. `assets/tablet.html`), relay, relay-proto.
> ADR: [0083](../adr/0083-stand-autoritaet-nach-fortschritt.md) ·
> [0084](../adr/0084-undo-stand-format-v2.md) ·
> [0085](../adr/0085-spielende-ueberholt-parken-nachzuegler.md).

## Kontext / Problem

Das Zähl-Tablet hält die Wahrheit über das laufende Spiel (Punkte, Aufschlag, Positionen,
Undo, Punktverlauf, Zettel-Ereignisse) und spiegelt sie je Punkt an den Turnier-PC (LAN) bzw.
das Relay (Cloud). Eine Code-Durchsicht fand vier Stellen, an denen dieser Stand verloren
geht oder etwas blockiert. Alle vier sind am Code nachvollzogen, keiner ist als Feldvorfall
belegt außer dem ersten (Relay-Kommentar „14.06.: Ersatz-Tablet sprang auf 0:0").

1. **Ersatz-Tablet überschreibt den gesicherten Stand mit 0:0.** Fällt ein Tablet aus, gibt
   der Server den Feld-Slot nach 10 s (LAN) bzw. 15 s (Relay) frei. Meldet sich danach ein
   Gerät ohne eigenen Stand an, erhält es `authoritative=true` (`reconnect_decision`: Feld
   frei → `KeepLocal`), setzt seinen leeren Stand durch, lädt mit `match_assigned` das Spiel
   bei 0:0 und überschreibt den Cache. Gleicher Effekt: selbes Gerät ohne nutzbaren
   `localStorage`. Umgekehrter Fall: Gerät A fällt aus, B zählt weiter, B fällt aus, A kommt
   mit altem Stand zurück und überschreibt B. Außerdem lebt der Cache nur im RAM — nach
   Host- oder Relay-Neustart gibt es nichts zu übernehmen. Im Betrieb LAN+Cloud erreicht der
   Zustand eines Cloud-Tablets den Turnier-PC nie; ein Ersatzgerät über den anderen Weg
   startet bei 0:0, und der Host spiegelt dieses 0:0 zurück ans Relay.
2. **Der gespiegelte Zustand wird zu groß und in der Cloud still verworfen.** 96 % des
   Zustands sind die 50 Undo-Stände; jeder trägt eine Vollkopie von Punktverlauf und
   Zettel-Ereignissen. Die Ereignis-Kopie liest niemand. Gemessen: 61–66 KB am Ende eines
   normalen Dreisätzers, im Schiri-Modus 81 KB ab Mitte Satz 2 und bis 590 KB bei 63
   Ereignissen. Über 64 KiB verwirft das Relay kommentarlos (auch den Spiegel Host→Relay);
   Cloud-Monitore und Übernahmen sehen dann einen alten Stand. Im LAN gibt es keinen Deckel.
   Unterhalb der Grenze reicht das Relay den vollen Zustand samt Sanktions-Ereignissen an
   jeden Cloud-Monitor.
3. **Ein nicht mehr zustellbares Ergebnis blockiert das Folgespiel.** Liegt inzwischen ein
   anderes Spiel auf dem Feld, wiederholt das Tablet den alten Auftrag endlos; „Ergebnis
   übermitteln" ist für das neue Spiel gesperrt. Losgelassen wird nur bei `finalized`
   desselben Spiels — dieses Signal fällt aus, wenn Hand-Ergebnis und nächste Zuweisung im
   selben BTP-Poll liegen.
4. **Punktverlauf und Zettel-Ereignisse reißen nach einem Abriss ab.** Fehlt der Host am
   Relay (Reconnect, App-Neustart), verwirft das Relay Ballwechsel und Ereignisse. Danach
   lehnt der Host jeden weiteren Ballwechsel des laufenden Satzes ab (Lückenprüfung), bis
   zufällig ein Voll-Sync kommt. Eine Karte aus der Lücke erreicht den Host nie. Im LAN
   passiert dasselbe nach einem Host-Neustart: Der Voll-Sync beim Verbinden wird verworfen,
   solange noch kein BTP-Snapshot da ist.

Betroffen sind Zähltafelbediener und Schiedsrichter (Stand weg, Knopf gesperrt), die
Turnierleitung (falscher Stand auf Monitor und Liveticker, unvollständiger Zettel) und die
Zuschauer am Liveticker.

## Zielbild & Erfolgskriterien

- Ein Gerätewechsel am Feld kostet keinen Spielstand: Ein Gerät ohne eigenen Stand übernimmt
  den gesicherten Stand samt 50 Undo-Schritten; gibt es keinen, steigt es mit dem Satzstand
  ein, den der Turnier-PC kennt. Nie 0:0 bei laufendem Spiel.
- Haben Gerät und Server beide einen Stand zum selben Spiel, gilt der weiter
  fortgeschrittene.
- Der gespiegelte Zustand bleibt ≤ 28 KB im Normalspiel und ≤ 40 KB im schlimmsten Fall
  (Doppel, 3 × 30:29, 63 Ereignisse) und wird nie still verworfen.
- Ein altes Ergebnis blockiert nie das Folgespiel und geht nie still verloren.
- Punktverlauf und Zettel-Ereignisse sind nach einem Verbindungsabriss ohne Zutun wieder
  vollständig, auch wenn das Ergebnis schon übermittelt ist.
- Für den Bediener ändert sich im Normalfall nichts; neu sichtbar ist nur das Hinweisband
  für ein geparktes Ergebnis.

## Nicht-Ziele

- Pakete 2 und 3 des Audits (badhub-Push, Relay-Härtung: WebSocket-Größengrenzen,
  mehrfaches `identify`, inline `process_result` in der Host-Schleife).
- Verein/Nation im `MatchBrief` am Tablet (Datensparsamkeit, eigener Punkt).
- Den vollen Tablet-Zustand auf Platte sichern. Nach einem Host-Neustart überleben nur die
  Sätze (`live-scores.json`), nicht Undo und Verlauf eines ausgefallenen Geräts.
- Meldung geparkter Ergebnisse in der Turnierleitungs-Oberfläche.
- Ein LAN- und ein Cloud-Tablet, die gleichzeitig aktiv dasselbe Feld zählen: Das bleibt
  ungelöst; es gibt nur eine Log-Warnung.
- Zettel-Ereignisse ganz aus dem Relay-RAM heraushalten (sie liegen weiter im
  Tablet-Zustand, nur nicht mehr in der Monitor-Fassung).
- Autodruck: Er druckt ausschließlich den Vorabzettel und ist von dieser Spec nicht berührt.
- Alte Tablet-Seiten nachträglich heilen. Sie verhalten sich wie heute, bis der
  Versionsabgleich sie nachlädt.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten (alles Erweiterung):**
  `src-tauri/assets/tablet.html` (Restore-/Zuweisungs-Zweig, `snapshot`/`restoreSnapshot`,
  `persistState`/`applyPersistedState`, `loadMatch`/`clearMatch`, `submit`/
  `trySubmitPending`, neues Hinweisband) · neue Module `src/io/standWahl.mjs`,
  `src/io/undoStand.mjs`, `src/io/geparkt.mjs` (jeweils mit Inline-Kopie), Erweiterung
  `src/io/resultRetry.mjs`, `src/io/abschlussAnsicht.mjs` ·
  `src-tauri/src/tablet/state.rs` (`set_court_state`, Zustands-Quelle, `score_mirror_of`,
  Nachzügler-Merker) · `tablet/server.rs` (`push_match`, `process_result`, Sync-Filter) ·
  `tablet/relay_client.rs` (`push_court`, neue Arme) · `tablet/timeline.rs` ·
  `relay/src/main.rs` (`store_court_state`, Monitor-Fassung, Durchleitungen, Vor-Match-
  Merker) · `relay-proto` (`hostSets`, `RelayFrame::StateSync`, `superseded`,
  `MAX_STATE_LEN`, `schlanker_anzeige_stand`).
- **Architekturregeln:**
  R1 nicht berührt (kein React-Code). R2 gewahrt: Der Einstieg mit Host-Sätzen nutzt den
  Stand, den der Host ohnehin hält; Court→Match kommt weiter nur aus BTP. R3: LAN, Cloud
  und LAN+Cloud sind jeweils ausdrücklich behandelt. R4 unverändert: ein aktives Tablet je
  Feld; ein zweites sieht „belegt". R5 gewahrt: „überholt" ist nie `ok`, entsteht nur in den
  bestehenden Absage-Zweigen von `process_result`; es gibt keinen neuen Annahme-Pfad ohne
  Feldvergleich. R6 nicht berührt.
- **Konfiguration & Abwärtskompatibilität:** Keine neuen Felder in `config.rs`. Alle
  Wire-Änderungen sind additiv (`default` + `skip_serializing_if`); unbekannte Frames und
  Felder werden auf allen Seiten ignoriert. Der vorhandene Laufzeit-Schalter
  `reconnect_legacy_rev` bleibt der Rollback für die Autoritäts-Regel: Im Legacy-Zweig
  bleibt das Tablet-Verhalten wortgleich, und der Host füllt `hostSets` nicht. Mischversionen
  siehe „Risiken & Rollback".
- **Datenschutz:** Kein neues personenbezogenes Feld. Verbesserung: Cloud- und LAN-Monitore
  erhalten keine Zettel-Ereignisse (Sanktionsdaten) mehr. Log-Zeilen zu verworfenen oder
  gekürzten Zuständen nennen nur Feld, Match-ID und Länge — nie Inhalt, Namen oder Karten.
  Der geparkte Auftrag liegt nur im `localStorage` des Tablets und reist nicht im Spiegel.
- **Abhängigkeiten:** Keine neue Cargo-/npm-Dependency. Relay-Deploy läuft automatisch bei
  jedem dieser PRs (geänderte `tablet.html`/`relay-proto`) und leert den Zustands-Cache des
  Relays.

## Fachliche Regeln

### A — Autorität (ADR 0083)

**Fortschritt** eines Stands = (Anzahl abgeschlossener Sätze, Punktsumme aller Sätze
einschließlich des laufenden), lexikografisch verglichen. `finished`, `retired` und
`walkover` zählen nicht mit.

**Herkunft:** Der gespiegelte Zustand trägt `schreiber` (Geräte-Kennung, ersatzweise eine
Zufallskennung je Seitenaufruf). Das Tablet führt zusätzlich einen persistierten Ring der
letzten 8 selbst gemeldeten Satzlisten.

Die Entscheidung fällt am Tablet, in zwei Stufen. Sie gilt nur im Ownership-Modus.

**Stufe 1 — bei `state_restore`:**
1. `authoritative=false` (Übernahme, finalisiert) → Server-Stand übernehmen (wie heute).
2. Kein lokaler Stand → Server-Stand übernehmen.
3. Lokaler Stand gehört zu einem anderen Spiel als der Server-Stand → Server-Stand
   vormerken; Stufe 2 entscheidet.
4. Selbes Spiel, Server-Stand stammt vom eigenen Gerät → `rev` ordnet; bei lokal ≥ gilt
   lokal.
5. Selbes Spiel, fremder oder fehlender `schreiber` → der höhere Fortschritt gilt; bei
   Gleichstand lokal.

**Stufe 2 — bei `match_assigned` (Spiel nicht finalisiert):**
1. Vorgemerkter Server-Stand passt zum zugewiesenen Spiel → übernehmen.
2. Kein Stand zum Spiel und Host-Sätze leer → Spiel bei 0:0 laden (wie heute).
3. Host-Sätze sind weiter als der Tablet-Stand **und** stehen nicht im eigenen Melde-Ring →
   Einstieg mit Host-Sätzen: Satzstand übernehmen, Aufstellung neu abfragen, Undo und
   Verlauf beginnen dort neu.
4. Sonst lokalen Stand behalten und an den Server melden.

Beim Übernehmen eines fremden Stands werden auch dessen Sende-Felder (`pendingResult`,
`submittedOk`, `resultRejected`) übernommen, und ein offener Auftrag wird sofort weiter
zugestellt.

**Host-Sätze:** `match_assigned` trägt auf Frame-Ebene `hostSets` (abgeschlossene Sätze +
laufender Satz) aus dem Live-Stand des Hosts (nach Host-Neustart aus `live-scores.json`).
Das Feld reist nur bei Anmeldung und Zuweisung, nicht je Punkt; das Relay reicht es durch
und speichert es nicht.

**LAN+Cloud über Kreuz:** Das Relay reicht jeden angenommenen Zustand eines Cloud-Tablets
als `RelayFrame::StateSync` an den Host weiter; der Host legt ihn mit Quelle „Cloud" ab.
Drei Riegel gegen Rück-Überschreiben:
1. Der Host spiegelt einen Zustand der Quelle „Cloud" nicht zurück — außer einmal als
   erster Spiegel je neuer Relay-Verbindung (nach Relay-Neustart).
2. Das Relay überschreibt seinen Zustand nicht mit dem Host-Spiegel, solange ein
   Cloud-Tablet das Feld hält.
3. Der Host verwirft einen Cloud-Zustand, solange ein LAN-Tablet das Feld hält.

### B — Zustandsformat (ADR 0084)

- Undo-Stände tragen keine Ereignis-Kopie mehr (nur `eventSeq`).
- Statt der Verlaufs-Vollkopie trägt ein Undo-Stand die Längen des Verlaufs je Satz;
  Wiederherstellen kürzt den aktuellen Verlauf auf diese Längen. Der Korrektur-Schritt
  „Mannschaften tauschen" (ADR 0067) behält die Vollkopie.
- Der Zustand trägt `fmt: 2`. Die neue Seite liest beide Formate und wandelt alte Stände
  beim Laden um (Vollkopie → Längen nur, wenn sie nachweislich ein Präfix ist).
- Eine alte Seite, die einen v2-Stand liest, zählt korrekt; nach einem Undo beginnt der
  Verlauf des laufenden Satzes „ab Zwischenstand". Das wird hingenommen.
- Das Tablet hält den Deckel selbst ein: Überschreitet der Zustand 56 KB, werden **nur für
  den Spiegel** die ältesten Undo-Stände gekürzt, bis er passt; `localStorage` behält alles.
  Eine Warnung je Spiel im Tablet-Log.
- Host (LAN) und Relay verwerfen Zustände über `MAX_STATE_LEN` (64 KiB, eine Konstante in
  `relay-proto`) und schreiben dazu eine Warnung — höchstens einmal je Feld und Spiel.
- Monitore (LAN und Cloud) erhalten nur die schlanke Anzeige-Fassung ohne `history`,
  `rallyLog` und `events`.
- Scheitert das Schreiben in den `localStorage`, gibt es eine Warnung je Spiel; gespiegelt
  wird trotzdem.

### C — Altes Ergebnis (ADR 0085)

- Der Host antwortet auf ein Ergebnis für ein Spiel, das nicht (mehr) auf dem Feld liegt,
  mit `superseded=true` („überholt"), wenn das Spiel in BTP beendet ist und einen Sieger
  hat. Die Antwort ist nie `ok` und nie `permanent`. In allen anderen Fällen bleibt die
  bisherige wiederholbare Absage.
- Das Tablet lässt bei „überholt" den Auftrag los und zeigt „In BTP steht für dieses Spiel
  schon ein Ergebnis" (Wortlaut bei der Umsetzung neutral gefasst: Auch eine abweichende
  Korrektur für ein geräumtes Spiel bekommt diese Antwort).
- Wechselt das Spiel auf dem Feld oder wird das Feld geleert, während ein Auftrag offen
  ist, wird er **geparkt**: eigene Liste, eigener Speicher-Schlüssel je Feld (überlebt das
  Leeren des Spielstands, reist nicht im Spiegel). „Ergebnis übermitteln" prüft nur noch den
  Auftrag des aktuellen Spiels.
- Geparkte Aufträge werden im Hintergrund weiter geprüft (nach 15 s, dann alle 60 s, und
  bei jeder Neuverbindung). `ok` und „überholt" lösen den Eintrag auf.
- Solange ein Auftrag geparkt ist, zeigt das Tablet auf Warte-, Setup- und Zählansicht ein
  Band: „Ergebnis von Spiel Nr. X (Sätze) nicht zugestellt — Turnierleitung informieren".
  Verwerfen ist jedem Bediener möglich, nach einer zweiten Abfrage, die das Ergebnis zeigt.
- Kommt das geparkte Spiel auf dieses Feld zurück, sendet das Tablet nicht still, sondern
  fragt: „Für dieses Spiel liegt ein nicht zugestelltes Ergebnis vor (Sätze)" — **Jetzt
  senden** oder **Verwerfen**.
- Ein geparkter Auftrag zählt beim Versionsabgleich wie ein offener Auftrag (kein
  automatisches Neuladen, wenn der Speicher nicht funktioniert).

### D — Verlauf und Ereignisse heilen (ADR 0085)

- Das Tablet schickt den vollen Verlauf und die vollen Ereignisse (a) bei `match_assigned`
  für das bereits geladene Spiel, (b) unmittelbar vor dem Senden des Ergebnisses, (c) einmal
  nach der Annahme des Ergebnisses.
- Der Host nimmt Voll-Syncs für das gerade beendete Spiel eines Felds noch **120 s** nach
  dem Ergebnis an — auch wenn das Feld schon neu belegt ist. Einzelne Ballwechsel und
  Einzel-Ereignisse bleiben nach dem Ergebnis verworfen. `finished` und `retired` des
  abgeschlossenen Verlaufs bleiben dabei erhalten.
- Das Relay reicht Voll-Syncs für das Vor-Spiel eines Felds ebenfalls 120 s lang durch.
- Der badhub-Verlauf-Push schiebt den nachgelieferten Verlauf über seinen Diff von selbst
  nach.

## Akzeptanzkriterien

- [ ] **AK1** Host-Neustart mitten im Spiel, selbes Tablet: Stand, Undo und Verlauf bleiben
  am Tablet; nach dem nächsten `match_assigned` hat der Host den vollen Verlauf des
  laufenden Satzes.
- [ ] **AK2** Host-Neustart, Ersatzgerät ohne Stand: Einstieg mit den Sätzen aus
  `live-scores.json`, Aufstellung wird neu abgefragt; nie 0:0.
- [ ] **AK3** Relay-Neustart mitten im Spiel: Das zählende Cloud-Tablet behält seinen
  Stand; ein Ersatzgerät erhält die Host-Sätze oder den zurückgespiegelten vollen Stand.
- [ ] **AK4** Frisches Gerät an freiem Feld, Server-Stand vorhanden: Es übernimmt ihn samt
  Undo-Schritten; der Server-Stand wird nicht überschrieben.
- [ ] **AK5** A fällt aus, B zählt weiter, B fällt aus, A kommt zurück: B's Stand gilt.
- [ ] **AK6** (negativ) Selbes Gerät kommt nach einem Offline-Undo oder „Wieder öffnen"
  zurück: Der lokale Stand gilt; der zurückgenommene Punkt kommt nicht wieder.
- [ ] **AK7** Zwei Geräte am selben Feld: Das zweite sieht „belegt"; eine Übernahme
  übernimmt den Stand, und ein offener Sendeauftrag wird vom Übernehmer zugestellt.
- [ ] **AK8** Tablet ohne `localStorage`: Ein Reconnect in derselben Sitzung hält den Stand;
  nach einem Reload greift AK2 bzw. AK4.
- [ ] **AK9** Alte Tablet-Seite gegen neuen Host/neues Relay: zählt, sendet und übernimmt
  wie bisher. Neue Seite gegen alten Host/altes Relay: fehlerfrei, ohne Host-Sätze.
- [ ] **AK10** Mit `reconnect_legacy_rev` verhält sich das Tablet wie vor dieser Spec, und
  `match_assigned` trägt keine `hostSets`.
- [ ] **AK11** Zustandsgröße: ≤ 28 KB bei Doppel 3 × 21:19 und 3 × 30:29 ohne
  Zusatz-Ereignisse; ≤ 40 KB bei Doppel 3 × 30:29 mit 63 Ereignissen. In keinem dieser
  Szenarien wird ein Zustand verworfen oder gekürzt.
- [ ] **AK12** Künstlich über 56 KB: Der gespiegelte Zustand ist gekürzt und ≤ 64 KiB, der
  lokale Stand vollständig, im Tablet-Log steht genau eine Warnung je Spiel.
- [ ] **AK13** Host und Relay verwerfen einen Zustand über 64 KiB, behalten den vorigen und
  schreiben höchstens eine Warnung je Feld und Spiel, ohne Inhalt.
- [ ] **AK14** Monitor-Zustände (LAN und Cloud) enthalten weder `history` noch `rallyLog`
  noch `events`.
- [ ] **AK15** Undo über eine Satzgrenze und über einen Korrektur-Schritt „Mannschaften
  tauschen" stellt Stand und Verlauf exakt wie im alten Format wieder her.
- [ ] **AK16** Ein Stand im alten Format (alter `localStorage` nach Update mitten im Spiel)
  wird geladen, umgewandelt und ist danach voll undo-fähig.
- [ ] **AK17** Doppelte Ergebnismeldung: zweimal `ok`, ein Schreibvorgang nach BTP
  (unverändert).
- [ ] **AK18** Hand-Ergebnis in BTP und sofort neues Spiel auf dem Feld: Der alte Auftrag
  erhält „überholt" und verschwindet; das Ergebnis des neuen Spiels lässt sich senden.
- [ ] **AK19** (negativ) Spiel ohne Ergebnis vom Feld genommen: Der Auftrag bleibt geparkt
  und sichtbar, überlebt Reload und `match_cleared`; das Folgespiel ist nicht blockiert.
  Kommt das Spiel zurück, erscheint die Abfrage.
- [ ] **AK20** (negativ) BTP-Neustart oder leerer Snapshot: wiederholbare Absage, weder
  „überholt" noch `permanent`; nichts wird losgelassen.
- [ ] **AK21** (negativ) Ein nach „Wieder öffnen" korrigiertes Ergebnis für das Spiel auf
  dem Feld wird wie bisher angenommen; „überholt" greift nicht.
- [ ] **AK22** Abriss Host↔Relay mit einer Karte in der Lücke: Nach dem Wiederverbinden
  stehen Verlauf und Karte beim Host.
- [ ] **AK23** Voll-Sync nach dem Ergebnis: binnen 120 s angenommen (`finished`/`retired`
  bleiben), auch bei neu belegtem Feld; danach verworfen. Ein Sync für ein fremdes Spiel
  und ein einzelner Ballwechsel nach dem Ergebnis bleiben verworfen.
- [ ] **AK24** LAN+Cloud: Ein per Cloud gezähltes Feld wird von einem LAN-Tablet mit dem
  vollen Stand übernommen. Der Host überschreibt nie den Zustand eines aktiven
  Cloud-Tablets am Relay, und umgekehrt.

## Tests

TDD je PR; `cargo test` (Workspace), `cargo clippy --workspace --all-targets -- -D warnings`,
`npm run build` grün. Neue Node-Skripte laufen als eigene Schritte in `ci.yml`; jede
Inline-Kopie bekommt einen Drift-Abschnitt in `scripts/test-inline-kopien.mjs`.

- **Node:** `scripts/test-undo-stand.mjs` (Schnitt/Wiederherstellung über Satzgrenze,
  Korrektur-Schritt mit Rücknahme, Kappung bei 50, alter Stand nach Update, neuer Stand auf
  alter `restoreSnapshot`-Fassung, Normalisierung nur bei Präfix) ·
  `scripts/test-stand-groesse.mjs` (Größen-Regression gegen 28/40 KB, Kürzung über 56 KB;
  Abgleich der Seiten-Konstante mit `MAX_STATE_LEN`) · `scripts/test-stand-wahl.mjs` (alle
  Regeln aus A samt Legacy, `hostSaetzeTeilen`, wiedereröffnet schlägt veraltet-beendet) ·
  `scripts/test-verlauf-sync.mjs` (Sync-Aufrufe und ihre Reihenfolge in `submit` und im
  `match_assigned`-Zweig) · `scripts/test-geparkt.mjs` (Parken bei Wechsel und Leeren,
  Folgespiel sendbar, Auflösen, Rückkehr → Abfrage, getrennte Persistenz) ·
  `scripts/test-result-retry.mjs` erweitert um „überholt".
- **Rust (relay-proto):** `match_assigned_hostsets_roundtrip_und_alt_lesbar`,
  `relay_frame_state_sync_roundtrip`, `result_ack_superseded_roundtrip`,
  `result_response_superseded_bleibt_im_normalfall_von_der_leitung`,
  `schlanker_anzeige_stand_streicht_events`.
- **Rust (src-tauri):** `set_court_state_verwirft_ueber_deckel_und_laesst_alten_stand`,
  `deckel_log_nur_einmal_je_match`, `push_match_traegt_hostsets_aus_live_scores`,
  `hostsets_leer_bei_legacy_schalter`, `host_uebernimmt_cloud_stand_mit_stale_filter`,
  `score_mirror_ohne_echo_bei_cloud_quelle`,
  `erster_spiegel_nach_relay_neustart_traegt_stand`,
  `cloud_stand_verworfen_bei_aktivem_lan_halter`,
  `nachzuegler_sync_nach_ergebnis_wird_angenommen`,
  `nachzuegler_nach_fensterablauf_verworfen`, `nachzuegler_fremdes_match_verworfen`,
  `nachzuegler_haelt_finished_und_retired`, `einzel_rally_nach_ergebnis_bleibt_verworfen`,
  `nachzuegler_bei_neu_belegtem_feld_angenommen`,
  `process_result_ueberholt_wenn_match_beendet_und_nicht_auf_dem_feld`,
  `process_result_nicht_ueberholt_wenn_match_ohne_ergebnis_vom_feld`,
  `process_result_ueberholt_ist_nie_ok`,
  `process_result_wiedereroeffnetes_match_nimmt_korrektur_an`.
- **Rust (relay):** `store_court_state_ueber_deckel_wird_geloggt_nicht_gespeichert`,
  `host_spiegel_ueber_deckel_wird_geloggt`, `monitor_state_traegt_schlanken_stand`,
  `relay_reicht_hostsets_durch_ohne_sie_zu_cachen`,
  `store_court_state_reicht_an_host_weiter`,
  `host_spiegel_ueberschreibt_aktives_cloud_tablet_nicht`,
  `relay_reicht_sync_fuers_vormatch_im_fenster_durch`.
- **Manueller Feldtest (vor dem Release-Tag):** Tablet mitten im Satz ausschalten →
  Ersatzgerät; App mitten im Spiel neu starten → Ersatzgerät; Ergebnis bei getrenntem WLAN,
  Hand-Eingabe in BTP, neues Spiel; Karte bei getrenntem Internet (Cloud), danach Zettel
  ansehen.

## Risiken & Rollback

- **Relay-Deploy leert den Cache.** Jeder PR dieser Spec deployt das Relay. Deshalb nicht
  während eines laufenden Turniers mergen. Ab PR 3a ist der Schaden begrenzt (Einstieg mit
  Host-Sätzen statt 0:0).
- **Mischversionen.** Alte Seite + neuer Host/Relay: Verhalten wie heute. Neue Seite +
  alter Host/Relay: Regeln A Stufe 1 und B wirken, Host-Sätze, „überholt" und Nachzügler
  nicht — fehlt „überholt", parkt das Tablet (sicherer Rückfall). Alte Seite liest
  v2-Stand: siehe B.
- **Fehldeutung des Melde-Rings.** Geht eine Punkt-Meldung verloren und nimmt der Bediener
  offline weiter zurück, als der Ring reicht, hält das Tablet die Host-Sätze für fremd und
  steigt mit ihnen ein. Selten, sichtbar, per Korrektur heilbar.
- **`finished` zählt nicht zum Fortschritt.** Eine nicht übermittelte Aufgabe eines
  ausgefallenen Geräts kann gegen einen gleich weit gezählten Stand verlieren; der Bediener
  sieht das Spiel dann als laufend und beendet es erneut.
- **Uplink.** Über Kreuz geht jeder Cloud-Zustand (≤ 40 KB je Punkt) zusätzlich an den Host.
  Gerechnet einige KB/s bei zehn Feldern, nicht gemessen; bei Bedarf im Relay entprellen.
- **Rollback.** Autorität: Laufzeit-Schalter `reconnect_legacy_rev`. Alles andere: Revert
  des jeweiligen PRs; ein v2-Stand bleibt für die alte Seite lesbar. Eine ältere App-Version
  ignoriert die neuen Frames und Felder.

## Offene Fragen / Annahmen

- Die Größenmessung nutzt ein gegenüber dem echten `MatchBrief` vereinfachtes Spiel; die
  Grenzen 28/40 KB enthalten deshalb Puffer. Der Regressionstest in PR 1 legt die echten
  Werte fest; liegen sie darüber, wird die Grenze in dieser Spec vor dem Merge angepasst.
- ADR 0016/0017/0018 stehen noch auf „proposed", obwohl umgesetzt; ihr Status wird mit
  ADR 0083/0085 nachgezogen.
- Annahme: Der nächste BTP-Poll nach einem Ergebnis kommt innerhalb des 120-s-Fensters; die
  Dauer ist nicht eigens nachgemessen.

## Betroffene Doku-Dateien

- `docs/tablet.md` (Reconnect-Wahrheit, Zustandsgröße, Sync-Zeitpunkte, geparktes Ergebnis)
- `docs/tablet-bedienen.md` (Handbuch: Hinweisband, Abfrage, „Ergebnis stand schon in BTP",
  Einstieg mit Satzstand — jeweils mit `<!-- pruef: … -->`-Anker)
- `docs/cloud-relay.md` (`hostSets`, `RelayFrame::StateSync`, `superseded`, Deckel,
  Monitor-Fassung, Vor-Match-Fenster, Deploy-Hinweis)
- `docs/court-monitor.md` (schlanker Stand ohne Ereignisse)
- `docs/multi-hall.md` (LAN+Cloud über Kreuz)
- `docs/punktverlauf.md`, `docs/schiedsrichterzettel.md` (Nachzügler, Voll-Sync)
- `docs/features/turnier-robustheit.md`, `docs/features/ergebnis-puffer.md` (Querverweise)
- `docs/roadmap.md`, `docs/changelog.md` je Version, `CLAUDE.md` (Zeile in der Doku-Tabelle)

## Umsetzungs-Hinweise

Fünf PRs in dieser Reihenfolge; jeder mit Versions-Bump (`src-tauri/Cargo.toml`,
`src-tauri/tauri.conf.json`, `package.json`), `code-reviewer` und `security-reviewer`.
Die Versionsnummer wird erst beim Merge festgelegt.

1. **PR 1 — Größe und Format** (Regeln B; AK11–AK16). `undoStand.mjs` mit Tests →
   Inline-Kopie und Leser für beide Formate → Schreiber v2 und Normalisierung →
   Selbstbegrenzung des Spiegels → `MAX_STATE_LEN` und `schlanker_anzeige_stand` nach
   `relay-proto`, Deckel und gedrosseltes Log in Host und Relay → Monitor-Fassung im Relay.
   ADR 0084.
2. **PR 2 — Sync und Nachzügler** (Regeln D; AK1 zweiter Teil, AK22, AK23). Drei
   Sync-Stellen im Tablet → Merker `kuerzlich_beendet` und gelockerter Sync-Filter im Host
   (LAN und Relay-Client) → `apply_sync` erhält `finished`/`retired` → Vor-Match-Merker im
   Relay. Teil von ADR 0085.
3. **PR 3a — Autorität und Host-Sätze** (Regeln A ohne Über-Kreuz; AK2–AK10).
   `standWahl.mjs` mit Tests → `schreiber` und Melde-Ring → beide Stufen im Tablet →
   `einstiegMitSaetzen` aus `submitEnterScore` herausziehen → `hostSets` in `relay-proto`,
   `push_match`, `push_court`, Relay-Durchleitung → Sende-Felder beim Übernehmen anstoßen.
   ADR 0083.
4. **PR 3b — Über Kreuz** (AK24, AK3 zweiter Teil). `RelayFrame::StateSync` → Weiterleiten
   im Relay und Riegel 2 → Arm im Relay-Client mit Stale-Filter und Riegel 3 →
   Zustands-Quelle und Riegel 1 samt einmaligem Rückspiegeln je Relay-Verbindung.
5. **PR 4 — Altes Ergebnis** (Regeln C; AK17–AK21). `superseded` in `relay-proto` →
   Bedingung in `process_result` → Durchleitung über `ResultAck` und Relay →
   `resultRetry.mjs` und `geparkt.mjs` mit Tests → Parken in `loadMatch`/`clearMatch`,
   eigener Speicher-Schlüssel → Band, Abfrage, Verwerfen → Versionsabgleich. ADR 0085.

Die Reihenfolge ist bindend für 1 → 3a (die Autorität setzt voraus, dass der Spiegel nicht
mehr verworfen wird) und 3a → 3b. PR 2 und PR 4 sind davon unabhängig.
