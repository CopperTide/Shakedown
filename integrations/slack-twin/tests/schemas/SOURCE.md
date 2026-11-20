# Source of `slack-web-subset.json`

An extract of Slack's Web API description (OpenAPI 2), from
[slackapi/slack-api-specs](https://github.com/slackapi/slack-api-specs) (MIT licence,
`LICENSE-slack-api-specs.md` beside this file). The licence allows committing it, as GitHub's is
(owner ruling, 2026-11-19: Slack's choice follows its licence). The repository was archived by Slack
in 2021, so the description is older than today's API: it lacks fields Slack added since (`blocks` on
every message, `edited`), and the twin answers only what it documents.

- URL: https://raw.githubusercontent.com/slackapi/slack-api-specs/bc08db49625630e3585bf2f1322128ea04f2a7f3/web-api/slack_web_openapi_v2_without_examples.json
- Commit: `bc08db49625630e3585bf2f1322128ea04f2a7f3` (master, 2021-09-07)
- sha256 of the full description: in the extract's `source.sha256`

The extract keeps every documented method's name (`methods`), the success (`200`) and error
(`default`) answers of the methods the twin serves (`slack_twin/methods.py`), keyed `method status`,
and the definitions they use, as JSON Schema draft 7 (the kit's `shakedown_part.twin.vendor_schema`; a
list-valued `items` on a non-array becomes `anyOf`, see the script). Rebuild it after changing the
methods:

    curl -sSfLo /tmp/slack.json <URL above>
    uv run python scripts/schema_subset.py /tmp/slack.json <URL above> <commit> tests/schemas/slack-web-subset.json
