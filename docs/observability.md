# Observability

Shakedown can send every run as an OpenTelemetry trace to Langfuse, Arize Phoenix or any OTLP/HTTP collector
(Jaeger, Grafana Tempo, Honeycomb, an OpenTelemetry Collector), serve Prometheus metrics, and take the spans your
own agent's OpenTelemetry SDK emits into the run's record.

What was checked: the span tree of a recorded run (`core/crates/shaked/tests/fixtures/otel/run_spans.json`),
export, retry and repeat against a local fake OTLP collector, the Prometheus text, and the receiver
(`core/crates/shaked/tests/observability.rs`). The Langfuse and Phoenix presets follow their documented OTLP
endpoints and headers; they were not run against a live Langfuse or Phoenix in those tests.

## Add a target

Settings › Advanced › Observability, or `POST /api/observability/targets` ([reference](reference.md#http-api)). Keys and
headers are sealed with the vault key, like model keys: they are never shown again, never sent to a browser, an
agent or a sandbox. Test sends one span and says what the collector answered.

| Preset | What you give | Where spans go | Headers sent |
|---|---|---|---|
| Langfuse | Cloud US, Cloud EU or your server's URL; public and secret key | `<host>/api/public/otel/v1/traces` | `Authorization: Basic base64(public:secret)` |
| Arize Phoenix | your Phoenix's URL (`http://127.0.0.1:6006` when blank) or your Phoenix Cloud space's URL; API key if it needs one; a project (`shakedown`) | `<url>/v1/traces` | `Authorization: Bearer <key>` and `api_key: <key>` |
| OTLP collector | the collector's URL; any headers | `<url>/v1/traces` (added unless the URL ends with it) | as given |

Examples for the OTLP preset:

| Backend | URL | Header |
|---|---|---|
| Jaeger (all-in-one, OTLP on) | `http://127.0.0.1:4318` | none |
| Grafana Tempo | `http://<tempo>:4318` | none, or what your gateway asks |
| Honeycomb | `https://api.honeycomb.io` | `x-honeycomb-team: <key>` |
| OpenTelemetry Collector | `http://<collector>:4318` | as its `otlp` receiver asks |

Spans go as OTLP/HTTP protobuf, at most 256 a request, uncompressed. A shaked that serves others reaches only
public HTTPS endpoints, checked when a target is saved and again on each send.

## What is sent

One trace per run; each branch (a fork) adds its own spans to it:

```text
run                      experiment, suite, scenario, setup, world, task, seed     (the run's first branch)
└─ branch                status, outcome, fork note, restore; world events and pauses as span events
   ├─ invoke_agent <name>       one per turn: model, tokens, cost, ok; handoffs, messages, reports as events
   │  ├─ chat <model>           tokens, cost, finish reasons, latency; prompt and completion (content)
   │  │  └─ reasoning           what the model thought (content)
   │  ├─ execute_tool <name>    rows read and written, keys, part, effect; arguments and result (content)
   │  └─ <your agent's spans>   what its OTel SDK sent during the run
   └─ evaluate                  the check: passed, differing rows, each named check as an event
```

Times are the record's: a model call starts when the gateway sent it (calls recorded before durations were kept
start at the agent's previous step), a tool call by its recorded duration. Span ids come from the run and branch
ids, so exporting a run again sends the same spans.

**Content** (prompts, completions, tool arguments and results, reasoning text, handoff and message text) goes
only when the target's Content switch is on. It is on by default for a target on your own server and off for
Langfuse Cloud and Phoenix Cloud. Without it, a span keeps its names, numbers and keys, and your agent's spans
keep only numbers and the attributes that name a step (model, tool, service, span kind).

## When runs are sent

A target with **Every run** on gets each branch made after it was added, once the branch ends. The exporter runs
in shaked every 5 seconds, from the stored record (in hosted Shakedown on the control box, never in a sandbox).
A failed send is tried again after 30 s, 2, 10 and 30 minutes, then every 2 hours, 6 tries in all; each try is on
the export's row (`GET /api/observability/exports`, and Settings › Advanced › Observability, Recent exports). To send a past
run, or send one again: `POST /api/branches/{id}/export`.

## Prometheus

`GET /metrics` serves Prometheus text, authenticated like the API: locally the person at this machine, else an
API token with `read` scope. In hosted Shakedown a token's metrics are its org's runs.

```yaml
scrape_configs:
  - job_name: shakedown
    metrics_path: /metrics
    static_configs:
      - targets: ["127.0.0.1:4510"]
    # Only where shaked serves others:
    # authorization:
    #   credentials: sd_...   # an API token, read scope
```

| Metric | Type | Labels |
|---|---|---|
| `shakedown_branches` | gauge | status, world, setup |
| `shakedown_pass_rate` | gauge | world, setup (passed over passed, failed and over budget) |
| `shakedown_run_duration_seconds` | histogram | world, setup |
| `shakedown_model_calls_total` | counter | model, provider, world, setup |
| `shakedown_model_tokens_total` | counter | model, provider, world, setup, type (`input`, `output`) |
| `shakedown_model_cost_usd_total` | counter | model, provider, world, setup |
| `shakedown_model_calls_uncosted_total` | counter | model, provider, world, setup |
| `shakedown_model_call_duration_seconds` | histogram | model, provider |

An OTLP collector target with **Metrics** on also gets these as OTLP metrics (cumulative) every minute, counting
the runs queued for it. Langfuse and Phoenix take traces only.

## Your agent's own spans

An agent you run yourself (an external run, a Code connection, an HTTP agent) can send what its own
OpenTelemetry instrumentation sees: its retrieval, planning, internal LLM calls. Its endpoints carry the
variables standard OTel SDKs read:

```text
OTEL_EXPORTER_OTLP_ENDPOINT=<shaked>/otlp/<branch>/<agent>
OTEL_EXPORTER_OTLP_TRACES_ENDPOINT=<shaked>/otlp/<branch>/<agent>/v1/traces
OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf
OTEL_EXPORTER_OTLP_HEADERS=Authorization=Bearer%20<the agent's token>
```

Start the agent with them (or point an exporter at `otlp_url`; an HTTP agent's run body has `otlp_traces_url`).
Each request becomes a `spans` event of that agent; its spans then go out under the agent's turn, keeping their
own parent-child order. Bounds: 4 MiB a request, 512 spans a request, 20,000 a branch, 20 requests a second, no
compression; metrics and logs sent there are taken and dropped.

## Attribute names

Each step carries the OpenTelemetry GenAI semantic conventions, OpenInference's names (what Phoenix's LLM views
read) and Langfuse's observation type, so each backend shows it as a model call, a tool or an agent.

| What | GenAI semantic conventions | OpenInference (Phoenix) | Langfuse | Shakedown |
|---|---|---|---|---|
| Kind of step | `gen_ai.operation.name` (`chat`, `execute_tool`, `invoke_agent`) | `openinference.span.kind` (`LLM`, `TOOL`, `AGENT`, `CHAIN`, `EVALUATOR`) | `langfuse.observation.type` | |
| Provider | `gen_ai.system`, `gen_ai.provider.name` | `llm.provider`, `llm.system` | | |
| Model | `gen_ai.request.model`, `gen_ai.response.model` | `llm.model_name` | | |
| Tokens | `gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens` | `llm.token_count.prompt`, `.completion`, `.total` | | |
| Cost | `gen_ai.usage.cost` | | | `shakedown.cost_usd`, `shakedown.cost_source` |
| Why it stopped | `gen_ai.response.finish_reasons` | | | `shakedown.model.tool_calls` |
| Prompt, completion (content) | events `gen_ai.content.prompt` (`gen_ai.prompt`), `gen_ai.content.completion` (`gen_ai.completion`) | `input.value`, `output.value`, `llm.input_messages.N.message.{role,content}`, `llm.output_messages.0.message.*` | reads `input.value`, `output.value` | |
| Tool | `gen_ai.tool.name`, `gen_ai.tool.call.id`, `gen_ai.tool.call.arguments`, `gen_ai.tool.call.result` (content) | `tool.name`, `tool.parameters` (content) | | `shakedown.tool.part`, `.effect`, `.duration_ms` |
| Rows | | | | `shakedown.db.rows_read`, `.rows_written`, `.keys_read`, `.keys_written` |
| Agent | `gen_ai.agent.id`, `gen_ai.agent.name` | | | `shakedown.agent.kind`, `.ok`, `.tokens`, `.wake_reason` |
| Run | | `session.id` | `langfuse.trace.name`, `langfuse.session.id` (the experiment, else the run) | `shakedown.run.*`, `.experiment.*`, `.suite.*`, `.setup.*`, `.scenario.id`, `.world`, `.task.id`, `.seed` |
| Outcome | | | | `shakedown.branch.status`, `shakedown.outcome.passed`, `shakedown.check.*` |
| Resource | `service.name=shakedown`, `service.version` | `openinference.project.name` (Phoenix's project) | | |
