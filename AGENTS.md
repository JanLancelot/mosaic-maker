## Agent skills

### Issue tracker

Issues are tracked in this repo's GitHub Issues via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Uses the five default triage labels (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`).

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.

## Commits

Commit subjects use a Conventional Commits header: `type: summary` with type one of `feat`, `fix`, `docs`, `test`, `refactor`, `chore`, `ci`, `build`, `perf`.

Work lands on `main` through a PR from a branch named with the same types: `type/short-summary`, e.g. `feat/build-and-show-mosaic`. Create the branch before the first commit. A PR's title follows the commit convention, since a squash merge uses it as the subject.

Commit before reviewing: when `/implement` reaches `/code-review`, run `git fetch` and pass `origin/main` as the fixed point and the ticket as the spec, then commit the fixes it prompts as follow-up commits.

## Git hooks

Enable the committed hooks once per clone: `git config core.hooksPath .githooks`. `pre-commit` runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `scripts/check-glossary-terms.sh` (no GLOSSARY.md _Avoid_ terms in code); `commit-msg` enforces the subject convention above (merge commits, and the root commit in CI, are exempt). CI runs the same checks, plus the branch name on PRs.

## UI code

eframe/egui move fast and are newer than most examples (e.g. `App::ui`, `egui::Panel`). Before writing UI code, read signatures from the version pinned in `crates/mosaic-app/Cargo.toml`, in its source under `~/.cargo/registry/src/`.
