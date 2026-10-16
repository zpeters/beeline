# beeline

A super-minimal API client desktop app. Send an HTTP request, see the
response. A lighter alternative to Bruno.

## Run

```
just run
```

Requires Rust from `rustup` and the Nix flake via direnv (provides `just`).

## Recipes

- `just build` — build the crate
- `just run` — launch the app
- `just test` — run the test suite
- `just check` — fmt, clippy (warnings are errors), and tests

## Layout

- `src/` — the crate (UI and request/response logic)
- `docs/superpowers/` — design specs and plans
- `TODO.md` — ideas not yet scoped
- `AGENTS.md` — guidance for AI coding agents
