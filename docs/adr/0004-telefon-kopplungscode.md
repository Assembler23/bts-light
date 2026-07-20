# ADR 0004 — Kopplung ferner Hallen über kurzlebigen 8-stelligen Telefon-Code

Status: akzeptiert (2026-07-17)

## Kontext

Die Kopplung eines Cloud-Slaves an den Master verlangt heute die Eingabe der
vollen `install_id` (36-stellige UUID). Am Turniertag steht die Crew der
fernen Halle oft nur telefonisch in Kontakt — eine UUID lässt sich praktisch
nicht fehlerfrei durchsagen. Gleichzeitig ist die `install_id` das
**Geheimnis** des Namespace (Bearer-Capability, R4/R6): Wer sie kennt, kann
Ansage-Daten lesen und seit [ADR 0003](0003-azure-tts-vererbung-relay.md)
auch den Azure-Key abrufen. Sie dauerhaft auf 8 Ziffern zu verkürzen
(10⁸ Kombinationen) wäre durchprobierbar und scheidet aus.

## Entscheidung

Die `install_id` bleibt unverändert das Namespace-Geheimnis. Zusätzlich kann
der Master beim Relay einen **kurzlebigen 8-stelligen Zahlen-Code** anfordern
(`POST /{ns}/pairing-code`): Der Relay erzeugt ihn kryptographisch zufällig
(`getrandom`, bereits im Dependency-Baum), hält ihn **nur im RAM**
(Code → Namespace, **1 Stunde** gültig, genau **ein aktiver Code je
Namespace**, neuer Code ersetzt den alten). Die ferne Halle tippt die
8 Ziffern ein; die App löst sie ein (`GET /pair/{code}`) und speichert die
zurückgegebene volle `install_id` als `master_namespace`. Nach der Kopplung
spielt der Code keine Rolle mehr; ein Code darf innerhalb seiner Gültigkeit
mehrfach eingelöst werden (mehrere ferne Hallen, ein Telefonat).

Schutz gegen Durchprobieren: begrenzte Gültigkeit (1 Stunde), sehr dünn besetzter
Code-Raum (aktive Codes ≈ Anzahl gerade koppelnder Master) und ein globales
Fehlversuchs-Limit am Relay (Sliding Window, danach `429`). Ein Code wird
nur für einen **verbundenen** Host-Namespace ausgestellt.

## Alternativen

- **`install_id` selbst 8-stellig numerisch:** verworfen — macht das
  dauerhafte Namespace-Geheimnis erratbar; bricht zudem alle bestehenden
  Kopplungen und QR-/Monitor-Links.
- **Kurzcode ohne Ablauf (dauerhafte Alias-Tabelle am Relay):** verworfen —
  gleicher Angriffsraum wie eine 8-stellige `install_id`, nur indirekt;
  zudem Zustandshaltung über Relay-Neustarts hinweg nötig.
- **Code in der App statt im Relay erzeugen:** verworfen — Kollisionen
  zwischen Namespaces müsste der Relay ohnehin arbitrieren; die Erzeugung
  am Relay ist der einfachste kollisionsfreie Ort.
- **Wörter-Code (z. B. drei Wörter):** verworfen — telefonisch anfälliger
  (Aussprache/Schreibweise) als Ziffern, und die Zielgruppe diktiert
  Zahlen problemlos.

## Konsequenzen

- Kopplung ist am Telefon in Sekunden erledigt; Tippfehler fallen sofort
  auf (Code wird ungültig quittiert statt still falsch gespeichert).
- Der lange Code funktioniert unverändert weiter (Fallback, z. B. wenn der
  Relay noch nicht aktualisiert ist).
- Relay wird um einen kleinen, flüchtigen Zustand reicher (Pairing-Map);
  ein Relay-Neustart macht offene Codes ungültig — verschmerzbar, der
  Master erzeugt einfach einen neuen.
- Rollout: Relay muss vor dem App-Release deployt sein, sonst schlägt das
  Einlösen mit klarer Fehlermeldung fehl (UUID-Weg bleibt nutzbar).
- **Bewusster Trade-off:** Das Fehlversuchs-Limit ist **global** (ein
  Zähler für den ganzen Relay, ohne IP-/Namespace-Bezug). Ein anonymer
  Angreifer kann es mit einer simplen Request-Schleife dauerhaft füllen und
  damit das **Telefon-Code-Einlösen relay-weit lahmlegen** (429 auch für
  legitime Hallen). Das ist akzeptiert, weil (a) der lange
  UUID-Kopplungs-Code als dokumentierter Fallback immer funktioniert,
  (b) die Alternative (per-IP-Limit) hinter nginx/Cloudflare
  X-Forwarded-For-Handling erfordert und mehr Komplexität kostet als das
  Feature rechtfertigt. Härtungs-Option bei Bedarf: nginx `limit_req` auf
  `/pair/` vor dem Relay.
- Neu zu bewerten, wenn Namespaces je ein echtes Auth-Token bekommen —
  dann sollte der Pairing-Code das Token aushändigen statt der nackten
  `install_id`.
