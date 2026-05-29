#!/usr/bin/env bash
# Launch the vector_to_vector_e2e_disk Antithesis run with a pinned fault profile.
#
# Every shot goes through this script so the webhook and fault inclusions are
# identical run to run — results stay comparable and a shot can never be launched
# with a fumbled or forgotten fault flag. Only duration and description vary, and
# the running git commit is stamped into the description so each shot records the
# code it tested.
#
# Required environment (read by snouty):
#   ANTITHESIS_TENANT       tenant name
#   ANTITHESIS_API_KEY      api key  (or ANTITHESIS_USERNAME + ANTITHESIS_PASSWORD)
#   ANTITHESIS_REPOSITORY   registry to push the built config + service images to
#
# Optional overrides:
#   DURATION=<minutes>      default 30
#   TEST_NAME=<name>        default vector_to_vector_e2e_disk
#   DESCRIPTION=<text>      default describes the fault profile; commit is appended
#   SOURCE=<identifier>     property-history key; default is the git branch
#   DRY_RUN=1               print the exact command and exit without submitting
# Extra snouty flags pass through, e.g.  ./launch.sh --recipients you@example.com
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Immutable per-build revision: the short commit, marked -dirty when the working
# tree has uncommitted changes so the tag never claims to be a clean commit it is
# not. Images are tagged by this, never :latest, so a shot can never reuse a stale
# mutable tag and every pushed image traces back to the source it was built from.
GIT_SHA="$(git -C "$SCRIPT_DIR" rev-parse --short HEAD 2>/dev/null || echo unknown)"
if [[ -n "$(git -C "$SCRIPT_DIR" status --porcelain 2>/dev/null)" ]]; then
  GIT_SHA="${GIT_SHA}-dirty"
fi
export V2V_IMAGE_TAG="$GIT_SHA"

WEBHOOK="persistent_storage"
DURATION="${DURATION:-30}"
TEST_NAME="${TEST_NAME:-vector_to_vector_e2e_disk}"
DESCRIPTION="${DESCRIPTION:-disk_v2 conservation under crash/hang/throttle of head and tail} (commit ${GIT_SHA})"

# Property-history key. Passing --source makes the run tracked (not ephemeral),
# so findings are produced and each property's history is grouped by this key.
# Default to the branch so history follows the branch; without it snouty runs
# ephemeral and no findings are available to triage.
SOURCE="${SOURCE:-$(git -C "$SCRIPT_DIR" rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)}"

# Pinned fault profile — the single source of truth for a shot. head and tail (the
# SUT) take node termination, hang, and throttle: that is the crash-and-reopen path
# on the persistent volume where the buffer's recovery bugs live. The oracle is
# omitted from termination and hang ONLY — its obligation ledger is in-memory, so
# killing or freezing it would erase the source of truth. It is deliberately NOT
# spared network faults: partitioning tail->oracle exercises the delivery path (the
# http sink must buffer and retry), and producer->head exercises injection. Those
# are safe because Antithesis stops all faults in the final eventually_ window, so
# links heal and the drain-wait reconciles the backlog before conservation is
# judged. (The producer's loopback /claim and /acked inside the oracle container are
# never network-faulted regardless.) cpu_mod perturbs the writer/reader/finalizer
# races; clock_jitter stresses the 500ms fsync window.
FAULTS=(
  --param custom.include_for_node_termination="head tail"
  --param custom.include_for_node_hang="head tail"
  --param custom.include_for_node_throttle="head tail"
  --param custom.cpu_mod=true
  --param custom.clock_jitter=true
)

for v in ANTITHESIS_TENANT ANTITHESIS_REPOSITORY; do
  if [[ -z "${!v:-}" ]]; then
    echo "error: $v is not set (required to build and submit the run)" >&2
    exit 1
  fi
done

# Rebuild the images from current source before submitting. snouty reuses a
# matching :latest tag instead of rebuilding, so without this a shot can ship
# stale code (e.g. an image baked before a config rename or a code change).
# Layer caching keeps this near-instant when nothing changed.
build=(docker compose -f "$SCRIPT_DIR/docker-compose.yaml" build)

cmd=(snouty launch
  --webhook "$WEBHOOK"
  --config "$SCRIPT_DIR"
  --test-name "$TEST_NAME"
  --description "$DESCRIPTION"
  --source "$SOURCE"
  --duration "$DURATION"
  "${FAULTS[@]}"
  "$@")

printf 'build: '; printf ' %q' "${build[@]}"; printf '\n'
printf 'launch:'; printf ' %q' "${cmd[@]}"; printf '\n'
if [[ "${DRY_RUN:-0}" == "1" ]]; then
  echo "(dry run; not building or submitting)"
  exit 0
fi
"${build[@]}"
exec "${cmd[@]}"
