"""A built world's benchmark program: `python -m worldbench --world world.json --version V` (docs/protocol.md,
"Benchmark calls")."""
import argparse
from pathlib import Path

from arena_part import serve_benchmark

from .runtime import WorldBenchmark


def main() -> None:
    p = argparse.ArgumentParser(prog="worldbench", description="Serve a built world as a benchmark")
    p.add_argument("--world", required=True, type=Path, help="the world.json of the built world")
    p.add_argument("--version", default="0.0.0", help="the version its manifest names")
    args = p.parse_args()
    serve_benchmark(WorldBenchmark(args.world, args.version))


main()
