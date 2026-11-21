"""The notices the twin reports to the world's people (`notice` events at the next advance)."""
import re

MENTION = re.compile(r"(?<![\w@])@([A-Za-z0-9](?:[A-Za-z0-9-]{0,38}))")


def review_requested(ctx, repo: str, pr, reviewer: str, by: str, checks: str) -> None:
    ctx.notify("review_requested", f"{by} asked {reviewer} to review {repo}#{pr['number']}",
               {"repo": repo, "number": pr["number"], "reviewer": reviewer, "author": pr["user"],
                "title": pr["title"], "checks": checks})


def review_submitted(ctx, repo: str, pr, review) -> None:
    ctx.notify("review_submitted", f"{review['user']} reviewed {repo}#{pr['number']}: {review['state']}",
               {"repo": repo, "number": pr["number"], "author": pr["user"], "reviewer": review["user"],
                "state": review["state"], "body": review["body"]})

def pr_opened(ctx, repo: str, pr) -> None:
    ctx.notify("pr_opened", f"{pr['user']} opened {repo}#{pr['number']}: {pr['title']}",
               {"repo": repo, "number": pr["number"], "author": pr["user"], "title": pr["title"],
                "head": pr["head_ref"], "base": pr["base_ref"]})


def pr_merged(ctx, repo: str, number: int, by: str, sha: str) -> None:
    ctx.notify("pr_merged", f"{by} merged {repo}#{number}", {"repo": repo, "number": number, "by": by, "sha": sha})


def checks_completed(ctx, repo: str, sha: str, conclusion: str, prs: list) -> None:
    ctx.notify("checks_completed", f"checks on {repo}@{sha[:7]} completed: {conclusion}",
               {"repo": repo, "sha": sha, "conclusion": conclusion, "prs": prs})


def mentions(ctx, repo: str, number: int, author: str, body: str | None) -> None:
    """A `mentioned` notice for each login `body` names with `@` but the author's (people of the world
    need not have called the twin yet)."""
    for login in dict.fromkeys(MENTION.findall(body or "")):
        if login != author:
            ctx.notify("mentioned", f"{author} mentioned {login} in {repo}#{number}",
                       {"user": login, "repo": repo, "number": number, "body": body})
