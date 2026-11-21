"""A driver that says which run it was started for: it registers its agent, writes SHAKEDOWN_FORK_OF to
stderr (empty on a first run) and finishes. No model and no tool call: it tests how shaked starts a
driver again for a fork."""
import json
import os
import sys
import urllib.request

base, branch = os.environ["SHAKEDOWN_URL"], os.environ["SHAKEDOWN_BRANCH"]


def post(path, body):
    req = urllib.request.Request(f"{base}/api/branches/{branch}/{path}", json.dumps(body).encode(),
                                 {"content-type": "application/json"})
    return urllib.request.urlopen(req).read()


post("agents", {"id": "worker"})
print(f"fork of: {os.environ.get('SHAKEDOWN_FORK_OF', '')}", file=sys.stderr)
post("finish", {})
