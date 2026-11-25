# Folder (`folder`)

Puts a folder of files in a Shakedown world: one folder per instance, on local disk. Use it for any state that is
plain files, such as notes, reports or exported documents an agent reads and writes. It needs no service.

| | |
|---|---|
| Program | `shakedown-folder` (Rust, crate `shakedown-folder`) |
| Restore | exact, from a save (a copy of the folder). No approximate restore. |
| World clock | not used |
| Agent tools | `list_files`, `read_file`, `write_file` |

## What it needs

- Local disk under `SHAKEDOWN_SAVES`. No service, no Docker and no GPU.

| Variable | Default | Purpose |
|---|---|---|
| `SHAKEDOWN_SAVES` | `saves` (relative to the working folder) | Where the live folders and saves go. `shaked` always passes its own. |

## Use it

A world names it as a part, for example `{part: "notes", integration: "folder", config: {root: "seed/notes"}}`.

| Config key | Type | Default | Meaning |
|---|---|---|---|
| `root` | string | none | A folder whose files the instance starts with. Ignored when the instance starts from a save. A path that is not a folder is refused (`-32602`). |

With no `root`, the instance starts empty.

| Tool | Effect | Does |
|---|---|---|
| `list_files` | read | Lists every file with its size. Reads `file/*`. |
| `read_file` | read | Reads a text file (`path`). A file that is not UTF-8 text is refused. Reads `file/<path>`. |
| `write_file` | write | Writes a text file (`path`, `content`), replacing it and creating folders as needed. Writes `file/<path>`. |

A path may not be absolute, may not contain `..`, and may not pass through a symlink that leads out of the
instance folder. Such a call is refused.

How it behaves:

- **Instances.** Each instance is the folder `$SHAKEDOWN_SAVES/folder/.live/<instance>`. Save ids never start with
  a dot, so `shaked`'s sweep of save folders skips `.live`. `stop` removes the folder, so save first.
- **Saves.** A save is `$SHAKEDOWN_SAVES/folder/<save>/`. A file whose size and modification time match the copy in
  the instance's previous save is hard-linked to it, so only changed bytes are written. The previous save is the
  last one the instance made, or the save it started from. An instance started without a save has none, so its
  first save copies everything. Starting from a save always copies, so a live folder never changes a save.
  Symlinks are kept as symlinks. Sockets and FIFOs are left out.
- **inspect and diff.** `inspect` keys are `file/<path>`, each `{bytes, content}` (`content` is null for a file
  that is not text). An empty key list returns every file. `diff` compares file contents by hash and names files
  only one side has.
- **usage and gc.** `usage` is 0: everything is under `SHAKEDOWN_SAVES`, which `shaked` measures itself. `gc`
  drops nothing in the program. `shaked` removes the save folders `keep` does not keep.

## How it's tested

```bash
cargo test -p shakedown-folder                      # from the repository root
```

The tests use a temporary folder only, so there is no `live_` test. They cover the three tools, refused paths
(`..`, absolute, a symlink out of the folder), save and restore, `inspect` and `diff`, hard-linking on a second
save, seeding from `root`, unknown instances and saves (`-32002`), `drop_save`, failed starts and saves leaving no
half-written folder, and read-only folders. One test builds `shakedown-part-check` and runs it on `check/`, the
package `folder-check` (this program with example calls):

```bash
SHAKEDOWN_BIN_DIR=core/target/debug core/target/debug/shakedown-part-check integrations/folder/check
```

## Limits

- Text files only through the tools. Binary files can be seeded from `root` and are saved, but `read_file` refuses
  them and `inspect` shows them with null `content`.
- Each save is a full folder tree. Only files unchanged since the previous save share space, through hard links.
- No approximate restore and no use of the world clock.
