# Architecture Decision Records (ADRs)

Kurze, versionierte Dokumente pro **bewusster, schwer reversibler** Entscheidung
(Tool-/Framework-Wahl, Protokoll-/Architektur-Schnitt, Release-Verfahren). **Nicht** für Alltägliches.

Format: [Nygard](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions) —
Vorlage in [`template.md`](template.md). Neue ADRs fortlaufend als `NNNN-kebab-titel.md` ablegen und
hier eintragen.

## Index

| Nr. | Titel | Status |
|---|---|---|
| [0001](0001-quality-gate-und-branch-protection.md) | Quality-Gate & Branch Protection | accepted |
| [0002](0002-ferne-halle-direkt-cloud-geraete.md) | Ferne Halle: Tablets & Monitore per Direkt-Cloud statt Slave-Multiplex | accepted |
| [0003](0003-azure-tts-vererbung-relay.md) | Azure-TTS-Konfiguration wird über den Relay an Cloud-Slaves vererbt | akzeptiert |
| [0004](0004-telefon-kopplungscode.md) | Kopplung ferner Hallen über kurzlebigen 8-stelligen Telefon-Code | akzeptiert |
| [0005](0005-lan-https-selbstsigniert.md) | LAN-Tablet-Server: HTTPS mit selbstsigniertem Zertifikat | akzeptiert |
| [0006](0006-master-identitaet-umziehen.md) | Master-Identität per Export/Import auf einen neuen PC umziehen | akzeptiert |
| [0007](0007-zaehltafelbediener.md) | Zähltafelbediener nach Vorbild Original-BTS, in zwei Phasen | akzeptiert |
| [0008](0008-auto-aussprache-serverseitig-badhub.md) | Automatische Aussprache-Vorschläge entstehen serverseitig bei badhub (opt-in) | vorgeschlagen |
