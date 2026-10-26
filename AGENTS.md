## Agent skills

### Issue tracker

Issues are tracked in this repo's GitHub Issues via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Uses the five default triage labels (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`).

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.

## Commits

Commit subjects use a Conventional Commits header: `type: summary` with type one of `feat`, `fix`, `docs`, `test`, `refactor`, `chore`, `ci`, `build`, `perf`.

Branch names use the same types as a prefix: `type/short-summary`, e.g. `feat/build-and-show-mosaic`. A PR's title follows the commit convention, since a squash merge uses it as the subject.

## Git hooks

Enable the committed hooks once per clone: `git config core.hooksPath .githooks`. `pre-commit` runs `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`; `commit-msg` enforces the subject convention above (merge commits, and the root commit in CI, are exempt). CI runs the same checks.
