# ADR-0010: Dependabot-Updates mit Minor-/Patch-Umfang automatisch mergen

- **Status:** Angenommen
- **Datum:** 2026-08-08
- **Betrifft:** `.github/workflows/dependabot-automerge.yml`, `.github/dependabot.yml`

## Kontext

`main` ist geschützt und verlangt zwei Prüfungen: `build` und `secret scan`.
Dependabot öffnet wöchentlich PRs für npm, Cargo und GitHub Actions; minor/patch
sind je Ökosystem gruppiert, Major kommt einzeln.

Am 2026-08-08 lagen **13 Dependabot-PRs offen**, die ältesten seit dem 28.07. —
also seit elf Tagen.

In diesem Repo wiegt das schwerer als anderswo. Der CI-Job läuft auf
`windows-latest`, weil die App eine Tauri-Windows-Anwendung ist, und
Windows-Minuten zählen bei GitHub **doppelt**: gemessen ~626 s pro Lauf, also
rund **22 berechnete Minuten**. Ein Rückstau kostet hier nicht nur
Aufmerksamkeit, sondern spürbar Kontingent, sobald er abgearbeitet wird — 13
PRs entsprechen grob 290 Minuten, knapp 10 % des Monatsbudgets von 3000.

## Entscheidung

Dependabot-PRs mit Umfang **minor** oder **patch** bekommen automatisch `--auto`
gesetzt. GitHub merged sie, **sobald die Required-Checks grün sind** — und nur
dann. Der Workflow selbst merged nichts.

**Major-Updates ausdrücklich nicht.** Sie bekommen einen erklärenden Kommentar
und bleiben liegen.

Dies revidiert die frühere Festlegung im Kopf von `.github/dependabot.yml`
(„bewusst ohne Auto-Merge", übernommen aus bvbb-hub ADR 0015). Der dortige
Kontext ist ein anderer: bvbb-hub verarbeitet Personen- und Abrechnungsdaten,
bts-light ist eine lokal laufende Turnier-Anwendung.

## Warum die Grenze bei Major liegt — hier besonders

Der allgemeine Grund gilt auch hier: Ein Major kann grün durchlaufen und
trotzdem Verhalten ändern, das kein Test abdeckt.

Dazu kommt ein Grund, den es nur in diesem Repo gibt: **Ein großer Teil der
Rust-Abhängigkeiten steht auf `0.x`.** Nach Cargo-Konvention ist dort schon ein
Minor-Sprung ein Bruch — `mdns-sd` 0.13 → 0.20 überspringt sieben solcher
Stufen. Dependabot ordnet diese Sprünge außerhalb der minor/patch-Gruppe ein,
sie erscheinen also als Einzel-PR und fallen damit von selbst aus dem
Automatismus. Das ist kein Zufall, auf den man sich verlassen sollte — deshalb
prüft der Workflow den Umfang zusätzlich selbst.

## Die Gruppierung ist Teil der Entscheidung

`.github/dependabot.yml` fasst minor/patch je Ökosystem zu **einem** PR zusammen.
Das ist die Voraussetzung dafür, dass Auto-Merge hier trägt: Ohne Gruppen würde
jedes einzelne Patch-Update einen eigenen 22-Minuten-Windows-Build auslösen. Wer
die Gruppen entfernt, verwandelt diesen ADR in einen Dauerregen.

## Sicherheitshinweis zum Trigger

Der Workflow läuft auf `pull_request_target`, weil `pull_request` auf
Dependabot-PRs nur einen **lesenden** Token vergibt.

Der bekannte Fallstrick dieses Triggers ist, **PR-Code auszuchecken und
auszuführen** — er läuft im Kontext des Basis-Branch mit Schreibrechten. Das
passiert hier nicht: kein `actions/checkout`, kein Build, kein Skript aus dem PR;
der Job liest GitHub-API-Metadaten und ruft `gh` auf. Die `if`-Bedingung prüft
**zweifach** auf Dependabot (`github.actor` *und* `pull_request.user.login`).

Der Job läuft bewusst auf `ubuntu-latest`, nicht auf Windows: Er braucht keine
Zielplattform, und Linux-Minuten zählen einfach.

## Folgen

**Positiv.** Aktualisierungen erreichen `main` in Stunden statt Wochen. Der
Rückstau, der diesen ADR ausgelöst hat, entsteht nicht erneut. Menschliche
Aufmerksamkeit bleibt für Major-Sprünge übrig, wo sie etwas bewirkt.

**Negativ.** Eine fehlerhafte Minor-Version, die beide Prüfungen besteht, landet
ohne Zwischenhalt auf `main`. Abgefedert wird das dadurch, dass `main` **nicht**
automatisch veröffentlicht wird: Ein Release entsteht erst durch einen
Versions-Tag, und der ist eine bewusste Handlung.

**Rücknahme.** Workflow-Datei löschen. Es gibt keinen Zustand, der davon abhängt.

## Verworfene Alternativen

**Alles von Hand lassen, dafür regelmäßig hinsehen.** Verworfen — der Zustand vom
2026-08-08 *war* dieser Zustand. Die PRs waren sichtbar, wöchentlich kamen neue,
und sie lagen trotzdem elf Tage.

**Auto-Merge nur für Security-Updates.** Verworfen als scheinbar konservativer.
Ein reguläres Update von heute ist der Sicherheits-Fix von übermorgen; die
Trennung verzögert genau die Aktualisierungen, die den Abstand klein halten.

**Den CI-Job nach Linux verlegen, um Major-Merges billiger zu machen.** Nicht
entschieden, sondern offen: `cargo fmt`, `clippy` und die Node-Tests liefen
vermutlich auch auf Linux, nur das Bauen braucht Windows. Das ist eine eigene
Untersuchung wert (Tauri braucht dort GTK-Abhängigkeiten) und gehört nicht in
diesen ADR.
