# Simulated people (`people`)

Puts the world's people in a Shakedown world: developers, reviewers, on-call engineers, product managers or any
other role, each acting on the world clock by rules written as data. People never call a system. A system's
notice reaches them through a link to `people_notice`. What they do is a world event `<system>_act {person,
action, args}` that a link hands to that system's `<system>_act` tool. So every act is a recorded step, which a
replay repeats and a fork reproduces. The software-factory world template uses it for the company's people, acting on
the GitHub, Jira, Slack and PagerDuty twins.

| | |
|---|---|
| Program | `python -m people_part` (Python, run with `uv`) |
| Restore | exact, from a save (one `state.json`, which holds every act a person still means to do). No approximate restore. |
| World clock | used: delays, daily slots and limits run on the world clock, and times of day are the world's |
| Agent tools | none. `people_notice` is a world tool that links call. |

## What it needs

- `uv`. No services, no Docker, no GPU and no ports.

| Variable | Default | Purpose |
|---|---|---|
| `SHAKEDOWN_SAVES` | `saves` (in the working directory) | Where saves are kept. `shaked` always passes its own. |

## Use it

A world names it as a part, for example `{part: "people", integration: "people", config: {...}}`, and declares the
links both ways ([`docs/protocol.md`](../../docs/protocol.md), "Links between parts"). The software-factory
template's links, for one system:

```yaml
- {from: people, event: github_act, to: github, tool: github_act, args: {person: /person, action: /action, args: /args}}
- {from: github, event: notice, to: people, tool: people_notice, args: {system: /system, kind: /kind, data: /data}}
```

| Config key | Type | Default | Meaning |
|---|---|---|---|
| `seed` | integer | `start`'s seed | The seed of every draw. |
| `people` | array | required | `[{id, name, login, role, ...}]`. Ids and logins are unique. The world builder's `{people}` placeholder gives this list. |
| `rules` | object | required | `{<role>: <rules doc>}`. A role without rules does nothing. |
| `systems` | array | every system | The systems the world has. An act on another one is skipped and counted in the person's `skipped`. |

A config that doesn't fit is refused with `-32602`, naming where, for example
`invalid config: rules.pm: rules[0].every: ...`.

### Time

Time is the clock `shaked` gives, in world ms, so a paused branch pauses people and the world's speed scales them.
Times of day are the world's: `world_time.epoch_ms` plus the clock, in UTC. Without a world time, clock 0 is
midnight, 1 January 1970. Running the clock fires each act that came due, earliest first, so how the advances were
split changes nothing.

### The rule format (`shakedown-people/1`)

A rules doc is checked by `people_part/rules.schema.json` (JSON Schema 2020-12) and by cross-checks: rule ids are
unique, `after` ranges are in order, every `{pick:...}` names a list in `lines`, `else` is the last branch, and
only a notice rule has `refresh`. One example per role is in `tests/fixtures/rules/` (`developer`, `reviewer`,
`oncall`, `pm`).

```yaml
format: shakedown-people/1
role: reviewer
lines: {approve: ["LGTM", "Looks good, ship it"], standup: ["on the sprint"]}
rules:
  - id: review-when-asked
    notice: {system: github, kind: review_requested, to: me}  # me: /reviewer, /assignee or /user is my login
    after: {minutes: [15, 60]}                                 # seeded, world minutes
    refresh:                                                   # later notices change what the act reads
      - notice: {system: github, kind: checks_completed}
        match: {/repo: /repo, /number: /prs}                   # mine: theirs (equal, or in their list)
        set: {/checks: /conclusion}
    do:
      - if: {/checks: success}
        act: {system: github, action: review, args: {repo: /repo, number: /number, event: APPROVE, body: "{pick:approve}"}}
      - else: true
        act: {system: github, action: review, args: {repo: /repo, number: /number, event: REQUEST_CHANGES, body: "Red."}}
  - id: standup
    every: {at: ["09:30", "16:00"]}                            # world time, UTC; or {minutes: n}; or once: {minute: n}
    who: one                                                   # one person of the role, seeded (default: all)
    limit: {per_day: 2}
    act: {system: slack, action: post, args: {channel: "#eng", text: "{person:name}: {pick:standup}"}}
```

- **Triggers.** A rule has one: `notice`, `every` or `once`. A `notice` has `system`, `kind`, an optional `to`
  (`me`, or a pointer whose value is, or lists, the person's login) and an optional `where` condition.
- **Conditions** map pointers into the notice's data to a value or a list of values.
- **Acts.** `act` is one act or a list, done together at the due time. `do` is a list of branches, and the first
  that holds is done.
- **Args.** A string that starts with `/` is the notice's value there. In other strings, `{/pointer}`,
  `{person:<field>}` and `{pick:<lines>}` are filled. Other braces pass through untouched, so a world template can
  include a rules file and still use its own placeholders, such as `{company.key}`.
- **Why `notice` and not `on`.** YAML 1.1 (PyYAML, which reads templates) reads a bare `on` as `true`.

**Draws.** Every draw uses `Random.random()` only. A person's delay comes from
`Random("{seed}:{person.id}:{rule.id}:{n}")`, where n counts how many times that rule set them off. Their lines
come from a second stream of it, and `who: one` from `Random("{seed}:{role}:{rule.id}:{k}:who")`.

### The world tool and events

`people_notice {system, kind, data}` is called by links. It refreshes waiting acts that read the notice and
queues the rules it sets off. Its `writes` name the people it touched. At each `advance`, every act that came due
is an event of kind `<system>_act` with `data = {person: <login>, action, args, at_ms, id, rule}`, where `at_ms`
is the world unix ms it was due.

### inspect and diff

| Key | Doc |
|---|---|
| `person/<id>` | `{name, role, login, pending: [{rule, at_ms}], acted, skipped}` |
| `role/<role>` | `{people, next: [{rule, at_ms}]}`: the next slots of its scheduled rules |
| `log/acts` | `{entries: [{at_ms, person, id, rule, system, action}]}` |

## How it's tested

```bash
cd integrations/people
uv run --with pytest pytest -q
```

The tests need no services. They cover the rule format (every fixture and every rules file of the templates in
`benchmarks/world/library/` and `benchmarks/world/templates/` validates, and refusals name where), seeded
determinism, delays in range in world time, daily slots and limits, advances split in different ways, a paused
clock, the world's speed, each fixture role's behaviour, and save and restore of pending acts. `tests/test_links.py`
runs people through a stand-in for `shaked`'s links (`tests/world.py`) with a neutral twin built on the kit. It
checks that every act is a recorded link step, that a fork from a save reproduces the steps, and that a replay of
the recorded inputs gives the same answers, with no connection opened.

The part is in the software-factory world, whose live tests under `shaked`
(`benchmarks/world/tests/software_factory/test_live_world.py`) start it with the four twins. The commit that added
them records them as passed once, on 2026-11-20.

## Limits

- People act only by rules. They do not read code, write text their rules and `lines` do not give, or decide
  anything a rule does not say. No model is called.
- What a system does with an act, and whether it refuses it, is up to that system. A refused act is recorded as
  the link step's error.
- A notice reaches people only through a link. A world without the links has people who never act on notices.
