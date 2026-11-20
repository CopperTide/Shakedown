"""A driver that finishes at once: it registers its agent, asks Shakedown to finish the run (which a
run with a world time window holds until the window ends), then exits 0. No model and no tool call:
it tests that the run is still scored at its window."""
import json
import os
import urllib.request

base = f"{os.environ['ARENA_URL']}/api/branches/{os.environ['ARENA_BRANCH']}"


def post(path, body):
    req = urllib.request.Request(f"{base}/{path}", json.dumps(body).encode(), {"content-type": "application/json"})
    return json.loads(urllib.request.urlopen(req).read())


post("agents", {"id": "early"})
print(json.dumps(post("finish", {})))
