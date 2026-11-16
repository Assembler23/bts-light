# 0102 — Vertrauensmodell des Pi-Fernupdates: eigene Signatur, nur höhere Nummer, Auftrag unauthentisiert

- **Status:** accepted
- **Datum:** 2026-11-16

## Kontext

Das Pi-Fernupdate (Spec `docs/features/pi-fernupdate.md`) lässt einen
Root-Dienst am Pi Skripte vom Turnier-PC laden. Der LAN-Server ist
unverschlüsselt und ohne Anmeldung, und der Pi prüft nicht, ob der gefundene
PC echt ist: Jedes Gerät im Hallen-WLAN mit offenem Port 8088 gilt als
Turnier-PC. Die Pis haben keine Echtzeituhr. Der Schlüssel des App-Updaters
liegt nur in GitHub-Secrets und wird nie gewechselt.

## Entscheidung

- Ein **eigenes Ed25519-Schlüsselpaar** nur für Pi-Pakete. Das Image trägt
  zwei öffentliche Schlüssel, aktiv und Reserve.
- Die Signatur deckt das **Manifest**; das Manifest bindet das Archiv über
  Länge und SHA-256. Der Pi prüft die Signatur, bevor er Nummer oder Inhalt
  liest.
- Geprüft wird am Pi mit `openssl pkeyutl -verify -rawin`; signiert wird in
  der CI mit Node `crypto`.
- Der Pi nimmt **nur eine höhere Paketnummer** an. Zurückstufen gibt es
  nicht.
- PC-Adresse und Auftrag bleiben **unauthentisiert**. Akzeptiertes
  Restrisiko: Ein fremder Rechner im WLAN kann einen Pi einmal auf ein
  echtes, neueres Paket heben und damit einen Neustart der Anzeige auslösen.
- Signiert wird nur im Tag-Release aus dem Secret `PI_PAKET_SIGNING_KEY`;
  ohne gültiges Secret bricht der Lauf ab.
- Builds ohne Secret signieren mit einem Entwickler-Schlüssel, den nur ein
  eigens gebautes, markiertes Test-Image annimmt.

## Alternativen

- **Updater-Schlüssel mitnutzen:** Ein Leck träfe beide Ketten, der
  Schlüssel ist nicht wechselbar, und vor einem Tag wäre kein Test am
  echten Pi möglich.
- **minisign, gpgv, signify:** Zusatzpaket am Pi oder in der CI, bei gpgv
  dazu Ablaufdaten ohne Uhr.
- **TLS oder Anmeldung am PC:** Pis ohne Uhr scheitern an Zertifikaten, und
  ein Geheimnis im Image wäre im LAN auslesbar.
- **Kopplung Pi ↔ PC:** ein Handgriff je Pi und je Verleih; widerspricht
  Plug-and-play.
- **Zurückstufen auf Knopfdruck:** Der Knopf schützt nicht, weil ein
  falscher PC den Auftrag selbst sendet.
- **Signiertes Paket einchecken:** Entweder wird ungeprüfter Code signiert,
  oder ein Workflow muss nach `main` zurückschreiben.

## Konsequenzen

- Ein PC mit älterer App kann einen neueren Pi nicht bedienen; die App zeigt
  „Pi neuer als dieser PC".
- Paket und Protokoll müssen vorwärtskompatibel bleiben.
- Der Admin hält ein weiteres Secret und verwahrt die Reserve offline.
- Gehen beide Schlüssel verloren oder werden offengelegt, braucht es neue
  Karten.
- Vor der Umsetzung ist die `openssl`-Prüfung am echten Pi zu messen. Fällt
  sie durch, wird `minisign` ins Image aufgenommen; Format und Protokoll
  bleiben.
