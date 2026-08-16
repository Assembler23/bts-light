# 0029 — Hallen-Vorverteilung: eigener turniergebundener Store + `HallSource::Auto`

- **Status:** accepted
- **Datum:** 2026-08-16

## Kontext

Die automatische Hallen-Vorverteilung (Spec
`docs/features/hallen-vorverteilung.md`) setzt Hallen an vielen Spielen je
Turnier. Die naheliegende Ablage — die manuellen Hallen (`spielorte.json`)
mitzubenutzen — scheidet aus: Die Datei ist **global** (nicht
turniergebunden, Key = nackte Match-ID → Kollisionen im nächsten Turnier),
ihr 2000er-Deckel löscht bei Überlauf den **gesamten** Bestand, und
Auto-Zuordnungen wären von echten Hand-Entscheidungen nicht mehr zu
unterscheiden — die Spec verlangt aber genau das (eigenes Badge,
Massen-Rücknahme nur der Auto-Einträge, Aufruf räumt nur Auto).

## Entscheidung

Eigenes Modul `tablet/hall_assign.rs` mit turniergebundenem Store
**`auto-halls.json`** nach dem ADR-0022-Muster (Turnier-Kopf,
`Ladung{Stand,Leer,Unlesbar}`, begrenzte Ladeversuche, atomares Schreiben,
`generation`-Zähler). Die Kaskade `assign::hall_for_match` bekommt eine
neue **letzte** Stufe mit eigener Herkunft **`HallSource::Auto`** (Wire
`"auto"`): Regel → Hand → BTP → Aufruf → Auto → keine. Idempotenz über
**Insert-only** (`insert_many` überschreibt nie): Ein Verteil-Lauf mit
unverändertem Input erzeugt keine Änderung, keine Persistenz und keine
TL-Revision — ein Fingerprint-Mechanismus ist unnötig.

## Alternativen

- **`spielorte.json` mitbenutzen:** einfachste Umsetzung, aber global,
  kollisions- und total-löschgefährdet, Herkunft verschleiert. Verworfen.
- **Nur RAM:** verletzt den Persistenzanspruch („fest = fest" muss einen
  Neustart überleben). Verworfen.
- **Fingerprint-Vergleich gegen Revisions-Flut:** durch Insert-only
  überflüssig. Verworfen.

## Konsequenzen

- Sechs Kaskaden-Aufrufer und `LivetickerContext` wachsen um einen
  Parameter (Compiler erzwingt Vollständigkeit).
- Turnierwechsel räumt automatisch; alte App-Versionen ignorieren die
  Datei (Rollback gefahrlos).
- Alte `tl.html`-Stände tolerieren den unbekannten Wire-Wert `"auto"`
  (geprüft: sie vergleichen nur gegen bekannte Werte).
