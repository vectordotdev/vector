#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

# skaffold-dockerignore.sh
#
# SUMMARY
#
#   Prepare .dockerignore for skaffold docker image build so we don't send the
#   whole `target/debug` dir to the docker as the context.

cat <<EOF >target/debug/.dockerignore
**/*
!vector
EOF
