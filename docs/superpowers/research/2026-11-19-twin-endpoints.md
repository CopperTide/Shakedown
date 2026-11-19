# Twin endpoints: GitHub + Jira (reference)

Date: 2026-11-19. Scope: what `gh`, PyGithub, Octokit core (REST) and pycontribs/jira call for an AI-software-factory workflow, plus git smart HTTP. Sources: primary client source at the commits named in each section; GitHub and Atlassian docs are linked inline.

## 0. Decisions this forces

| # | Decision | Recommendation | Why |
|---|---|---|---|
| D1 | Hostname mode for the GitHub twin | **GHES-style host** (e.g. `github.<sim>.internal`) on **443/TLS** with a CA trusted in the sandbox. REST at `/api/v3`, GraphQL at `/api/graphql`, git at `/{o}/{r}.git`. | gh treats any non-github.com host as GHES. It then introspects the schema and degrades to fewer mutations and older ID-based ones, and reads `/meta.installed_version`. PyGithub and Octokit only need `base_url`. Spoofing `github.com` itself (DNS + MITM CA) is the only option for agents that hard-code github.com. It also means dotcom-only gh paths (actor mutations, `requestReviewsByLogin`, semantic search, `event-subscriptions`), so keep it as a later mode. |
| D2 | GraphQL | **Required.** Build a real executor over a pruned copy of the published GHES schema (about 45 types, 12-14 mutations). Canned queries will not work. | Every `gh pr`/`gh issue` command is GraphQL, and its queries are generated dynamically (§2.3). |
| D3 | GraphQL error wording | Use **graphql-ruby wording** (`Field 'x' doesn't exist on type 'Y'`), or implement the probed fields as empty values. | gh string-matches these errors to tolerate missing features (§2.3). |
| D4 | Jira mode | Serve `/rest/api/{2,3,latest}` and `/rest/agile/1.0`. `serverInfo.deploymentType` selects Cloud or DC per instance. Default to **Cloud + v2 strings**, with v3 ADF on the same store. | The client defaults to v2. Cloud picks the `search/jql` token paging (§4). |
| D5 | URL minting | One configured external base URL per twin. Every `url`/`self`/`Link`/`jobs_url` is built from it. | PyGithub asserts host, port and path prefix. The Jira client PUTs and GETs `self` directly. |

## 1. Schemas to test the twins against

| Spec | URL | Notes |
|---|---|---|
| GitHub REST, GHES | `https://raw.githubusercontent.com/github/rest-api-description/main/descriptions/ghes-3.22/ghes-3.22.json` (+ `ghes-3.22.2022-11-28.json`, `ghes-3.22.2026-03-10.json`, `.yaml`; `dereferenced/ghes-3.22.deref.json` ≈74 MB) | Dirs exist from ghes-2.18 to **3.22** (docs `enterprise-server@latest` = 3.22). Pin the GHES version you claim in `/meta` (gh logic only changes at 3.17/3.18/3.21). |
| GitHub REST, dotcom | `descriptions/api.github.com/api.github.com.json` (14.3 MB, all versions), `api.github.com.2022-11-28.json`, `api.github.com.2026-03-10.json` | https://github.com/github/rest-api-description. Use the `2022-11-28` variant: gh pins that version. |
| GitHub GraphQL | dotcom: https://docs.github.com/public/fpt/schema.docs.graphql (1.6 MB). GHES: https://docs.github.com/public/ghes-3.22/schema.docs-enterprise.graphql (note the `-enterprise` suffix; `ghes-*/schema.docs.graphql` is 404). GHEC: `/public/ghec/schema.docs.graphql`. npm `@octokit/graphql-schema` 15.26.1. | Prune the GHES SDL to the types in §2.4 and replay gh's test-fixture queries against it. |
| Jira Cloud platform v3 | https://developer.atlassian.com/cloud/jira/platform/swagger-v3.v3.json (433 paths) | |
| Jira Cloud platform v2 | https://developer.atlassian.com/cloud/jira/platform/swagger.v3.json (424 paths) | |
| Jira Software (Agile) Cloud | https://developer.atlassian.com/cloud/jira/software/swagger.v3.json (78 paths; `/rest/agile/1.0` + `/rest/software/1.0`) | |
| Jira DC 11.3 (platform + agile) | https://dac-static.atlassian.com/server/jira/platform/jira_software_dc_11003_swagger.v3.json | Older DC ships WADL only |

Conformance idea: gh's own `httpmock` fixtures (`pkg/cmd/**/*_test.go`) list the exact operation names each command must hit. pycontribs/jira's `tests/` (integration suite that expects a live Jira at a URL: point it at the twin) and PyGithub's `tests/ReplayData/*.txt` (1,041 recorded HTTP exchanges) are ready-made contract corpora.

## 2. `gh` CLI (cli/cli @ `acee710`, 2026-11-18; go-gh v2.17.0)

Source refs are relative to https://github.com/cli/cli/tree/trunk and https://github.com/cli/go-gh/tree/v2.17.0.

### 2.1 Host detection, URLs, auth

| Item | Behaviour | Source |
|---|---|---|
| Host class | `github.com` → dotcom; `*.ghe.com` → tenancy; `github.localhost` → local; **anything else → GHES** (`IsEnterprise`) | go-gh `pkg/auth/auth.go:161` |
| REST base (GHES) | `https://HOST/api/v3/` | `internal/ghinstance/host.go` `RESTPrefix` |
| GraphQL (GHES) | `POST https://HOST/api/graphql` | same, `GraphQLEndpoint` |
| Scheme | **Always https** for GHES. Only `github.localhost` gets plain http (`http://api.github.localhost/` and `/graphql`, no `/api/v3`). Ports: `HostnameValidator` rejects `:`, so plan on 443 + TLS with a CA the agent sandbox trusts (`SSL_CERT_FILE`). | `host.go` |
| `api_host` (experimental, new) | `gh config set api_host gw.example --host HOST` sends API calls for HOST to another hostname; must be 443, no port. | `internal/config/config.go:847`, `docs/api-host-test-harness.md` |
| Token env | GHES host: `GH_ENTERPRISE_TOKEN` then `GITHUB_ENTERPRISE_TOKEN` then `hosts.yml`. `GH_TOKEN`/`GITHUB_TOKEN` are ignored for GHES hosts. | go-gh `auth.go:63-100` |
| Default host | `GH_HOST` env, else the only host in `hosts.yml`, else github.com | go-gh `auth.go:146` |
| Repo from git remote | Remotes are filtered to known hosts (`GH_HOST`, hosts.yml keys). Remote URL host must equal GH_HOST, otherwise "none of the git remotes ... correspond to the GH_HOST". | `pkg/cmd/factory/remote_resolver.go` |
| Auth header | `Authorization: token <T>` (not Bearer). Dropped on cross-host redirect. | `api/http_client.go:251` |
| Default headers | `User-Agent: GitHub CLI <ver>[ Agent/<name>]`, `X-GitHub-Api-Version: 2022-11-28`, `Accept: application/vnd.github.merge-info-preview+json, application/vnd.github.nebula-preview`, `Time-Zone`, GraphQL adds `GraphQL-Features: merge_queue` | `api/client.go:19-25`, go-gh `pkg/api/http_client.go:183-198` |
| Agent detection | gh appends `Agent/<name>` to UA when it detects Claude Code (`CLAUDECODE`), Codex (`CODEX_SANDBOX`/`CODEX_CI`/`CODEX_THREAD_ID`), Amp, etc. Free attribution signal for the twin. | `internal/agents/detect.go` |
| Background calls to github.com | Update notifier and telemetry. Set `GH_NO_UPDATE_NOTIFIER=1`, `GH_NO_EXTENSION_UPDATE_NOTIFIER=1`, `GH_TELEMETRY=0` (or `DO_NOT_TRACK=1`); telemetry is auto-disabled for GHES hosts. | `internal/telemetry/telemetry.go:104` |
| Git credential helper | `gh auth setup-git` → `gh auth git-credential` prints `username=x-access-token` (or login) + `password=<token>` → Basic auth on git smart HTTP. | `pkg/cmd/auth/gitcredential/helper.go:16,192` |
| Response caching | Feature-detection calls go through a disk cache with 24 h TTL (`X-GH-CACHE-TTL`); POST `/api/graphql` is cacheable. Schema changes in the twin can be masked for a day; wipe `~/.cache/gh` per run. | go-gh `pkg/api/cache.go:44`, `featuredetection` callers |

### 2.2 Startup / auth calls

| Command | Calls (GHES host) | Fields read |
|---|---|---|
| `gh auth login --with-token --hostname H` | `GET /api/v3/` (reads `X-OAuth-Scopes`; must contain `repo` and `read:org`, or be **empty** = "integration token, skip check"), GraphQL `query UserCurrent{viewer{login}}` | header + `data.viewer.login` |
| `gh auth status` | Same two calls per host | same |
| `gh api user` | `GET /api/v3/user` | prints body |
| `gh api <path> --paginate` | REST: follows `Link: <url>; rel="next"` (regex `<([^>]+)>;\s*rel="([^"]+)"`). GraphQL: requires `$endCursor` var + `pageInfo{hasNextPage,endCursor}` | `api/client.go:28,194` |
| First call needing a feature (GHES only) | `GET /api/v3/meta` → `installed_version` (semver). Drives: `<3.17` → Projects v1 (`projectCards`) queried; `>=3.18` → advanced issue search; `>=3.21` → workflow dispatch run details. **Return `"3.20.0"`** to get the leanest path. | `internal/featuredetection/feature_detection.go:332,396,523,560` |

### 2.3 GraphQL is mandatory

Every `gh pr *` and `gh issue *` subcommand uses GraphQL; only `pr diff`, `--delete-branch`, `label create/edit/delete`, `run *`, `api` use REST. Queries are **built dynamically** from `--json` fields (`api/query_builder.go`), use fragments, inline fragments on unions/interfaces (`...on User`, `...on CheckRun`), aliases (`statusCheckRollup: commits(last:1)`, `issue: issueOrPullRequest(...)`, `repo_000: repository(...)`) and variables with defaults. String-matching canned queries will not work: the twin needs a real GraphQL executor (graphql-js, async-graphql, gqlgen, Strawberry...) over a pruned GitHub schema.

**Schema introspection (GHES only).** gh asks the server which fields exist and degrades. The twin controls gh's code paths through what it advertises:

| Operation | Query | Effect |
|---|---|---|
| `Issue_fields` | `Issue: __type(name:"Issue"){fields(includeDeprecated:true){name}}` | `blockedBy` present → issue relationships on |
| `PullRequest_fields` | `PullRequest: __type(...)`, `StatusCheckRollupContextConnection: __type(...)` | `isInMergeQueue` → merge queue; `checkRunCount` → count-by-state rollups |
| `PullRequest_fields2` | `WorkflowRun: __type(...)` | `event` → `checkSuite{workflowRun{event}}` in `pr checks` |
| `Repository_fields` | `Repository: __type(...)` | `pullRequestTemplates`, `visibility`, `autoMergeAllowed` |
| `SearchType_enumValues` | `SearchType: __type(...){enumValues(includeDeprecated:true){name}}` | `ISSUE_ADVANCED` → opt-in advanced search |

Recommendation: advertise none of the optional fields (no `blockedBy`, `isInMergeQueue`, `checkRunCount`, `event`, `pullRequestTemplates`, `autoMergeAllowed`) → fewest code paths. GHES mode also sets `ApiActorsSupported=false`, so gh uses the older ID-based mutations (`requestReviews`, `updatePullRequest{assigneeIds}`) rather than `requestReviewsByLogin`/`replaceActorsForAssignable`.

**Error-text gotcha.** gh matches **graphql-ruby error strings** to tolerate missing fields:
`Field 'projectItems' doesn't exist on type 'PullRequest'` (and `...type 'Issue'`, `'projectsV2' ... 'Repository'/'User'/'Organization'`) — `api/queries_projects_v2.go:12-18`; `Field 'viewerMergeHeadlineText' doesn't exist` — `pkg/cmd/pr/merge/http.go`. graphql-js says `Cannot query field "x" on type "Y".`, which gh does not tolerate. Either implement the fields (return empty connections) or rewrite validation messages into graphql-ruby wording. `gh pr view` **always** runs `PullRequestProjectItems` because `projectItems` is in its default fields.

Error envelope read by go-gh: `{"data":..., "errors":[{"type":"NOT_FOUND","message":"Could not resolve to a Repository with the name 'o/r'.","path":["repository"]}]}`. `RepositoryNetwork` tolerates `NOT_FOUND` errors only.

### 2.4 GraphQL operations by command (GHES mode, non-interactive)

Test stubs in `pkg/cmd/<cmd>/*_test.go` (httpmock registry, `Verify` asserts all were hit) confirm these lists.

| Command | Operations, in order | Root fields / key selections |
|---|---|---|
| `gh repo clone o/r` | `RepositoryInfo` (static: `id databaseId name owner{login} sshUrl hasIssuesEnabled description hasWikiEnabled viewerPermission defaultBranchRef{name} parent{...} mergeCommitAllowed rebaseMergeAllowed squashMergeAllowed`) → `git clone https://HOST/o/r.git` | `repository(owner,name)` — `api/queries_repo.go:356` |
| base-repo resolution (TTY, or several remotes) | `RepositoryNetwork` (`viewer{login}` + `repo_000: repository(...){id databaseId name owner{login} viewerPermission defaultBranchRef{name} isPrivate parent{...}}`) | `api/queries_repo.go:513` |
| `gh pr create --title --body [--base] [--head]` | `RepositoryInfo` → `Issue_fields` → `PullRequestForBranch` (existing-PR check) → [`PullRequestTemplates` only if body missing and `pullRequestTemplates` advertised] → **`PullRequestCreate`** `createPullRequest(input:{repositoryId,baseRefName,headRefName,title,body,draft,maintainerCanModify}){pullRequest{id url}}` → [`PullRequestCreateMetadata` `updatePullRequest(input:{pullRequestId,labelIds,assigneeIds,milestoneId,projectIds})`] → [`PullRequestCreateRequestReviews` `requestReviews(input:{pullRequestId,userIds,teamIds,union:true})`]. Label/assignee/reviewer names are first resolved by `RepositoryLabelList`, `RepositoryAssignableUsers`, `OrganizationTeamList`, `RepositoryMilestoneList`. | `api/queries_pr.go:490-600`, `pkg/cmd/pr/create/create.go:433-620` |
| `gh pr view [N]` | `PullRequestByNumber` (`repository.pullRequest(number)`) or `PullRequestForBranch` (`repository.pullRequests(headRefName,states,first:30,orderBy:{field:CREATED_AT,direction:DESC}){nodes} defaultBranchRef{name}`), then in parallel: `ReviewsForPullRequest` / `CommentsForPullRequest` (`node(id){...on PullRequest{reviews|comments(first:100,after)}}`, only if `hasNextPage`), `PullRequestStatusChecks` (paginated rollup), `PullRequestProjectItems` | default fields: url number title state body author autoMergeRequest isDraft maintainerCanModify mergeable additions deletions commits{totalCount} baseRefName headRefName headRepositoryOwner headRepository isCrossRepository reviewRequests reviews assignees labels milestone comments reactionGroups createdAt statusCheckRollup projectItems — `pkg/cmd/pr/view/view.go:100`, `pkg/cmd/pr/shared/finder.go:200-330,364,404` |
| `gh pr list` | `PullRequestList` `repository.pullRequests(states,baseRefName,headRefName,first,after,orderBy){totalCount nodes{...pr} pageInfo}`; with `--author/--assignee/--label/--search/--draft` → `SearchType_enumValues` + `PullRequestSearch` `search(type:ISSUE[_ADVANCED],query:"repo:o/r is:pr ...")` | `pkg/cmd/pr/shared/lister.go:54`, `pkg/cmd/pr/list/http.go:14` |
| `gh pr checks [--watch]` | finder (`number`,`headRefName`) → `PullRequest_fields` + `PullRequest_fields2` → `PullRequestStatusChecks` `node(id){...on PullRequest{statusCheckRollup: commits(last:1){nodes{commit{statusCheckRollup{contexts(first:100,after){nodes{__typename ...on StatusContext{context state targetUrl createdAt description isRequired(pullRequestId:$id)} ...on CheckRun{name checkSuite{workflowRun{workflow{name}}} status conclusion startedAt completedAt detailsUrl isRequired(pullRequestId:$id)}} pageInfo}}}}}}}`; `--watch` re-polls | `pkg/cmd/pr/checks/checks.go:174-271`, `api/query_builder.go` `RequiredStatusCheckRollupGraphQL` |
| `gh pr merge N --merge/--squash [--delete-branch]` | finder (fields `id number state title lastCommit mergeStateStatus headRefName baseRefName headRefOid isCrossRepository isInMergeQueue isMergeQueueEnabled`; the last two trigger `PullRequest_fields`) → [`PullRequestMergeText` only when editing the message] → **`PullRequestMerge`** `mergePullRequest(input:{pullRequestId,mergeMethod:MERGE/SQUASH/REBASE,commitHeadline,commitBody,expectedHeadOid,authorEmail}){clientMutationId}` → `--delete-branch`: REST `DELETE /repos/o/r/git/refs/heads%2F<branch>` (slash URL-encoded) + local `git checkout/pull`. `--auto` → `enablePullRequestAutoMerge` | `pkg/cmd/pr/merge/merge.go:571`, `http.go` |
| `gh pr review N --approve/--request-changes/--comment -b` | finder (`id number`) → `PullRequestReviewAdd` `addPullRequestReview(input:{pullRequestId,event:APPROVE/REQUEST_CHANGES/COMMENT,body}){clientMutationId}` | `api/queries_pr_review.go:249` |
| `gh pr comment N -b` | finder → `CommentCreate` `addComment(input:{subjectId,body}){commentEdge{node{url}}}` | `api/queries_comments.go:57` |
| `gh pr edit` | finder → `PullRequestUpdate` `updatePullRequest(input:{pullRequestId,title,body,baseRefName,assigneeIds,milestoneId})`, `LabelAdd`/`LabelRemove` (`addLabelsToLabelable`/`removeLabelsFromLabelable`), reviewers via `requestReviews` | `pkg/cmd/pr/shared/editable_http.go` |
| `gh pr close/reopen/ready` | finder → `PullRequestClose` `closePullRequest` / `PullRequestReopen` / `PullRequestReadyForReview` `markPullRequestReadyForReview` (+ REST branch delete) | `api/queries_pr.go:783-820` |
| `gh pr diff N` | finder → REST `GET /repos/o/r/pulls/N` with `Accept: application/vnd.github.v3.diff` (or `.patch`) → raw text | `pkg/cmd/pr/diff/diff.go:213` |
| `gh pr checkout N` | finder → `git fetch origin refs/pull/N/head:<branch>` → **the git server must advertise `refs/pull/N/head`** | `pkg/cmd/pr/checkout/checkout.go:246` |
| `gh issue create -t -b` | `IssueRepositoryInfo` (`id databaseId name owner{login} hasIssuesEnabled viewerPermission`) → `Issue_fields` → [`RepositoryLabelList`...] → `IssueCreate` `createIssue(input:{repositoryId,title,body,labelIds,assigneeIds,milestoneId}){issue{id url}}` | `api/queries_issue.go:328` |
| `gh issue list` | `IssueList` `repository{hasIssuesEnabled issues(first,after,orderBy,states,filterBy:{assignee,createdBy,mentioned}){totalCount nodes pageInfo}}`; `--label/--search/--milestone` → `IssueSearch` `search(type,last,after,query)` | `pkg/cmd/issue/list/http.go:30,135` |
| `gh issue view N` | `IssueByNumber` `repository{hasIssuesEnabled issue: issueOrPullRequest(number){__typename ...on Issue{...} ...on PullRequest{...}}}` (+ `CommentsForIssue`, `IssueProjectItems`) | `pkg/cmd/issue/shared/lookup.go:157` |
| `gh issue comment/close/edit` | `IssueByNumber` → `CommentCreate` / `IssueClose` `closeIssue(input:{issueId,stateReason})` / `IssueUpdate` `updateIssue(input)` | `pkg/cmd/issue/close/close.go:207` |
| `gh label list` / `create` | GraphQL `LabelList`; REST `POST/PATCH/DELETE /repos/o/r/labels[/name]` | `pkg/cmd/label/http.go` |

**Count for the PR workflow** (clone → create → view/list → checks → review → comment → merge, plus issues): 5 introspection ops (one generic `__type` resolver) + ~22 named operations, which use 7 query root paths (`viewer`, `repository`, `node`, `search`, `__type`, `repository.pullRequest/pullRequests/issueOrPullRequest/issues/labels/assignableUsers/milestones`, `organization.teams`) and **12 mutations**: `createPullRequest`, `updatePullRequest`, `mergePullRequest`, `closePullRequest`, `reopenPullRequest`, `markPullRequestReadyForReview`, `addPullRequestReview`, `requestReviews`, `addComment`, `createIssue`, `updateIssue`, `closeIssue` (+ `addLabelsToLabelable`/`removeLabelsFromLabelable` for edit).

**GraphQL object types needed:** `Query`, `Mutation`, `User`, `Bot`, `Team`, `Organization`, `RepositoryOwner`/`Actor` interfaces, `Repository`, `Ref`, `PullRequest`, `Issue`, `IssueOrPullRequest` union, `IssueComment`, `PullRequestReview`, `ReviewRequest` (+ `RequestedReviewer` union), `PullRequestCommit`, `Commit`, `GitActor`, `PullRequestChangedFile`, `Label`, `Milestone`, `StatusCheckRollup`, `StatusCheckRollupContext` union, `StatusContext`, `CheckRun`, `CheckSuite`, `WorkflowRun`, `Workflow`, `ReactionGroup`, `AutoMergeRequest`, `ProjectV2Item` (empty), connections/`PageInfo`, enums (`PullRequestState`, `IssueState`, `MergeableState`, `MergeStateStatus`, `PullRequestReviewState`, `PullRequestReviewEvent`, `PullRequestMergeMethod`, `CheckStatusState`, `CheckConclusionState`, `StatusState`, `SearchType`, `RepositoryPermission`, `CommentAuthorAssociation`), and the input types for the mutations. About 45 types; take them verbatim from the published schema and prune.

Enum values gh switches on: PR `state` OPEN/CLOSED/MERGED; `mergeable` MERGEABLE/CONFLICTING/UNKNOWN; `mergeStateStatus` BEHIND/BLOCKED/CLEAN/DIRTY/HAS_HOOKS/UNSTABLE/MERGED; check buckets (`pkg/cmd/pr/checks/aggregate.go`): SUCCESS → pass; SKIPPED/NEUTRAL → skipping; ERROR/FAILURE/TIMED_OUT/ACTION_REQUIRED → fail; CANCELLED → cancel; anything else → pending. For a `CheckRun`, gh uses `conclusion` if set, else `status`; for a `StatusContext`, `state`.

### 2.5 `gh run` (REST only)

| Command | Calls | Fields read (`pkg/cmd/run/shared/shared.go:85-260`) |
|---|---|---|
| `gh run list [-b br] [-w wf]` | `GET /repos/o/r/actions/runs?per_page=&exclude_pull_requests=true[&branch=&actor=&status=&event=]` or `GET .../actions/workflows/{id}/runs`; `GET .../actions/workflows` (names) | `{total_count, workflow_runs:[{id,name,display_title,status,conclusion,event,workflow_id,run_number,run_attempt,head_branch,head_sha,head_commit,html_url,jobs_url,created_at,updated_at,run_started_at,head_repository}]}` |
| `gh run view ID [--log/--log-failed]` | `GET .../actions/runs/{id}?exclude_pull_requests=true` (`/attempts/{n}`), `GET .../actions/workflows/{workflow_id}`, `GET {jobs_url}` (**absolute URL from the run, so it must point at the twin**), `GET .../check-runs/{job_id}/annotations` (job id = check-run id), GraphQL `PullRequestForRun`, `GET .../actions/runs/{id}/artifacts`; logs: `GET .../actions/runs/{id}/logs` (zip, usually a 302 redirect) with layout `jobname/N_step.txt` + `K_jobname.txt`, fallback `GET .../actions/jobs/{job_id}/logs` | jobs: `{total_count, jobs:[{id,status,conclusion,name,steps:[{name,status,conclusion,number,started_at,completed_at}],started_at,completed_at,html_url,run_id}]}`; annotations: `[{path,start_line,annotation_level,message}]` |
| `gh run watch ID` | Polls `GET .../actions/runs/{id}` and `{jobs_url}` every 3 s until `status=completed`, plus annotations | same |

Status values: `queued`, `in_progress`, `completed`, `waiting`, `requested`, `pending`; conclusions `success`, `failure`, `cancelled`, `skipped`, `neutral`, `timed_out`, `action_required`, `stale`, `startup_failure`.

## 3. GitHub REST (PyGithub @ `d2d9f3b` 2026-11-18, Octokit core/request), git smart HTTP, webhooks

PyGithub refs are `github/<File>.py:<line>` in https://github.com/PyGithub/PyGithub.

### 3.1 Client behaviour
| Topic | PyGithub (`Github(base_url="https://H/api/v3")`) | Octokit (`new Octokit({baseUrl:"https://H/api/v3"})`) |
|---|---|---|
| First request | None at construction. `get_user()` is lazy. **`get_repo()` is eager** (`GET /repos/o/r`). Any object built from a bare `url` does a completing GET when an attribute is read. (MainClass.py:436-526, GithubObject.py:682) | per call |
| URL building | Relative paths get the base path prepended. For absolute URLs from responses only path+query are kept. Sub-resources are derived from the object's `url`/`issue_url`, e.g. `create_status` takes the parent of the parent of commit `url` (Commit.py:252). (Requester.py:1411) | `Link` next URL is used verbatim |
| **Host assertion** | `__assertUrlAllowed` is a hard `assert`. A returned URL must share scheme, host and **port** with base_url and start with `/api/v3`, `/api/graphql`, `/api/` or `/login/oauth`. **Every `url`, `issue_url`, `pull_request_url`, `jobs_url` and `Link` must use the exact external base URL; never emit `api.github.com`.** (Requester.py:415-423, 965-1004) | n/a |
| GraphQL URL | `/api/v3` becomes `/api/graphql`. Used for automerge, `convert_to_draft`, `mark_ready_for_review`, minimize (Requester.py:488) | `@octokit/graphql` does the same |
| Auth | `Authorization: token X` (Auth.Token); `Bearer` (JWT/App); `Basic` (login) (Auth.py:90,120,170,191) | `token X`; `bearer` if the token is a 3-segment JWT |
| Default headers | `User-Agent: PyGithub/Python`. No Accept (so `*/*`). `X-GitHub-Api-Version` is sent only when `api_version=` is given. | `accept: application/vnd.github.v3+json`, `user-agent: octokit-core.js/...`. No API-version header. |
| Rate-limit headers read | `x-ratelimit-remaining`+`-limit` (both together), `-reset` (epoch), `-resource`, `-used`, `x-oauth-scopes` (Requester.py:1288) | throttling: `x-ratelimit-remaining`, `x-ratelimit-reset`, `retry-after` |
| Error strings matched | 401 `"Bad credentials"`. 403 `"API rate limit exceeded..."` / `"You have exceeded a secondary rate limit"` / `"...please retry your request again later."`. 404 needs `"Not Found"` (Requester.py:1007-1062). | message + `errors[]` + `documentation_url`. 304 throws. |
| Retry | Retries 5xx **including POST** (total 10). Retries 403 only with `Retry-After` or a rate-limit message. **429 is not retried.** Default pacing is 0.25 s between requests and 1 s between writes. (GithubRetry.py, MainClass.py:206) | plugin-retry: 3×, never 4xx. throttling: 403/429, secondary limit → `retry-after` (default 60 s). |
| Redirects | Follows a same-host 301. A 302 is followed only for `get_contents`. A 202 on GET sleeps 2 s and retries. A job-logs 302 returns `Location` without following (WorkflowJob.py:202). | fetch follows redirects |
| ETag | Only `obj.update()` sends `If-None-Match`/`If-Modified-Since`, so return `ETag` and `Last-Modified` (GithubObject.py:751) | 304 = error |
| `X-GitHub-Enterprise-Version` | not read. Send it anyway, since GHES always does. | not read |

### 3.2 Pagination
| Rule | Detail |
|---|---|
| Link format | Exactly `<URL>; rel="next", <URL>; rel="last"`. PyGithub splits on `", "` and `"; "` (PaginatedList.py:566). gh uses regex `<([^>]+)>;\s*rel="([^"]+)"`. Octokit uses `<([^<>]+)>;\s*rel="next"`. |
| `totalCount` | PyGithub re-requests with **`per_page=1`** and reads `page=N` from `rel="last"`. Otherwise it uses `total_count` (PaginatedList.py:345). Always include `last` with `page=`. |
| Namespaced lists | `{total_count, <key>:[...]}` for `items` (search), `check_runs`, `workflow_runs`, `jobs`, `workflows`, `artifacts`. Octokit normalizes when `total_count`/`total_commits` is present and **there is no `url` key**: it deletes `total_count`, `incomplete_results`, `repository_selection` and `total_commits`, then takes `Object.keys(data)[0]`, **so no other key may remain** ([normalize](https://github.com/octokit/plugin-paginate-rest.js/blob/main/src/normalize-paginated-list-response.ts)). The combined status carries `url`, so it is not normalized. |
| `per_page` | Default 30, max 100 |

### 3.3 Endpoints and the fields clients read
Every object needs `url` (pointing at the twin), `id`, `node_id` and `html_url`. JSON types must be correct, because PyGithub raises `BadAttributeException` on access otherwise. Timestamps are ISO-8601 `Z`.

| Area | Method + path | Client caller | Required response fields / gotchas |
|---|---|---|---|
| user | GET `/user`, `/users/{u}` | `get_user()`, `gh api user` | `login,id,node_id,url,html_url,type,name,email` |
| repo | GET `/repos/{o}/{r}` | `get_repo` (eager) | `url,full_name,name,owner{login,url},default_branch,private,permissions{admin,push,pull},clone_url,html_url` |
| branches | GET `/branches`, `/branches/{b}` | `get_branches/get_branch` | `name,commit{sha,url},protected` |
| contents | GET `/contents/{path}?ref=` | `get_contents` | file: `type:"file",encoding:"base64",content,sha(blob),size,name,path,url`; dir: array |
| contents | PUT `/contents/{path}` | `create_file/update_file` | req `message,content(b64),branch,sha(update)`; resp `{content{sha,url,...},commit{sha,url}}`. Stale/missing sha → 409/422 |
| contents | DELETE `/contents/{path}` | `delete_file` | **JSON body** `message,sha,branch` → `{commit,content:null}` |
| commits | GET `/commits[?sha,path,author,since,until]`, `/commits/{ref}` | `get_commits/get_commit` | `sha`, **`url=.../repos/o/r/commits/{sha}`**, `commit{message,author,committer,tree}`, `parents`, `files[]`, `stats` |
| compare | GET `/compare/{base}...{head}` | `compare` | `status,ahead_by,behind_by,total_commits,commits[],files[],merge_base_commit,url` |
| refs | GET `/git/ref/{ref}`; POST `/git/refs`; PATCH/DELETE `/git/refs/{ref}` | `get_git_ref`, `create_git_ref` (how agents branch), gh `--delete-branch` (`heads%2F<b>` encoded) | `ref,object{sha,type,url},url`; 422 "Reference already exists" |
| trees/blobs | GET `/git/trees/{sha}?recursive=1`; POST `/git/blobs,/git/trees,/git/commits` | `get_git_tree`, `create_git_*` | `sha,url,tree[],truncated` |
| PR | POST `/pulls` | `create_pull` | 201. `url,issue_url,number,state,title,body,head/base{label,ref,sha,repo,user},user,merged,mergeable(null while computing),mergeable_state,draft,node_id,html_url` |
| PR | GET `/pulls?state&head=owner:br&base&sort&direction` | `get_pulls` | paginated |
| PR | GET/PATCH `/pulls/{n}` | `get_pull`, `edit`; gh/Octokit `.diff`/`.patch` Accept | single GET adds `commits,additions,deletions,changed_files,merge_commit_sha`. `Accept: application/vnd.github.v3.diff` → raw diff |
| PR | PUT `/pulls/{n}/merge` | `merge(merge_method, commit_title, commit_message, sha)` | 200 `{sha,merged:true,message}`; 405 not mergeable; 409 sha mismatch |
| PR | GET `/pulls/{n}/merge` | `is_merged` | 204 if merged, else 404 |
| PR | GET `/pulls/{n}/files`, `/commits` | `get_files/get_commits` | files: `filename,status,additions,deletions,changes,patch,sha,blob_url,raw_url,contents_url` |
| reviews | POST/GET `/pulls/{n}/reviews` | `create_review(body,event,commit_id,comments[])` | event APPROVE/REQUEST_CHANGES/COMMENT (omitted = PENDING). **Non-completable:** return `id,user,body,state(APPROVED/CHANGES_REQUESTED/COMMENTED/PENDING),commit_id,submitted_at,html_url,pull_request_url` up front |
| reviews | POST `/pulls/{n}/reviews/{id}/events`; PUT `/reviews/{id}/dismissals` | submit pending (Octokit/raw), `dismiss` | |
| review comments | POST/GET `/pulls/{n}/comments`, POST `/comments/{id}/replies` | `create_review_comment` | `body,commit_id,path,line|position,side,in_reply_to` |
| reviewers | POST/DELETE/GET `/pulls/{n}/requested_reviewers` | `create_review_request` etc. | `reviewers[],team_reviewers[]`; DELETE takes a JSON body; GET → `{users[],teams[]}` |
| issues | POST/GET `/issues`, GET/PATCH `/issues/{n}` | `create_issue/get_issue/edit` | PR-issues carry `pull_request{url}` |
| comments | POST/GET `/issues/{n}/comments`, PATCH/DELETE `/issues/comments/{id}` | `create_issue_comment` (via `issue_url`) | |
| labels | POST/GET/PUT/DELETE `/issues/{n}/labels[/{name}]`, POST/GET `/labels`, PATCH/DELETE `/labels/{name}` | `add_to_labels`, gh `label create` | `id,name,color,description,url` |
| search | GET `/search/issues?q=` | `search_issues` | `{total_count,incomplete_results,items}` |
| statuses | POST `/statuses/{sha}`; GET `/commits/{ref}/status`; GET `/commits/{ref}/statuses` | `create_status`, `get_combined_status` | req `state(error/failure/pending/success),target_url,description,context`; combined `{state,statuses[],sha,total_count,url}` |
| checks | POST `/check-runs`, PATCH `/check-runs/{id}`, GET `/commits/{ref}/check-runs`, `/check-suites/{id}/check-runs`, GET `/check-runs/{id}/annotations` | `create_check_run`, `CheckRun.edit` (PATCH `self.url`), gh run view | `name,head_sha,status,conclusion,started_at,completed_at,output{title,summary,text,annotations},details_url,external_id`; list `{total_count,check_runs}` |
| actions | GET `/actions/workflows[/{id}]`, `/actions/workflows/{id}/runs`, `/actions/runs[/{id}]`, `/actions/runs/{id}/jobs`, `/actions/jobs/{id}/logs` (302), `/actions/runs/{id}/logs` (zip); POST `/dispatches`, `/rerun`, `/cancel` | `get_workflow_runs`, gh `run *` | run needs `jobs_url`,`logs_url` (absolute, twin); see §2.5 |
| hooks | POST/GET `/hooks`, GET/PATCH/DELETE `/hooks/{id}`, POST `/hooks/{id}/pings`, `/tests` | `create_hook` | req `{name:"web",config{url,content_type,secret,insecure_ssl},events[],active}` |
| meta | GET `/rate_limit`, GET `/meta`, GET `/` (root) | | `/meta.installed_version` (gh); root must 200 with `X-OAuth-Scopes` (gh auth) |

Response headers to copy from real GitHub: `content-type: application/json; charset=utf-8`, `x-github-media-type: github.v3; format=json`, `x-github-api-version-selected`, `x-ratelimit-{limit,remaining,used,reset,resource}`, `x-github-request-id`, `X-OAuth-Scopes: repo, read:org, workflow`, `X-GitHub-Enterprise-Version: 3.20.0`, `ETag`.

### 3.4 Git smart HTTP ([gitprotocol-http](https://git-scm.com/docs/gitprotocol-http), [git-http-backend](https://git-scm.com/docs/git-http-backend), [gitprotocol-v2](https://git-scm.com/docs/gitprotocol-v2))
Clone and push were verified against git 2.43 using a `git http-backend` wrapper (`scratchpad/scripts/ghb.py`).

| Item | Rule |
|---|---|
| Discovery | `GET /{o}/{r}.git/info/refs?service=git-upload-pack|git-receive-pack` → `Content-Type: application/x-<svc>-advertisement`. A v0 body starts `001e# service=git-upload-pack\n0000`. Send `Cache-Control: no-cache`. A missing repo gets 404. |
| RPC | `POST /{o}/{r}.git/git-upload-pack` (`application/x-git-upload-pack-request` → `-result`); the same pattern for receive-pack. Accept `Content-Encoding: gzip`. |
| Protocol v2 | git sends `Git-Protocol: version=2` on fetch/clone (not push). Pass it through as the `GIT_PROTOCOL` env. The v2 advertisement has no `# service=` line. |
| Auth | The first request is anonymous. Respond **401 + `WWW-Authenticate: Basic realm="..."`** so the credential helper runs. Accept any username with the token as password (`x-access-token`, `oauth2`, login) and also `Bearer`. Set `REMOTE_USER`, otherwise http-backend refuses receive-pack. |
| Paths | Serve both `/{o}/{r}.git/...` and `/{o}/{r}/...`. The bare repo `HEAD` must point at the default branch. **Advertise `refs/pull/N/head`** (and optionally `refs/pull/N/merge`) for `gh pr checkout` and CI. |
| Implementation | Wrap the `git http-backend` CGI (`GIT_PROJECT_ROOT`, `GIT_HTTP_EXPORT_ALL=1`, `PATH_INFO`, `QUERY_STRING`, `REQUEST_METHOD`, `CONTENT_TYPE`, `REMOTE_USER`, `GIT_PROTOCOL`) and stream the bodies. The alternative is `git upload-pack --stateless-rpc`. |
| Push → events | A `post-receive` hook (stdin `<old> <new> <ref>`) emits: a `push` event (`created`/`deleted` when one side is all zeros); a `pull_request` `synchronize` event for open PRs whose `head.ref` matches, with their `head.sha` updated; recomputed `mergeable` for PRs on that base; and optionally a `check_suite` for CI. |

### 3.5 Webhooks ([events and payloads](https://docs.github.com/en/webhooks/webhook-events-and-payloads))
Headers:
- `X-GitHub-Event`
- `X-GitHub-Delivery` (GUID)
- `X-GitHub-Hook-ID`
- `X-Hub-Signature-256: sha256=<hex HMAC-SHA256(secret, raw body)>` (only when a secret is set)
- `User-Agent: GitHub-Hookshot/<hex>`
- `X-GitHub-Hook-Installation-Target-Type: repository` and `-ID`
- `Content-Type: application/json` (or form `payload=`)

Every payload has `action`, `repository` and `sender`.

| Event | Actions | Keys |
|---|---|---|
| `ping` | – | `zen,hook_id,hook` (sent on hook create) |
| `push` | – | `ref,before,after,created,deleted,forced,base_ref,compare,commits[]{id,tree_id,message,timestamp,url,author,committer,added,removed,modified,distinct},head_commit,pusher` |
| `pull_request` | opened, edited, closed (`merged` flag), reopened, synchronize (`before`,`after`), review_requested/removed, labeled/unlabeled, assigned, ready_for_review, converted_to_draft | `number,pull_request` (+ `changes`, `requested_reviewer`, `label`) |
| `pull_request_review` | submitted, edited, dismissed | `review,pull_request` |
| `pull_request_review_comment` | created, edited, deleted | `comment,pull_request` |
| `check_run` | **repo hooks receive only created and completed** | `check_run` |
| `check_suite` | **repo hooks receive only completed** | `check_suite` |
| `status` | – | `id,sha,name,state,context,description,target_url,commit,branches,created_at,updated_at` |
| `issues`, `issue_comment` | opened/edited/closed/reopened/labeled/...; created/edited/deleted | `issue` (+ `comment`); `issue_comment` also fires for PR conversation comments |
| `workflow_run`, `workflow_job` | requested/in_progress/completed; queued/in_progress/completed/waiting | `workflow,workflow_run` / `workflow_job` |

## 4. Jira twin (pycontribs/jira `main` @ `0525ec1`, 2026-11-05)

Line refs are to `jira/client.py` unless another file is named (https://github.com/pycontribs/jira). `api2:` = `/rest/api/2/`.

### 4.1 URL building and defaults
| Item | Value | Ref |
|---|---|---|
| Core base | `{server}/rest/api/{rest_api_version}/{path}`. The default version is **"2"**. The client never chooses v3 itself; v3 is used only with `options={"rest_api_version":"3"}`, and the client's plain-string bodies are then wrong for v3. | resources.py:105, 660 |
| `latest` | `assign_issue`, `group`, `user` admin, `role`, `filter/{id}/permission` and `application-properties` call `/rest/api/latest/...`. **Alias `latest`, `2` and `3` to the same handlers.** | 2790, 5278 |
| Agile base | `{server}/rest/{agile_rest_path}/{agile_rest_api_version}/` (default `agile`/`1.0`). Sprint-report calls are hard-coded to `/rest/greenhopper/1.0/`; skip them. | resources.py:1572, 693 |
| Default headers | `Content-Type: application/json`, `X-Atlassian-Token: no-check`, `Cache-Control: no-cache`, `Accept: application/json,*/*;q=0.9` | 669-678 |
| Leniency | `use_post` search sends JSON **nulls** (`expand:null`). Bool query params are Python-cased (`validateQuery=True`). Parse case-insensitively and accept nulls. | 5294-5323, 4220 |

### 4.2 Startup (`JIRA(server, basic_auth=...)`)
| Step | HTTP | When | Ref |
|---|---|---|---|
| Auth | None. requests sends `Authorization: Basic b64(email:api_token)` pre-emptively. `token_auth=` sends `Bearer <PAT>` (DC) and retries a 401 once without cookies. | always | 871, 5109-5119, 531-560 |
| Session check | `GET /rest/auth/1/session` | `validate=True` only (default False) | 884-892 |
| **Server info** | **`GET api2:serverInfo`**, retried 3× on an empty body | default `get_server_info=True` | 895-904, 4533 |
| `/field` | Lazy. Loaded for field-name mapping, `rank()` and the sprint field id. | on demand | 913-966, 6634 |

The client reads `serverInfo.versionNumbers` (int list, **required**) and `deploymentType` (`"Cloud"` → `_is_cloud=True`). Cloud mode response: `{"baseUrl","version":"1001.0.0-SNAPSHOT","versionNumbers":[1001,0,0],"deploymentType":"Cloud","buildNumber":100000,"serverTitle":"Jira"}`. DC mode: `"deploymentType":"Server","versionNumbers":[10,3,0]`.

`_is_cloud` switches: the search endpoint, user identifier (accountId vs name), user-search param (`query` vs `username`), createmeta item key, board `location`, and the `@cloud_api` methods.

### 4.3 Resource parsing contract
- An empty dict raises in `_parse_raw`, so every GET returns a non-empty object (resources.py:492).
- A nested dict with a `self` key becomes a typed Resource, chosen by regex on `self`: `issue/[^/]+$`, `issue/X/comment/Y`, `project/X`, `status/X`, `priority/X`, `issuetype/X`, `user\?(username|key|accountId)=` (resources.py:1786-1826).
- **`self` must be an absolute, dereferenceable URL at the twin's real path.** `Resource.update` does PUT `self` then GET `self`; `delete` does DELETE `self` (resources.py:305-462).
- `User` reads `raw["self"]` unconditionally (resources.py:1468).

Minimal shapes:
- **Issue:** `{id:"10001", key:"P-1", self, fields:{summary, status{self,id,name,statusCategory{key,name}}, issuetype{self,id,name,subtask}, project{self,id,key,name}, priority{self,id,name}, assignee|null, reporter, labels[], description, created, updated, resolution|null, issuelinks[], comment{comments[],total}, parent?}}`
- **User:** Cloud `{self:".../user?accountId=X", accountId, displayName, active, emailAddress?, accountType:"atlassian"}`. Server `{self:".../user?username=x", name, key, displayName, emailAddress, active}`.

### 4.4 Projects and metadata
| Method + path | Client method | Response read | Ref |
|---|---|---|---|
| GET `api2:project` | `projects()` | bare array `{self,id,key,name,projectTypeKey}` | 3842 |
| GET `api2:project/{k}?expand=issueTypes` | `project()`, `create_issue` (str project), `issue_type_by_name` | `id`, `issueTypes[]{self,id,name,subtask}` | 3861, 2318 |
| GET `api2:issuetype` | `issue_types()` | array | 3635 |
| GET `api2:issue/createmeta/{p}/issuetypes[/{id}]` | `project_issue_types()`, `project_issue_fields()` | offset page; items under `issueTypes`/`fields` (Cloud) or `values` (Server) | 3648-3705 |
| GET `api2:issue/createmeta` (legacy) | `createmeta()` | `{projects:[{issuetypes:[{fields}]}]}`; Cloud only (removed in DC 9) | 2644 |
| GET `api2:field` | `fields()` | `{id,name,custom,clauseNames[],schema{type,custom,customId}}`; **include `Rank` (`com.pyxis.greenhopper.jira:gh-lexo-rank`) and `Sprint`** | 1941, 6634 |
| GET `api2:status`, `priority`, `project/{k}/statuses`, `issueLinkType` | `statuses()` etc. | arrays (`issueLinkTypes` key for link types) | 4559, 3816, 4575, 3605 |

### 4.5 Issues
| Method + path | Client method | Request | Response / notes | Ref |
|---|---|---|---|---|
| GET `api2:issue/{k}` | `issue(id, fields, expand)` | `fields` comma-joined; `expand=changelog,renderedFields,transitions,names` | full issue | 2253 |
| POST `api2:issue` | `create_issue(fields=…)` | `{"fields":{project:{id|key}, summary, issuetype:{id|name}, description, assignee:{accountId}|{name}, labels, priority:{name|id}, parent:{key}, customfield_N}}` | **`{id,key,self}` required**, then GET `issue/{key}` | 2295-2350 |
| PUT `api2:issue/{k}` | `Issue.update(fields, update, **kw)` | `{"fields":{...},"update":{"labels":[{"add":"x"}|{"remove":"x"}|{"set":[]}],"comment":[{"add":{"body":".."}}]}}`; kw `assignee="x"` becomes `{"name":"x"}` **even on Cloud** | 204, then GET `self` | resources.py:826-893 |
| DELETE `api2:issue/{k}?deleteSubtasks=False` | `Issue.delete()` | | 204 | resources.py:927 |
| PUT `/rest/api/latest/issue/{k}/assignee` | `assign_issue` | Cloud `{"accountId": id|null|"-1"}`, Server `{"name"}` | 204; a non-accountId string triggers `user/search` first | 2717-2795 |
| GET `api2:issue/{k}/transitions[?expand=transitions.fields]` | `transitions()`, `find_transitionid_by_name()` | | `{"transitions":[{id,name,to{self,id,name,statusCategory},hasScreen,fields?}]}`; name matched case-insensitively | 3088-3135 |
| POST `api2:issue/{k}/transitions` | `transition_issue(issue, id|name, fields, comment)` | `{"transition":{"id"},"fields":{} (always sent),"update":{"comment":[{"add":{"body"}}]}}` | 204 | 3137-3195 |
| GET `api2:issue/{k}/comment` | `comments()` | `startAt,maxResults,orderBy,expand` | `{comments[],startAt,maxResults,total}` | 2797 |
| POST `api2:issue/{k}/comment` | `add_comment(issue, body, visibility)` | `{"body":"text","visibility":{type,value}}` | `{self:".../issue/{id}/comment/{cid}",id,author,body,created,updated}` | 2851 |
| PUT comment `self` | `Comment.update(body=)` | `{"body"}` | then GET `self` | resources.py:959 |
| POST `api2:issueLink` | `create_issue_link` | `{"type":{name},"inwardIssue":{key},"outwardIssue":{key}}` (resolves types via `issueLinkType` first) | 201, empty body | 3524 |
| GET `api2:issue/{k}/editmeta` | `editmeta()` | | `{fields:{}}` | 2933 |

### 4.6 Search (JQL)
| Mode | Endpoint | Paging | Ref |
|---|---|---|---|
| **Cloud** `search_issues()` | delegates to `enhanced_search_issues`: **`GET api2:search/jql`** (POST if `use_post`). Params: `jql, fields=*all, expand, properties, reconcileIssues, maxResults, nextPageToken`. | **Token.** Loops while `nextPageToken` is present and the page is non-empty. Reads `issues`. There is no `total` (`ResultList.total=None`). | 4205-4224, 4312-4395 |
| Cloud `search_issues(startAt>0)` | raises client-side, no HTTP | | 4222 |
| Cloud count | `POST api2:search/approximate-count {jql}` returns `count` | | 4420 |
| **Server/DC** | **`GET api2:search`** with `jql,startAt,maxResults,validateQuery,fields,expand` | **Offset.** Reads `issues,startAt,maxResults,total` (and `isLast`). | 4226-4262 |

Cloud facts: `search/jql` returns `{issues, nextPageToken?, isLast, names?, schema?}`; the default `fields` is `id`; `nextPageToken` is omitted on the last page; the page cap is 100. `/rest/api/{2,3}/search` is deprecated (CHANGE-2046) and Cloud has returned **410 Gone** since about Sept 2025.

**Minimal JQL grammar for agents**
| Category | Support |
|---|---|
| Fields | `project`, `key`/`issuekey`, `status`, `statusCategory`, `issuetype`/`type`, `assignee`, `reporter`, `priority`, `labels`, `resolution`, `fixVersion`, `component`, `parent`, `sprint`, `created`, `updated`, `duedate`, `resolved`, `summary`, `description`, `text`, `comment`, `cf[N]` |
| Operators | `=`, `!=`, `in ()`, `not in ()`, `~`, `!~`, `is [not] EMPTY`/`NULL`, `> >= < <=` on dates and priority |
| Values | bare words, `"quoted"`, numbers, `P-12`, accountIds, relative dates `-7d -2w -1h -30m`, `"2026-11-01[ 10:00]"` |
| Functions | `currentUser()`, `openSprints()`, `closedSprints()`, `futureSprints()`, `startOfDay/endOfDay/startOfWeek(±n)`, `now()`; `membersOf()` optional |
| Special values | `resolution = Unresolved`; `statusCategory in ("To Do","In Progress","Done")` or keys `new/indeterminate/done` |
| Logic and sort | `AND OR NOT ()`, case-insensitive keywords; `ORDER BY created|updated|priority|rank|key|duedate|status [ASC|DESC]` |
| Errors | 400 `{"errorMessages":["Error in the JQL Query: ..."],"errors":{}}`; for an unknown value: "The value 'X' does not exist for the field 'project'." |

### 4.7 Agile `/rest/agile/1.0/`
| Method + path | Client method | Request / response | Ref |
|---|---|---|---|
| GET `board?type&name&projectKeyOrId&startAt&maxResults` | `boards()` | `{values:[{id,self,name,type:"scrum"|"kanban",location{projectKey,projectId}}],startAt,maxResults,total,isLast}` | 6215 |
| GET `board/{id}` | `Board.find` | board | resources.py:1603 |
| GET `board/{id}/sprint?state=future,active,closed` | `sprints()` | `values:[{id,self,state,name,startDate,endDate,completeDate,originBoardId,goal}]` | 6254 |
| GET `sprint/{id}` | `sprint()` | sprint | 6443 |
| POST `sprint` | `create_sprint(name, board_id, startDate, endDate, goal)` | `{name,originBoardId,...}` returns the sprint (`future`) | 6512 |
| PUT `sprint/{id}` | `update_sprint(...)` | Partial body. `state:"active"` starts the sprint (needs dates); `state:"closed"` closes it. Returns JSON. | 6319 |
| POST `sprint/{id}/issue` | `add_issues_to_sprint` | `{"issues":[keys]}` (≤50) returns 204 | 6548 |
| POST `backlog/issue` | `move_to_backlog` | `{"issues"}` returns 204 | 6657 |
| PUT `issue/rank` | `rank()` | `{"issues":[k],"rankBeforeIssue"|"rankAfterIssue","rankCustomFieldId"}` returns 204/207 | 6599 |
| GET `board/{id}/issue`, `board/{id}/backlog`, `sprint/{id}/issue` | not called by the client; raw-REST agents use them | offset page with an `issues` key. Cloud deprecates these for `/rest/software/1.0/...` (nextPageToken). | Cloud Software spec |

Agile pagination is `values` + `startAt/maxResults/total/isLast`. The client stops on `isLast`, on a short page, or at `start>=total`.

### 4.8 Users
| Method + path | Client method | Notes | Ref |
|---|---|---|---|
| GET `api2:myself` | `myself()`, `current_user()` | `accountId` (Cloud) or `name/key` (Server), plus `displayName`, `emailAddress` | 4549, 5647 |
| GET `api2:user/search` | `search_users` | Cloud `query=`, Server `username=`, `includeActive=True&includeInactive=False`. **Bare array.** | 4897 |
| GET `api2:user?accountId=|username=` | `user(id)` | | 4630 |
| GET `api2:user/assignable/search`, `user/viewissue/search` | | `query/username` + `issueKey/projectKey` | 4937 |

Cloud GDPR mode: no `name`/`key`. accountIds must match `^[0-9a-f]{24}$` or `^\d+:<uuid>$`, otherwise `assign_issue` triggers a user search (client.py:122-125). https://developer.atlassian.com/cloud/jira/platform/deprecation-notice-user-privacy-api-migration-guide/

### 4.9 Errors and rate limits
| Item | Behaviour | Ref |
|---|---|---|
| Non-2xx | `JIRAError`. Text is `errorMessages[]` joined, overridden by `errors{}` values when non-empty; it also reads `message`/`errorMessage`. A 403 with `X-Authentication-Denied-Reason` uses that header. | resilientsession.py:50-142 |
| Body | `{"errorMessages":["Issue does not exist or you do not have permission to see it."],"errors":{}}` or `{"errorMessages":[],"errors":{"summary":"You must specify a summary of the issue."}}` | |
| Codes | 400 validation/JQL, 401, 403, 404 (Cloud hides invisible issues as 404), 409, 410 (Cloud legacy search), 429, 503 | |
| Retry | Only 429/503/ConnectionError, `max_retries=3`. Delay is `2*max(int(Retry-After),1)`, capped at 60 s. **`Retry-After` must be integer seconds** (`int()` crashes on an HTTP-date). | resilientsession.py:282-360 |
| Cloud 429 headers | `Retry-After`, `X-RateLimit-Limit/Remaining/Reset` (ISO 8601), `X-RateLimit-NearLimit`, `RateLimit-Reason` | https://developer.atlassian.com/cloud/jira/platform/rate-limiting/ |

### 4.10 Webhooks
| Kind | API | Notes |
|---|---|---|
| Admin (Cloud + DC) | `POST/GET /rest/webhooks/1.0/webhook`, `PUT/DELETE .../webhook/{id}` | `{name,url,events[],filters{"issue-related-events-section":JQL},excludeBody,secret?}` |
| Dynamic (Cloud apps only) | `POST/GET/DELETE /rest/api/{2,3}/webhook`, `PUT .../webhook/refresh` | `{url,webhooks:[{events[],jqlFilter}]}`; 30-day expiry |
| Events | `jira:issue_created|updated|deleted`, `comment_created|updated|deleted`, `sprint_created|updated|started|closed|deleted`, `board_*`, `jira:version_*` | |
| Payload | `timestamp`(ms), `webhookEvent`, `issue_event_type_name`, `user`, `issue`, `changelog{id,items[{field,fieldtype,from,fromString,to,toString}]}`, `comment` | |
| Delivery headers | `X-Hub-Signature: sha256=<HMAC>`, `X-Atlassian-Webhook-Identifier`, `X-Atlassian-Webhook-Retry`; ≤5 retries on 408/409/425/429/5xx | https://developer.atlassian.com/cloud/jira/platform/webhooks/ |

### 4.11 Cloud v2 vs v3 vs DC
| Aspect | Cloud v2 | Cloud v3 | DC (api/2) |
|---|---|---|---|
| Rich text (description, environment, comment body, worklog comment, textarea custom fields) | string (wiki markup) | **ADF** `{"type":"doc","version":1,"content":[{"type":"paragraph","content":[{"type":"text","text":"..."}]}]}` in requests and responses; a string gets 400 | string |
| Search | `search/jql` (token) | same | `search` (offset + `total`) |
| Users | accountId | accountId | name/key (+ `username` params) |
| Paths | ≈ identical (424 vs 433 paths in the specs) | | legacy createmeta removed in 9.0 |
| `deploymentType` | Cloud | Cloud | Server |
| Auth | Basic email:API-token, OAuth 2.0 (3LO), Connect JWT | same | Basic, **Bearer PAT** (8.14+) |

## 5. Minimum viable twins, ranked by what an agent hits first

### 5.1 GitHub twin: 27 ranked groups = 3 git endpoints + ~50 REST method+path pairs + GraphQL executor (7 query roots, 12 mutations, `__type`)
| # | Item | Hit by |
|---|---|---|
| 1 | git `GET info/refs?service=git-upload-pack` + `POST git-upload-pack` (v0+v2, 401 Basic challenge) | clone/fetch, every agent |
| 2 | `GET /api/v3/` (200, `X-OAuth-Scopes`) | gh auth login/status |
| 3 | GraphQL `viewer{login}` (`UserCurrent`) | gh auth, pr create |
| 4 | `GET /api/v3/user` | PyGithub/Octokit auth probe, `gh api user` |
| 5 | `GET /api/v3/meta` → `installed_version:"3.20.0"` | gh feature detection |
| 6 | GraphQL `__type(name){fields(includeDeprecated){name} enumValues{name}}` | gh introspection (5 ops) |
| 7 | GraphQL `repository(owner,name)` with core Repository fields | gh repo clone, pr create, issue create |
| 8 | `GET /repos/{o}/{r}` | PyGithub `get_repo` (eager) |
| 9 | `GET /contents/{path}`, `GET /branches[/{b}]`, `GET /git/ref/heads/{b}` | REST agents reading code |
| 10 | git `POST git-receive-pack` + post-receive → push/synchronize | every push |
| 11 | `POST /git/refs`, `PUT /contents/{path}` | API-only agents (no git) |
| 12 | GraphQL `repository.pullRequests(headRefName,states,baseRefName,first,after,orderBy)` + `PullRequest` type (see §2.4 field list) | gh pr create existing-PR check, pr list, pr view by branch |
| 13 | GraphQL mutation `createPullRequest` | gh pr create |
| 14 | `POST /pulls`, `GET /pulls`, `GET/PATCH /pulls/{n}` (+ `.diff` Accept) | PyGithub/Octokit, gh pr diff |
| 15 | GraphQL `repository.pullRequest(number)` + `node(id)` (`...on PullRequest` reviews/comments/statusCheckRollup) + empty `projectItems` | gh pr view/checks |
| 16 | `POST /statuses/{sha}`, `POST/PATCH /check-runs`, `GET /commits/{ref}/status`, `GET /commits/{ref}/check-runs` | CI posting, agents reading |
| 17 | GraphQL `statusCheckRollup.contexts` (`StatusContext` / `CheckRun` union, `isRequired`) | gh pr checks [--watch] |
| 18 | `GET /pulls/{n}/files`, `GET /compare/{b}...{h}`, `GET /commits[/{ref}]` | review agents |
| 19 | GraphQL `addPullRequestReview`, `addComment`; REST `POST/GET /pulls/{n}/reviews`, `POST/GET /pulls/{n}/comments`, `POST/GET /issues/{n}/comments` | review agents |
| 20 | GraphQL `requestReviews`, `updatePullRequest`; REST `POST/DELETE /pulls/{n}/requested_reviewers` | orchestrators |
| 21 | GraphQL `mergePullRequest`; REST `PUT /pulls/{n}/merge`, `GET /pulls/{n}/merge`, `DELETE /git/refs/heads%2F{b}` | merge step |
| 22 | GraphQL `repository.issues/issueOrPullRequest`, `createIssue`, `updateIssue`, `closeIssue`, `labels`/`assignableUsers`/`milestones` lookups; REST `POST/GET/PATCH /issues[/{n}]`, labels CRUD | issue triage |
| 23 | GraphQL `search(type:ISSUE)` + REST `GET /search/issues` (query subset: `repo: is:pr|issue is:open|closed author: assignee: label: head: base:` + free text) | gh pr/issue list with filters |
| 24 | Repo hooks CRUD + `ping` + deliveries for push, pull_request, pull_request_review, check_run, check_suite, status, issues, issue_comment | external orchestrators |
| 25 | `GET /actions/runs[/{id}]`, `/runs/{id}/jobs`, `/actions/workflows[/{id}]`, `/check-runs/{id}/annotations`, `/actions/jobs/{id}/logs` (302), `/actions/runs/{id}/logs` (zip) | gh run list/view/watch |
| 26 | git `refs/pull/N/head` advertised | gh pr checkout, CI |
| 27 | `GET /rate_limit`, `ETag`, rate-limit headers | throttling plugins |

GraphQL total: 7 query roots/sub-roots + 12 mutations (`createPullRequest`, `updatePullRequest`, `mergePullRequest`, `closePullRequest`, `reopenPullRequest`, `markPullRequestReadyForReview`, `addPullRequestReview`, `requestReviews`, `addComment`, `createIssue`, `updateIssue`, `closeIssue`), with `addLabelsToLabelable`/`removeLabelsFromLabelable` optional. This serves the ~27 named gh operations in §2.4.

### 5.2 Jira twin: 34 routes
| # | Route | Hit by |
|---|---|---|
| 1 | GET `/rest/api/{2,3}/serverInfo` | client constructor (Cloud/DC switch) |
| 2 | GET `/myself` | every agent's first call |
| 3-4 | GET+POST `/search/jql` (token paging) | Cloud search |
| 5-6 | GET+POST `/search` (offset; 410 in strict-Cloud mode) | DC / legacy agents |
| 7-9 | GET/POST/PUT `/issue[/{k}]` | read/create/update |
| 10-11 | GET+POST `/issue/{k}/transitions` | status moves |
| 12-15 | GET+POST `/issue/{k}/comment`, GET+PUT `/comment/{id}` | comments (v2 string / v3 ADF) |
| 16 | PUT `/rest/api/{2,3,latest}/issue/{k}/assignee` | assign |
| 17-18 | GET `/project`, `/project/{k}?expand=issueTypes` | create_issue lookups |
| 19-20 | GET `/issue/createmeta/{p}/issuetypes[/{id}]` | create forms |
| 21 | GET `/field` (with Rank, Sprint) | field-name mapping, rank |
| 22-23 | GET `/user/search`, `/user` | assign by name/email |
| 24-26 | GET `/issuetype`, `/status`, `/priority` | metadata |
| 27 | DELETE `/issue/{k}` | cleanup |
| 28-29 | GET `/issueLinkType`, POST `/issueLink` | dependencies |
| 30 | GET `/rest/agile/1.0/board[/{id}]` | sprint agents |
| 31 | GET `/board/{id}/sprint` | |
| 32 | POST `/sprint`, GET/PUT `/sprint/{id}` | sprint lifecycle |
| 33 | POST `/sprint/{id}/issue` | sprint planning |
| 34 | GET `/board/{id}/issue`, `/board/{id}/backlog`, `/sprint/{id}/issue` | raw-REST agents |

Next tier: `backlog/issue`, `issue/rank`, `/rest/webhooks/1.0/webhook` (+ delivery with `X-Hub-Signature`), `editmeta`, `search/approximate-count`, `issue/bulk`, `/rest/software/1.0/*`.

## 6. Open risks

1. **gh GraphQL fidelity is the largest single cost.** Queries are generated from `--json` field lists, so agents can request any of ~60 PullRequest/Issue fields (`pkg/cmd/pr/view` `--json`). Unknown fields fail the whole command. Mitigation: load the real GHES SDL, implement resolvers for the §2.4 set, return null or empty values for the rest (rather than leaving them out of the schema), and replay gh's fixture queries in CI. Pin a gh version in the sandbox image, because new gh releases add fields and feature probes (e.g. `api_host`, `gh event listen` → `event-subscriptions`, Nov 2026).
2. **TLS + hostname plumbing.** gh accepts only https on 443 for GHES (no port in the hostname). PyGithub asserts the host, port and path of every returned URL. git needs the credential helper and a 401 challenge. One misconfigured proxy or port-forward breaks every client in a different way. Agents hard-coded to `github.com` need a DNS + CA spoof mode, which then exposes dotcom-only code paths in gh.
3. **Computed state needs a real git engine and must be deterministic.** This covers `mergeable`/`mergeStateStatus`, compare `ahead_by`, PR files `patch`, squash/merge commits, `refs/pull/N/head`, and `synchronize` on push, all on real bare repos. Jira has the same problem with JQL semantics (`text ~`, relative dates and time zones, `ORDER BY rank`) and with workflow/transition rules. Agents assert on result sets, so small divergences read as agent failures.
4. **Exact strings and shapes are load-bearing.** gh matches graphql-ruby error text. PyGithub matches "Bad credentials" / "Not Found" / rate-limit messages, parses `Link` by `", "` splitting, and counts via `per_page=1` + `rel="last"`. Octokit takes the first key of namespaced lists. Jira's client requires `versionNumbers`, integer `Retry-After`, a non-empty body on every GET, and dereferenceable `self` URLs.
5. **Retries vs fault injection.** PyGithub retries POST on 5xx up to 10× and does not retry 429. Octokit throttles 403/429. Jira retries only 429/503. If a fault is injected after a write commits, agents create duplicate PRs, comments or issues. Inject before commit, or make writes idempotent per request.
6. **Jira dual format and dual deployment.** v2 wiki strings and v3 ADF must round-trip in one store, and Cloud vs DC switches identifiers (accountId vs name), user-search params, createmeta keys and search endpoints. The client also sends technically invalid input (`{"name":..}` assignee on Cloud, `True`/`False` bools, JSON nulls), so the twin must be lenient on input and exact on output.
7. **Caches mask twin changes.** gh caches feature-detection responses for 24 h (`~/.cache/gh`). Reset it per scenario.
