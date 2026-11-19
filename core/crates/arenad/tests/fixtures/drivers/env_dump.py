"""A driver that writes its environment as JSON to the file its first argument names, and exits.
No model and no tool call: it tests what arenad gives a driver."""
import json
import os
import sys

with open(sys.argv[1] + ".tmp", "w") as f:
    json.dump(dict(os.environ), f)
os.replace(sys.argv[1] + ".tmp", sys.argv[1])
