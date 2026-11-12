# 0091 — Ergebnisse im Host: ein Arbeiter je Feld

- **Status:** accepted
- **Datum:** 2026-11-11
- Spec: [`relay-haerten`](../features/relay-haerten.md), Abschnitt c).
- Berührt Architekturregel R5 und [ADR 0018](0018-ergebnis-weg-verlustsicher.md)
  (Idempotenz des Ergebnis-Wegs).

## Kontext

Die Host-Schleife der Relay-Verbindung (`relay_client.rs`) wartet `process_result` inline
ab. BTP braucht bis 5 s Connect und 10 s Read, das Schreib-Gate des TP-Uploads hält ein
Ergebnis bis 20 s auf. In dieser Zeit liest die Schleife nichts und beantwortet kein Ping;
der Relay verwirft den Host nach 15 s Stille. Das trifft alle Cloud-Felder und öffnet das
Fenster, in dem der Host-Slot übernommen werden kann. Werbe-Upload und Control-Sync warten
ebenso inline.

Ein blankes `tokio::spawn` je Ergebnis löst das Warten, bricht aber die Reihenfolge: Der
8-s-Retry des Tablets liefe gleichzeitig mit dem ersten Versuch; beide sähen das Spiel noch
auf dem Feld, weil die Idempotenz aus ADR 0018 erst nach dem Räumen greift. Das Schreib-Gate
schützt davor nicht.

Hinzu kommt die Quittung: Der Relay zählt `req_id` je Namespace ab 1. Nach einem
Relay-Neustart könnte eine Quittung aus einer alten Sitzung ein fremdes Ergebnis mit „ok"
bestätigen — das Tablet löschte dann ein Ergebnis, das nie geschrieben wurde.

## Entscheidung

1. **Ein Arbeiter je Feld** im Cloud-Pfad: eine Warteschlange mit 4 Plätzen und ein Task je
   CourtID, angelegt beim ersten Ergebnis. Er gehört dem Verbindungs-Lauf (`run`), nicht
   der einzelnen Sitzung.
2. **Reihenfolge je Feld strikt nach Eingang; Felder laufen nebeneinander.**
3. **`process_result` bleibt unverändert die einzige Prüfung (R5).** Der Arbeiter ruft es
   nur auf; er prüft nichts selbst.
4. **Der Retry reiht sich hinter den ersten Versuch** und trifft danach die Idempotenz
   (`settled_ok`) — „ok" ohne zweiten BTP-Write.
5. **Die Quittung geht an die Sitzung, aus der der Auftrag kam.** Ist sie beendet, verpufft
   die Quittung; ein laufender Write wird dennoch zu Ende geführt. Über eine neue Sitzung
   geht nie eine alte Quittung.
6. **Volle Warteschlange:** sofortige, wiederholbare Absage.
7. **Werbe-Upload und Control-Sync** laufen als eigene Tasks mit je einem „läuft
   schon"-Wächter; Zettel in `spawn_blocking`. `handle_score` bleibt in der Schleife
   (Reihenfolge der Punkte je Feld).

## Verworfene Alternativen

- **Eine geordnete Warteschlange für alle Felder:** einfachste Ordnung, aber ein hängendes
  Feld hielte die Ergebnisse aller anderen auf.
- **Blankes `spawn` mit „läuft schon"-Wächter je Feld:** entkoppelt, aber der Wächter muss
  den Retry entweder abweisen (das Tablet wartet eine weitere Runde) oder parallel
  durchlassen (doppelter Write).
- **Feld-Schloss in `process_result`:** deckte auch den LAN-Weg, ändert aber die
  R5-Funktion und garantiert die Reihenfolge der Wartenden nicht. Für den LAN-Weg als
  Roadmap-Punkt vermerkt.
- **Quittung über die jeweils aktuelle Sitzung senden:** das Tablet bekäme seine Antwort
  auch nach einem Abriss, aber nach einem Relay-Neustart träfe die Nummer ein fremdes
  Ergebnis.

## Folgen

- Träges BTP wirft den Host nicht mehr ab; Pings werden während eines Writes beantwortet.
- Geht eine Quittung verloren, wiederholt das Tablet; die Idempotenz antwortet „ok" ohne
  zweiten Write. Verlustsicherheit und Platten-Queue aus ADR 0018 bleiben unberührt.
- Der LAN-Ergebnisweg ist unverändert — dort gibt es weiter keine Ordnung je Feld.
- Je Feld mit Ergebnis lebt ein kleiner Task bis zum Ende des Verbindungs-Laufs.
- Offen bleibt der große Zettel über einen langsamen Uplink: Ein Pong kann eine laufende
  große Nachricht nicht überholen, und der Relay stempelt nur vollständige Nachrichten
  (Roadmap).
- Die Schleife bekommt einen testbaren Kern (`sitzung_fahren`) mit injizierbarem
  Ergebnis-Verarbeiter.

## Nachtrag aus der Umsetzung (PR 1, v0.9.379)

- Die Quittung kann jetzt **nach** einem `MatchCleared`/`MatchAssigned` des 2-s-Takts beim
  Tablet ankommen (vorher garantierte der Inline-Write die umgekehrte Reihenfolge). Im
  Hallennetz war das immer so; das Tablet verträgt es seit v0.9.376.
- Ein Punkt desselben Spiels, der während des Writes eintrifft, wird noch aufgenommen.
- Ergebnisse für eine CourtID, die der Turnier-PC nicht kennt, teilen sich einen
  Sammel-Arbeiter — die CourtID ist fremdbestimmt.
- Ein an einer Panik gestorbener Arbeiter wird beim nächsten Ergebnis des Felds ersetzt.
