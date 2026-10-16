# AGENTS.md

Guidance for AI coding agents working in this repo.

## What this is

beeline is a minimal API client desktop app in the spirit of Bruno, not a
clone. Collections are folders of plain JSON request files; environments
supply `{{vars}}`. The current scope is in
`docs/superpowers/specs/2026-10-15-basic-full-client-design.md`.

Out of scope for now: tabs for several open requests, copy as cURL,
history, scripting, assertions, cookies, GraphQL, websockets, file
uploads, a plugin system, and any cloud sync. Do not build them
speculatively. Future ideas live in `TODO.md`.

The request editor's own tabs (Params, Headers, Body, Auth, Vars) are in
scope; they are not the "tabs for several open requests" above.

## Stack and layout

Pure modules (no I/O, unit-tested):

- `model.rs` — the JSON file format (serde). Every field has a default.
- `vars.rs` — `{{name}}` interpolation and unknown-name detection.
- `query.rs` — URL query string ⇄ params table sync.
- `resolve.rs` — request + variables → exactly what goes on the wire.
- `format.rs` — response display helpers (pretty JSON, sizes, times).

I/O modules:

- `http.rs` — sends a resolved request (ureq). Tested against a local
  socket; one `#[ignore]` live HTTPS test.
- `collection.rs` — all filesystem access: scan, read/write requests and
  environments, create/rename, move to Trash. Tested against temp dirs.

UI (egui/eframe 0.37, immediate mode; verified by build + launch):

- `app.rs` — app state, file actions, modals, frame layout.
- `ui/editor.rs`, `ui/response.rs`, `ui/sidebar.rs`, `ui/mod.rs`
  (shared key/value table). Widgets edit the state they are handed or
  return actions; they do no I/O.

Other:

- `examples/demo/` — sample collection; a test checks it resolves.
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
  feature needs it; a blocking request on a background thread is enough.
- egui/eframe 0.37 renamed things: `App::ui(&mut self, ui: &mut Ui, ..)`
  replaces `update`, panels are `egui::Panel::{left,top,bottom}` and take
  a `&mut Ui`, and `Id::new` is deprecated (`Id::unique`).
- Keep `serde_json`'s `preserve_order` feature: it keeps response keys in
  server order when pretty-printing.
- The URL holds the query string; `params` mirrors it. Disabled params
  exist only in `params`.
- Screenshots with `screencapture` are allowed on this Mac to verify UI
  changes. The first run may raise a screen-recording permission prompt
  for the terminal; that is expected.
