# 0001 — Quality-Gate & Branch Protection

- **Status:** accepted
- **Datum:** 2026-06-26

## Kontext

bts-light wird als Auto-Update-Installer an Vereine/Turnierhallen ausgeliefert. Ein kaputter
`main`, der getaggt wird, shipped einen kaputten Installer in die Halle. Bisher konnte direkt auf
`main` gepusht werden; die CI (`build`: cargo fmt/clippy/test + npm build) lief zwar auf PR und
main, war aber nicht erzwungen. Das Repo ist **public** → Branch Protection ist kostenlos.

## Entscheidung

Drei-Schichten-Modell aus dem Quality-Playbook übernehmen:

1. **Lokaler Pre-Commit-Hook** (`.githooks/`, via `core.hooksPath`): `cargo fmt --check` vor dem
   Commit, `cargo clippy` im `pre-push`. Spiegelt die CI.
2. **CI** (`build`, unverändert): Pflicht-Check.
3. **Branch Protection auf `main`:** PR-Pflicht, `build` als *required status check*, kein
   Direkt-/Force-Push, lineare Historie, `enforce_admins=true`. **Self-Merge, 0 Approvals.**

Der Release bleibt **entkoppelt**: ein Merge nach `main` shipped nicht; Release ist weiterhin ein
manueller Tag (`v*` → `release.yml`). Damit stört die PR-Pflicht den Ship-Loop nicht.

## Alternativen

- **Direkt auf `main` (Status quo)** — verworfen: ein ungeprüfter Push kann getaggt und an Hallen
  ausgeliefert werden.
- **Approval-Pflicht** — verworfen für Solo-Betrieb (nur Flaschenhals). Später nachrüstbar.
- **Leichtgewichtig (nur Force-Push-Sperre, kein PR-Gate)** — verworfen: ohne PR greifen
  required-Checks nicht, `main` bliebe ungeschützt gegen rote Builds.

## Konsequenzen

- Was auf `main` landet, hat `build` bestanden — die Tag-/Ship-Basis ist immer grün.
- Auch der Owner geht über Branch + PR + Self-Merge. Escape-Hatch im Notfall: Regel kurz via
  GitHub-UI/API deaktivieren.
- `required_linear_history` → Squash-/Rebase-Merge.
- Hooks sind via `--no-verify` umgehbar (Komfort/Schnell-Feedback); die CI ist die nicht-umgehbare
  Instanz.
