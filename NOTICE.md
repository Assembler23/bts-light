# NOTICE – Herkunft & Clean-Room-Vorgehen

## Zusammenhang mit phihag/bts

BTS Light verfolgt denselben Zweck wie [phihag/bts](https://github.com/phihag/bts)
(und dessen Fork [letilo/bts](https://github.com/letilo/bts)): einen Badminton
Tournament Planner (BTP) über das TP-Network-Protokoll auszulesen.

Das Repo `phihag/bts` enthält **keine Lizenzdatei**. Ohne Lizenz sind alle Rechte
am Code vorbehalten – ein Fork oder eine Code-Übernahme ist daher rechtlich nicht
zulässig. BTS Light ist deshalb **kein Fork** und übernimmt **keinen Quellcode**
aus phihag/bts oder letilo/bts.

## Clean-Room-Regeln für die BTP-Protokoll-Implementierung

Das TP-Network-Wire-Protokoll selbst (XML-Schema `VISUALXML`, gzip-Framing,
4-Byte-Längen-Header, Action-IDs, Feldnamen) ist von Visual Reality /
TournamentSoftware definiert, nicht von phihag erfunden. Es ist beobachtbar und
darf eigenständig nachgebaut werden.

**Erlaubt:**

- Das Wire-Protokoll aus öffentlicher Doku, Netzwerk-Mitschnitten (Wireshark)
  und Verhaltensbeobachtung gegen ein laufendes BTP rekonstruieren.
- BTP-Antwort-Mitschnitte (`.gz`-Binärdumps) als Test-Fixtures verwenden – das
  sind Daten von Visual Reality, kein Code.
- Die Funktionsweise von phihag/bts als Referenz *lesen*, um das Protokoll zu
  *verstehen*.

**Nicht erlaubt:**

- Quellcode aus phihag/bts oder letilo/bts kopieren, übersetzen oder Zeile für
  Zeile nachbilden – auch nicht sprachübersetzt (JS → Rust).

## Drittanbieter-Komponenten

Abhängigkeiten werden über Cargo (`src-tauri/Cargo.toml`) und npm
(`package.json`) verwaltet; ihre jeweiligen Lizenzen gelten unverändert.
