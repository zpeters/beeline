# Sidebar redesign after Tufte, with sparklines

## Intent

The sidebar has the right parts but looks heavy: filled buttons are the
loudest things on it, and the requests (the data) are the quietest. Apply
Tufte's principles so the data leads, and add per-request sparklines of
response times.

## Principles applied

- **Data-ink.** Toolbar actions become plain text buttons; fills go.
- **No repeated ink.** A row's ⋯ shows only when the row is hovered or
  selected. Its space stays reserved so rows do not shift.
- **Smallest effective difference.** The selected row gets a faint tint and
  a thin accent rule on the left, not a solid block.
- **Layering.** Hierarchy comes from size and grey, not boxes: collection
  name, then environment, then the tree. Folder counts are muted.
- **Micro/macro.** Each request row shows its method in small muted
  monospace before the name.

## Sparklines

- Word-sized line of the request's recent response times, right-aligned so
  rows compare down the column. Last 20 responses.
- **Session only** (user's decision): kept in memory, never written to disk.
- The end point is a dot: accent colour for success, red for a 4xx/5xx.
  The last time shows beside it in small muted text.
- **Animation:** a new response eases its point in over 400 ms. While a
  request is in flight, the end dot pulses. Frames are only requested while
  something animates.
- Transport errors (no response) add no point.

## Shape

- `src/spark.rs` — pure: the capped series and its scaling to a box, with
  the animated last point. Unit-tested.
- `collection.rs` — `Node::Request` carries the method, read during scan.
- `ui/sidebar.rs` — row layout and a `canvas` program that draws the line.
- `app.rs` — records responses per file, tracks arrival times and pending
  sends, and subscribes to frames while animating.

## Dependency

iced's `canvas` feature, which brings in `lyon` for path drawing. Needed
to draw the line; no other crate is added.
