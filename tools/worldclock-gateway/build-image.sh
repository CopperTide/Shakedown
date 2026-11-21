#!/bin/sh
# Builds the gateway (release) and its image shakedown/worldclock-gateway:<version>, never pulling: the base image
# debian:bookworm-slim must be present (`docker pull debian:bookworm-slim`). The binary is checked to need no
# glibc newer than Debian 12's 2.36.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$here/Cargo.toml" | head -1)
(cd "$root" && cargo build --release -p worldclock-gateway)
bin="$root/core/target/release/worldclock-gateway"
too_new=$(objdump -T "$bin" | grep -oE 'GLIBC_2\.(3[7-9]|[4-9][0-9])' | sort -u || true)
[ -z "$too_new" ] || { echo "the binary needs $too_new, newer than Debian 12's glibc" >&2; exit 1; }
ctx=$(mktemp -d)
trap 'rm -rf "$ctx"' EXIT
cp "$here/Dockerfile" "$bin" "$ctx/"
docker build --pull=false -t "shakedown/worldclock-gateway:$version" "$ctx"
echo "shakedown/worldclock-gateway:$version"
