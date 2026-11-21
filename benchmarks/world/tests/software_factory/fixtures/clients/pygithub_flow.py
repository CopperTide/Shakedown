"""An agent's PyGithub, unchanged, in the environment shaked gives it: python pygithub_flow.py <owner/repo> <number>
<writer|readonly>. Prints `listed <title>` once the pull request is among the open ones; a read-only agent then tries
to merge it and prints `refused <status>` (exit 0) or `merged` (exit 1)."""
import os
import sys

from github import Auth, Github, GithubException

repo_name, number, role = sys.argv[1], int(sys.argv[2]), sys.argv[3]
gh = Github(base_url=os.environ["GITHUB_API_URL"], auth=Auth.Token(os.environ["GH_ENTERPRISE_TOKEN"]))
repo = gh.get_repo(repo_name)
pull = repo.get_pull(number)
assert number in [p.number for p in repo.get_pulls(state="open")]
print("listed", pull.title)
if role == "readonly":
    try:
        pull.merge(merge_method="merge")
    except GithubException as e:
        print("refused", e.status)
        sys.exit(0)
    print("merged")
    sys.exit(1)
