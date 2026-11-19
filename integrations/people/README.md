# people

**A simulation.** Puts the world's people in it: developers, reviewers, on-call engineers, product managers or any
other role, each acting on the world clock by rules written as data. People never call a system. A system's
notice reaches them through a link to `people_notice`; what they do is a world event `<system>_act {person, action,
args}` that a link hands to that system's `<system>_act` tool. So every act is a recorded step, which a replay
repeats and a fork reproduces. `describe.system` is `{kind: "people", product: "Simulated people acting by rules
written as data", simulated: true}`. Restore: exact from a save (one `state.json`, which holds every act a person
still means to do); approximate: no. `uses_clock`: true.

A Python package on the kit's `arena_part.jsonstate` (`ClockedJsonStatePart`). Time is the clock arenad gives
(world ms), so a paused branch pauses people and the world's speed scales them; times of day are the world's
(`world_time.epoch_ms` + clock, UTC; clock 0 is midnight 1970 without a world time). Running the clock fires each
act that came due, earliest first, so how the advances were split changes nothing.

## Config

| key | what |
|---|---|
| `seed` | the seed of every draw (default: `start`'s seed) |
| `people` | `[{id, name, login, role, ...}]`, as stage 3 generates them (`{people}`); ids and logins unique |
| `rules` | `{<role>: <rules doc>}`; a role without rules does nothing |
| `systems` | the systems the world has; an act on another one is skipped (counted in the person's `skipped`) |

A config that doesn't fit is `-32602` naming where (`rules.pm: rules[0].every: ...`).

## The rule format (`shakedown-people/1`)

Checked by `people_part/rules.schema.json` (JSON Schema 2020-12) and cross-checks (rule ids unique, `after`
ranges in order, every `{pick:...}` names a list of `lines`, `else` last, `refresh` only on a notice rule).
Examples, one per role of the builder's mockup: `tests/fixtures/rules/{developer,reviewer,oncall,pm}.yaml`.

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

- A rule has one trigger: `notice` (`system`, `kind`, optional `to` — `me` or a pointer whose value is, or lists,
  the person's login — and `where`, a condition), `every` or `once`.
- A condition maps pointers into the notice's data to a value or a list of values.
- `act` is one act or a list, done together at the due time; `do` is a list of branches, the first that holds.
- In `args`, a string that starts with `/` is the notice's value there. In other strings, `{/pointer}`,
  `{person:<field>}` and `{pick:<lines>}` are filled. These forms pass stage 3's placeholders untouched, so a
  template can include a rules file and still use `{company.key}` in it.
- The trigger is `notice`, not `on`: YAML 1.1 (PyYAML, which reads templates) reads a bare `on` as `true`.

Draws use `Random.random()` only: a person's delay `Random("{seed}:{person.id}:{rule.id}:{n}")` (n = how many
times that rule set them off), their lines a second stream of it, and `who: one`
`Random("{seed}:{role}:{rule.id}:{k}:who")`.

## Tools and events

`people_notice {system, kind, data}` (a world tool, called by links): refreshes waiting acts that read it and
queues the rules it sets off; `writes` names the people it touched. At each `advance` every act that came due is an
event of kind `<system>_act`, `data = {person: <login>, action, args, at_ms, id, rule}` (`at_ms` is the world unix
ms it was due).

## Docs

`person/<id>` `{name, role, login, pending: [{rule, at_ms}], acted, skipped}`; `role/<role>` `{people, next:
[{rule, at_ms}]}` (the next slots of its scheduled rules); `log/acts` `{entries: [{at_ms, person, id, rule, system,
action}]}`.

## Verified

`uv run --with pytest pytest -q` (37 tests): the rule format (every fixture and template rules file validates;
refusals name where), seeded determinism, delays in range in world time, daily slots and limits, split-independent
advances, a paused clock, the world's speed, each mockup role's behaviour from the fixture rules, save and restore of
pending acts, and, through a stand-in for arenad's links with a neutral twin on the kit, that every act is a
recorded link step, that a fork from a save reproduces the steps and a replay of the recorded inputs gives the same
answers, with no connection opened. Run by hand (not in the suite) against the real PagerDuty, Slack, Jira and
GitHub twins of this branch through the same stand-in: every act the fixture rules made was accepted. Not yet run
under arenad.
