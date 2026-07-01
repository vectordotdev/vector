#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Auto-detect Colima Docker socket and set TMPDIR to a path the VM can mount
COLIMA_SOCK="$HOME/.colima/default/docker.sock"
if [[ -z "${DOCKER_HOST:-}" && -S "${COLIMA_SOCK}" ]]; then
    export DOCKER_HOST="unix://${COLIMA_SOCK}"
fi
if [[ -n "${DOCKER_HOST:-}" && "${DOCKER_HOST}" == *colima* ]]; then
    # macOS /var/folders tmp dir isn't mounted in Colima; use a home-dir path
    mkdir -p "$HOME/.tmp-smp"
    export TMPDIR="$HOME/.tmp-smp"
fi

usage() {
    cat <<EOF
Usage: $0 <command> [args...]

Commands:
  build  <tag>                              Build a Vector image from current source
  derive <base-tag> <new-tag> KEY=VAL ...   Create a variant image with env vars
  run    <case> <baseline> <comparison> ..  Run an smp comparison
  cases                                     List available regression cases
EOF
    exit 1
}

cmd_build() {
    local tag="${1:?Usage: $0 build <tag>}"
    echo "Building vector:${tag} ..."
    DOCKER_BUILDKIT=1 docker buildx build \
        --build-context "vrl=${SCRIPT_DIR}/../vrl" \
        -f "${SCRIPT_DIR}/Dockerfile.bench" \
        -t "vector:${tag}" \
        "${SCRIPT_DIR}"
}

cmd_derive() {
    if [[ $# -lt 3 ]]; then
        echo "Usage: $0 derive <base-tag> <new-tag> KEY=VAL [KEY=VAL ...]" >&2
        exit 1
    fi
    local base="$1"; shift
    local new_tag="$1"; shift

    local dockerfile="FROM vector:${base}"
    for kv in "$@"; do
        dockerfile="${dockerfile}"$'\n'"ENV ${kv}"
    done

    echo "Deriving vector:${new_tag} from vector:${base} ..."
    echo "${dockerfile}" | docker buildx build -t "vector:${new_tag}" -
}

cmd_run() {
    if [[ $# -lt 3 ]]; then
        echo "Usage: $0 run <case> <baseline-tag> <comparison-tag> [smp args...]" >&2
        exit 1
    fi
    local case_name="$1"; shift
    local baseline="$1"; shift
    local comparison="$1"; shift

    smp local run \
        --experiment-dir "${SCRIPT_DIR}/regression" \
        --case "${case_name}" \
        --baseline-image "vector:${baseline}" \
        --comparison-image "vector:${comparison}" \
        "$@"
}

cmd_cases() {
    ls "${SCRIPT_DIR}/regression/cases/"
}

[[ $# -lt 1 ]] && usage

command="$1"; shift
case "${command}" in
    build)  cmd_build "$@" ;;
    derive) cmd_derive "$@" ;;
    run)    cmd_run "$@" ;;
    cases)  cmd_cases ;;
    *)      usage ;;
esac
