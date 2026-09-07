## Was / Warum

<!-- kurz: was aendert dieser PR und warum -->

## Checkliste

- [ ] Lokal grün: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm run build`
- [ ] Keine Secrets/Tokens im Diff
- [ ] Doku aktualisiert, falls nötig (`docs/**`, `CHANGELOG`) · ADR (`docs/adr/`) bei Architekturentscheidung
- [ ] **Handbuch**: Ändert sich etwas, das ein Turnierleiter oder Schiedsrichter merkt?
      Dann das betroffene Kapitel nachziehen — der Lauf „Handbuch prüfen?" nennt es.
      Neue nachprüfbare Angabe (Beschriftung, Port, Standardwert)? Mit
      `<!-- pruef: … -->` verankern, damit sie beim nächsten Umbau nicht still veraltet.
- [ ] Bei BTP-/Relay-/Protokoll-Änderung: Regressionstest (`relay-proto`, `src-tauri/tests`) angefasst
- [ ] Release erfolgt separat per Tag (`git tag v… && git push --tags`), nicht durch diesen Merge
