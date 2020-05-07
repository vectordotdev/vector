#!/bin/bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

make -f tests/Makefile "$1" -Bnd | make2graph | graph-easy --as=boxart
