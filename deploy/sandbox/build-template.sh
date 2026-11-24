#!/usr/bin/env bash
# Builds the E2B template a hosted branch runs in, `shakedown-worker-<shaked rev>`, and prints its
# id (see README.md). Needs the E2B CLI, E2B_API_KEY, and release builds of shaked and the package
# programs for Linux x86_64 (glibc 2.39 or older: Ubuntu 24.04).
#   SHAKED_BIN_DIR       where those builds are (default core/target/release)
#   SHAKED_LINK          the control box's /worker (default wss://sd.coppertide.ai/worker)
#   TEMPLATE             the template's name (default shakedown-worker-<rev>)
#   CPU_COUNT MEMORY_MB  a sandbox's size (default 2 and 2048)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
: "${E2B_API_KEY:?set E2B_API_KEY (the owner keeps it in ~/.bashrc)}"
rev=$(git -C "$repo" rev-parse --short=12 HEAD)
git -C "$repo" diff --quiet HEAD -- || rev="$rev-dirty"
bin=${SHAKED_BIN_DIR:-$repo/core/target/release}
link=${SHAKED_LINK:-wss://sd.coppertide.ai/worker}
template=${TEMPLATE:-shakedown-worker-$rev}
[ -x "$bin/shaked" ] || {
  echo "no shaked in $bin: cargo build --release in core/ and in the root workspace first" >&2
  exit 1
}

# The context: bin/, shakedown/ (the packages), sandbox/ (this folder) and the Dockerfile.
ctx=$(mktemp -d)
trap 'rm -rf "$ctx"' EXIT
mkdir -p "$ctx/bin" "$ctx/shakedown/kits"
find "$bin" -maxdepth 1 -type f -perm -u+x \( -name shaked -o -name 'shakedown-*' \) \
  -exec cp {} "$ctx/bin/" \;
skip=(--exclude .venv --exclude __pycache__ --exclude .pytest_cache --exclude target
  --exclude node_modules --exclude tests)
rsync -a "${skip[@]}" "$repo/integrations" "$repo/benchmarks" "$ctx/shakedown/"
rsync -a "${skip[@]}" "$repo/kits/python" "$ctx/shakedown/kits/"
rsync -a --exclude /README.md --exclude /Dockerfile --exclude /build-template.sh \
  --exclude /check-template.sh "$here/" "$ctx/sandbox/"
cp "$here/Dockerfile" "$ctx/Dockerfile"
printf 'SHAKED_LINK=%s\nSHAKED_AGENT_IMAGE=shakedown-agent:%s\n' "$link" "$rev" >>"$ctx/sandbox/worker.env"

echo "building $template ($(ls "$ctx/bin" | wc -l) programs, link $link)"
e2b template create "$template" -p "$ctx" -d Dockerfile \
  -c /opt/shakedown/sandbox/start.sh --ready-cmd "docker info" \
  --cpu-count "${CPU_COUNT:-2}" --memory-mb "${MEMORY_MB:-2048}"
e2b template list | grep -E "/$template +" || { echo "$template is not listed" >&2; exit 1; }
