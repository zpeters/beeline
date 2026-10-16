# Request tabs

## Intent

Let people keep several requests open at once and switch between them,
as conventional API clients do. Today only one request is open; opening
another discards the first if it has no unsaved edits, and asks otherwise.

## Decisions

These come from the user unless marked as a call of the builder.

- **Sidebar click opens a tab.** A request that is already open gets
  focus instead of a second tab for the same file.
- **Tabs come back on restart.** The open file paths and the active tab
  are saved in `~/.config/beeline/state.json`. Unsaved edits are not kept.
- **Closing a dirty tab asks first.** It uses the existing Save / Discard /
  Cancel dialog.
- **Untitled requests get tabs.** Cmd+T opens one. It stays untitled until
  saved, as the scratch request does today.
- **Opening a collection closes all tabs.** Call of the builder: tabs
  belong to the collection they came from.
- **Quit and collection switch check every tab.** Call of the builder:
  each dirty tab is shown in turn with the same dialog. Discard reverts
  that tab to its saved copy.
- **Keyboard.** Cmd+T new tab, Cmd+W close tab. Call of the builder.
- **Each tab has its own request, body text, editor tab, response and
  in-flight send.** Environment selection stays global, as it is now.

## Shape

- `src/tabs.rs` — pure `Tabs<T>`: an ordered list with an active index.
  Open, activate, find, close (the tab to the right becomes active, else
  the one to the left), and iteration. No I/O, no iced. Unit-tested.
- `app.rs` — a `Doc` per tab holds what used to be single-request fields:
  request, saved copy, file path, body editor content, editor tab,
  pending flag, generation and result. Responses route to a tab by id.
- `ui/tab_strip.rs` — the tab strip above the editor: name, dirty dot, close
  button, horizontal scroll when there are many.

## Edge cases

- Rename, move and delete update every tab whose file they touch. A
  deleted file's tab becomes untitled, as the scratch request does now.
- A tab closed while its request is in flight drops the response.
- With no tabs, the main pane shows a short prompt instead of the editor.
- Restoring skips files that no longer exist.

## Not in scope

Reordering tabs by drag, pinned tabs, split views, and tabs across
collections.
