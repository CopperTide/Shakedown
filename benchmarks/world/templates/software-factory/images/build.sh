#!/usr/bin/env bash
# Builds the software factory's own images on this machine, under the names images.yaml gives them:
#   base  python + the pinned wheelhouse + platform-lib (needs the network: pip downloads the pinned wheels)
#   apps       each repository's image, as CI builds it (no network)
#   workbench  the designs' workbench (designs/software-factory/workbench) on the base image, so their agents'
#              run_tests has the services' test dependencies offline (needs the network: apt, pip, gh)
#   all        all three (the default)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
template="$(dirname "$here")"
repo="$(cd "$template/../../../.." && pwd)"

image_of() {  # the `image:` of the images.yaml entry with this id
    awk -v id="$1" '$1 == "-" && $2 == "id:" { found = ($3 == id) } found && $1 == "image:" { print $2; exit }' \
        "$template/images.yaml"
}

build_base() {
    docker build --pull=false -t "$(image_of base)" -f "$here/base/Dockerfile" "$here"
}

build_apps() {
    for app in storefront orders inventory; do
        docker build --pull=false --network none -t "$(image_of "$app")" "$template/repos/$app"
    done
}

build_workbench() {
    WORKBENCH_BASE="$(image_of base)" "$repo/designs/software-factory/workbench/build.sh"
}

case "${1:-all}" in
    base) build_base ;;
    apps) build_apps ;;
    workbench) build_workbench ;;
    all) build_base; build_apps; build_workbench ;;
    *) echo "usage: $0 [base|apps|workbench|all]" >&2; exit 2 ;;
esac
