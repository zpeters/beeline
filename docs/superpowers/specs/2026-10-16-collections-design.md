# Collections: create, rename, delete, recents

## Intent

A collection is a folder of requests. Today the app can only open one, with
the folder picker. Add the rest of the lifecycle in the app, and remember
the collections used recently.

## Decisions

- **Create** picks a parent folder, then asks for a name. It makes an empty
  folder. It does not add the examples.
- **Rename changes the folder name on disk.** Open tabs under the collection
  follow the new path, as renames inside a collection already do.
- **Delete moves the folder to the Trash**, after the existing confirmation
  dialog. Its tabs close and the app shows no collection.
- **Recents** are the last 10 collections opened, newest first, stored in
  `~/.config/beeline/state.json`. Missing folders are dropped when the list
  is read.
- **The switcher** is at the top of the sidebar, a pick list of recents. The
  current collection's Rename and Move to Trash sit beside it, and New
  collection beside those.

## Shape

- `src/recents.rs` — pure list: put a path at the front without duplicates,
  cap the length, drop missing paths, follow a rename, forget a path.
- `collection.rs` — existing `rename`, `delete` and `validate_name` are reused.
- `app.rs` — `SavedState.recents`; new name and confirm flows for create and
  rename; delete clears the collection when it is the current one.
- `ui/sidebar.rs` — the switcher row and its buttons.

## Not in scope

Renaming a collection from outside the app is picked up by the normal tree
reload only; no file watching. Sorting or pinning recents.
