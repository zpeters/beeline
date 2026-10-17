# TODO

- windows packaging / install

- **Sign and notarise the macOS app.** `just dmg` builds an unsigned
  disk image, so Gatekeeper warns when it is opened on another Mac. Needs
  an Apple Developer ID; cargo-bundle has `apple_signing_*` settings, and
  notarising means submitting with `xcrun notarytool` and stapling.

- Request history (left out of the basic client on purpose; sparklines
  keep response times for the session only).

- **Import from API docs (Paprika-style).** Browse an API documentation
  site inside the app, then parse the page into a collection of
  endpoint requests. For APIs with no OpenAPI spec, where today the
  endpoints are copied by hand. Start with a few popular doc layouts.
  Down the road; not part of the basic full client.

- **Export, and more import formats.** Import of cURL, OpenAPI 3, Postman
  v2.1 and Bruno is done. Still to do: export (Postman v2.1 first), and
  importing Insomnia and HAR.

- github

- **Edit menu on macOS.** The standard one breaks ⌘A/⌘C/⌘V in text fields
  (see `src/menu.rs`). Needs custom Edit items that act on iced's focused
  field, without taking the keys from it.
