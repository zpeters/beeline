# AGENTS.md

Guidance for AI coding agents working in this repo.

## What this is

beeline is a super-minimal API client desktop app, a lighter replacement
for Bruno. The MVP sends one HTTP request and shows the response. Request
config is meant to be plain JSON, but JSON loading and saving are NOT in
the MVP.

Out of scope for now: collections, environments, auth helpers, scripting,
a plugin system, and any cloud sync. Do not build them speculatively.

## Stack and layout

- `src/main.rs` — app entry point and the egui/eframe UI.
- `src/` — keep request building and response handling as pure functions
  (no UI types) so they can be unit-tested. Put the HTTP call behind a
  small function so the UI does not know how the network works.
- `docs/superpowers/specs/` and `docs/superpowers/plans/` — design docs
  and implementation plans for past and future work. Read the relevant
  one before a large change; they record *why*, not just *what*.

## Principles

Cross-project coding philosophy lives in `~/.claude/PHILOSOPHY.md` and
`~/.claude/PRINCIPLES.md` — they apply here. This section is for
**project-specific** rules only:

- **Pure core, thin shell.** Request/response logic has no UI and no
  clocks. The egui layer only renders state and forwards user actions.
- **`#![forbid(unsafe_code)]`** at the crate root.
- **Minimal dependencies.** Each new crate needs a reason in the commit
  message.

## Dev environment

Nix flake (`flake.nix`) + direnv (`.envrc`) provide `just`.
Rust/Cargo comes from your own `rustup`, not the flake.

```
just              # list recipes
just run          # launch the app
just check        # fmt --check + clippy -D warnings + test
```

## Version control

`jj` (Jujutsu), colocated with git — use `jj`, not `git commit`. `git`
stays fine for read-only inspection, `push`, and tags.

**Run `jj new` before starting a new unit of work** if you're not
already on a fresh empty commit. Editing and re-`describe`-ing one
commit across unrelated changes silently collapses them into one.

```
jj new
# ... changes ...
jj describe -m 'Message'
jj bookmark set main -r @
```

This is a personal project: **merge locally, never push feature
branches or open PRs.** `jj git push --bookmark main` when a unit of
work is done and green.

## Testing expectations

- Logic changes: add or update `#[cfg(test)] mod tests` in the same file.
  Run `cargo test` before calling a change done.
- The UI layer has no unit suite. Verify it by `cargo build`, `clippy`,
  `fmt`, and a manual launch.
- TDD is the default for new features and bugfixes: failing test
  first, minimal code to green, then refactor.

## Before making a change

- The UI is egui (immediate mode). Do not add an async runtime unless a
  feature needs it; a blocking request on a background thread is enough
  for the MVP.
- Runtime behavior is not verified yet. Launch the app before saying a
  UI change works.
