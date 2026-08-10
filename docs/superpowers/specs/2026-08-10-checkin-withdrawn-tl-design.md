# Turnierleitungs-Sicht: badhub-Zustand `withdrawn` (abgemeldet)

**Datum:** 2026-08-10 · **Status:** entworfen, vom Betreiber freigegeben
**Gegenstück:** badhub-PR #344 (Migration 157) — `checkin_players.state` kennt
seit heute den vierten Wert `withdrawn`. Die badhub-Admin-Oberfläche
(`/admin/checkin`) setzt ihn; der `/tl/stand`-Payload liefert `state` roh aus,
bts-light bekommt den Wert also ungefragt.

## Problem

bts-light kennt nur `open · checked_in · query`. Ein Abgemeldeter fällt heute
überall in den „fehlt"-Zweig:

1. **Die Fehlt-Ansage ruft ihn aus** (`CheckinPlayer::is_missing()` =
   `state != "checked_in"`) — eine abgemeldete Person wird über die
   Hallen-Lautsprecher gesucht. Das ist der kritische Teil.
2. **Die Zähler stimmen nicht:** `fehlend = gemeldet − eingecheckt` rechnet
   Abgemeldete als fehlend; badhubs TL-`gemeldet` zählt sie mit (bewusste
   badhub-Entscheidung, bleibt so).
3. **Die Spielerzeile zeigt „fehlt"** statt „abgemeldet".

Das Parsing selbst ist tolerant (`state: String` mit serde-Default) — nichts
crasht, es wird nur falsch angezeigt und angesagt.

## Entscheidungen

- **Einchecken bleibt für Abgemeldete möglich** (Betreiber-Entscheidung
  2026-08-10). Der bestehende `check_in`-Eingriff des `/tl/`-Kanals setzt
  einen Abgemeldeten direkt auf `eingecheckt` — badhub lässt das zu
  (Zugehörigkeits-Prüfung, keine Zustands-Plausibilität). Der Knopf-Tooltip
  macht das Überschreiben explizit („Trotz Abmeldung als anwesend
  eintragen"). Kein `readmit` über den `/tl/`-Kanal — den gibt es dort nicht,
  und er wird nicht gebraucht.
- **Zähler werden clientseitig korrigiert**, nicht in badhub: badhubs
  TL-Zählung bleibt spielerbasiert inklusive Abgemeldeter (dortige geparkte
  Review-Entscheidung). bts-light hat die volle Spielerliste und rechnet
  selbst: `abgemeldet` je Klasse aus `players`, `fehlend = gemeldet −
  eingecheckt − abgemeldet`.
- **Keine Protokoll-Änderung.** Kein neuer Endpunkt, kein neues Feld.
  Alte bts-light-Versionen bleiben funktionsfähig (zeigen Abgemeldete
  lediglich als „fehlt" — der heutige Zustand).

## Änderungen

### Rust — `src-tauri/src/badhub/checkin_state.rs`

- Docblock von `CheckinPlayer.state`: `open · checked_in · query · withdrawn`
  (seit badhub-Migration 157, gesetzt über die badhub-Verwaltung).
- `is_missing()` → `self.state != "checked_in" && self.state != "withdrawn"`.
  Begründung im Kommentar: Abgemeldete werden nicht gesucht — weder in der
  Ansage (AK-C7) noch in der Fehlt-Liste. `query` zählt weiterhin als
  fehlend (unverändert).
- Neuer Helfer `is_withdrawn()` (`self.state == "withdrawn"`), damit UI-nahe
  Stellen nicht gegen String-Literale vergleichen.

### TypeScript — `src/types.ts`

- Kommentar von `CheckinPlayer.state` um `withdrawn` ergänzen.

### TypeScript — `src/pages/CheckinPanel.tsx`

- `spielerZustand()`: **vor** der `locked`-Prüfung der neue Zweig
  `state === "withdrawn"` → Text „abgemeldet", Klasse
  `text-slate-400 line-through`.
- Einchecken-Knopf: bleibt für Abgemeldete sichtbar (sie sind nicht `da`);
  der `title` unterscheidet: bei Abgemeldeten „Trotz Abmeldung als anwesend
  eintragen", sonst wie bisher „Als anwesend eintragen". Der
  Zurücksetzen-Knopf bleibt an `da` gebunden (unverändert).
- Zähler je Klasse: `abgemeldet = players.filter(state === "withdrawn").length`;
  Kopfzeile „X von Y da" rechnet `Y = gemeldet − abgemeldet`; dahinter
  „· Z abgemeldet", nur wenn `Z > 0`. `fehlend` (für den Ansage-Knopf-Text)
  = `gemeldet − eingecheckt − abgemeldet`.
- Gesamtsumme im Seitenkopf: gleiche Rechnung über alle Klassen.

### Doku — `docs/features/spieler-check-in.md`

Im Schnitt-C-Abschnitt drei neue Akzeptanzkriterien:

- **C16** Ein in badhub abgemeldeter Spieler (`state = withdrawn`) erscheint
  in der TL-Sicht als „abgemeldet" (grau, durchgestrichen) und zählt weder
  als eingecheckt noch als fehlend.
- **C17** Die Fehlt-Ansage nennt Abgemeldete nicht — weder namentlich noch
  in der Anzahl.
- **C18** Die TL kann einen Abgemeldeten über den bestehenden
  `check_in`-Eingriff trotzdem einchecken; der Knopf benennt das
  Überschreiben der Abmeldung.

Dazu ein Absatz: Herkunft des Zustands (badhub `/admin/checkin`,
Migration 157), Abmelden/Wiederanmelden selbst gibt es nur dort.

## Tests (Rust, `cargo test`)

In den bestehenden `#[cfg(test)]`-Block von `checkin_state.rs`, nach dem
Muster der vorhandenen JSON-Fixture-Tests:

- Ein `withdrawn`-Spieler wird geparst (kein Fehler, Zustand bleibt erhalten).
- `is_missing()` ist für `withdrawn` falsch, für `open`/`query` wahr, für
  `checked_in` falsch.
- Die Fehlt-Ansage einer Klasse mit einem Abgemeldeten und einem Offenen
  nennt nur den Offenen; sind alle übrigen eingecheckt und einer abgemeldet,
  gibt es keine Ansage (C8 greift).

Frontend: kein Test-Runner im Repo für Komponenten — die TS-Änderungen
sichert der TypeScript-Compiler (`npm run build`), Verhalten deckt der
Rust-Teil.

## Ausdrücklich nicht in diesem Schnitt

- Kein `readmit`/`withdraw` über den `/tl/`-Kanal (bleibt badhub-Admin).
- Keine Änderung an badhub (Payload bleibt, Zählung bleibt).
- Keine Sortier-Änderung (Abgemeldete bleiben alphabetisch einsortiert).
