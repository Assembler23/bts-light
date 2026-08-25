# 0048 — Ferne Halle: Träger-Verbindung, Substrom-Adressierung und lokale Terminierung

- **Status:** accepted
- **Datum:** 2026-08-25
- Schreibt [ADR 0002](0002-ferne-halle-direkt-cloud-geraete.md) fort
  (dort „Weg B", aufgeschoben — hier die Transport-Hälfte davon)
- Berührt [ADR 0017](0017-reconnect-ownership.md) und
  [ADR 0020](0020-tote-verbindung-read-idle-tablet-stale.md), ohne sie zu ändern

## Kontext

Heute leitet die Slave-Brücke der fernen Halle nur weiter: `/court/{id}` und
`/monitor` antworten mit einer Umleitung auf `badhub.de`, und selbst die lokal
gerenderte Feldauswahl verlinkt in die Cloud. Jedes Gerät der Halle baut damit
seine **eigene** Verbindung ins Internet auf. Das ist ADR 0002 („Weg A") und war
so gewollt.

Der Betreiber möchte stattdessen, dass die Geräte **nur mit dem Slave** reden
und dieser den Verkehr gebündelt zum Master-Relay trägt. Nach Vorlage der
vollständigen Kosten hat er sich am 25.08.2026 ausdrücklich für **echtes
Multiplexing** entschieden — nicht für einen Reverse-Proxy, der die
Verbindungszahl unverändert ließe.

Drei Befunde aus dem Code bestimmen, wie das gebaut werden **muss**:

1. **Es gibt keine Trägerverbindung, die man nachnutzen könnte.**
   `commands.rs:1100` (`if !slave_mode && mode.cloud_enabled()`) schließt den
   Relay-Client im Slave-Modus aus; der Slave pollt ausschließlich HTTP. Der
   Träger ist eine **neue Rolle** im Namespace.
2. **Der Relay unterscheidet Geräte über den Transportkanal.**
   `Tx::same_channel()` wird an **acht** Stellen ausgewertet — darunter
   `is_holder` (`relay/main.rs:2945`), das „ein aktives Tablet je Court" (R4)
   durchsetzt, und `release_host_slot` (`:3489`). Kanal-Identität ist dort der
   Stellvertreter für Geräte-Identität.
3. **Die Seiten-Marke wird von der ausliefernden Binärdatei berechnet.**
   `seiten_marke` ist ein Hash über den Seiteninhalt
   (`relay-proto:1380-1385`); der Client vergleicht sie gegen den `Pong`
   (`tablet.html:1412`). Die Doku am Enum sagt ausdrücklich: „`marke` füllt
   **der jeweilige Server** mit seiner eigenen Seite … die fremde Marke wäre
   die falsche."

## Entscheidung

### 1. Ein eigener Kanal je Substrom — der Relay-Kern bleibt unangetastet

Der Relay bekommt eine neue Route `/{ns}/carrier-ws`. Pro angemeldetem
Substrom erzeugt sie einen **eigenen `mpsc`-Kanal** und ruft damit die
**bestehende** Sitzungslogik auf; ein Fan-in-Task bündelt die Antworten zurück
auf die Trägerverbindung.

Damit bleiben `type Tx` und **alle acht `same_channel`-Stellen unverändert** —
und mit ihnen R4, ADR 0017 (Reconnect-Ownership) und ADR 0020 (tote
Verbindungen) **strukturell** gültig, statt neu bewiesen werden zu müssen.

Die Träger-Rolle steht **neben** dem `host`-Slot, nie darin: `try_claim_host`
und `release_host_slot` werden nicht angefasst, „genau ein Host je Namespace"
(R4) bleibt wörtlich erhalten.

### 2. WebSocket-Verkehr durch den Träger, HTTP daneben

Über die Tablet-WebSocket läuft nichts Großes: `MAX_STATE_LEN` ist **64 KB**,
Punktverlauf 8 KB. Die dicken Brocken — Werbebilder **12 MB**, Turnierlogo
**2 MB** — sind HTTP-Uploads Host→Relay und HTTP-Downloads Relay→Anzeige.

**Head-of-Line-Blocking droht deshalb nicht aus dem Bestand, sondern entstünde
erst dadurch, dass der Mux die HTTP-Strecken mit in den Träger zöge.** Also
tut er es nicht: WS-Verkehr wird multiplext, HTTP beantwortet der Slave selbst
(Seiten aus eigenem Bestand, Bilder und Zustandsabrufe per eigener HTTPS-Anfrage
zum Relay).

### 3. Der Slave terminiert lokal und stempelt selbst

Aus Befund 3 folgt zwingend: Lieferte der Slave die Seite aus, während der
Relay den `Pong` stempelt, wichen die Marken **fast immer** ab — zwei
unabhängig deployte Binärdateien. Jedes Tablet der fernen Halle meldete
dauerhaft „veraltet", und ein `ReloadTablets` löste eine **Reload-Schleife
mitten im Turnier** aus.

Der Slave terminiert daher die Tablet-WebSocket lokal und stempelt den `Pong`
mit **seiner** Marke — die für die von ihm ausgelieferte Seite die richtige
ist. Der Träger transportiert nur Fachframes.

Das löst zwei weitere Probleme gratis mit: die `__BASE__`-Injektion für
`monitor.html` (das im Gegensatz zu `tablet.html` **nicht** origin-relativ ist)
und die Vereinslogo-Weiche in `tablet.html:1051`.

### 4. Was ausdrücklich draußen bleibt

Kein schreibender Rückkanal (Steuerung läuft über TL-Web), keine
TL-Web-Geräte im Träger (so reisen keine Schreib-Token durch), kein
Zustandsspiegel, keine Offline-Pufferung. Der Slave bleibt zustandslos.

## Alternativen

**Explizite `StreamId` im Namespace-Modell.** `tablets`, `monitor_subs` und
`monitor_subs_all` speichern `(Tx, StreamId)`; alle acht `same_channel`-Stellen
werden auf Stream-Identität umgestellt. Konzeptionell direkter, aber ein
Eingriff **quer durch `relay/main.rs`** in genau die Stellen, die R4 und die
Reconnect-Ownership durchsetzen. `is_holder` ist der Punkt, an dem ein falsches
Ergebnis in ein fremdes Feld laufen könnte — dort will man keinen Umbau, wenn
er vermeidbar ist. **Verworfen.**

**Reverse-Proxy statt Multiplexing.** Je Gerät eine eigene Upstream-Verbindung;
der Slave reicht nur durch. Erfüllt alle *erkennbaren* Ziele (lokale Adressen,
kein IP-Tippen, verschlüsselte Strecke), lässt aber die Verbindungszahl
unverändert, und ADR 0017/0020 blieben unberührt. Vom Betreiber am 25.08.2026
nach Vorlage der Kosten **abgelehnt** — festgehalten, weil der Nutzen des Mux
über den Proxy hinaus **allein** in der Verbindungszahl liegt und diese als
Problem nicht gemessen ist.

**HTTP mit in den Träger ziehen.** Wäre konsequenter für das Ziel „kein Gerät
sieht badhub", brächte aber die 12-MB-Werbebilder in denselben Strang wie die
Punkt-Frames. Verworfen (siehe Entscheidung 2); das Ziel wird stattdessen
dadurch erreicht, dass der Slave die HTTP-Anfragen **selbst** beantwortet.

## Konsequenzen

**Positiv**

- Der sicherheitskritische Kern bleibt unberührt: `process_result` (R5),
  `is_holder`, `attach_tablet`, `release_host_slot` und sämtliche Deckel
  arbeiten unverändert weiter.
- In der Halle sieht kein Gerät mehr eine badhub-Adresse.
- Die Slot-Freigabe wird **genauer** als heute: Der Slave hält die echte
  Verbindung zum Gerät und sieht einen Abriss sofort, statt auf die 15 s
  `TABLET_STALE` des Relays zu warten.
- Etappen im Relay sind rückwärtskompatibel und für sich wirkungslos — sie
  fügen eine Route hinzu, die niemand ruft. Sie können gefahrlos vorlaufen.

**Negativ / Grenzen**

- **Der Slave wird Single Point of Failure der Halle.** Heute liegt er nicht
  im Tablet-Datenpfad; ein Ausfall kostete nur die Ansage. Danach nimmt er der
  Halle alle Geräte gleichzeitig. Notnagel bleiben die weiterhin gültigen
  Direkt-Cloud-Adressen — mit dem ausdrücklichen Hinweis, dass ein
  unbestätigtes Ergebnis dabei neu zu erfassen ist.
- **Beidseitiger Neubau.** Der Relay kennt heute kein Multiplex-Konstrukt:
  keine Stream-IDs, keine Per-Strom-Lebensdauern. Dazu kommt ein Refactoring,
  das Sitzungs- von Transportlogik trennt (`tablet_conn`, `monitor_conn`).
- **Ping und Stale wandern in den Slave.** Der Relay misst hinter einem Träger
  nur noch den Träger; die Verantwortung für die Slot-Freigabe je Gerät liegt
  damit beim Slave. Fällt er falsch aus, hält ein totes Tablet seinen Court.
- **Der Umzug ist ein Origin-Wechsel.** `pendingResult` liegt origin-gebunden
  im `localStorage` — die Umstellung einer Halle gehört **zwischen** zwei
  Turniere, nie in ein laufendes.
- **Die Latenz des Fan-in ist ungemessen.** Beim Feldtest gegen die bekannte
  Zahl p50 15 ms (Punkt → Anzeige) zu prüfen.
- **Relay vor Client deployen.** Der Relay deployt automatisch bei jedem
  main-Merge; die Relay-Etappen müssen daher vor der Slave-Seite gemergt sein.

Spec: [`docs/features/ferne-halle-transport-buendelung.md`](../features/ferne-halle-transport-buendelung.md).
