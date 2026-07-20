# ADR 0007 — Zähltafelbediener nach dem Vorbild des Original-BTS, in zwei Phasen

Status: akzeptiert (2026-07-20)

## Kontext

Bei Turnieren zählt oft ein **Zähltafelbediener** (Tabletoperator) das Spiel
am Feld — typischerweise der **Verlierer des vorigen Spiels** auf demselben
Feld. Nutzerwunsch: das **so ähnlich wie im Original-BTS (letilo/bts)**
handhaben.

**Tilos Modell** (aus der Roadmap-Analyse):
- **Auswahl automatisch:** nach jedem regulär beendeten Spiel kommt der
  **Verlierer** in eine **FIFO-Warteschlange**. Walkover/Aufgabe/DQ erzeugen
  keinen Eintrag. (Optionen bei Tilo: ab Viertelfinale der Gewinner; Doppel
  splittbar — bei uns zunächst nicht.)
- **Zuweisung beim Feld-Aufruf:** der Feld-Aufruf zieht den ältesten Wartenden
  (serialisiert, race-frei) und hängt ihn ans Match; bevorzugt das Feld, auf
  dem er gerade gespielt hat.
- **Ansage: ja** — „Tabletbedienung: {Name}" als Teil der Court-/
  Vorbereitungs-Ansage, plus ein Zweitaufruf-Knopf („… bitte als
  Tabletbedienung melden!").
- **Absicherung:** bei Zuweisung wird der Bediener in **BTP ausgecheckt**
  (`CheckedIn=false` über ein eigenständiges Spieler-Update — Tilos
  „Schreibweg 2"), damit BTP ihn nicht parallel für ein eigenes Spiel einplant.
  Danach optional eine garantierte **Mindestpause** (Default 300 s).
- **Anzeige:** nur im Admin (Warteliste + Match-Panel), **nicht** auf den
  Court-Displays.

**Ist-Zustand bts-light:** Es gibt bereits einen *pro-Feld*-Hinweis
(`track_scorekeepers`/`scorekeeper_by_court` in `sync.rs`): der Verlierer des
zuletzt auf **diesem** Feld beendeten Spiels wird dem Tablet als
Bediener-Hinweis (`MatchBrief.scorekeeper`) mitgegeben. Das ist eine
Heuristik ohne globale Warteschlange und ohne Zuweisungs-/Ansage-/BTP-Logik.

Der BTP-**Auscheck bei Zuweisung** ist ein **neuartiger** BTP-Schreibpfad
(Spieler-Update außerhalb des Spielendes). Er berührt das `Status`-Bitfeld der
Check-ins — genau die Stelle der Regression v0.9.103 („Feldzuweisung löschte
Check-in-Bits"). Das ist der riskanteste Baustein und am echten BTP zu
verifizieren.

## Entscheidung

Wir bauen den Zähltafelbediener nach Tilos Modell, aber **in zwei Phasen**,
damit der erprobte Stand nicht durch den riskanten BTP-Schreibpfad gefährdet
wird:

**Phase 1 — rein in bts-light (kein neuer BTP-Write):**
1. **Globale FIFO-Warteschlange** im Rust-State: der Verlierer eines regulär
   beendeten Spiels wird eingereiht (Walkover/Aufgabe/DQ ausgenommen). Doppel =
   **ein** Eintrag (das ganze Team). Manuelle Pflege: vorziehen, zurückstellen,
   entfernen, manuell hinzufügen.
2. **Zuweisung beim Feld-Aufruf** (manuell `assign_court` + Auto-Vergabe in
   `sync.rs`): den ältesten Wartenden ans Match heften, bevorzugt aufs zuletzt
   gespielte Feld; serialisiert im State (race-frei).
3. **Ansage** „Tabletbedienung: {Name}" als Segment der Feld-/
   Vorbereitungs-Ansage + Zweitaufruf-Knopf (nutzt die bestehende
   `callStage`-Mechanik der 2./3.-Aufruf-Funktion).
4. **UI:** Wartelisten-Panel (Reihenfolge + Pflege-Knöpfe) + Bediener-Anzeige
   am Match in der Felderübersicht. **Nicht** auf den Hallen-TVs.
5. **Mindestpause** rein in bts-light: ein „frühestens-wieder-ab"-Zeitstempel
   je Spieler (Default 300 s), damit der Bediener nicht sofort wieder gezogen
   wird — ohne BTP-Write.
6. **Konfiguration minimal:** Schalter „Zähltafelbediener verwalten" +
   Mindestpause-Sekunden.

In Phase 1 verhindert bts-light eine Doppelbelegung **app-seitig** (ein
zugewiesener Bediener wird nicht zugleich als Spieler seines eigenen Spiels
gezogen), **ohne** in BTP zu schreiben.

**Phase 2 — BTP-Auscheck (später, eigener ADR-Nachtrag oder Freigabe):**
Der Auscheck des Bedieners in BTP (`CheckedIn=false`, Tilos „Schreibweg 2")
wird erst umgesetzt, nachdem am **echten BTP** verifiziert ist, dass ein
eigenständiges Spieler-Update die Check-in-Bits nicht kaputtschreibt
(v0.9.103-Falle). Bis dahin bleibt die Absicherung app-seitig (Phase 1).

## Alternativen

- **Nur den bestehenden pro-Feld-Hinweis lassen:** verworfen — deckt weder die
  globale Reihenfolge (wer ist als Nächstes dran) noch Ansage/Pflege ab, die
  Tilos erprobter Ablauf bietet.
- **Sofort mit BTP-Auscheck (Tilo 1:1) bauen:** verworfen für den ersten
  Wurf — der neue Spieler-Schreibpfad berührt die Check-in-Bits (v0.9.103) und
  ist ohne echten-BTP-Gegencheck ein Risiko für den erprobten Stand.
- **Zähltafelbediener nach BTP zurückschreiben (Players-Block am Spielende):**
  bereits vorhanden (seit 0.9.147 landet der Verlierer im Players-Block); keine
  neue Entscheidung nötig.

## Konsequenzen

- Der komplette Bediener-Ablauf (Reihenfolge, Zuweisung, Ansage, Pflege,
  Pause) steht in Phase 1 zur Verfügung, ohne den erprobten BTP-Schreibpfad
  anzufassen — geringes Risiko, sofort nutzbar.
- **Grenze von Phase 1:** Die Doppelbelegungs-Sperre wirkt nur in bts-light.
  Plant die Turnierleitung im BTP selbst parallel, kann ein Bediener theoretisch
  doch für ein eigenes Spiel angesetzt werden — das schließt erst Phase 2
  (BTP-Auscheck) aus.
- Phase 1 fügt State (Warteschlange + Pausen-Zeitstempel), Commands
  (Warteschlangen-Pflege), ein Ansage-Segment und ein UI-Panel hinzu; jeweils
  mit Rust-Unit-Tests (FIFO-Reihenfolge, Ausschluss von Walkover/Aufgabe,
  Zuweisungs-Auswahl, Pausen-Filter).
- **Neu bewerten:** Phase 2 wird erst nach dem echten-BTP-Gegencheck des
  Spieler-Auscheck-Writes freigegeben (Change-Gate „Wie getestet?"). Tilos
  Sonderoptionen (VF-Gewinner, Doppel-Split, Landesverband) bleiben bis zu
  konkretem Bedarf außen vor.
