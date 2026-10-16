# TODO

Feature ideas and known gaps not yet scoped into a plan. Each needs its
own brainstorming pass before implementation (see `docs/superpowers/specs/`
for design docs and past plans).

## Next up

- Click through the whole UI by hand (send, save, rename, move to Trash,
  environments). It was built overnight and verified only by tests,
  build and launch.

## Known gaps

- Environments cannot be renamed in the app; rename the file.
- Environment variables are saved as a sorted map, so the editor
  re-sorts rows on save.
- Secrets in environment files are plain text in the collection folder.

## Ideas

- Request tabs, copy as cURL, request history (left out of the basic
  client on purpose).

- **Import from API docs (Paprika-style).** Browse an API documentation
  site inside the app, then parse the page into a collection of
  endpoint requests. For APIs with no OpenAPI spec, where today the
  endpoints are copied by hand. Start with a few popular doc layouts.
  Down the road; not part of the basic full client.
- **Import/export common formats.** For example OpenAPI, Postman
  collections, Bruno, Insomnia, HAR and cURL. Down the road.

## Recently shipped

- **Basic full client** (2026-10-15) — collections on disk, request
  editor (params, headers, body types, auth), environments with
  `{{vars}}`, response viewer.
- **Scaffold** — initial project setup.
