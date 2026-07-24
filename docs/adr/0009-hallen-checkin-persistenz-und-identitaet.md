# 0009 — Hallen-Check-In: Persistenz in badhub, Turnier-Identität ist die turnier.de-GUID

- **Status:** accepted
- **Datum:** 2026-07-24

Gehört zur Spezifikation
[docs/features/spieler-check-in.md](../features/spieler-check-in.md).

## Kontext

Spieler sollen vor Beginn ihrer Spielklasse über eine öffentlich erreichbare
Webseite selbst bestätigen, dass sie in der Halle sind („Hallen-Check-In"), damit
die Turnierleitung **vor der Auslosung** weiß, wer fehlt. Daraus folgen drei
Entscheidungen, die einander bedingen: **wo** die Check-In-Stände liegen, **wie**
sie dorthin kommen, und **welcher Schlüssel** das Turnier identifiziert.

Kräfte und Randbedingungen:

- **Der Client ist ein beliebiges Handy im Internet.** bts-light ist eine
  Desktop-App auf dem Turnier-PC, oft hinter einer Firmen-/Hallen-Firewall und
  ohne eingehende Erreichbarkeit. Die Spieler können sie nicht direkt ansprechen.
- **Der Check-In-Zeitpunkt liegt vor der Auslosung.** Alles, was erst mit den
  Matches entsteht, ist zu spät.
- **BTPs eigener Check-in passt fachlich nicht.** `Player.CheckedIn` und
  `FirstCheckIn` hängen am Spieler und gelten **turnierweit**; „in Herrendoppel B
  anwesend, in Herreneinzel A noch nicht" ist damit nicht abbildbar. Ein
  Rückschreiben nach BTP scheidet damit ohnehin aus (R2 bleibt unberührt: BTP ist
  weiterhin die Wahrheit für Klassen, Meldungen und Spieler).
- **Die Turnier-Identität ist heute uneinheitlich.** Es kursieren vier Kandidaten:
  die turnier.de-Turnier-GUID, badhubs `liveticker_tournaments.tournament_key`,
  badhubs `tournaments.external_id`/`tournament_uuid` und bts-lights
  `install_id`.
- **Im BTP-Snapshot steht die turnier.de-ID nicht.** Alle `Settings` und alle
  GUID-Vorkommen eines echten Mitschnitts wurden geprüft: Setting 1008 ist nur
  die Zeichenkette `www.turnier.de`, die einzigen UUIDs gehören zu
  `MatchWarnings`. Das Original-BTS trägt sie deshalb von Hand ein (Feld `tguid`
  im Admin-UI).
- **badhubs beide Turnierbegriffe sind entkoppelt.** `tournaments` (aus dem
  turnier.de-Import) und `liveticker_tournaments` (die Push-Tenants) haben keinen
  Fremdschlüssel und keine gemeinsame Spalte. Entscheidend: `tournaments`
  entsteht **nach** dem Turnier durch den Ergebnis-Import, `liveticker_tournaments`
  existiert **vor und während** dem Turnier und hat bereits Besitzer und
  Gültigkeitsfenster.
- **`install_id` ist bereits doppelt belegt** (Relay-Namespace **und**
  Log-Zuordnung, R6) und identifiziert eine *Installation*, nicht ein Turnier —
  ein Rechnertausch erzeugt eine neue.
- **Der Schreibpfad ist unauthentifiziert.** Wer auf seinen Namen klickt, weist
  sich nicht aus. Das ist eine bewusste fachliche Entscheidung („wer nicht da
  ist, hat verloren"), verlagert die Absicherung aber vollständig auf den Server.

## Entscheidung

1. **Persistenz in badhub.** Die Check-In-Stände, die Anfangszeiten und die
   Anmeldeschlüsse liegen in badhub, nicht lokal in bts-light und nicht im Relay.
2. **Transport über den bestehenden Liveticker-Kanal.** bts-light sendet die
   Meldeliste als neuen Nachrichtentyp per authentifiziertem HTTPS-POST
   (Bearer = Liveticker-Passwort → Tenant) und ruft die Stände per HTTPS-GET ab.
   Kein Relay, kein WebSocket, kein zweiter Auth-Weg.
3. **Fachliche Turnier-Identität ist die turnier.de-Turnier-GUID** (36 Zeichen,
   aus `turnier.de/tournament/<GUID>/matches`). Sie wird **einmalig in bts-light
   eingetragen** und als Nutzdatum mitgeschickt. Sie ist **kein Fremdschlüssel**,
   sondern ein eigenständiger Wert, und bildet den öffentlichen URL-Bestandteil
   der Check-In-Seite.
4. **Authentifiziert wird weiterhin über den `tournament_key`.** Die
   Check-In-Tabellen führen beide Schlüssel: den `tournament_key` als Tenant und
   Besitzer, die GUID als fachliche Identität.
5. **Spieler-Schlüssel ist die BTP-`PlayerID` innerhalb des Turniers.** Die
   Lizenznummer (`MemberID`) wird mitgeschickt, **wenn** BTP sie liefert — für das
   Anonymisierungs-Gate und später für Push-Abos —, ist aber **nie Pflicht**.
6. **Der öffentliche Schreibendpunkt bleibt unauthentifiziert**, mit
   serverseitiger Validierung als Mitigation: jede Anfrage muss gegen die zuletzt
   gepushte Meldeliste dieses Turniers und gegen das offene Zeitfenster geprüft
   werden, dazu ein IP-Rate-Limit; ein Zurücknehmen ist über den öffentlichen
   Pfad **nicht** möglich. Das ist die sinngemäße Übertragung von R5
   („der Server validiert jede fremde Eingabe") auf einen neuen Kanal.

## Alternativen

**Persistenz lokal in bts-light** — verworfen. Die Spieler-Handys könnten den
Turnier-PC gar nicht erreichen; er ist typischerweise nicht von außen
erreichbar. Zudem müsste die Turnierleitung Zeiten und Zahlungs-Rückfragen dann
am Turniertag pflegen statt vorab von zuhause.

**Persistenz im Relay** — verworfen. Der Relay ist auf Tablets und Monitore
zugeschnitten (ein Host je Namespace, ein aktives Tablet je Court, R4) und hält
bewusst keinen dauerhaften Zustand. Öffentliche Web-Clients in unbekannter Zahl
passen weder zum Sicherheits- noch zum Betriebsmodell.

**Turnier-Schlüssel `install_id`** — verworfen. Identifiziert eine Installation,
nicht ein Turnier; ist bereits doppelt belegt (R6); wechselt beim Rechnertausch.

**Turnier-Schlüssel `tournament_key` allein** — verworfen. Er ist ein
selbstvergebener Slug, der pro Turnier neu angelegt und später recycelt werden
kann. Die öffentliche Check-In-URL wäre nicht vorab stabil und der Bezug zu
turnier.de fehlte dauerhaft. Er bleibt aber der **Auth**-Schlüssel.

**Fremdschlüssel auf `tournaments.id`** — verworfen, obwohl relational am
saubersten. Diese Zeile entsteht erst **nach** dem Turnier durch den
Ergebnis-Import; am Turniertag existiert sie in aller Regel nicht. Das Feature
liefe ins Leere.

**Rückschreiben nach BTP** — verworfen, siehe Kontext: BTPs Check-in-Felder sind
turnierweit und können den klassenweisen Fall nicht abbilden.

## Konsequenzen

**Positiv**

- Kein neuer Auth-Weg und kein neues Geheimnis: der Schreibkanal ist der
  bestehende, bereits erprobte Liveticker-Push.
- Die öffentliche URL ist vorab stabil und druckbar (QR-Aushang), unabhängig
  davon, ob der Liveticker gerade läuft.
- Sobald der Ergebnis-Import später eine `tournaments`-Zeile anlegt, lässt sich
  über die GUID **nachträglich** verknüpfen — ohne dass der Check-In je davon
  abhängt.
- Der Check-In ist **additiv**: fällt badhub oder das Internet aus, läuft das
  Turnier unverändert weiter.
- Ein Turnier ohne gepflegte Lizenznummern funktioniert vollständig.

**Negativ / Preis**

- **Zwei Schlüssel in einer Tabelle** (Tenant und fachliche Identität). Sie
  beantworten verschiedene Fragen, kosten aber Erklärung.
- **Ein manueller Schritt** für die Turnierleitung: die GUID muss einmalig
  eingetragen werden, inklusive Tippfehler-Risiko. Turniere ohne
  turnier.de-Eintrag können den Check-In nicht nutzen.
- **Das Feature braucht Internet auf beiden Seiten.** Im reinen LAN-Betrieb ohne
  Internet ist es nicht verfügbar; bts-light blendet den Bereich dann aus.
- **Ein unauthentifizierter Schreibendpunkt bleibt bestehen.** Die serverseitige
  Validierung verhindert *technischen* Missbrauch (fremde Turniere, fremde
  Klassen, geschlossene Fenster, Massenanfragen), aber nicht den *sozialen* Fall,
  dass jemand einen Abwesenden anklickt. Dieses Restrisiko ist fachlich bewusst
  akzeptiert; die Gegenmaßnahme ist organisatorisch (QR-Verteilung erst in der
  Halle) und kurativ (die Turnierleitung kann jeden Check-In zurücksetzen und
  sperren).
- **Zwei Repos müssen koordiniert ausgerollt werden.** badhub geht zuerst;
  bts-light behandelt 404/400 als „Feature nicht verfügbar".
- **Eine neue Ausnahme von badhubs Schema-Regel**: die Check-In-Tabellen tragen
  kein `federation_id`, weil der Check-In verbandsübergreifend ist — wie der
  Liveticker. Die Ausnahme ist dort zu dokumentieren.
