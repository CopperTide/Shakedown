# World builder (`worldbench`)

Templates written as data, worlds generated from them, and the benchmark a built world runs as
(`docs/superpowers/plans/2026-11-19-world-builder.md`).

- `library/<id>/`: the templates shipped with the builder (`small-manufacturer`); its manifest passes
  `--templates library` to the program, which also reads its own `templates/` and `ARENA_TEMPLATES`.
- `<root>/<id>/template.yaml`: a template, checked by `worldbench/schema/template.schema.json` (JSON Schema
  draft 2020-12) and by the cross-checks in `worldbench/validate.py` (ids unique per list, the systems a list
  names exist, `rules` files exist, sizes and defaults in range).
- `worldbench.expand`: placeholders (`{company.name}`, `{size}`, `{people.<role>}`, `{param.<name>}`,
  `{world_dir}`, …; `{{` and `}}` are literal braces) and includes (`@file.yaml`, `@file.md`, `@folder/`), which
  never leave the template folder.

A gate's `key` and `require` and `scenario_options` are passed through as written: their `{arg}` placeholders
are arenad's.

## Verified

`uv run --with pytest pytest -q` (Linux, Python 3.12): the tests under `tests/` over the neutral fixture
templates in `tests/fixtures/templates/`.
