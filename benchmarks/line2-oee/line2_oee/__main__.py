"""line2-oee: reach a target OEE with a plant's simulation model (docs/protocol.md, "Benchmark calls")."""
from shakedown_part import serve_benchmark

from .bench import Line2Oee

serve_benchmark(Line2Oee())
