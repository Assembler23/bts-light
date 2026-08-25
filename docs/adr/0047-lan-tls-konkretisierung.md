# 0047 — LAN-TLS: Crate-Wahl, Zertifikatsstrategie und Port-Koexistenz

- **Status:** accepted
- **Datum:** 2026-08-25
- Konkretisiert [ADR 0005](0005-lan-https-selbstsigniert.md)

## Kontext

[ADR 0005](0005-lan-https-selbstsigniert.md) hat am 19.07.2026 entschieden, dass
der LAN-Tablet-Server zusätzlich HTTPS mit selbstsigniertem Zertifikat anbietet —
Begründung damals: die Battery-API braucht einen *Secure Context*, LAN-Tablets
hatten deshalb keine Akkuanzeige.

**Implementiert wurde das nie.** `8443` kommt im Code nicht vor, Server-TLS
existiert nirgends; `rustls` liegt ausschließlich client-seitig im Baum
(`src-tauri/Cargo.toml:32,35` für reqwest und tokio-tungstenite). ADR 0005 liest
sich wie gebaut, ist es aber nicht.

Beim Einlösen der Entscheidung traten vier Zwänge zutage, die ADR 0005 nicht
kannte und die eine eigene Entscheidung erfordern:

1. **Die Court-Monitor-Pis haben keine Echtzeituhr.**
   `pi/shared-startbrowser.sh:46-48` hält als Felderfahrung fest, warum der
   Log-Upload bewusst Klartext-HTTP spricht: *„kein TLS und keine korrekte
   Pi-Uhr nötig (Pis haben keine RTC; bei falscher Uhr scheiterte HTTPS sonst
   still)"*. Ein Zertifikat trägt `notBefore`/`notAfter` — ein Pi, der ohne
   NTP-Uplink bootet, verwirft es **lautlos**.
2. **Die Pi-Flotte ist gespalten.** `pi/shared-startbrowser.sh:162` setzt
   `--ignore-certificate-errors`, der neuere `pi/setup-monitor.sh:152-158`
   **nicht** — dafür `--kiosk --incognito`. Dort ist die Warnung ohne Tastatur
   nicht wegklickbar, und `--incognito` verwirft eine gesetzte Ausnahme bei
   jedem Start.
3. **`--ignore-certificate-errors` erzeugt keinen Secure Context.** Der Flag lädt
   die Seite, aber die Battery-API bleibt gesperrt. Der Nutzen, mit dem ADR 0005
   begründet war, trägt für Kiosk-Geräte also **nicht**.
4. **Ein Origin-Wechsel ist teuer.** `https://…:8443` ist eine andere Origin als
   `http://…:8088`; `assets/tablet.html` hält `pendingResult` und
   Kartenereignisse origin-gebunden im `localStorage`. Ein Umschalten im
   laufenden Turnier verlöre unbestätigte Ergebnisse.

## Entscheidung

**1. Crates: `rcgen` + `tokio-rustls` mit eigenem Listener-Adapter.**

Die Zertifikatserzeugung übernimmt `rcgen` (0.14.9, MIT OR Apache-2.0,
veröffentlicht 10.08.2026, Repo `rustls/rcgen`). Das TLS-Serving übernimmt
`tokio-rustls` (0.26.4, MIT OR Apache-2.0, Repo `rustls/tokio-rustls`) über einen
Adapter, der den `axum::serve::Listener`-Trait erfüllt. Die rustls-Version wird
an `Cargo.lock` (0.23.40) angeglichen.

Drei Festlegungen, die sich erst beim Bauen ergaben:

- **Ablage als DER, nicht PEM.** `rustls` liest DER unmittelbar; PEM bräuchte
  zusätzlich `rustls-pemfile`. Da diese Crate noch nicht im Baum liegt, spart
  DER eine ganze Abhängigkeit — am Ende kommt nur **`rcgen`** wirklich neu dazu,
  `tokio-rustls` wird von transitiv auf direkt gehoben.
- **Der Krypto-Provider wird ausdrücklich übergeben.** Im Abhängigkeitsbaum
  liegen `ring` **und** `aws-lc-rs` (beide als rustls-Abhängigkeit). Damit
  existiert kein eindeutiger Prozess-Default, und das bequeme
  `ServerConfig::builder()` paniekte erst **zur Laufzeit** — also im Turnier,
  nicht im Build. Wir wählen `ring` explizit über `builder_with_provider`:
  keine C-Toolchain nötig, deterministisch unabhängig davon, was andere Crates
  aktivieren.
- **Der Handschlag läuft nicht in `accept`.** Je Verbindung übernimmt ihn ein
  eigener Task, fertige Verbindungen kommen über einen Kanal herein, und ein
  Zeitlimit begrenzt jeden Versuch. Läge er in `accept`, blockierte ein
  einziger Client, der die TCP-Verbindung öffnet und dann schweigt, die
  Annahme **aller** anderen Geräte — in einem schwachen Hallen-WLAN kein
  Randfall.

**2. Zertifikat: langlebig und weit vordatiert.**

`notBefore` steht **fest auf 2020-01-01**, die Laufzeit auf zehn Jahre. Das ist
die direkte Antwort auf Zwang 1: Ein Pi mit falsch gestellter Uhr darf das
Zertifikat nicht als „noch nicht gültig" verwerfen. Die SAN-Liste umfasst
`bts-light.local`, `localhost`, `127.0.0.1` und die lokalen IPv4-Adressen zum
Erzeugungszeitpunkt. Das Zertifikat wird **einmal** erzeugt und persistiert; ein
IP-Wechsel löst **keine** Neuerzeugung aus.

**3. Port-Koexistenz: 443 zusätzlich, 8088 bleibt.**

Kein Zwangs-Rollout auf die Pi-Flotte, kein HTTP→HTTPS-Redirect. Die Pi-Skripte
bleiben unverändert auf HTTP. Der Anspruch lautet ausdrücklich **nicht**
„durchgängig verschlüsselt", sondern *verschlüsselt, wo das Gerät es kann*.

**4. Beide Adressen parallel, kein Umschalten.**

Die Oberfläche zeigt je Feld HTTP **und** HTTPS an — dem bestehenden Muster
folgend, das für LAN und Cloud bereits zwei QR-Codes je Feld anbietet. HTTP
bleibt die Vorgabe. Eine Umstellung ist eine bewusste Handlung **zwischen**
Turnieren (Zwang 4).

**5. ALPN fest auf `http/1.1`.**

Böte der TLS-Stack `h2` an, bräche der WebSocket-Upgrade und damit `/ws`,
`/monitor-ws` und `/tl-ws`. Ein Unit-Test wacht darüber.

## Alternativen

**`axum-server` statt eigenem Adapter** (0.8.0, MIT, 8,9 Mio Downloads/Monat).
`bind_rustls` nähme einem den Accept-Loop vollständig ab. Verworfen: Die Crate
hängt an einer Einzelperson, ist seit 06.12.2025 unverändert und müsste sowohl
der `axum`- als auch der `rustls`-Version folgen; ein Versatz erzeugte zwei
rustls-Kopien im Baum. Für rund 40 Zeilen eigenen, testbaren Code nehmen wir
lieber keine zusätzliche Bus-Faktor-Abhängigkeit in den kritischen Pfad.
`rcgen` und `tokio-rustls` stammen dagegen beide aus dem `rustls`-Dach.

**Lokale CA mit Trust-Store-Rollout.** Bereits von ADR 0005 als Option C
verworfen (Betriebsaufwand je Gerät). Unverändert gültig.

**Zertifikat bei jedem IP-Wechsel neu erzeugen.** Verworfen: Es entwertete
**jede** weggeklickte Ausnahme auf **allen** Tablets — und zwar genau dann, wenn
das Netz ohnehin gerade wackelt. Stattdessen ist der mDNS-Name die stabile
Identität; ein Knopf „Zertifikat neu erzeugen" bleibt als bewusste Notfallaktion.

**8088 durch TLS ersetzen.** Verworfen: Jeder Bestands-Pi mit hart kodiertem
`http://…:8088` und jede eingetippte Adresse wären sofort tot; beide
Pi-Skript-Generationen müssten vorher ausgerollt sein, inklusive der bereits
ausgelieferten Images.

**Zweiter mDNS-`ServiceInfo` für den TLS-Port.** Verworfen als YAGNI: Der
bestehende Hostname löst bereits auf die IP auf, und der Port steht in der URL.
Ein zweiter Service-Eintrag wäre nur für Service-Discovery nötig — die liest
heute niemand außer den Pi-Skripten, und die nutzen mDNS erst als dritte Wahl
hinter IP-Cache und Subnetz-Scan.

## Konsequenzen

**Positiv**

- Spielstände, Spielernamen und Lizenznummern reisen im Hallennetz verschlüsselt.
- Handbediente Tablets bekommen den Secure Context und damit die Akkuanzeige, die
  ADR 0005 seit einem Jahr verspricht.
- Rein additiv: Kein Bestandsgerät ändert sein Verhalten. `tls.enabled = false`
  schaltet zurück; eine ältere App-Version ignoriert das Config-Feld dank
  `serde(default)`.
- Der Baustein ist so geschnitten, dass der geplante Slave-Proxy der fernen Halle
  ihn mitnutzen kann.

**Negativ / Grenzen**

- **Einmalige Zertifikatswarnung je Tablet.** Abschreckend für eine Zielgruppe
  ohne IT-Hintergrund; muss in der Bedienanleitung stehen.
- **Kiosk-Geräte bekommen keinen Secure Context** — ihr Akku bleibt unsichtbar.
  Der Nutzen des Vorhabens ist damit gespalten: Vertraulichkeit für alle,
  Akkuanzeige nur für handbediente Tablets.
- **Der Anspruch bleibt unvollständig.** Solange 8088 offen ist, sprechen die
  Court-Monitore weiter Klartext. Das ist bewusst gewählt und darf nicht als
  „verschlüsselt" beworben werden.
- **Ein IP-Wechsel entwertet Ausnahmen**, die auf die IP erteilt wurden. Die
  Betriebsempfehlung lautet deshalb: `bts-light.local` nutzen, nicht die IP.
- **Ein privater Schlüssel liegt auf Platte** (`app_config_dir`). Unter Unix mit
  `0600`; unter Windows trägt das Nutzerprofil den Schutz. Es gibt im Workspace
  bisher keinen Präzedenzfall für gesetzte Dateirechte.
- **Die lange Laufzeit (bis 2036)** widerspricht der 398-Tage-Regel für
  TLS-Server-Zertifikate. Nach Apples eigener Beschreibung gilt diese Grenze
  **nicht** für Zertifikate aus nutzer- oder administratorseitig hinzugefügten
  Vertrauensankern — also genau unserem Fall (manuell bestätigte Ausnahme).
  Bliebe sie dennoch wirksam, träfe es vor allem iPadOS/Safari, wo die
  Ablehnung **hart** sein kann, also ohne „trotzdem fortfahren". Für die
  Zielgeräte des Akku-Nutzens (Android-Tablets) ist das nachrangig; ein iPad
  im Feldtest würde es klären. Kürzen wäre teuer: Jede Neuausstellung
  entwertet alle bereits bestätigten Ausnahmen.
- **Ein Nachziehen der Laufzeit ist später schwer.** Deshalb ist sie eine
  Konstante mit Begründung, kein Konfigurationswert — eine Änderung soll eine
  bewusste Entscheidung mit ADR-Nachtrag sein.

> **Nachtrag 2026-08-25:** Ursprünglich lag der TLS-Port auf **8443**. Er
> liegt jetzt auf **443**, dem Standard-Port für https — nur so lässt sich die
> Adresse ohne Portangabe schreiben (`https://192.168.1.50`) und am Telefon
> diktieren. **8443 bleibt als Ausweichport:** Unter Windows darf zwar jeder
> Prozess an 443 binden (dort gibt es keine privilegierten Ports), aber
> `http.sys` kann ihn für IIS oder WinRM reserviert haben. Ist das der Fall,
> weicht der Server aus und meldet den **tatsächlich** belegten Port an die
> Oberfläche — sonst zeigten QR-Codes auf einen Port, an dem niemand lauscht.
> Wer in der `config.json` selbst einen Port einträgt, bekommt **keinen**
> Rückfall: Eine bewusste Wahl wird nicht übergangen.

## Ausblick: die Zertifikatswarnung loswerden

Festgehalten am 25.08.2026, damit die Optionen bei der von
[ADR 0005](0005-lan-https-selbstsigniert.md) vorgesehenen Neubewertung
(„falls die Warnung im Verleih-Betrieb zum Support-Problem wird") nicht neu
durchdacht werden müssen. **Noch nichts davon ist entschieden** — Auslöser
wäre ein Feldtest, der die Warnung als echtes Ärgernis zeigt.

Ein Browser vertraut nur, was auf einen Anker in seinem Trust Store
zurückführt. Es gibt daher genau zwei Richtungen: einen Anker aufs Gerät
bringen, oder ein Zertifikat von einem Anker besorgen, der schon drauf ist.

**A — Eigene CA einmalig auf die Geräte.** Eine kleine lokale CA, deren
Zertifikat auf jedem Tablet installiert wird; die Server-Zertifikate stammen
daraus. Für die **Verleih-Tablets des Projekts** realistisch: einmal je Gerät,
danach jahrelang Ruhe. Zwei Haken: Android verlangt dafür einen eingerichteten
Sperrbildschirm und zeigt anschließend dauerhaft „Das Netzwerk wird
möglicherweise überwacht"; und für **mitgebrachte Fremdgeräte** ist der Weg
praktisch tot — dort installiert niemand eine CA. Entspricht Option C aus
ADR 0005, aber auf die eigenen Geräte begrenzt statt auf alle.

**B — Öffentlich vertrauenswürdiges Zertifikat auf private IPs.** Das
Plex-Muster: eine Domain (z. B. `lan.badhub.de`), deren DNS **private**
Adressen auflöst (`192-168-1-50.lan.badhub.de` → `192.168.1.50`), dazu ein
Wildcard-Zertifikat per DNS-Challenge. Ergebnis wäre ein grünes Schloss ohne
jeden Eingriff am Gerät.

Der Preis ist hoch, und ein Punkt davon ist vermutlich ein K.o.: **Es braucht
funktionierendes DNS, also Internet in der Halle.** Genau die Offline-
Fähigkeit, die den LAN-Modus ausmacht, wäre dahin — fällt der Uplink aus,
lösen die Tablets den Namen nicht mehr auf und erreichen den Server gar nicht
mehr. Dazu käme eine Serverkomponente auf badhub.de (Zertifikatsverteilung,
DNS-Registrierung je Installation) und ein privater Schlüssel für eine echte
Domain in jeder ausgelieferten App. Nur erwägenswert, wenn verlässliches
Internet in den Hallen ohnehin vorausgesetzt wird.

**C — Nichts tun.** Der Klartext-Port bleibt offen; wer die Warnung nicht
will, nutzt `http://…:8088` und verzichtet auf die Akkuanzeige. Das ist heute
die Vorgabe im QR-Code und bleibt der Rückfall für jedes Fremdgerät.

**Neigung, falls es so weit kommt:** A für die eigenen Verleih-Tablets, C für
alles andere. Einmaliger Aufwand an Geräten, die uns gehören, kein Serverbau,
keine neue Internet-Abhängigkeit. Fremdgeräte sind ohnehin selten die, deren
Akkustand die Turnierleitung braucht.

Spec: [`docs/features/lan-tls-verschluesselt.md`](../features/lan-tls-verschluesselt.md).
