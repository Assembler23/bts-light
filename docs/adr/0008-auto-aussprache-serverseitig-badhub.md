# ADR 0008 — Automatische Aussprache-Vorschläge für Turnier-Namen entstehen serverseitig bei badhub (opt-in), nicht in der Desktop-App

Status: vorgeschlagen

## Kontext

Die Feld-/Vorbereitungs-Ansage spricht Spielernamen. Für ungewohnte Namen ist
die Aussprache schwach: Die deutsche Web-Speech-Stimme (Offline-Default)
buchstabiert ungewohnte Graphemfolgen (belegt: „Chybych" wurde komplett
buchstabiert); die Azure-Neural-Stimme ist deutlich besser, patzt aber bei
Einzelfällen (falsche Betonung/Vokale) und braucht ohne Hinweis mitunter eine
`<phoneme>`- oder `<lang>`-Korrektur.

Es gibt bereits drei Korrektur-Ebenen (siehe [`../announcements.md`](../announcements.md)):
mitgeliefertes **Basis-Wörterbuch** (frequenzbasierte häufige Fremdnamen),
**lokale Nutzer-Tabelle** (Einzelfall, Vorrang) und ein **geteiltes
Community-Wörterbuch** bei badhub (`/api/v1/pronunciations`, opt-in). Seit
v0.9.167 wirkt die phonetische Ersatzschreibweise (`say`) auf **beiden**
Stimmen (vorher nur Web-Speech).

Alle drei Ebenen sind **manuell gepflegt**. Der Wunsch: die Aussprache **für
alle Spieler:innen eines Turniers automatisch** verbessern, ohne dass jemand
jeden Namen von Hand nachschlägt (heute googelt der Turnierleiter IPA einzeln).

Kräfte / Randbedingungen:

- **Namensquelle:** BTP liefert den kompletten Roster; bts-light hat die Namen
  bereits. badhub bekommt dieselben Namen ohnehin für den Liveticker und
  **hostet bereits das geteilte Aussprache-Wörterbuch**.
- **Der einzige Mechanismus, der beliebige Namen „versteht", ist ein
  Sprachmodell/G2P-Dienst** — genau das, was heute der Mensch per Hand mit einer
  KI tut. Ein LLM ist ein **neuer Auftragsverarbeiter** (Datenschutz: CLAUDE.md
  „keine sensiblen Daten in externen APIs ohne Prüfung"; Spielernamen nur im
  Liveticker-Zweck).
- **Datenschutz-Asymmetrie:** Namen liegen bei badhub bereits vor (Liveticker).
  Würde die **Desktop-App** selbst ein LLM anrufen, entstünde ein **neuer**
  Datenabfluss aus **jeder** Installation an einen Dritten.
- **Kosten & Nutzen:** Zentrale Generierung teilt Ergebnisse über alle Turniere
  (Netzwerkeffekt — das Wörterbuch wird mit jedem Event besser); pro-Installation
  entstünden Kosten/Key-Verwaltung mehrfach und ohne Teilen.
- **Ohne Entscheidung** bleibt es beim manuellen Nachpflegen, oder es entsteht
  wildwuchsartig ein direkter LLM-Aufruf in der App (schlechteste Datenschutz-
  und Kostenvariante).

## Entscheidung

Automatische Aussprache-**Vorschläge** entstehen **serverseitig bei badhub**,
nicht in der bts-light-Desktop-App. Konkret:

1. badhub erweitert das bestehende Aussprache-Wörterbuch: Für Namen, die noch
   **nicht** im Lexikon stehen, erzeugt der Server **einmalig** über einen
   LLM-/G2P-Dienst einen Vorschlag mit **IPA** *und* deutscher
   **Ersatzschreibweise (`say`)** und speichert ihn.
2. bts-light bleibt **reiner Konsument**: Es lädt das Wörterbuch wie heute
   (offline gecacht) und wendet es über die bestehende Korrektur-Pipeline an
   (`say` wirkt seit v0.9.167 auch auf Azure). **Kein** LLM-Aufruf aus der App.
3. **Opt-in & Transparenz:** Automatische Vorschläge sind als solche markiert
   (Herkunft „auto"), von Menschen bestätigte/korrigierte Einträge haben Vorrang.
   Die Nutzer-Tabelle (lokal, Vorrang) übersteuert jederzeit.
4. **Datenschutz:** Es werden nur **Namen** (öffentliche Wettkampfdaten, bei
   badhub ohnehin vorhanden) an den Generierungs-Dienst gegeben — **kein**
   Geburtsjahr, keine sonstigen Personendaten (CLAUDE.md). Der Dienst ist als
   Auftragsverarbeiter zu behandeln (AVV/Standort EU prüfen).

Die **Umsetzung** ist ein Feature im **badhub-Repo** und beginnt erst nach
Freigabe dieses ADR. bts-light-seitig ist nichts weiter nötig (die Pipeline
existiert).

## Alternativen

- **Desktop-App ruft selbst ein LLM auf (Weg 2):** verworfen — neuer
  Datenabfluss aus jeder Installation, Kosten/Key pro Gerät, kein Teilen der
  Ergebnisse, Datenschutz schwerer zu kontrollieren. Auf jeder Achse schlechter
  als die serverseitige Variante.
- **Offline-G2P (espeak-ng) in der App bündeln (Weg 3):** verworfen als
  Primärweg — datenschutzsauber, aber Namensqualität mäßig, Sprache muss geraten
  werden, schlägt Azures eigene G2P nicht klar. Bleibt als optionaler
  Offline-Zusatz denkbar, löst das Kernproblem aber nicht.
- **Nur weiter manuell pflegen (Status quo):** verworfen als alleiniger Weg —
  skaliert nicht auf einen ganzen Roster; der Turnierleiter müsste weiter jeden
  schwierigen Namen einzeln nachschlagen.
- **Individuelle Namen ins mitgelieferte Basis-Wörterbuch aufnehmen:**
  verworfen — das Basis-Wörterbuch ist bewusst **frequenzbasiert** (häufige
  Fremdnamen), kein Ablageort für einzelne reale Personen; das wäre ein
  dauerhafter Personendaten-Footprint im öffentlichen Repo und widerspräche dem
  Liveticker-Zweck. Einzelfälle gehören in die lokale Tabelle bzw. den opt-in
  Community-Sync.

## Konsequenzen

- **Positiv:** Ganze Roster werden ohne Handarbeit besser; Ergebnisse teilen
  sich über alle Turniere (das Wörterbuch wächst); die Desktop-App bleibt
  schlank und ohne neuen externen Aufruf; die Datenschutzgrenze verschiebt sich
  **nicht** (Namen sind bei badhub bereits für den Liveticker).
- **Kosten/Grenzen:** Ein LLM-/G2P-Dienst als **Auftragsverarbeiter** muss
  datenschutzrechtlich sauber eingebunden werden (AVV, EU-Standort, Zweckbindung
  „nur Namen"). Generierte Aussprachen sind **Vorschläge** — nicht immer korrekt;
  deshalb Herkunfts-Markierung und Vorrang menschlicher Korrekturen. Serverseitige
  Kosten (LLM-Aufrufe) fallen bei badhub an, wenn auch einmalig je neuem Namen.
- **Abhängigkeit:** Die eigentliche Arbeit liegt im **badhub-Repo**; bts-light
  hängt nur am bestehenden `/api/v1/pronunciations`-Format (ggf. um ein
  Herkunfts-/`ipa`-Feld erweitert — abwärtskompatibel via Default).
- **Reliabilitäts-Vorbehalt:** Bevor in Auto-Generierung investiert wird, ist zu
  klären, ob die schlechte Aussprache am Wochenende primär daran lag, dass
  **Azure gar nicht durchlief** (Rückfall auf die buchstabierende Offline-Stimme,
  Banner `reportAzureFallback`). Wenn ja, ist Azure-Zuverlässigkeit der größere
  Hebel — eine Aussprache-Datenbank hülfe dann nur begrenzt. Diese Frage sollte
  **vor** der Umsetzung beantwortet sein.
- **Neu bewerten, wenn:** ein offline-fähiges G2P mit ausreichender Namensqualität
  verfügbar wird (dann Weg 3 als datenschutzfreier Ersatz prüfen), oder wenn der
  Generierungs-Dienst datenschutzrechtlich nicht sauber einzubinden ist (dann
  bleibt es beim manuellen Community-Wörterbuch).
