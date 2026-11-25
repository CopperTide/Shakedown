# Software factory, prebuilt (`software-factory`)

The [`software-factory`](../world/templates/software-factory/README.md) world template built with its default
choices (company Northwind Labs, seed 1), shipped as a benchmark package so that hosted Shakedown, which builds no
world, can run it. What it measures, its tasks (`sprint`, `contract-change`), checks and needs are the template's.

Everything here but this README is generated: `world.json`, the template's copied folders under `files/`,
`shakedown-package.toml` (from `benchmarks/world/worldbench/install.py`), and `describe.json` (its live `describe`,
for a control box that starts no package program). Do not edit them by hand. After changing the template:

```sh
cd benchmarks/world && uv run python -m worldbench.prebuilt        # world.json, files/, the manifest
cd core && SHAKEDOWN_WRITE_DESCRIBE=1 cargo test -p shaked --test hosting \
  the_prebuilt_software_factorys_description_file_is_its_live_describe  # describe.json
```

`benchmarks/world/tests/test_prebuilt.py` and that shaked test fail while either is stale. Its command runs the
world builder's project at `${SHAKEDOWN_DIR}/benchmarks/world`, so it needs `benchmarks/world` beside it.
