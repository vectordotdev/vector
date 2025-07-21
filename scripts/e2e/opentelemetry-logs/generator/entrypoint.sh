#!/bin/sh
set -ex

DIR="$(cd "$(dirname "$0")" && pwd)"
pip install --no-cache-dir -r "$DIR/requirements.txt"
exec python "$DIR/logs_generator.py" "$@"
