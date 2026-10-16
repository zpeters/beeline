# beeline — basic full client

Date: 2026-10-15. Status: agreed feature scope; design written overnight
for morning review.

## Intent

A minimal desktop API client in the spirit of Bruno: fast, local,
everything stored as plain files you can read, diff and hand-edit. Not a
clone — no scripting, no tests, no plugins, no cloud, no `.bru` format.
Config is plain JSON.

Success: open a folder of JSON request files, pick an environment, edit a
request, send it, read the response, save the request back to disk — all
without the app feeling heavy.

## Agreed features

Chosen by the user on 2026-10-15:

1. **Collections on disk** — a collection is a folder. Each request is one
   `*.json` file; subfolders group requests. Sidebar tree; click to open;
   Cmd/Ctrl+S saves; new request, new folder, rename, delete.
2. **Headers + query params** — key/value tables with enable checkboxes.
   Query params stay in sync with the URL.
3. **Response viewer** — status, time, size; body tab (pretty JSON when
   the body parses as JSON) and headers tab; copy body.
4. **Environments + `{{vars}}`** — environment JSON files in the
   collection, an environment picker, `{{name}}` substitution in URL,
   params, headers, body and auth.
5. **Auth helpers** — none / bearer token / basic.
6. **Body types** — none / JSON / text / form-urlencoded, with an
   automatic `Content-Type` unless the user sets one.

Out of scope: request tabs, copy as cURL, history, scripting, assertions,
cookies, GraphQL, websockets, file uploads, secrets management.

## On-disk format

```
my-api/                      # the collection (any folder)
  environments/
    dev.json                 # environment "dev"
    prod.json
  users/                     # folder = group
    list users.json          # request "list users"
    create user.json
  health.json
```

- A request's name is its file stem. A folder's name is its directory
  name. `environments/` at the collection root is reserved and not shown
  in the tree. Hidden entries (`.` prefix) are ignored.
- Every field has a default, so `{"url": "https://example.com"}` is a
  valid request file.
- Files are written as pretty JSON (2-space indent) with a trailing
  newline, so diffs stay small.

Request file:

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

- `body.type`: `none` | `json` (`text`) | `text` (`text`) | `form`
  (`fields`: key/value list).
- `auth.type`: `none` | `bearer` (`token`) | `basic` (`username`,
  `password`).
- `enabled` defaults to `true`.

Environment file:

```json
{ "variables": { "base": "https://api.example.com", "token": "abc" } }
```

## Behavior

**Query params sync.** The URL holds the query string. Editing the URL
re-parses it into the enabled params (disabled params are kept). Editing
the params table rebuilds the URL query from the enabled params. The
params list is also saved in the file so disabled params survive.

**Variables.** `{{name}}` is replaced with the selected environment's
value. Whitespace inside the braces is allowed (`{{ name }}`). Unknown
variables are left as written, so the problem stays visible in the
request. Substitution happens only at send time; the editor always shows
the raw text.

**Resolving a request** (pure function): interpolate everything, keep
enabled headers, add the auth header (unless an `Authorization` header is
set), encode the body, add `Content-Type` for the body type (unless set),
and normalize the URL with the `url` crate (adds percent-encoding for
spaces etc., validates). Errors are shown in the response area.

**Sending.** One request at a time on a worker thread. Non-2xx responses
are normal responses. Elapsed time covers connect through reading the
body.

**Saving.** Cmd/Ctrl+S writes the open request. The title shows a `•`
when there are unsaved changes. Opening another request while dirty asks
Save / Discard / Cancel. Delete asks for confirmation. With no collection
open, the editor works on an unsaved scratch request (the MVP behavior).

**Persistence.** The last opened collection and selected environment are
remembered with eframe's storage.

Environments can be created and edited in a small window (key/value
table, save); they are also plain files to hand-edit.

## Architecture

Pure core, thin shell (see `AGENTS.md`).

| Module | Role | I/O |
|---|---|---|
| `model.rs` | serde types: `RequestFile`, `KeyValue`, `Body`, `Auth`, `Environment` | none |
| `vars.rs` | `{{var}}` interpolation | none |
| `query.rs` | URL ⇄ params sync | none |
| `resolve.rs` | `RequestFile` + vars → `Resolved` (method, url, headers, body) | none |
| `http.rs` | send a `Resolved`, return `Response` (status, headers, body, time, size) | network |
| `collection.rs` | scan tree, read/write requests and environments, create/rename/delete | filesystem |
| `app.rs` + `ui/*.rs` | egui state and rendering | none beyond calling the above |

New dependencies, each for one job: `serde` + `serde_json` (file
format), `url` (URL normalizing + form encoding), `base64` (basic auth),
`rfd` (native folder picker), `tempfile` (tests only). eframe gains the
`persistence` feature.

## Testing

- Unit tests for `vars`, `query`, `resolve`, `model` (defaults and
  round-trip) and `collection` (against a temp dir).
- `http` and the UI have no unit tests; verify by build, clippy, and a
  manual launch plus one real request.
