"""PyGithub, unchanged, in the environment shaked gives an agent (live_twins.rs):
  pygithub_flow.py open <owner/repo>          a branch `p` with one file, and a pull request of it: `opened <n>`
  pygithub_flow.py approve <owner/repo> <n>   an approving review
  pygithub_flow.py merge <owner/repo> <n>     a squash merge: `merged <bool>`"""
import os
import sys

from github import Auth, Github

gh = Github(base_url=os.environ["GITHUB_API_URL"], auth=Auth.Token(os.environ["GH_ENTERPRISE_TOKEN"]))
what, repo = sys.argv[1], gh.get_repo(sys.argv[2])
if what == "open":
    main = repo.get_branch(repo.default_branch)
    repo.create_git_ref("refs/heads/p", main.commit.sha)
    repo.create_file("P.md", "Add P", "a line\n", branch="p")
    print("opened", repo.create_pull(title="P", body="From PyGithub.", head="p", base=repo.default_branch).number)
elif what == "approve":
    repo.get_pull(int(sys.argv[3])).create_review(event="APPROVE", body="Good")
elif what == "merge":
    print("merged", repo.get_pull(int(sys.argv[3])).merge(merge_method="squash").merged)
