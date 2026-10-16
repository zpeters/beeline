# Import fixtures

Sample files for the import parsers, read by the tests in `src/import/`.

- `openapi/petstore-expanded.yaml` — the OpenAPI Initiative's Petstore
  example, from github.com/OAI/learn.openapis.org (Apache-2.0).
- `postman/shop.postman_collection.json` — written for these tests in the
  shape of a Postman v2.1 export: nested folders, folder and collection
  auth, a `noauth` override, disabled rows, a test script, and a form-data
  upload that has no beeline equivalent.
- `curl/*.sh` — commands in the style of Chrome's "Copy as cURL", GitHub's
  REST docs and Stripe's docs. Tokens are placeholders.
- `bruno/` — real Bruno collection files (to come).
