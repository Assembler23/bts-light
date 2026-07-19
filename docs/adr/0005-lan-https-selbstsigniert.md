# ADR 0005 — LAN-Tablet-Server bietet HTTPS mit selbstsigniertem Zertifikat an

Status: akzeptiert (2026-07-19)

## Kontext

Die Tablet-Akkuanzeige (und künftig Wake Lock) braucht die Battery-API des
Browsers, die nur in **Secure Contexts** verfügbar ist. Cloud-Tablets
(https://badhub.de) melden ihren Akkustand daher, LAN-Tablets
(`http://<IP>:8088`) prinzipbedingt nicht — die Turnierleitung sieht im
LAN-Betrieb keine Akkus (Turnier-Feedback 19.07.2026). Tilos Original-BTS
hat dasselbe Problem und betreibt in der Praxis HTTPS mit selbstsigniertem
Zertifikat, dessen Browser-Warnung auf den Tablets einmalig weggeklickt
wird — eine so geladene Seite gilt trotzdem als Secure Context.

## Entscheidung

Der eingebettete Tablet-Server bietet **zusätzlich** zu HTTP (`:8088`)
einen HTTPS-Port (`:8443`) mit einem **selbstsignierten, lokal erzeugten
Zertifikat** an. Die Tablet-QR-Codes/-URLs zeigen auf die HTTPS-Variante;
auf jedem Tablet wird die Zertifikatswarnung einmalig bestätigt
(„Erweitert → trotzdem fortfahren"). HTTP bleibt unverändert bestehen
(Pis/Monitore/Alt-Geräte). Das Zertifikat wird einmal erzeugt und
persistiert, damit die Geräte-Ausnahmen Neustarts überleben.

## Alternativen

- **Nur Cloud-Weg für Akkustände (Option A):** verworfen als Alleinlösung —
  Turniere ohne Internet blieben ohne Akkuanzeige; als dokumentierter
  Nebenweg bleibt sie bestehen.
- **Lokale CA mit Zertifikats-Installation auf den Tablets (Option C):**
  verworfen — deutlich mehr Betriebsaufwand (Trust-Store je Gerät,
  Rotation, IP-Wechsel) für den einzigen Gewinn, dass die einmalige
  Browser-Warnung entfällt. Neubewertung, falls die Warnung im
  Verleih-Betrieb zum Support-Problem wird.
- **Gar kein HTTPS (Status quo):** verworfen — Nutzerentscheidung
  19.07.2026, Akkustände im LAN sind gewünscht; Tilos Praxis belegt die
  Tauglichkeit des gewählten Wegs.

## Konsequenzen

- Akkustand (und künftig Wake Lock) funktioniert auch für reine
  LAN-Tablets; Fully-Kiosk-Geräte melden wie bisher über die eigene API.
- Einmalige, abschreckend wirkende Browser-Warnung je Tablet beim
  Ersteinrichten; die Ausnahme muss nach Zertifikatswechsel erneut
  bestätigt werden — daher Zertifikat langlebig und stabil halten.
- Manche Kiosk-Browser erlauben das Wegklicken nicht — dort bleibt der
  HTTP-Weg (ohne Akkuanzeige) oder Fully Kiosk.
- Neue Abhängigkeit für Zertifikats-Erzeugung/TLS (rcgen + rustls-Anbindung
  an axum) — vor der Umsetzung durch den dependency-auditor prüfen.
- Umsetzungsplan: [../roadmap-plaene-2026-07.md](../roadmap-plaene-2026-07.md), Punkt 6.
