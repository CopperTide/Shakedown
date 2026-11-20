# World builder (`worldbench`)

Templates written as data, worlds generated from them, and the benchmark a built world runs as
(`docs/superpowers/plans/2026-11-19-world-builder.md`).

- `library/<id>/`: the templates shipped with the builder (`small-manufacturer`); its manifest passes
  `--templates library` to the program, which also reads its own `templates/` and `ARENA_TEMPLATES`.
- `<root>/<id>/template.yaml`: a template, checked (with its tasks' checks, in place or included) by
  `worldbench/schema/template.schema.json` (JSON Schema draft 2020-12) and by the cross-checks in
  `worldbench/validate.py` (ids unique per list, the systems a list names exist, `rules` files exist, sizes and
  defaults in range).
- `worldbench.expand`: placeholders (`{company.name}`, `{size}`, `{seed}`, `{people.<role>}`,
  `{people.<role>.<field>}` (that field of each of the role's people, a list), `{param.<name>}`, `{world_dir}`, …;
  `{{` and `}}` are literal braces) and includes (`@file.yaml`, `@file.md`, `@folder/`; the path may hold
  placeholders, as `@env/{options.obs.profile}.yaml`), which never leave the template folder; a folder copied
  holds no links and never holds or sits in another one copied.
- A system may have no parts of its own and an `apply` (the events' ops, plus `merge`, which updates an object in
  a part's config): a chosen system's ops are applied, in template order, before the events'. A task's check may
  name the systems it `needs` and the `events` it belongs to; without them (or with one of those events off) it
  is dropped, and listed in `dropped`.
- An event may be written in its own file (`events: ["@events/x.yaml"]`, held to the event's schema). A `fixed`
  event is a fact of the template's data (a flaky test in a repository, say): always on, it may apply nothing, and
  turning it off is a problem.
- `worldbench/limits.py`: what bounds a template (file size, values and nesting per file, include depth and
  cycles, what one value expands to, a copied folder's files and bytes); the schema bounds the size, name pools
  and list lengths. A template that can't be read, whatever is wrong with it, is logged and left out of the list.

A gate's `key` and `require` and `scenario_options` are passed through as written: their `{arg}` placeholders
are arenad's.

## Verified

`uv run --with pytest pytest -q` (Linux, Python 3.12): the tests under `tests/` over the neutral fixture
templates in `tests/fixtures/templates/`.
