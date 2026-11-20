"""A driver that hangs: it registers its agent, then waits forever without finishing. No model and
no tool call: it tests that shaked ends a run whose driver never finishes."""
import json
import os
import time
import urllib.request

url = f"{os.environ['ARENA_URL']}/api/branches/{os.environ['ARENA_BRANCH']}/agents"
req = urllib.request.Request(url, json.dumps({"id": "stuck"}).encode(), {"content-type": "application/json"})
urllib.request.urlopen(req).read()
time.sleep(600)
