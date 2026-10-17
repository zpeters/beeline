# AGENTS.md

Guidance for AI coding agents working in this repo.

## What this is

beeline is a minimal API client desktop app in the spirit of Bruno, not a
clone. Collections are folders of plain JSON request files; environments
supply `{{vars}}`. The current scope is in
`docs/superpowers/specs/2026-10-15-basic-full-client-design.md`.

Request tabs (several open requests, one tab each) are in scope; see
`docs/superpowers/specs/2026-10-16-request-tabs-design.md`.

Request history is session-only (the last sends of each tab, in memory);
history saved to disk is not in scope.

Out of scope for now: scripting, assertions,
cookies, GraphQL, websockets, file uploads, a plugin system, and any
cloud sync. Do not build them speculatively. Future ideas live in
GitHub Issues: https://github.com/zpeters/beeline/issues.

## Stack and layout

Pure modules (no I/O, unit-tested):

- `model.rs` — the JSON file format (serde). Every field has a default.
- `vars.rs` — `{{name}}` interpolation and unknown-name detection.
- `query.rs` — URL query string ⇄ params table sync.
- `resolve.rs` — request + variables → exactly what goes on the wire.
- `format.rs` — response display helpers (pretty JSON, sizes, times).
- `curl.rs` — a resolved request as a shell-safe `curl` command.
- `spark.rs` — session-only response-time series for sidebar sparklines.
- `history.rs` — a capped, numbered list of a tab's sends (session only).
- `highlight.rs` — splits one line of JSON into tokens for syntax colours.
- `recents.rs` — the recent-collections list.
- `settings.rs` — the settings file format and its limits.

I/O modules:

- `http.rs` — sends a resolved request (ureq). Tested against a local
  socket; one `#[ignore]` live HTTPS test.
- `collection.rs` — all filesystem access: scan, read/write requests and
  environments, create/rename, move to Trash. Tested against temp dirs.

- `tabs.rs` — pure ordered list of open tabs with one active. No iced.

UI (iced 0.14, Elm-style: `update` returns a `Task`, `view` builds
`Element`s from state; verified by build + launch):

- `app.rs` — app state (one `Doc` per request tab), file actions, modals,
  frame layout.
- `menu.rs` — the macOS menu bar (muda): File, Collection and app menus
  send `menu::Command`s to `App::update`. No Edit menu: its ⌘C/⌘V items
  would take those keys from iced's text fields. On macOS the bar owns ⌘S
  and ⌘W, so the in-window shortcut handler skips them.
- `ui/editor.rs`, `ui/response.rs`, `ui/highlight.rs` (iced Highlighter
  over `highlight.rs`), `ui/sidebar.rs`, `ui/tab_strip.rs`,
  `ui/dialog.rs`, `ui/theme.rs`, `ui/mod.rs` (shared key/value table).
  Widgets edit the state they are handed or return messages; they do no
  I/O.

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
  clocks. The iced layer only renders state and forwards user actions.
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
just bundle-mac   # build target/release/bundle/osx/beeline.app (and a .dmg)
just install-mac  # copy beeline.app into /Applications
just dmg          # build dist/beeline-<version>.dmg (drag-to-Applications)
```

## Releases

`.github/workflows/release.yml` makes them. Pushing a `v<version>` tag
builds `beeline-<version>-macos-arm64.dmg` and
`beeline-<version>-linux-x86_64.tar.gz` and opens a **draft** GitHub
release with both attached. The tag must match `Cargo.toml`.

1. Bump `version` in `Cargo.toml` (it also changes `Cargo.lock`) and
   merge it to `main`.
2. `jj tag set v<version> -r main`, then `git push github v<version>`
   (and to `origin`).
3. On GitHub, edit the draft's notes and publish it.

If the release already exists (made by hand on GitHub), the files are
uploaded to it instead. To build for an existing tag, use "Run workflow"
on the Release workflow in the Actions tab and give it the tag.

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

- The UI is iced 0.14. Route every user action through a `Message`
  variant handled in `App::update`; widgets never mutate `App` directly.
  Do not add an async runtime unless a feature needs it; a blocking
  request on a background thread with a oneshot channel is enough.
- Keep `serde_json`'s `preserve_order` feature: it keeps response keys in
  server order when pretty-printing.
- The URL holds the query string; `params` mirrors it. Disabled params
  exist only in `params`.
- Screenshots with `screencapture` are allowed on this Mac to verify UI
  changes. The first run may raise a screen-recording permission prompt
  for the terminal; that is expected.
