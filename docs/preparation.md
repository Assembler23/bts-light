# Spiele in Vorbereitung aufrufen

Die Turnierleitung wählt eingeplante Spiele aus und „ruft sie in die
Vorbereitung". Aufgerufene Spiele erscheinen auf der Liveticker-Aufruf-
Anzeige (`/live?display=next`) hervorgehoben — Spieler sehen rechtzeitig,
dass ihr Spiel ansteht, bevor es aufs Feld kommt. Bei Mehr-Hallen-
Turnieren lässt sich pro Aufruf eine Halle wählen; ein Meeting-Point-TV
je Halle (`?display=next&halle=…`) zeigt dann nur die Spiele *seiner*
Halle, und bts-light kann je gerufenem Spiel eine **gesprochene Hallen-
Ansage** auslösen (Knopf neben dem Aufruf).

BTP kennt keinen Vorbereitungs-Zustand — bts-light verwaltet ihn selbst,
genau wie die Walkover-Vorschläge ([walkover.md](walkover.md)).

Eingeführt in v0.9.14; Hallen-Filter auf `display=next` mit v0.9.14
(badhub-Seite); Hallen-Ansage mit v0.9.16.

## Ablauf

1. **Auswählen** — auf dem `In Vorbereitung`-Tab im Tablet-Spielzettel
   listet bts-light alle eingeplanten Spiele (`MatchStatus::Scheduled`)
   mit feststehender Paarung. Die Turnierleitung hakt eines oder mehrere
   an und (bei Mehr-Hallen-Turnieren) wählt die Halle.
2. **Aufrufen** — der Knopf „In Halle X aufrufen" schickt einen
   `call_preparation`-Command an den Kern. Pro Match-ID gibt es höchstens
   einen aktiven Aufruf (ein zweiter ersetzt den ersten).
3. **Stempeln** — der Sync-Lauf ruft `apply_preparation_calls` auf den
   nächsten BTP-Snapshot an: aufgerufene Matches bekommen die transienten
   Felder `preparation_call_ts` (Unix-ms) und `preparation_hall`
   (aufgelöster `BtpLocation`-Name) gestempelt.
4. **Pushen** — `build_tset` legt diese Felder in den `upcoming_matches`-
   Eintrag (`TsetMatch.preparation_call_ts` und `.hall`). Geänderte
   Aufrufe lösen sofort einen vollen `Update::Full` aus (statt erst beim
   Heartbeat — siehe Fingerabdruck in `badhub/diff.rs`).
5. **Anzeigen** — die badhub-Seite (`public/assets/js/live.js`,
   `renderNextRow`) liest `preparation_call_ts` und kennzeichnet die
   Zeile mit „In Vorbereitung · seit X Min" (Status-Pille). Mit
   `&halle=<Name>` filtert sie auf `upcoming_matches[].hall`.
6. **Ansage** — der „Ansage"-Knopf je gerufenem Spiel löst
   `playPreparationAnnouncement` aus: Gong → „In Vorbereitung" →
   Disziplin → Paarung → „Bitte in Halle X". Sprache aus `AnnounceConfig`
   oder automatisch (siehe [announcements.md](announcements.md)).
7. **Aufräumen** — kommt das Match auf Court, beendet es oder verschwindet
   es aus dem BTP-Stand, verwirft `apply_preparation_calls` den Aufruf
   automatisch (keine Geister-Aufrufe). `diff.rs` löst dabei einen vollen
   `tset` aus, sodass der Aufruf sofort vom Monitor verschwindet.
8. **Zurücknehmen** — die Turnierleitung kann einen Aufruf jederzeit per
   `retract_preparation` manuell zurücknehmen.

## Auto-Feldvergabe: Reihenfolge + Spieler-Verfügbarkeit

`auto_assign` (`src-tauri/src/sync.rs`) belegt freie Felder automatisch:

- **Reihenfolge:** `(call.is_none(), planned_time, match_num, id)` — manuell
  gerufene Spiele zuerst, sonst den **BTP-Zeitplan** (`PlannedTime`, geparst zu
  `BtpMatch.planned_time` als `YYYYMMDDHHMM`), dann Spielnummer/ID. Die
  Kandidatenliste (`preparation_candidates`, `info_preparation_state`) sortiert
  identisch.
- **Spieler-Verfügbarkeit:** Ein spielbereites Match wird übersprungen, wenn ein
  Spieler gerade OnCourt ist, in diesem Zyklus schon ein Feld bekam, oder noch in
  seiner **Pause** ist. Identität via `player_key` (Lizenznr., sonst Name).
- **Pause:** `pause_ms` aus `config.auto_assign.pause_minutes` (>0 = Override) bzw.
  `BtpSnapshot.rest_minutes` aus **BTP-Setting 1303**; je Spieler gegen das
  zuletzt beendete Spiel (`finished_at`) geprüft.

## Was ist „ruf-bar"?

Kandidaten der Liste (`commands::preparation_candidates`) sind alle
Matches mit:

- `status == Scheduled` (BTP) — nicht auf Court, nicht beendet.
- Beide Mannschaften nicht leer — also eine **echte Paarung**. Spiele,
  bei denen ein Slot noch von einem Feeder-Match abhängt, erscheinen
  nicht; sie könnten nicht sinnvoll gerufen werden.

`apply_preparation_calls` prunet Aufrufe mit derselben Bedingung — so
fallen Aufrufe für Matches, die einen Spieler verloren haben, ebenfalls
heraus (keine Inkonsistenz zwischen Liste und State).

## Datenmodell

`tablet/state.rs`:

- `PreparationCall { match_id, location_id: Option<i64>, called_at_ms }`
  — ein offener Aufruf; je `match_id` höchstens einer.
- `TabletState.preparation_calls: RwLock<Vec<PreparationCall>>` — geteilt
  zwischen Sync-Loop und Tauri-Command-Handlern.
- `add_preparation_call` / `preparation_calls` / `remove_preparation_call`
  — Mutationen / Abfrage, Muster wie die Walkover-Vorschläge.
- `apply_preparation_calls(&mut snapshot)` — räumt nicht-ruf-bare Aufrufe
  auf und stempelt die überlebenden in den Snapshot.

`btp/model.rs` – zwei transiente Felder auf `BtpMatch`, die der **Parser
nicht** setzt:

- `preparation_call_ts: Option<u64>`
- `preparation_hall: Option<String>`

Beide werden ausschließlich von `apply_preparation_calls` belegt, exakt
nach dem Vorbild von `finished_at`.

`badhub/payload.rs`:

- `TsetMatch` bekommt `preparation_call_ts` und `hall` mit
  `#[serde(skip_serializing_if = "Option::is_none")]`.
- `to_upcoming_match` füllt sie aus dem gestempelten `BtpMatch`.
- `upcoming()` sortiert gerufene Matches **vor** den ungerufenen, damit
  ein Aufruf nie aus `UPCOMING_LIMIT = 15` herausfällt. Ohne Aufrufe
  degeneriert die Sortierung zur bisherigen Spielnummern-Reihenfolge.

`badhub/diff.rs`:

- Fingerabdruck `BTreeMap<i64, (u64, Option<&str>)>` = Match-ID →
  (Aufruf-Zeit, Halle). Ändert er sich, wird `Update::Full` erzwungen —
  Aufrufe erreichen den Monitor sofort statt erst beim Heartbeat (bis 60 s).

`commands.rs`:

- `preparation_candidates() -> PreparationView { candidates, locations }`
  — reiner Lesepfad, nicht-ruf-bare Matches erscheinen einfach nicht.
- `call_preparation(match_ids: Vec<i64>, location_id: Option<i64>)`
- `retract_preparation(match_id: i64)`

Frontend-Spiegel:

- `src/types.ts` — `PreparationCandidate { ..., team1: string[], team2:
  string[], team1_nationalities: string[], ..., discipline: string, call:
  PreparationCallInfo | null }` (Einzel-Spielernamen + Nationalitäten +
  Disziplin sind die Grundlage der Hallen-Ansage).
- `src/api.ts` — `preparationCandidates` / `callPreparation` /
  `retractPreparation`.
- `src/pages/PreparationPanel.tsx` — dritter Tab; Polling alle 4 s.

## Hallen-Filter auf `display=next`

`public/assets/js/live.js` (badhub-Repo):

- `filterUpcomingByHall(upcoming)` filtert vor dem `NEXT_MONITOR_LIMIT`-
  Cap — sonst schnitte der Cap Spiele weg, bevor der Hallen-Filter sie
  sieht.
- **Kein** Rückfall auf „alle" wie beim Court-Grid: nicht gerufene Spiele
  tragen keine Halle, ein leeres Ergebnis ist also der Normalfall (noch
  nichts in diese Halle gerufen), kein Tippfehler-Symptom.
- Halle steht in der `<h2>`-Überschrift; hallenspezifischer Leer-Hinweis.

## Hallen-Ansage

`src/io/announcer.ts`:

- `AnnouncePreparationInput { discipline, teamANames, teamBNames, hall? }`.
- `buildPreparationSegments(input, lang)` — Segmente: „In Vorbereitung."
  → Disziplin → Team A → „gegen …" → „Bitte in *hall*." (Letzteres
  entfällt bei Ein-Hallen-Turnieren).
- `playPreparationAnnouncement(input, lang, opts)` — gleiche Gong-/TTS-
  Pipeline wie die Feld-Ansage.
- `resolveAnnouncementLanguage(nationalities, mode)` — geteilte Helper-
  Funktion (auch von `MatchAnnouncer.tsx` benutzt). Auto-Modus: Englisch,
  sobald ≥ Hälfte international (≠ GER).

`src/pages/PreparationPanel.tsx`:

- „Ansage"-Knopf je gerufenem Spiel (nur sichtbar, wenn
  `AnnounceConfig.enabled === true`).
- Der Knopf-Klick selbst ist die User-Geste, die WebView2 zum Entsperren
  des AudioContexts braucht — ein separater `unlockAudio()`-Aufruf ist
  hier nicht nötig.

## Bewusste Designentscheidungen

- **Manuell statt automatisch:** Aufruf und Ansage sind getrennte Schritte
  und beide explizit. Die Turnierleitung entscheidet pro Spiel, wann sie
  ruft und wann sie spricht — kein Auto-Mechanismus.
- **Per Match-ID statt per Entry:** Aufrufe identifizieren sich über die
  Match-ID. Anders als bei Walkover (per `EntryID`) gibt es keinen
  Mannschaftsbezug.
- **Aufrufe fallen automatisch raus, sobald sie obsolet sind:**
  `apply_preparation_calls` läuft in jedem Sync-Zyklus und prunet —
  garantiert konsistente Liste ohne zusätzliche Pflege.
- **Reiner Lesepfad im Command:** `preparation_candidates` mutiert den
  State nicht (anders als `walkover_proposals`, das per GC-on-read
  aufräumt). Der Sync-Lauf ist der einzige Schreibpunkt für `prep_calls`.

## Bekannte Grenzen

- Halle wird **nur bei gerufenen Spielen** gepusht. Ungerufene Spiele in
  `upcoming_matches` tragen keine Halle — der `?halle=`-Filter blendet sie
  daher aus (das ist gewollt, der Meeting-Point-TV ist ein Aufruf-Display).
- Die Hallen-Ansage spricht den Hallennamen **wörtlich**. Bei reinen Zahl-
  Namen („1", „2") spricht der TTS-Browser „eins", „zwei" — funktioniert,
  ist aber stilistisch nicht so klar wie „Halle eins". BTP-Hallennamen
  enthalten meist das Wort „Halle".
- Polling-Latenz: das Panel pollt alle 4 s; ein Aufruf erscheint daher
  spätestens nach 4 s in der „Aufgerufen"-Liste eines anderen Operators.

## Beteiligte Dateien

**Rust-Kern**

- `src-tauri/src/tablet/state.rs` — `PreparationCall`,
  `apply_preparation_calls`.
- `src-tauri/src/btp/model.rs` — `BtpMatch.preparation_call_ts` /
  `preparation_hall`.
- `src-tauri/src/commands.rs` — `preparation_candidates` /
  `call_preparation` / `retract_preparation` + View-Structs.
- `src-tauri/src/badhub/payload.rs` — `TsetMatch.preparation_call_ts` /
  `.hall`, `upcoming()`-Sortierung.
- `src-tauri/src/badhub/diff.rs` — Fingerabdruck → `Update::Full`.
- `src-tauri/src/sync.rs` — `apply_preparation_calls` in `run_once`.
- `src-tauri/src/lib.rs` — Tauri-Command-Registrierung.

**Frontend**

- `src/types.ts` — Datenmodell-Spiegel.
- `src/api.ts` — Command-Wrapper.
- `src/io/announcer.ts` — `playPreparationAnnouncement`,
  `buildPreparationSegments`, `resolveAnnouncementLanguage`.
- `src/pages/PreparationPanel.tsx` — Tab-Inhalt, Polling, Ansage-Knopf.
- `src/pages/TabletPanel.tsx` — Tab-Bar.
- `src/App.tsx` — reicht `AnnounceConfig` durch.

**Badhub (Liveticker)**

- `public/assets/js/live.js` — `renderNextRow` („In Vorbereitung"-Pille),
  `renderNextMonitor` + `filterUpcomingByHall`.
- `docs/features/liveticker_bts.md` (badhub-Repo) — Doku des
  `display=next`-Monitors und des `&halle=`-Filters.
