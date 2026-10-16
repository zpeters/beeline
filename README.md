# beeline

A minimal desktop API client in the spirit of Bruno. Collections are
plain folders of JSON files you can read, diff and hand-edit.

## Run

```
just run                     # opens the last collection
cargo run -- examples/demo   # open a folder as the collection
```

Requires Rust from `rustup` and the Nix flake via direnv (provides `just`).

## Features

- **Collections on disk** — a folder is a collection; each request is a
  `*.json` file and subfolders group them. New, rename, and move to Trash
  from the sidebar (right-click for folders and requests).
- **Request editor** — method, URL, query params (kept in sync with the
  URL), headers, body (none / JSON / text / form URL-encoded), auth
  (bearer / basic).
- **Environments** — `environments/<name>.json` in the collection; pick
  one in the sidebar and use `{{name}}` anywhere in a request.
- **Response viewer** — status, time, size, pretty JSON, headers, copy.

Shortcuts: Cmd/Ctrl+Enter sends, Cmd/Ctrl+S saves.

## File format

```
my-api/
  environments/dev.json      {"variables": {"base": "https://api.example.com"}}
  users/list users.json      a request; its name is the file name
  health.json
```

A request file — every field is optional:

```json
{
  "method": "POST",
  "url": "{{base}}/users?notify=true",
  "params": [{ "key": "notify", "value": "true", "enabled": true }],
  "headers": [{ "key": "Accept", "value": "application/json", "enabled": true }],
  "body": { "type": "json", "text": "{\"name\": \"bee\"}" },
  "auth": { "type": "bearer", "token": "{{token}}" }
}
```

`body.type`: `none`, `json`, `text`, `form` (with `fields`).
`auth.type`: `none`, `bearer` (`token`), `basic` (`username`, `password`).
See `examples/demo` for a working collection.

## Recipes

- `just build` — build the crate
- `just run` — launch the app
- `just test` — run the test suite (`cargo test -- --ignored` adds a live
  HTTPS check)
- `just check` — fmt, clippy (warnings are errors), and tests

## Layout

- `src/` — the crate; see `AGENTS.md` for module boundaries
- `examples/demo/` — sample collection (tested to parse and resolve)
- `docs/superpowers/` — design specs and plans
- `TODO.md` — ideas not yet scoped
