# Log-Review Zwei-Hallen-Turnier 17.–19.07.2026

Systematische Auswertung aller Server-Logs (Relay-Logs 17./18./19.07.,
hochgeladene App-Logs Master/Slave, Turnier auf v0.9.146→0.9.147).
Durchgeführt 20.07.2026. Hinweis zur Datenlage: App-Logs werden beim
10-Minuten-Upload **überschrieben** — es liegt je Installation nur die
letzte Session vor (Samstag app-seitig verloren, Relay-Logs vollständig).

## Kernbefunde

### 1. Ergebnisweg: fehlerfrei (Sonntag: 148/148)

Der Master (v0.9.147) hat am Sonntag **148 Tablet-Ergebnisse übermittelt
bekommen und alle 148 erfolgreich nach BTP geschrieben** („BTP-Schreiben
OK", inkl. Feldfreigabe). Kein einziges abgelehntes, verlorenes oder
unvollständiges Ergebnis; keine Walkover-Sonderfälle nötig. Der
BTP-Ergebnis-Fix (Status-Feld, Ein-Request-Prinzip) und die neuen Felder
(CourtID bleibt, Duration, Players-Block) liefen den ganzen Tag stabil.

### 2. Tablet-Reconnect: Vorher/Nachher-Beweis in Zahlen

| Metrik (Relay, Cloud-Halle WR) | Sa 18.07. (v0.9.146) | So 19.07. (v0.9.147) |
|---|---|---|
| Tablet-Verbindungen | 45 | ~90 |
| „Feld belegt"-Blockaden | **33** | **0** |
| Manuelle Übernahmen | **42** | 1 (echter Gerätetausch) |
| Nahtlose Same-Device-Reconnects | 0 (gab es nicht) | 6 |

Samstag musste praktisch jeder Reconnect manuell „übernehmen", teils
„Übernahme ohne gespeicherten Stand" (= Punktverlust, der gemeldete Bug).
Sonntag heilten sich zwei WLAN-Massenausfälle (je 6 Tablets, 10:58 und
11:14) vollautomatisch; Musterbeispiel Feld WR-2/Match 1504: Abriss
mitten im Spiel, 13-KB-Stand wiederhergestellt, 13 Minuten später
komplettes 3-Satz-Ergebnis in BTP. Der LAN-Pfad zeigte dasselbe Bild
(3 nahtlose Ablösungen); die einzige „belegt"-Meldung mit Übernahme am
Sonntag war ein bewusster Gerätetausch (HM-06, 17:24) — gewollter Flow.

### 3. Offene Schwachstellen (quantifiziert)

1. **Leere BTP-Snapshots** — 2× am Sonntag (08:38, 15:55): BTP lieferte
   je einen Abruf lang „0 Hallen/Felder/Matches" → Massen-Freigabe aller
   Felder, Sekunden später automatische Wiederzuweisung. Ursache
   BTP-seitig (u. a. während Tilos Gruppen-Umbau, der 5 laufende Spiele
   kappte). **Fix: Leer-Snapshot-Guard** — einen leeren Snapshot direkt
   nach einem gefüllten erst nach zweiter Bestätigung übernehmen +
   Dashboard-Warnung. (S)
2. **Zombie-Host blockiert Reconnect** — 18:22–18:39: **333×** „Zweiter
   Host abgewiesen" in 17 Minuten. Nach einem Netzwechsel hielt die tote
   alte Master-Verbindung den Namespace; der eigene Reconnect wurde als
   „zweiter Host" abgewiesen, bis TCP die Leiche erkannte. In der Zeit
   war die Cloud-Halle vom Master abgeschnitten. **Fix: Host-Stale-
   Erkennung im Relay** (Ping/Pong-Timeout wie bei Tablets; ein neuer
   Host ersetzt einen stummen alten nach ~15 s). (S/M)
3. **DNS-Ausfälle am Master-PC** — 23× über den ganzen Sonntag verteilt
   („Host unbekannt", 06:51–16:31): Hallen-Router-DNS unzuverlässig.
   Backoff-Reconnect hat jedes Mal geheilt; Ausfälle je ≤ Backoff-Dauer.
   Maßnahme: Betriebs-Doku (öffentlichen DNS 1.1.1.1/8.8.8.8 am
   Turnier-PC eintragen); optional Resolver-Fallback in der App prüfen.
4. **Tablet-Doze (HM/LAN)** — **140** Stale-Schließungen am Sonntag:
   Display-/WLAN-Schlaf der LAN-Tablets erzeugt Dauer-Reconnect-Zyklen
   (funktional folgenlos, aber Rauschen + träge Anzeige). Maßnahme:
   Keep-Screen-On-Empfehlung in docs/tablet.md; langfristig Wake Lock —
   braucht Secure Context → Synergie mit ADR 0005 (LAN-HTTPS).
5. **Kosmetik:** 7 Ergebnis-Logzeilen mit leerem Hallen-Label
   („Feld 38 ('')") — Label-Lookup nach bereits aufgehobener Zuweisung.
   Mini-Fix im Logging. — Ferner zeigt der Court-Score-Cache nach
   Match-Wechsel kurz den alten Stand unterm neuen Spiel (HM-03-Befund):
   Cache beim Zuweisungswechsel serverseitig leeren.

### 4. Unauffällig / Kontext

- **Slave (WR)**: kompletter Sonntag ohne eine einzige Warnung.
- **Samstag-Ticker-Verwirrung** („beendet mit Teilstand"): Folge von
  Tilos Gruppen-Umbau + in BTP direkt gewerteten Aufgaben; Anzeige-Fix
  „Aufgabe/kampflos-Badge" steht in der Roadmap.
- **17.07.** (Chaos-Tag): 19 Host-Verbindungswechsel am Relay spiegeln
  den Master-PC-Tausch — Motivation für „Master-Identität umziehen"
  (Roadmap Prio 1) bestätigt.
- Historisch im Log-Bestand: Juni-Turnier (14.06., v0.9.96/99) mit
  84/84 Ergebnissen OK — der Ergebnisweg war auch damals stabil.

## Abgeleitete Arbeitsaufträge (in Prioritätsfolge)

1. Leer-Snapshot-Guard (S) — verhindert Massen-Resets durch BTP-Aussetzer.
2. Zombie-Host-Ablösung im Relay (S/M) — verhindert minutenlange
   Cloud-Hallen-Blockade nach Netzwechsel des Masters.
3. Keep-Awake-Doku + später Wake Lock via ADR 0005 (Doku sofort).
4. Score-Cache-Reset bei Match-Wechsel + Label-Kosmetik (S).
5. DNS-Hinweis in die Betriebs-/Setup-Doku (Doku).

Alle Punkte fließen in die offizielle Release-Version nach dem Turnier
(siehe [roadmap.md](roadmap.md) / [roadmap-plaene-2026-07.md](roadmap-plaene-2026-07.md)).
