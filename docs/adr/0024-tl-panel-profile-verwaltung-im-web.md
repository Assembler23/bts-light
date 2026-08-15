# 0024 — Panel-Profil-Verwaltung direkt in tl.html, zweite Ausnahme von der Setup-Regel

- **Status:** accepted
- **Datum:** 2026-08-15

## Kontext

`docs/features/turnierleitung-web.md` legt fest: „Kein Setup und keine
Einstellungen aus dem Web — BTP-Verbindung, Verbindungsmodus, Passwörter,
Gerätekopplung bleiben in der Desktop-App", mit genau einer benannten
Ausnahme (Auto-Vergabe-Schalter). [TL-Web-Panelsystem](../features/tl-web-panelsystem.md)
führt benannte Profile ein (Panel-Sichtbarkeit/-Reihenfolge/-Höhe + die
bisherigen Anzeige-Einzelschalter), die Turnierleiter am Turnierort anlegen,
bearbeiten und wechseln müssen — typischerweise spontan, während ein
Wandmonitor oder ein zusätzliches Tablet in Betrieb genommen wird, nicht am
Rechner mit der Desktop-App.

Der Grill-Schritt zu diesem Feature (`docs/features/_intake/tl-web-panelsystem/2-grill.md`,
Blocker 1) hat diesen Konflikt mit der bestehenden Nicht-Ziel-Klausel explizit
benannt und musste vor der Spec-Finalisierung geklärt werden.

## Entscheidung

Profil-Verwaltung (Anlegen/Bearbeiten/Löschen/Wählen/als-Standard-markieren)
läuft direkt in `tl.html`, über den bestehenden `TlAction`-Kanal (wie
Schiedsrichter-Pflege, Feldvergabe-Ausnahme) — als zweite, bewusste Ausnahme
von der „kein Setup aus dem Web"-Regel. Die Nicht-Ziel-Klausel der
freigegebenen TL-Web-Spec wird entsprechend ergänzt.

Begründung für die Ausnahme, in Abgrenzung zur bestehenden Regel: Profile sind
reine Darstellungs-/Layout-Präferenzen ohne Sicherheitsrelevanz — anders als
BTP-Verbindung, Passwörter oder Gerätekopplung berühren sie weder
Netzwerkzugänge noch Turnierdaten noch Geheimnisse. Ein falsch angelegtes
Profil hat höchstens eine unpraktische Darstellung zur Folge, kein
Sicherheits- oder Datenrisiko.

Da damit eine neue Schreib-Oberfläche über eine internetseitige,
tokengeschützte Seite entsteht, ist `security-reviewer` für die Umsetzung
verbindlich (siehe Umsetzungs-Hinweise der Spec).

## Alternativen

- **Verwaltung im React-Setup-Wizard** (wie das Hallen-Raster,
  `FieldOverviewPage.tsx`): konsistent mit der bestehenden Regel, aber TL
  müsste für jede Profil-Änderung an den Turnier-Rechner — widerspricht dem
  eigentlichen Zweck des Features (spontane, geräteseitige Anpassung während
  des laufenden Betriebs).
- **Nur Lesen in tl.html, Schreiben ausschließlich im Setup-Wizard**:
  verworfen — hätte denselben Bedienungsnachteil wie oben, nur für die
  Erst-Einrichtung statt für alle Änderungen.

## Konsequenzen

- Die Nicht-Ziel-Klausel in `docs/features/turnierleitung-web.md` bekommt
  eine zweite benannte Ausnahme.
- `security-reviewer` ist für Schritt „Profil-Verwaltungs-UI" der
  Umsetzung verbindlich vorgeschrieben, nicht optional.
- Künftige Features, die ähnliche geräteseitige Konfiguration brauchen,
  können sich auf dieses Präzedens-ADR berufen, statt die Grundsatzfrage
  erneut aufzurollen — sollten aber jeweils selbst prüfen, ob ihre
  Konfiguration wirklich sicherheitsneutral ist wie hier.
