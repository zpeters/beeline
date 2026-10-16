# Import: cURL, OpenAPI 3, Postman v2.1, Bruno

## Decisions (user)

- Import all four: a pasted cURL command, OpenAPI 3 (JSON or YAML),
  Postman collection v2.1, and a Bruno collection folder.
- No export yet.
- An import lands in a new folder inside the current collection. Nothing is
  overwritten.
- "Import from API docs" stays in TODO for later.

## Builder's calls

- **Entry point:** the collection menu gets Import cURL…, Import file…
  (OpenAPI or Postman, detected from the content) and Import Bruno folder….
- **cURL** makes one request, so it lands at the collection's top level, not
  in a folder of its own. It is named from its method and path.
- **Folder names** come from the source: the API title, the Postman
  collection name, or the Bruno folder name. A name already taken gets
  " 2", " 3", and so on.
- **Request file names** are sanitised (no `/` or `:`), and duplicates in
  one folder get " 2", " 3".
- **OpenAPI:** one request per operation, in folders by first tag. The URL is
  the first server's URL plus the path, with `{id}` path parameters written
  as `{{id}}`. Header and query parameters become rows; a JSON request body
  uses the spec's example if it has one, else `{}`.
- **Postman and Bruno** keep their `{{vars}}`, which use the same syntax.
  Environments are not imported.
- Things beeline cannot represent (multipart form bodies, scripts, tests)
  are skipped, and the import reports how many requests it wrote.

## Shape

- `src/import/` — one pure parser per format, each returning a list of
  (relative path, `RequestFile`). Unit-tested with inline fixtures.
- `src/import/mod.rs` — naming, de-duplication, and writing the files into
  the new folder through `collection.rs`.
- `app.rs` — menu items, file pickers, and the paste dialog for cURL.

## Dependencies

- `shell-words` — splits a pasted command line the way a shell would.
- A YAML parser for OpenAPI YAML files.
