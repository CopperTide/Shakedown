# Import from git: a customer's repo becomes the world

Design, 2026-11-22, branch `git-import` (project 4 of the airline demo). Owner's rule (2026-11-22): customers
import the systems and agents they already built, from their repo, so a world matches production on day one.
Rebuilding integrations inside Shakedown is not an onboarding path. The demo: point Shakedown at the airline
repo (`airline-world`, a project folder with Python parts and benchmark packages) and get its worlds,
scenarios, setups, experiments, views and packages, then deploy runs.

One operation, `project.import_git`, in the operation registry, so the CLI, the HTTP API, the control MCP
server, the assistant and the app all do the same thing.

## 1. Fetch

- The source is a git URL (`https://`, `ssh://`, `git@host:path`), a `file://` URL or a local path (absolute;
  the CLI resolves a relative one against its working folder). Other transports (`ext::`, `fd::`) are refused:
  every git call runs with `-c protocol.allow=never` and only `https`, `http`, `ssh`, `git` and `file`
  allowed.
- The ref is a branch, a tag or a full commit SHA (default `HEAD`). shaked makes an empty repository, fetches
  that one ref with `--depth 1`, and checks it out detached, with hooks off (`core.hooksPath=/dev/null`) and no
  terminal prompts. Submodules are not fetched.
- The checkout lands in the project's own area, `<project>/.shakedown/imports/<id>/<commit12>/`. The id comes
  from the repo's name (`airline-world`), or is given. `.shakedown/` is git-ignored in every project shaked
  makes, so a checkout is never committed into the importing project. Hosted shaked files each org's project
  under `Layer::project_dir`, so each org's imports land in its own project.
- The commit SHA is recorded in `<project>/.shakedown/imports/<id>/import.json` with the URL, the ref, the
  packages registered, the items written (with their file hashes) and the edits shaked made to the checkout.

## 2. Detection

In the checkout, without running anything:

- **A project folder:** `shakedown.yaml` at the root. Its worlds (with their folders), scenarios, setups and
  experiments are opened with the project loader, so their problems show before anything is written. Each
  world's `views:` files are listed.
- **Packages:** every `shakedown-package.toml` (or the old `arena-package.toml`) anywhere in the tree, except
  under `.git`, `.venv`, `node_modules`, `target`, `__pycache__` and other dot folders. A manifest that does
  not parse is a problem.
- **Compose files** (`compose.y(a)ml`, `docker-compose.y(a)ml`) outside any package folder: candidates for the
  `service` integration. The plan names their services and volumes and the part config a world would use
  (`{compose_file, volumes}`). They are reported, not run: a world that wants one adds the part itself.
- **Python environments:** a package whose command starts with `uv` runs in a uv project: its `--project`
  (`--directory`) folder, else its own folder, and up to the uv workspace root that holds it.

## 3. Install and register

- **Python, through uv, one environment per uv workspace per commit**, at
  `<project>/.shakedown/imports/<id>/env/<commit12>-<root>/` (`UV_PROJECT_ENVIRONMENT`), so no `.venv` or build
  output lands in the checkout. shaked runs `uv sync --all-packages` there once, at import. The package's
  programs get that `UV_PROJECT_ENVIRONMENT` and `UV_FROZEN=1`: at run time uv uses what was installed and
  never re-resolves.
- **Shakedown's own Python packages come from the Shakedown that imports.** A repo built against the part kit
  and the SDK names them by a path on its author's machine (`shakedown-part = { path = "/home/…/kits/python" }`;
  its `uv.lock` holds that path relative to the repo). Those sources are rewritten in the checkout to this
  shaked's `kits/python` and `sdk/python` (`shakedown-part`, `arena-part`, `shakedown-sdk`, `arena-sdk`), and
  `uv sync` re-locks. The kit must speak this shaked's protocol anyway. The rewrite is recorded and shows as
  `git diff` in the checkout. Any other absolute path source is kept when it exists on this machine (reported:
  the repo does not travel), and is a problem when it does not.
- **Registering:** the packages join shaked's registry, as built worlds do, under the import's id. A name an
  installed package already has (from `SHAKEDOWN_PACKAGES` or another import) is refused before anything is
  installed. shaked registers every recorded import of its project at start. The deploy operation checks
  the record first, so an import made by `shaked import` while shaked runs is registered before the run.

## 4. What becomes project items

The repo's worlds (the whole world folder: data, views, rules), scenarios, setups and experiments are copied
into the importing project under their own ids. If the importing folder is not a project yet, it becomes one
with the repo's `shakedown.yaml` (name, defaults, permissions). An id the project has already is kept, unless
this import wrote it and it is unchanged since (same file hash): then it is replaced. The plan says which. The
packages' programs keep reading the checkout (`--world ../../worlds/hub-day`), so the world a part serves is
the commit that was imported.

Without `confirm`, the operation fetches and detects and answers the plan (commit, packages, items and what
happens to each, views, compose candidates, Python environments, problems). Nothing is installed, registered
or written into the project. With `confirm`, it installs, registers and writes.

## 5. Re-import on a new commit

The same id fetched at another commit gets a new checkout and a new environment beside the old ones. Only
when both are ready are the packages swapped in the registry: a run already going keeps its program. The
checkout and environment of the commit before are kept, so a run going on them finishes; older ones are
removed. A part's `describe` fingerprints the code and the world folder it
serves, so the fingerprint changes with the commit. Old runs keep the fingerprint they recorded, and a restore
of an old save into the new code is refused by the fingerprint check, as for any package that changed.

## 6. Trust

Imported code is a package like any other: a `command` program that runs on this machine with a cleared
environment, in its package folder. Import itself runs no repo code beyond installing it (git checkout with
hooks off, and `uv sync`, which may run the repo's build backend). Programs start only when a run, a describe
or a check asks for them. Hosted shaked (a layer that serves others) refuses the import (403): repo code must
run in the per-run sandbox there, and hosted packages need a per-org registry. Both are follow-ups (section 8).

## 7. Surfaces

- CLI: `shaked import <git-url-or-path> [--ref <ref>] [--id <id>] [--project <folder>] [--dry-run]`. It runs
  the operation in-process on the project folder (default `SHAKEDOWN_PROJECT`), as the person at the machine.
  The person typing is the confirmation; `--dry-run` prints the plan only.
- API: `POST /api/project/import-git` `{url, ref?, id?, confirm?}`, access class `edit`.
- MCP and the assistant: `project_import_git`, from the same registry entry.
- App: **Import from git** on the Worlds page. Give a URL or path and a ref, **Check** shows the plan, and
  **Import** runs it and lists what was registered and written.

## 8. Not in this change

- Hosted imports: the per-run sandbox runs the import's install and programs, and the registry gets a
  per-org namespace.
- A tracked record (`imports:` in `shakedown.yaml`) so a teammate's `shaked import --sync` rebuilds the same
  checkouts. Today the record is in `.shakedown/` on the machine that imported.
- Making worlds from compose candidates in one step (the `service` part config is in the plan).
- Non-uv Python (pip, poetry) and non-Python package builds: those packages run as their manifests say.
