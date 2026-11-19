#!/bin/sh
# Builds the workbench image under the tag shared.yaml names. WORKBENCH_BASE picks the base
# image (default python:3.12-slim); see README, "The workbench image".
set -eu
here=$(cd "$(dirname "$0")" && pwd)
tag=$(sed -n 's/^workbench_image: *//p' "$here/../shared.yaml")
docker build --build-arg "BASE=${WORKBENCH_BASE:-python:3.12-slim}" -t "$tag" "$here"
