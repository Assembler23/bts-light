# 0051 — Offene Spiele reisen als eigene, zuerst gekappte Liste

- **Status:** accepted
- **Datum:** 2026-08-30

## Kontext

Die Turnierleitungs-Oberfläche soll auch Spiele zeigen, deren Teilnehmer noch
nicht feststehen (Spec [`tl-offene-paarungen`](../features/tl-offene-paarungen.md)).
Sie sollen **eingereiht** an ihrer regulären Sortierposition erscheinen, und ein
Schalter je TL-Gerät soll sie ausblenden können.

Beides kollidiert mit zwei harten Grenzen des bestehenden Systems:

1. **Ein Zustand für alle Geräte.** `tl_state`, `push_tl_state` und
   `state_for_relay` bauen genau **einen** `TlState` mit einer geteilten
   Revision (`TlStateCache`). Das aktive Panel-Profil reist nur als
   `X-Tl-Active-Profile`-Header mit. Ein geräteabhängiger Zustand würde Cache
   und Revisionslogik brechen.
2. **64 KiB über die Cloud.** `MAX_TL_STATE_LEN` (`relay-proto/src/lib.rs:2452`)
   deckelt den Frame; `tl.rs:833` kappt die Warteliste dafür über die Stufen
   `[40, 20, 10, 5]`. Im dokumentierten Worst Case (26 belegte Felder, 30
   Ergebnisse, `docs/cloud-relay.md:355`) liegt der Zustand bereits bei
   62 467 B und die Warteliste steht dort schon auf Stufe 5. Die Doku hält
   ausdrücklich fest: „die verbleibende Reserve trägt keine zweite
   Erweiterung."

Naiv in `queue` eingereihte offene Spiele würden also in einem KO-Turnier genau
die echten Arbeitsspiele aus dem 40er-Fenster verdrängen — und weil der
Schalter nur clientseitig wirken kann, holte „Schalter aus" sie nicht zurück.
Ein TL-Gerät sähe nach dem Update **weniger** echte Spiele als vorher.

## Entscheidung

Offene Spiele reisen in einer **eigenen, schlanken Liste** im `TlState`, neben
`queue` statt darin. Sie trägt nur die Felder, die eine offene Zeile braucht
(Identität, Nummer, Zeit, Draw/Runde/Klasse, Disziplin, die beiden
Platz-Labels, Halle) — rund 230–320 B statt 530–890 B je Eintrag.

Zwei Regeln machen sie unschädlich für den Bestand:

- **Deckel am Host** (`OPEN_QUEUE_LIMIT`) begrenzt sie schon beim Bauen.
- **Adaptive Füllung über die Cloud, als letzter Zusatz:** `state_for_relay`
  legt `open_queue` zuerst beiseite, lässt die bestehende Leiter
  `[40, 20, 10, 5]` **unverändert** die Stufe der echten Warteliste bestimmen
  und füllt danach die offene Liste über eine eigene Leiter
  `[40, 20, 10, 5, 0]` so weit auf, wie das 64-KiB-Fenster es noch hergibt.

Ein **fester** Zahlenwert wäre hier falsch: Im Worst-Case-Fixture liegt der
Zustand nach Opferung von `time_stats` bereits bei 55 286 B (`tl.rs:5947`) —
dort ist für keine feste Zahl Platz, während ein mittleres Turnier mühelos 100
offene Einträge trüge. Die adaptive Variante macht das Erfolgskriterium „kein
TL-Gerät sieht weniger echte Wartelisten-Spiele als vorher" außerdem
**beweisbar**: derselbe Zustand einmal mit und einmal ohne offene Spiele muss
dieselbe `queue.len()` ergeben.

Die Sichtbarkeit entscheidet der **Client**: `assets/tl.html` mischt beide
Listen beim Rendern und blendet sie bei ausgeschaltetem Schalter aus. Die
Mischposition kommt dabei **vom Host** — jeder offene Eintrag trägt
`queue_index` = „wie viele echte Wartelisten-Spiele stehen vor mir". Der Client
fügt an dieser Position ein, statt selbst zu sortieren; `tl.html:6089` sagt
ausdrücklich zu, dass die Sortierung serverseitig verbindlich ist, und diese
Zusage bleibt wörtlich erhalten.

## Alternativen

- **Gemeinsames Budget** — offene Spiele zählen wie jeder andere Eintrag gegen
  die 40 Relay-Plätze. Ergäbe exakt das BTP-Bild, verdrängt aber über die Cloud
  echte Arbeitsspiele. Verworfen: Ein Anzeige-Feature darf die Arbeitsliste
  nicht beschneiden.
- **Deckel anheben / `MAX_TL_STATE_LEN` vergrößern** — löst das Verdrängen,
  belastet aber jeden Push-Takt und die Renderzeit auf schwachen Tablets und
  verlangt eine eigene Messreihe. Verworfen als unnötig für ein Feature, dessen
  Inhalt in gekürzter Form vollständig transportierbar ist.
- **Profilabhängiger Zustand serverseitig** — der Host baut je Profil einen
  eigenen `TlState`. Wäre die sauberste Semantik (Namen erreichten nur Geräte,
  die sie anzeigen), bricht aber `TlStateCache` und die eine geteilte `rev` und
  vervielfacht die Bauten je Takt. Verworfen als unverhältnismäßig.

## Konsequenzen

- Die echte Warteliste bleibt in jeder Situation vollständig; kein TL-Gerät
  sieht nach dem Update weniger echte Spiele als vorher.
- **Negativ, bewusst getragen:** Die Kandidaten-Labels reisen unabhängig vom
  Schalter im Zustand mit, auch zu Geräten, die sie nicht anzeigen. Deshalb
  tragen sie keine Lizenznummern und keine Spieler-IDs (siehe
  [ADR 0052](0052-beschriftung-offener-plaetze.md)).
- **Negativ:** Unter Frame-Druck verschwinden die offenen Spiele zuerst und
  ohne Vorwarnung — genau das Verhalten, das ein Anzeige-Extra haben soll, aber
  der Nutzer sieht auf dem Cloud-Gerät dann weniger als auf dem LAN-Gerät.
- Die Liste ist ein reines Transport-Detail: Wer die Anzeige-Reihenfolge ändert,
  fasst nur `tl.html` an, nicht den Zustand.
- Kein `relay-proto`-Eingriff — der TL-Zustand reist als opakes JSON.
