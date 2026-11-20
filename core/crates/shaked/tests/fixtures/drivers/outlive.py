"""A driver that outlives its run: it writes its pid to PID_FILE, finishes the run, then sleeps.
No model and no tool call: it tests that shaked kills it once the run is over."""
import os
import time
import urllib.request

with open(os.environ["PID_FILE"], "w") as f:
    f.write(str(os.getpid()))
url = f"{os.environ['ARENA_URL']}/api/branches/{os.environ['ARENA_BRANCH']}/finish"
req = urllib.request.Request(url, b"{}", {"content-type": "application/json"})
urllib.request.urlopen(req).read()
time.sleep(120)
