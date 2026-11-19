#!/usr/bin/env bash
# Builds the software factory's own images on this machine, under the names images.yaml gives them:
#   base  python + the pinned wheelhouse + platform-lib (needs the network: pip downloads the pinned wheels)
#   apps  each repository's image, as CI builds it (no network)
#   all   both (the default)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
template="$(dirname "$here")"

image_of() {  # the `image:` of the images.yaml entry with this id
    awk -v id="$1" '$1 == "-" && $2 == "id:" { found = ($3 == id) } found && $1 == "image:" { print $2; exit }' \
        "$template/images.yaml"
}

build_base() {
    docker build --pull=false -t "$(image_of base)" -f "$here/base/Dockerfile" "$here"
}

build_apps() {
    for repo in storefront orders inventory; do
        docker build --pull=false --network none -t "$(image_of "$repo")" "$template/repos/$repo"
    done
}

case "${1:-all}" in
    base) build_base ;;
    apps) build_apps ;;
    all) build_base; build_apps ;;
    *) echo "usage: $0 [base|apps|all]" >&2; exit 2 ;;
esac
