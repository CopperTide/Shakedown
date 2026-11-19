"""A driver that crashes: it registers its agent with the run Shakedown started it for, then exits
with code 3 before finishing. No model and no tool call: it tests how arenad ends the run."""
import json
import os
import sys
import urllib.request

url = f"{os.environ['ARENA_URL']}/api/branches/{os.environ['ARENA_BRANCH']}/agents"
req = urllib.request.Request(url, json.dumps({"id": "crasher"}).encode(),
                             {"content-type": "application/json"})
urllib.request.urlopen(req).read()
print("the team's code broke", file=sys.stderr)
sys.exit(3)
