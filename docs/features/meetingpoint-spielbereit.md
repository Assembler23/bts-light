# Meetingpoint: Einchecken je Partie und „spielbereit" — Spezifikation

> Status: **freigegeben 02.11.2026** (via /idee: Brief → Grill →
> How-To → Spec+Review, manuell geführt).
> Betroffene Crates: src-tauri, relay-proto, relay (neue Routen, Frames,
> eingebettete Seiten → **Relay-Deploy vor App-Release** in PR 2, 3, 8).
> ADRs: [0071](../adr/0071-meetingpoint-zustand-ein-store.md) (Zustand),
> [0072](../adr/0072-sr-vorab-zuteilung-vorlaeufig.md) (SR vorab),
> [0073](../adr/0073-strikter-modus-zusaetzliches-gate.md) (strikter Modus),
> [0074](../adr/0074-meetingpoint-seite-unter-namespace.md) (Seite/Cloud),
> [0075](../adr/0075-bedarf-der-aufruf-automatik.md) (Bedarfsrechnung).

## Kontext / Problem

Die Turnierleitung (TL) ruft Spiele heute „in Vorbereitung" (`docs/preparation.md`).
Ob die Beteiligten dann wirklich bereitstehen, sieht sie nicht: Ein Spiel kommt
aufs Feld, ein Spieler fehlt, das Feld steht leer. Gewünscht ist ein
**Meetingpoint** je Halle, an dem sich jeder Beteiligte einer gerufenen Partie
**einzeln** eincheckt. Die TL — und auf Wunsch die Feld-Automatik — bringen
dann nur **spielbereite** Partien aufs Feld. Eine optionale Aufruf-Automatik
sorgt dafür, dass genug spielbereite Partien warten, wenn Felder frei werden,
aber nicht zu viele Paarungen lange am Meetingpoint sitzen.

Abgrenzung: Der **Hallen-Check-In** (`docs/spieler-check-in.md`, badhub, je
Klasse vor der Auslosung) bleibt unverändert. BTPs `Player.CheckedIn` ist
turnierweit und wird **nicht** benutzt (R2).

## Zielbild & Erfolgskriterien

Am Meetingpoint steht ein Tablet/Touch-TV mit der Meetingpoint-Seite seiner
Halle. Gerufene Partien erscheinen dort; Spieler (und ggf. der Schiedsrichter)
tippen ihren Namen an. Die TL sieht im TL-Web und am Desktop je Partie „3/4 ·
fehlt: Müller" bzw. **spielbereit**. Mit strikter Feld-Automatik kommen nur
spielbereite Partien aufs Feld.

**Erfolg (Feldtest):** An einem Turniertag mit eingeschaltetem Meetingpoint
und strikter Automatik kommt kein Spiel mit fehlendem Spieler aufs Feld, das
nicht von Hand vergeben wurde; nach einem App-Update stehen alle Aufrufe und
Check-ins unverändert da.

## Begriffe

- **Aufruf** — ein Spiel ist „in Vorbereitung" gerufen, mit Halle (bisheriger
  `PreparationCall`).
- **Platz** — eine einzucheckende Person einer Partie: Spieler (Seite 1/2,
  Position 0/1) oder **SR** (nur Schiedsrichter, nicht Assistent; nie der
  Zähltafelbediener).
- **spielbereit** (im Code `mp_bereit`, weil „spielbereit" dort schon
  „Paarung steht, nicht blockiert" heißt) — alle Spieler-Plätze eingecheckt
  **und**, wenn `require_umpire` an ist, der SR-Platz eingecheckt **oder** die
  Partie steht auf „ohne Schiedsrichter".
- **vergebbar** — Partie könnte jetzt ein Feld bekommen (kein Spieler auf dem
  Feld, keiner in Pflichtpause, nicht ausgenommen).

## Fachliche Entscheidungen

Aus Brief (B), Grill (G) und How-To (H), alle vom Nutzer bestätigt.

1. **Ort (B1):** neue bts-light-Seite am Meetingpoint-Gerät; kein Handy/badhub-Weg.
2. **Antippen genügt (B6):** jeder kann jeden ein- und auschecken; die TL korrigiert.
3. **Ein Meetingpoint je Halle (B5):** Seite filtert per `?halle=<Name>`; ohne
   Parameter im Ein-Hallen-Turnier alle Aufrufe, im Mehr-Hallen-Turnier zeigt
   sie eine Hallenwahl statt Partien (kein hallenübergreifendes Einchecken).
   Unbekannter Hallenname → Hinweis „Halle unbekannt" + Hallenwahl.
4. **Lebenszyklus (G6, H):**
   - Check-in gilt je (Partie, Platz) und ist an die **Person** gebunden
     (Fingerabdruck: BTP-`PlayerID`, ersatzweise Namens-Hash; SR: Official-ID).
     Wechselt BTP die Person auf dem Platz, ist der Platz **nicht** eingecheckt.
   - Rücknahme des Aufrufs löscht alle Check-ins der Partie.
   - Neuaufruf in **dieselbe** Halle behält Check-ins und den ersten
     Aufrufzeitpunkt (Fehlende-Uhr läuft weiter); Neuaufruf in **andere** Halle
     setzt Check-ins zurück und startet die Uhr neu.
   - Steht eine Person in einer **anderen** Partie auf dem Feld, gilt sie in
     allen übrigen Partien als ausgecheckt.
   - Kommt die Partie aufs Feld, wird der Aufruf **geparkt** (nicht gelöscht).
     Wird sie wieder vom Feld genommen (bestätigt über 3 Abrufe als „geplant"
     ohne Feld, egal ob über Desktop, TL-Web oder BTP), leben Aufruf und
     Check-ins wieder auf. Bei Spielende oder Verschwinden aus BTP wird gelöscht.
5. **Persistenz (G2):** Aufrufe und Check-ins liegen turniergebunden auf der
   Platte und überleben Neustart, Update und „Übertragung stoppen/starten".
   Nebeneffekt: das BTP-Gelb (P1) bleibt nach einem Neustart korrekt zugeordnet.
6. **Schiedsrichter (G1):**
   - Ist SR-Management mit SR-Rotation an, teilt die Rotation einer gerufenen
     Partie schon beim Aufruf einen **freien** SR vorläufig zu (nicht im Dienst,
     Pause vorbei, nicht schon vorläufig vergeben). Wird später ein SR frei,
     geht er an bereits gerufene Partien ohne SR (in Aufrufreihenfolge).
   - Die vorläufige Zuteilung wird erst beim Aufs-Feld-Kommen zur echten
     Zuweisung und erst dann nach BTP geschrieben. Rücknahme hinterlässt in BTP
     nichts. Ein in BTP oder von Hand gesetzter SR hat Vorrang.
   - Die TL kann jeder gerufenen Partie **„ohne Schiedsrichter"** geben.
   - Schalter `require_umpire` (Standard **aus**): an → SR-Platz gehört zur
     Spielbereitschaft. Ansagen zum Meetingpoint warten nie auf den SR.
7. **TL-Sicht:** je Partie Zähler eingecheckt/gesamt, wer fehlt, Marke
   „spielbereit", Ein-/Auschecken von Hand, Filter „nur spielbereite" (TL-Web
   und Desktop-Panel).
8. **Strikter Modus (B8, G4, G5):** Schalter `strict_auto_assign` (Standard aus).
   An → die Feld-Automatik vergibt **nur** spielbereite Partien, im
   Mehr-Hallen-Betrieb nur an Felder der Aufruf-Halle; nie gerufene Spiele
   bekommen kein Feld. Hand-Vergabe bleibt uneingeschränkt. Ein Wunschfeld hält
   sein Feld erst frei, wenn die Partie spielbereit ist. Bleibt ein Feld frei,
   weil nichts spielbereit ist, zeigt die TL eine Warnung; keine Kopplung an die
   Aufruf-Automatik. Schalter aus → Verhalten exakt wie heute (inkl. ADR 0030).
9. **Aufruf-Automatik (B4, G8, H):** Modus `aus | fest | restzeit` (Standard aus).
   - Je Halle. **Vorrat** = aktive Aufrufe dieser Halle mit vergebbarer Partie
     (auch wenn noch nicht alle eingecheckt sind).
   - `fest`: Ziel = `fixed_per_hall`.
   - `restzeit`: Ziel = min(`cap_per_hall`, Anzahl Felder der Halle, die jetzt
     frei sind oder laut Prognose innerhalb `horizon_minutes` frei werden, +
     `buffer`). Ein freies Feld zählt als „jetzt frei"; ohne Messwerte gilt die
     Startdauer der Prognose.
   - Ruft in Reihenfolge der Feld-Automatik, solange Vorrat < Ziel und aktive
     Aufrufe der Halle < `cap_per_hall`.
   - Ruft jede Partie in ihre aufgelöste Halle; im Mehr-Hallen-Betrieb ohne
     aufgelöste Halle gar nicht. Überspringt ausgenommene Partien, Partien mit
     Wunschfeld in anderer Halle, Partien mit Spieler auf dem Feld oder in Pause.
   - Läuft unabhängig von der Feld-Automatik; die TL-Pause der Automatik hält
     auch sie an. Nimmt nie selbst einen Aufruf zurück. Läuft nur am Master.
10. **Fehlende (B7):** je Schalter mit Minuten (Standard aus, 5 min):
    - Warnung in der TL, wenn eine Partie X Minuten nach dem Aufruf nicht
      spielbereit ist, mit den Namen der Fehlenden.
    - Ansage der Fehlenden namentlich („<Name>, bitte zum Meetingpoint
      <Halle>"), SR eingeschlossen, **genau einmal** je Partie und Stufe, auch
      über Neustarts.
11. **Zähl-Tablet (B3, G3):** Schalter `field_check_enabled` (Standard aus).
    Steht die Partie auf dem Feld, können Spieler/SR am Zähl-Tablet „Ich bin da"
    bestätigen; die TL sieht „am Feld bestätigt". **Keine** Zählsperre, kein
    Einfluss auf „spielbereit". Der Zustand gilt je Partie bis Spielende.
12. **Tablet als Meetingpoint-Gerät (B3, H):** Die Anzeige-Hülle bekommt die
    Ansicht „Meetingpoint" mit Hallenwahl; dort ist das Umschalten per Tipp
    abgeschaltet (Tipp = Einchecken).
13. **Cloud (G7):** Seite unter `/{ns}/meetingpoint` wie Tafel/Anzeige. Dass dort
    die Namespace-Kennung (zugleich Tablet-Zugang, ADR 0012) sichtbar ist, ist
    **bewusst hingenommen**. Jede Meldung prüft der Turnier-PC.
14. **Ferne Halle (G10):** Der Slave-PC liefert die Seite aus und reicht
    Abfrage und Check-in an den Master durch.

## Nicht-Ziele

- Einchecken per Handy/QR, badhub-Erweiterung, Rückschreiben nach BTP.
- Check-in für Zähltafelbediener und Assistenz-SR (AR).
- Identitätsprüfung (PIN je Person), eigener Gerätezugang für den Meetingpoint.
- Zählsperre am Tablet.
- Änderung am Hallen-Check-In und am badhub-TV `display=next`.
- Live-Kanal (WebSocket) für die Meetingpoint-Seite — 2-s-Abruf genügt zunächst.
- Aufrufe, die nur über BTP-Gelb oder eine Hallenzuweisung entstehen, erscheinen
  **nicht** am Meetingpoint (nur bts-light-Aufrufe).

## Betroffene Komponenten / Architektur

- **Zustand (PR 1/2):** neues Modul `tablet/meetingpoint.rs` mit
  `MeetingpointStore` (`meetingpoint.json`, Muster ADR 0022/`wish_court.rs`:
  lazy, turniergebunden, unlesbar → leer). Je Partie: Halle, erster
  Aufrufzeitpunkt, Phase aktiv/geparkt, Check-ins je Platz (Zeit, Fingerabdruck),
  vorläufiger SR, Ansage-Stufen der Fehlenden; daneben Feld-Bestätigungen je
  Partie. `TabletState::preparation_calls()` bleibt Projektion auf **aktive**
  Aufrufe (P1-Gelb, Ansage, badhub-tset unverändert). Frühbindung ans Turnier
  vor `auto_assign` (wie `reconcile_match_times`). Alle Merker in Store oder
  `TabletState`, nie in der `SyncEngine` (Sync-Neustart bei jedem Speichern).
- **Wire (relay-proto):** `MpPlatz` (Spieler{seite,pos} | Sr, mit `serde(other)`),
  `MpOrt` (meetingpoint | feld, mit `serde(other)`),
  `TlAction::MeetingpointSet{match_id, platz, present}`,
  `TlMatch.meetingpoint` (nur Flags je Platz, `bereit`, Aufrufzeit),
  `RelayFrame::MeetingpointCheckin` / `HostFrame::MeetingpointAck` (Muster
  `result`, `pending`, `MAX_PENDING_PER_NS`), additive Felder an `PreparedMatch`,
  `MatchBrief.feld_check: bool` (`serde(default)`), neue `AnnounceJobKind`-Arten
  für Erst-Aufruf der Automatik und Fehlende. Kennung nach außen ist **(Partie,
  Platz)**, nie `player_key`/Lizenznummer.
- **Routen:** LAN `GET /meetingpoint`, `GET /meetingpoint/state?halle=`,
  `POST /meetingpoint/checkin`; Relay dieselben unter `/{ns}/…`; Slave-Brücke:
  Weiterleitungs-Betrieb 303 auf den Relay, Träger-Betrieb Seite lokal +
  Durchreichen. Routen antworten 404, solange `meetingpoint.enabled` aus ist.
- **Eine Prüffunktion** für alle Wege (R5-Analogie): Partie in **dieser** Halle
  aktiv gerufen (bzw. bei `ort=feld`: auf dem Feld und Schalter an), Platz
  existiert, Body-Größe gedeckelt.
- **Feld-Automatik:** `sync.rs` `auto_assign` — zusätzlicher Filter im strikten
  Modus, Wunschfeld-Reservierung ab `mp_bereit`, Übernahme des vorläufigen SR
  vor `officials_for_write`. `require_call`/`halle_statt_aufruf` unverändert.
- **SR:** `officials.rs` (Auswahl des nächsten freien SR öffentlich),
  zustandsbasierter Zuteil-Schritt nach `track_officials`, zustandsbasierte
  Übernahme in `track_officials`; „ohne SR" = vorhandenes `OfficialClear{Sr}`.
- **Prognose:** Freiwerde-Rechnung aus `tl.rs` `build_state_limited`
  (Z. 3633–3716) als Helfer nach `predict.rs`, von TL-Zustand und Sync-Loop
  geteilt (eigener Refactor-PR, Verhalten unverändert).
- **Aufruf-Automatik:** Schritt `auto_call` im Sync-Loop nach `auto_assign`,
  derselbe Store-Pfad wie ein Hand-Aufruf (räumt Auto-Halle).
- **Cloud-Takt:** `push_prepared` zusätzlich im 2-s-Takt (Fingerabdruck).
- **UI:** `assets/meetingpoint.html` + `src/io/meetingpoint.mjs` (Inline-Kopie),
  `assets/tl.html`, `assets/tablet.html`, `assets/anzeige.html` +
  `src/io/anzeigeZiel.mjs` (Layout `meetingpoint` + Halle),
  `pages/PreparationPanel.tsx`, `pages/OfficialsPanel.tsx`, `SetupWizard.tsx`.
- **Config** `config.rs` `MeetingpointConfig` (alle Standard aus):
  `enabled=false`, `require_umpire=false`, `strict_auto_assign=false`,
  `auto_call.mode=aus` (Enum mit Auffang), `auto_call.fixed_per_hall=2`,
  `auto_call.horizon_minutes=10`, `auto_call.buffer=1`,
  `auto_call.cap_per_hall=4`, `missing_warn_enabled=false`,
  `missing_warn_minutes=5`, `missing_announce_enabled=false`,
  `missing_announce_minutes=5`, `field_check_enabled=false`. Schalter, die die
  TL zur Laufzeit ändert, in `keep_host_managed_fields`.
- **Datenschutz:** Meetingpoint-Seite und Relay-Ausgabe nur Namen, Spiel-
  bezeichnung, Halle, Flags — keine Lizenznummer, kein Verein, keine Nation, kein
  Geburtsjahr. Wächter `the_state_never_carries_personal_data_beyond_its_purpose`
  wird um `meetingpoint` erweitert; neuer Wächter-Test für `/meetingpoint/state`
  (LAN und Relay).

## Akzeptanzkriterien

**Zustand & Lebenszyklus**
1. Ein Aufruf mit drei Check-ins steht nach App-Neustart unverändert da (Test:
   Store speichern → neu laden).
2. Rücknahme löscht alle Check-ins der Partie.
3. Neuaufruf in dieselbe Halle: Check-ins und erster Aufrufzeitpunkt bleiben.
   Neuaufruf in andere Halle: Check-ins leer, Zeitpunkt neu.
4. BTP ersetzt Spieler auf Seite 1/Pos 0 → dieser Platz ist nicht eingecheckt,
   die anderen bleiben.
5. Spieler X kommt in Partie P1 aufs Feld → X gilt in P2 (gerufen) als
   ausgecheckt; P2 ist nicht spielbereit.
6. Partie kommt aufs Feld → kein aktiver Aufruf, kein BTP-Gelb; nach „Vom Feld
   nehmen" und 3 bestätigten Abrufen: Aufruf aktiv, Check-ins wie vorher.
   BTP-Flackern (1 Abruf geplant) reaktiviert nicht.
7. Partie endet → Eintrag gelöscht; Ergebniskorrektur Finished→Scheduled
   reaktiviert nicht.
8. Turnierwechsel verwirft den Stand; unlesbare Datei → leerer Stand, Warnung im Log.

**Meetingpoint-Seite & Prüfung**
9. `/meetingpoint/state?halle=A` zeigt nur in Halle A aktiv gerufene Partien.
10. Check-in für Partie in Halle B über `?halle=A` → abgelehnt (409), Zustand unverändert.
11. Check-in für nicht existierenden Platz oder nicht gerufene Partie → abgelehnt.
12. Zweiter Tipp auf einen eingecheckten Namen checkt aus.
13. Antwort enthält keine Lizenznummer, keinen Verein, keine Nation (Test LAN + Relay).
14. Cloud: Check-in über `/{ns}/meetingpoint/checkin` erreicht den Host und wird
    bestätigt; antwortet der Host nicht, meldet die Seite „Turnier-PC antwortet
    nicht" (Timeout).
15. Slave-Brücke: Seite und Check-in funktionieren über den Slave-PC
    (Weiterleitungs- und Träger-Betrieb).
16. Ein Check-in löst keinen vollen badhub-Push (`Update::Full`) aus.
17. `meetingpoint.enabled=false` → Routen 404, kein Unterschied zu heute.
18. Anzeige-Hülle: Ansicht „Meetingpoint" mit Halle wählbar; Tipp auf die Seite
    schaltet die Ansicht **nicht** um.

**TL-Sicht**
19. TL-Web und Desktop zeigen je gerufener Partie eingecheckt/gesamt, Fehlende,
    Marke spielbereit; Filter „nur spielbereite" blendet den Rest aus.
20. TL checkt einen Platz von Hand ein/aus (`MeetingpointSet`) → sofort auf der Seite sichtbar (≤ 2 s LAN, ≤ 4 s Cloud).

**Schiedsrichter**
21. Rotation an, Partie gerufen, SR S frei → S vorläufig zugeteilt, **kein**
    BTP-Write. Partie kommt aufs Feld (automatisch) → S im selben Write als SR.
22. Partie kommt per Hand/BTP aufs Feld, auch direkt nach Sync-Neustart → S wird übernommen.
23. SR im Dienst oder in Pause wird nicht vorläufig zugeteilt; wird er frei, geht
    er an die älteste gerufene Partie ohne SR.
24. Rücknahme → vorläufige Zuteilung weg, BTP unberührt.
25. In BTP gesetzter SR verdrängt die vorläufige Zuteilung.
26. `require_umpire` an: Partie mit allen Spielern, SR nicht eingecheckt → nicht
    spielbereit; nach „ohne Schiedsrichter" → spielbereit. `require_umpire` aus:
    alle Spieler genügen.
27. Feldschalter `sr` aus → vorläufiger SR wird verworfen, nicht übernommen.

**Strikter Modus**
28. Strikt an, freies Feld, nur nicht gerufene Spiele → kein Feld vergeben,
    Warnung „Felder frei, keine spielbereite Partie" in der TL.
29. Strikt an, gerufene, nicht spielbereite Partie → kein Feld; nach letztem
    Check-in → Feld im nächsten Lauf.
30. Mehr-Hallen strikt: spielbereite Partie aus Halle A bekommt kein Feld in Halle B.
31. Strikt an, Wunschfeld-Partie nicht spielbereit → Wunschfeld wird anderweitig vergeben.
32. Hand-Vergabe im strikten Modus bleibt möglich.
33. Strikt aus → alle bestehenden `auto_assign`-Tests unverändert grün.

**Aufruf-Automatik**
34. `fest`, N=2, Halle A ohne Aufrufe → 2 Partien gerufen; nach Rücknahme
    einer Partie wird im nächsten Lauf nachgerufen.
35. `restzeit`, 4 Felder belegt, 1 laut Prognose in 5 min frei, X=10, Puffer=1,
    Deckel 4 → Ziel 2.
36. Turnierbeginn, 6 freie Felder, Deckel 4 → 4 Partien gerufen.
37. Partie mit Spieler auf dem Feld/in Pause/ausgenommen wird übersprungen; im
    Mehr-Hallen-Betrieb Partie ohne Halle nicht gerufen.
38. TL-Pause aktiv → keine Aufrufe; Feld-Automatik aus → Aufruf-Automatik läuft weiter.
39. Automatik nimmt nie einen Aufruf zurück; Slave ruft nie.

**Fehlende**
40. Warnung an, 5 min → Partie nicht spielbereit nach 5 min → TL zeigt
    Warnung mit Namen; nach Neuaufruf in dieselbe Halle läuft die Uhr weiter.
41. Ansage an → genau eine Ansage je Partie, nur Fehlende, auch nach Neustart
    nicht doppelt; alte Ansage-Geräte bleiben bei der neuen Art stumm.

**Zähl-Tablet**
42. `field_check_enabled` an, Partie auf dem Feld → Tablet bietet „Ich bin da"
    je Platz; TL zeigt „am Feld bestätigt". Zählen ist jederzeit möglich.
43. Host ohne Fähigkeit (`MatchBrief.feldCheck` fehlt) → Tablet zeigt nichts.

## Tests

- Rust-Unit-Tests je Kriterium (Store, Lebenszyklus, Prüffunktion, `auto_assign`
  strikt, SR-Vorab inkl. „`officials_entries` schreibt nichts für vorläufige SR",
  `auto_call`, Fehlende-Stufen, Serde-Roundtrips aller neuen Wire-Typen,
  Datenschutz-Wächter, Relay Ack/Timeout/`MAX_PENDING`).
- Refactor-PR: Vergleichstest Restzeit/Prognose vor/nach.
- JS: `scripts/test-meetingpoint.mjs` (+ Drift der Inline-Kopie),
  `scripts/test-anzeige-ziel.mjs` erweitert, Ansagetext-Test.
- `npm run build`, `cargo test`, `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt`.

## Umsetzungs-Hinweise (PR-Schnitt)

| PR | Inhalt | Relay-Deploy |
|---|---|---|
| 1 | Store, Aufrufe persistiert, Parken/Reaktivieren, Halle gleich/anders (AK 1–3, 6–8) | nein |
| 2 | Check-in-Zustand, `mp_bereit`, Personenwechsel/Feld anderswo, `MeetingpointSet`, TL-/Desktop-Sicht, Config, Setup (AK 4–5, 16, 19–20) | **vorher** |
| 3 | Meetingpoint-Seite LAN/Relay/Slave, Prüffunktion, 2-s-Push, Anzeige-Hülle, Handbuch-Kapitel (AK 9–15, 17–18) | **vorher** |
| 4 | SR vorläufig, „ohne SR", `require_umpire` (AK 21–27) | ja |
| 5 | Strikter Modus, Wunschfeld, Warnung (AK 28–33) | ja |
| 6a | Refactor Freiwerde-Rechnung → `predict.rs` | nein |
| 6b | Aufruf-Automatik (AK 34–39) | ja |
| 7 | Fehlende: Warnung + Ansage (AK 40–41) | ja |
| 8 | Feld-Check am Zähl-Tablet (AK 42–43) | **vorher** |

Stolperfallen (Details `_intake/…/3-how-to.md`): Merker nie in der
`SyncEngine`; SR-Übernahme zustandsbasiert (kantenbasiert verschluckt der erste
Lauf nach Sync-Neustart sie); geparkte Aufrufe nicht in `preparation_calls()`;
`announceJobs.ts` spricht Stufe 1 als „Zweiter Aufruf" → eigene Ansage-Arten;
`TlAction` hat keinen Auffang und wird vom Relay geparst → Relay zuerst.
Jeder PR: `code-reviewer`; PR 3 zusätzlich `security-reviewer`.

## Risiken & Rollback

- **Öffentliche Schreibroute:** wer die Cloud-Adresse kennt, kann Personen
  einchecken. Gemildert durch Host-Prüfung, Deckel, TL-Korrektur; strikt wirkt nur
  auf die Automatik. Bewusst hingenommen (ADR 0074).
- **Strikter Modus legt Felder still**, wenn niemand ruft → Warnung in der TL.
- **Rollback:** alle Schalter standardmäßig aus; aus → Verhalten wie heute.
  `meetingpoint.json` kann gelöscht werden; fremde/unlesbare Stände verwirft der
  Store. Neue Wire-Felder sind optional; ein alter Host ignoriert neue
  Relay-Frames (Seite meldet Timeout).

## Offene Fragen / Annahmen

- Annahme: 2-s-Abruf der Seite genügt; sonst später WebSocket (Nicht-Ziel).
- Annahme: Eine Hallen-Ansage-Station pro Halle ist für die Fehlenden-Ansage
  angeschlossen; ohne Ansage-Gerät bleibt es bei der TL-Warnung.

## Betroffene Doku-Dateien

Neues Handbuch-Kapitel `docs/meetingpoint.md` (in `docs/handbuch.json`, mit
`pruef`-Ankern) · `docs/preparation.md` · `docs/turnierleitung-web.md` ·
`docs/schiedsrichter-management.md` · `docs/announcements.md` ·
`docs/tablet.md` + `docs/tablet-bedienen.md` · `docs/court-monitor.md`
(Anzeige-Hülle) · `docs/oberflaechen.md` · `docs/einstellungen.md` ·
`docs/multi-hall.md` · `docs/cloud-relay.md` · `docs/spielzeiten-prognose.md` ·
`docs/btp_protocol.md` (P1 nach Neustart) · `docs/features/tl-wunschfeld.md` ·
`docs/changelog.md` · `docs/roadmap.md` · neue Zeile in der CLAUDE.md-Doku-Tabelle.
