# Plan — basic full client

Spec: `docs/superpowers/specs/2026-10-15-basic-full-client-design.md`.

Each step is one jj commit on `main`, green on `just check`. Pure modules
are written test-first.

1. **Request model + editor.** `model`, `query`, `resolve` (with an empty
   variable map), `http`. UI: method, URL, Send; tabs Params / Headers /
   Body / Auth with a shared key/value table widget.
2. **Response viewer.** Status line (status, reason, time, size), Body tab
   with pretty JSON and copy, Headers tab.
3. **Collections on disk.** `collection` module; open folder (rfd);
   sidebar tree; open/save (Cmd/Ctrl+S); dirty marker; new request, new
   folder, rename, delete with modals; remember last collection.
4. **Environments.** `vars`; environment loading; picker; resolve uses
   the selected environment; environment editor window; remember the
   selected environment.
5. **Polish + docs.** Shortcuts (Cmd/Ctrl+Enter sends), README/AGENTS/TODO
   updates, sample collection under `examples/`.
