# 0030 — Die Halle bindet die automatische Feldvergabe (Constraint + Aufruf-Ersatz)

- **Status:** accepted
- **Datum:** 2026-08-16

## Kontext

Vor diesem ADR prüfte die automatische Feldvergabe (`sync.rs::auto_assign`)
Hallen-Zuordnungen kaum: Nur die Disziplin/Klasse→Halle-Regel wirkte als
Constraint; von Hand gesetzte Hallen waren reine Anzeige. Die
Hallen-Vorverteilung (Spec `docs/features/hallen-vorverteilung.md`) will
Spielern aber ein **Versprechen** geben („dein Spiel läuft in Halle B —
geh schon mal rüber"). Ein Versprechen, das die Vergabe ignorieren darf,
ist Beschilderungs-Theater.

## Entscheidung

Im **Mehr-Hallen-Betrieb** ist die aufgelöste Halle eines Spiels ein
**hartes Feld-Constraint** der automatischen Vergabe, wenn ihre Herkunft
**Regel, Hand oder Auto** ist: Das Spiel bekommt nur Felder seiner Halle.
Für **Auto**-vorverteilte Spiele **ersetzt die Vorverteilung zusätzlich
die Aufruf-Pflicht** (`require_call`): Sie werden in ihrer Halle auch ohne
Vorbereitungs-Aufruf vergeben — das frühe Hallen-Signal (Monitor/badhub)
tritt an die Stelle des Aufrufs; ein tatsächlicher Aufruf bleibt möglich
und räumt die Auto-Zuordnung (E3). Spieler-Prüfungen (Mindestpause,
läuft-gerade) bleiben unangetastet.

## Alternativen

- **Nur Auto bindend:** vermeidet die Verhaltensänderung für Hand-Hallen —
  aber dann bliebe die Hand-Halle eine Falschauskunft gegenüber Spielern,
  die ihr folgen (genau der Missstand, den der Grill aufgedeckt hat).
  Verworfen (bewusster Nutzer-Entscheid E1).
- **Auch BTP-`LocationID` bindend:** riskiert Verhungern bei Turnieren mit
  veraltet gepflegter Spielort-Spalte; von E1 nicht gedeckt. Verworfen.
- **Anzeige-only (kein Vergabe-Umbau):** verfehlt das Feature-Ziel.
  Verworfen.
- **Zusatz-Schalter „Hallen binden ja/nein":** dritte Betriebsart,
  vergrößert die Testmatrix und entwertet die Vorverteilung. Verworfen.

## Konsequenzen

- **Verhaltensänderung für Bestands-Turniere:** Hand-/Regel-Hallen binden
  jetzt wirklich — ein Spiel mit Hand-Halle B wartet auf ein B-Feld, auch
  wenn A frei ist. Rückweg je Spiel: Hand-Halle auf „–". Prominent in
  Changelog und `docs/turnierleitung-web.md` ausgewiesen; der Umbau ist
  ein eigener, einzeln revertierbarer Commit.
- Spiele können ohne Vorbereitungs-Ansage aufs Feld kommen (dokumentiert).
- Der `multi_hall`-Guard ist zwingend: In Ein-Hallen-Turnieren wäre jeder
  gesetzte Hallenname sonst eine Vergabe-Sperre.
