# Engine internals

Internal engineering docs for `shaked`, the Shakedown engine: how it is built, the rules it keeps, and where
each rule is enforced. They are for people changing the engine. For how to use Shakedown, read
[concepts](../concepts.md), the [protocol](../protocol.md) and the [reference](../reference.md).

Every claim cites a file. The code is the authority: when a page and the code disagree, the page is wrong, so
fix it in the same change. Diagrams are Mermaid, which GitHub renders.

## Reading order

| # | Page | What you get |
|---|---|---|
| 1 | [Architecture](architecture.md) | The system in one page, the crate and module map, the process layout, the five core invariants, where to start reading. |
| 2 | [Runs and worlds](runs-and-worlds.md) | A run's lifecycle, one step through `ToolExec`, the runners, forks and restores, saves, world time. |
| 3 | [Data model](data-model.md) | Tables, migrations 0001 to 0005, how an event is written, the 19 event kinds, the main types. |
| 4 | [Model traffic](model-traffic.md) | The gateway, providers, agents, the world MCP endpoint, the proxy, budgets, auth and `can()`, the layer. |
| 5 | [API and operations](api-and-ops.md) | The HTTP router, the one operation registry behind HTTP, MCP and the assistant, suites and experiments, review and metrics. |
| 6 | [Sandbox runs](sandbox-runs.md) | `shaked worker`, link frames, the window and give-up, holds, budgets on the control box, the sweep, `Hosting`, the E2B backend. |

Read 1 to 3 before any change. Read the area page before you touch its modules.

## Each area page has

- its job, in a few sentences
- a module table, one line per file
- Mermaid sequence or flow diagrams of the main flows
- its invariants, each with the file that enforces it
- its extension points
- its tests.

## Related docs

- [`AGENTS.md`](../../AGENTS.md) and [`CONTRIBUTING.md`](../../CONTRIBUTING.md): code rules, checks before a pull
  request.
- [`docs/embedding.md`](../embedding.md): every `Layer` hook, for a deployment that embeds shaked.
- [`docs/packages.md`](../packages.md): writing an integration or a benchmark.
- [`docs/protocol.md`](../protocol.md): `shakedown-part/1`, including the determinism contract for replay.
