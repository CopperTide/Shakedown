"""An agent's Jira workflow with the unchanged `jira` client, for the core's live test through the
proxy: it finds the twin from the route env (`JIRA_SERVER`, `JIRA_EMAIL`, `JIRA_API_TOKEN`), files an
issue, starts it, comments, puts it in the board's first sprint, and prints the issue's key and status."""
import os

from jira import JIRA


def run(server: str, email: str, token: str) -> tuple[str, str]:
    j = JIRA(server=server, basic_auth=(email, token))
    issue = j.create_issue(project="AC", summary="S", issuetype={"name": "Task"})
    j.transition_issue(issue, "In Progress")
    j.add_comment(issue, "on it")
    j.add_issues_to_sprint(j.sprints(1)[0].id, [issue.key])
    return issue.key, j.issue(issue.key).fields.status.name


if __name__ == "__main__":
    print(*run(os.environ["JIRA_SERVER"], os.environ["JIRA_EMAIL"], os.environ["JIRA_API_TOKEN"]))
