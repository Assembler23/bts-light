# 0010 — Schreibender Cloud-Pfad für Turnierleitungs-Geräte: Whitelist fixierter Aktionen, der Host validiert

- **Status:** proposed
- **Datum:** 2026-08-07

Gehört zur Spezifikation
[docs/features/turnierleitung-web.md](../features/turnierleitung-web.md).
Die Geräte-Identität ist Gegenstand von
[ADR 0011](0011-tl-web-geraete-identitaet.md) — beide Entscheidungen
bedingen einander.

## Kontext

Die Turnierleitungs-Weboberfläche („TL-Web") soll aus dem Internet bedienbar
sein: Felder belegen, umhängen, freigeben, Spiele aufrufen, Ergebnisse
nachtragen, Walkover werten, die Zähltafelbediener-Warteschlange pflegen und
die automatische Feldvergabe umschalten.

Das durchbricht eine bewusste Grenze. Der Cloud-Pfad ist bis heute für
Fremdgeräte **schreibend nur in einer einzigen, eng validierten Form**
geöffnet: ein Tablet meldet den Spielstand und das Ergebnis seines eigenen
Courts, und `process_result` prüft jede Meldung gegen das Court-Match (R5).
Alles andere ist read-only — der Cloud-Slave ausdrücklich (R4).

Kräfte und Randbedingungen:

- **Der Relay ist bis heute ein Broker ohne Fachwissen.** Er kennt weder
  Matches noch Felder noch Turnierregeln; er reicht Frames durch und hält
  opaken Zustand. Diese Unwissenheit ist ein Merkmal, kein Mangel: sie hält
  die Turnierlogik an genau einer Stelle, dem Host.
- **Der Relay hat kein Auth-Konzept.** Jede Namespace-Route ist bewusst
  tokenfrei; die Namespace-UUID ist das einzige Zugangsmerkmal
  (`relay/src/main.rs:616-619,1019-1022`).
- **`RelayFrame` kennt heute ausschließlich tablet-gebundene Nachrichten**
  (`TabletConnected`, `TabletDisconnected`, `ScoreUpdate`, `Result`,
  `Battery`, `Alert`). Es gibt keinen Client-Typ „Turnierleitung" und keinen
  Rückkanal vom Slave zum Host.
- **Der Präzedenzfall ist bereits geplant:** Für eine **einzige** Aktion
  (Vorbereitungs-Aufruf vom Slave) fordert Roadmap-Plan 1 Stufe 2 ein
  eigenes ADR. Hier geht es um rund ein Dutzend Aktionen.
- **ADR 0009 hat den Relay für Web-Clients ausdrücklich verworfen:**
  „Öffentliche Web-Clients in unbekannter Zahl passen weder zum Sicherheits-
  noch zum Betriebsmodell." Diese Begründung ist zu prüfen, nicht zu
  umgehen.
- **Das Relay-Binary ist global.** Ein Deploy trifft alle Installationen,
  auch Turniere, die TL-Web nie einschalten.

## Entscheidung

**Der Schreibkanal ist eine Whitelist namentlich fixierter Aktionen; der
Host validiert und entscheidet, der Relay leitet nur weiter.**

Konkret:

1. **Geschlossener Aktionssatz.** `TlAction` ist ein Rust-Enum mit
   Serde-Tag; unbekannte Aktionen sind nicht darstellbar. Der Satz steht in
   der Spec und wächst nur durch eine bewusste Änderung beider Seiten.
2. **Der Relay bleibt fachlich blind.** Er prüft ausschließlich Token,
   Namespace-Zuordnung und Gerätegrenze. Er kennt weder Feldbelegung noch
   Match-Zustand und trifft keine Entscheidung über eine Aktion.
3. **Der Host validiert jede Mutation** in einem gemeinsamen Modul
   (`tablet/tl.rs`), das LAN- und Cloud-Pfad sich teilen — exakt das Muster
   von `process_result` (R5). Es gibt keinen Weg, der an dieser Validierung
   vorbeiführt.
4. **Antwort synchron über das erprobte Korrelations-Muster:** Der Relay
   hält eine wartende Anfrage unter einer `req_id`, schickt ein
   `TlCommand`-Frame an den Host und beantwortet den HTTP-Request mit
   dessen `TlAck`. Das ist derselbe Mechanismus, über den heute
   Ergebnismeldungen quittiert werden. Das auslösende Gerät bekommt damit
   eine **echte** Quittung nach dem BTP-Schreiben, kein Fire-and-forget.
5. **Höchstens 8 tokenauthentisierte Turnierleitungs-Geräte je Namespace**,
   im Relay erzwungen; veraltete Plätze werden **vor** der Grenzprüfung
   geräumt, damit ein einmal weggefallenes Gerät kein echtes aussperrt.
   Bewusst **kein** stilles Verdrängen des ältesten Geräts: eine klare
   Fehlermeldung ist im Turnierbetrieb besser als ein Gerät, das grundlos
   nicht mehr reagiert.
6. **TL-Geräte sind eine dritte Client-Klasse.** Sie landen nie in der
   Tablet-Liste eines Namespace, übernehmen nie eine Court-Session und
   senden nie Tablet-Nachrichten. **R4 in `CLAUDE.md` wird um genau diesen
   Satz erweitert** — die Regel „ein aktives Tablet je Court" bleibt
   unangetastet.
7. **Ohne ausdrückliches Opt-in am Host ist der Pfad unerreichbar.** Der
   Relay kennt TL-Geräte ausschließlich über die vom Host gepushten Tokens;
   ein Host mit ausgeschaltetem Feature pusht keine → jede Anfrage wird
   abgewiesen, **bevor** neuer Code Zustand berührt. Zusätzlich ein
   Not-Aus per Umgebungsvariable, der die Routen gar nicht erst
   registriert.

**Abgrenzung zu ADR 0009.** Dort ging es um *öffentliche* Clients in
*unbekannter* Zahl ohne Authentifizierung — Spieler-Handys, die eine
Check-In-Seite aufrufen. Hier geht es um höchstens acht Geräte, die einzeln
vom Turnier-PC ausgestellt, gezählt, jederzeit widerrufbar und dem
Turnierleitungs-Team zuzurechnen sind. Das ist ein anderes Betriebsmodell,
und die damalige Begründung trägt hier nicht.

## Alternativen

**(a) Generischer Command-Kanal mit Rollenmodell im Relay.** Der Relay
kennte Rollen und entschiede, wer was darf.
*Verworfen:* Das trüge Turnierlogik in den Broker und weichte damit die
R5-Mitigation auf — die Stelle, an der heute jede Fremdeingabe validiert
wird, wäre nicht mehr die einzige. Zudem müsste der Relay bei jeder neuen
Aktion mitwachsen, obwohl er sie fachlich nicht beurteilen kann.

**(b) Über das badhub-Backend, nach dem Muster von ADR 0009.** TL-Web
authentifizierte sich bei badhub; die Befehle liefen von dort weiter.
*Verworfen:* Erzeugt eine Cross-Repo-Kopplung, die die Spec ausdrücklich
ausschließt (die Seite lebt im bts-light-Repo, ein Deploy). Die Befehle
müssten trotzdem durch den Relay zum Host — man hätte einen Umweg mehr,
nicht einen weniger. Und der reine LAN-Betrieb bräuchte Internet.

**(c) Keine Cloud-Schreibrechte, TL-Web nur im Hallennetz.**
*Verworfen:* Das strich die ausdrückliche Anforderung „auch aus dem
Internet bedienbar" und den belegten Engpass des Zwei-Hallen-Turniers.

**(d) WebSocket je TL-Gerät statt HTTP mit Korrelation.**
*Verworfen:* Acht Handys und Tablets im Standby halten keine dauerhaften
Verbindungen; es bräuchte eine weitere Client-Klasse mit eigenem
Lebenszyklus im Broker — genau der Code, der bei den Tablets die meisten
Turnierbefunde erzeugt hat (Reconnect, Zombie-Sessions, veraltete Stände).
Der HTTP-Weg heilt sich nach Standby von selbst.

## Konsequenzen

**Positiv**

- Die Turnierlogik bleibt an einer Stelle; der Relay bleibt ein Broker.
- Jede Mutation existiert genau einmal und wird genau einmal validiert —
  LAN und Cloud teilen sie sich, wie bei den Ergebnissen.
- Der neue Pfad ist ohne Opt-in unerreichbar; Turniere ohne TL-Web sind
  vom Relay-Deploy unberührt.
- Der geschlossene Aktionssatz ist überschaubar und einzeln testbar.

**Negativ / Kosten**

- Jede neue Aktion braucht eine Protokolländerung auf beiden Seiten —
  bewusst, aber es bremst.
- Das Relay-Binary wächst um eine Client-Klasse, eine Token-Map und vier
  Routen; der Broker ist danach nicht mehr ganz so schmal.
- Der Relay hält künftig Spielernamen im Arbeitsspeicher (wie heute schon
  bei den gerufenen Spielen) — nur im RAM, gekappt, nie protokolliert.
- R4 muss in `CLAUDE.md` erweitert werden; die Regel ist danach länger zu
  erklären.
- Die Antwortzeit einer Aktion hängt an der Kette Browser → Relay → Host →
  BTP. Die Oberfläche muss den Wartezustand ehrlich zeigen, statt Erfolg
  vorzutäuschen.
