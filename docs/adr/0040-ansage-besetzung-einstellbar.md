# 0040 — Ob Bedienung und Schiedsrichter angesagt werden, entscheidet ein Schalter, nicht die Herkunft des Namens

- **Status:** accepted (umgesetzt 20.08.2026)
- **Datum:** 2026-08-20

Löst [ADR 0007](0007-zaehltafelbediener.md) ab.
Gehört zu [docs/announcements.md](../announcements.md),
[docs/zaehltafelbediener.md](../zaehltafelbediener.md) und
[docs/schiedsrichter-management.md](../schiedsrichter-management.md).

## Kontext

Ein Turnierleiter meldete am 20.08.2026: Bei der automatischen Feldansage fehlen
Schiedsrichter **und** Zähltafelbedienung, obwohl beides eingetragen ist; beim
zweiten Aufruf ist der Schiedsrichter dabei, die Bedienung weiterhin nicht. Der
Befund führte auf zwei verschiedene Ursachen.

**Erstens** bauten zwei Wege ihren Ansagetext getrennt zusammen. Der manuelle
Aufruf, jeder Nachruf und die zweite/dritte Aufrufstufe laufen über
`announceCourt` (`src/io/announceCourt.ts`) — dort reisten Schiedsrichter und
Aufschlagrichter längst mit. Die **automatische** Ansage neu belegter Felder
(`src/components/MatchAnnouncer.tsx`) setzte ihr `AnnounceMatchInput` dagegen
selbst zusammen und ließ beide Felder schlicht weg. Dieselbe Belegung klang
also unterschiedlich, je nachdem, wer die Ansage auslöste. Das ist ein Fehler
ohne Entwurfsfrage und wurde geradegezogen.

**Zweitens** war die Bedienung eine bewusste Entscheidung: ADR 0007 sagte nur
den beim Aufruf **zugewiesenen** Bediener an und verschwieg den älteren
pro-Feld-Hinweis („Verlierer des zuletzt auf diesem Feld beendeten Spiels").
Die Begründung war real und bleibt gültig: Ein Ansage-Gerät baut seinen
Feld-Stand lokal auf; auf einem LAN-Ansage-Slave mit ausgeschalteter
Bediener-Verwaltung — für einen reinen Ansage-PC der Normalfall — räumt der
Sync-Lauf die Zuweisungen und füllt den Hinweis weiter (Review 18.08.2026).
Ohne die Prüfung riefe die Anlage dort den Verlierer des Vorspiels aus.

Die Kehrseite war aber ebenso real: Turniere, die **nur** mit dem
pro-Feld-Hinweis arbeiten, hörten nie eine Bedienungs-Ansage, obwohl der Name
auf dem Bildschirm stand. Für den Turnierleiter sah das nach einem Defekt aus,
nicht nach einer Regel — die Regel war nirgends bedienbar.

## Entscheidung

Zwei Schalter in `AnnounceConfig` (`src-tauri/src/config.rs`), beide
standardmäßig **an**:

- `announce_scorekeeper` — Zähltafelbedienung mit ansagen. Ist er an, wird
  angesagt, **was am Feld steht**: zugewiesener Bediener *oder* pro-Feld-Hinweis.
  Die Herkunftsprüfung `scorekeeper_assigned` entfällt für die Ansage; sie
  bleibt für die **Anzeige** erhalten.
- `announce_umpire` — Schiedsrichter und Aufschlagrichter in der Feldansage
  nennen.

Beide wirken auf **alle** Feldansagen: automatisch, manuell, Nachruf, jede
Aufrufstufe, LAN wie Cloud-Slave. Unberührt bleiben die ausdrücklichen Knöpfe
„SR/AR ansagen" (`announceOfficials`) und der Bedienungs-Nachruf aus der
Spielübersicht — wer sie drückt, will genau diese Ansage.

## Alternativen

**ADR 0007 unverändert lassen und nur den Schiedsrichter-Fehler beheben.**
Verworfen: Der gemeldete Schmerz — „die Bedienung wird nicht gerufen, obwohl
sie dasteht" — bliebe bestehen, und zwar unbehebbar, weil nicht bedienbar.

**Die Ansage an die Bediener-Verwaltung koppeln (Verwaltung an ⇒ ansagen).**
Verworfen: Das ist genau die heutige, unsichtbare Kopplung in neuer Form. Ein
Turnierleiter, der die Verwaltung nicht nutzt, hätte weiterhin keinen Weg zur
Ansage — und die Kopplung wäre nirgends erklärt.

**Nur ein gemeinsamer Schalter für Bedienung und Schiedsrichter.** Verworfen:
Die beiden Angaben haben verschiedene Adressaten. Ein Turnier ohne
Schiedsrichter, aber mit Bedienung ist der Normalfall, nicht die Ausnahme.

**Drei Stufen statt an/aus (aus · nur zugewiesene · alle).** Verworfen (YAGNI):
Die mittlere Stufe ist genau das alte Verhalten, für das sich niemand
entschieden hatte — sie war eine Folge der Umsetzung. Wer den pro-Feld-Hinweis
nicht ausgerufen haben will, schaltet die Bedienungs-Ansage aus.

## Konsequenzen

- Bestandsinstallationen bekommen beide Felder per `#[serde(default)]` still
  dazu und hören ab dem Update **mehr**, nicht weniger: Der pro-Feld-Hinweis
  wird ab jetzt mit ausgerufen. Das ist gewollt und der Punkt der Änderung —
  wer es anders will, hat jetzt einen Schalter. Der Test
  `announce_block_without_new_switches_defaults_to_on` hält den Default fest.
- Die in ADR 0007 beschriebene Falle ist damit **nicht verschwunden**, sondern
  bedienbar geworden: Ein reiner LAN-Ansage-PC ohne Bediener-Verwaltung ruft
  jetzt den pro-Feld-Hinweis aus, bis jemand den Schalter umlegt. Der Kommentar
  an `announceScorekeeper` sagt das an Ort und Stelle.
- Am **Cloud-Ansage-Slave** wirkten zunächst nur der Bedienungs-Schalter, weil
  `CloudAnnounceCourt` (`commands.rs`) keine SR/AR-Listen führte. **Nachgezogen
  in v0.9.248:** Die Namen reisten im `MatchBrief` ohnehin bis zum Slave (der
  Host füllt sie, der Relay reicht sie durch) — sie fielen allein bei der
  Umwandlung heraus. Beide Schalter wirken dort jetzt wie am Master.
- Die Ansage kann nun Namen tragen, die BTP nie einem Spiel zugewiesen hat.
  Datenschutzlich unbedenklich: Es sind dieselben Namen, die die Anzeige am
  Feld ohnehin zeigt, und sie dienen demselben Zweck — die richtige Person ans
  richtige Feld zu holen.
