# folder integration

Puts a folder of files in the world: one folder per instance. Restore: exact from a save (a copy, with unchanged files
hard-linked); approximate: no. `uses_clock`: false. Verified: `cargo test -p shakedown-folder` runs on local disk only (no
Docker), including the `shakedown-part-check` conformance run. Nothing here needs a service, so there is no `live_` test.

Each instance is a folder of files; each save is a copy of it. Saves restore exactly.

- Live folders are `$SHAKEDOWN_SAVES/folder/.live/<instance>`. The leading dot keeps them from being
  taken for a save folder (save ids never start with one), so `gc` and shaked's save-folder sweep
  skip them. `stop` removes the folder, so save first.
- A save is `$SHAKEDOWN_SAVES/folder/<save>/`. Files unchanged since the instance's previous save (same
  size and mtime) are hard-linked to it, so only new bytes are written. After a respawn there is no
  previous save in memory, so the next save copies everything. Starting from a save always copies.
- Config `root` (optional): a folder whose files the instance starts with. Ignored when starting from a save.
- Tools: `list_files` (reads `file/*`), `read_file` (reads `file/<path>`), `write_file` (writes `file/<path>`).
  A path may not be absolute, contain `..`, or pass through a symlink that leaves the instance folder.
- `inspect` keys are `file/<path>` (`{bytes, content}`); `diff` compares file contents.
- `usage` is 0: everything lives under `SHAKEDOWN_SAVES`, which shaked measures.
- Conformance: `shakedown-part-check integrations/folder/check`.
