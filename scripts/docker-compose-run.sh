#!/bin/bash
set -eu

DC_FILE=docker-compose.test.yml
docker-compose -f $DC_FILE rm -svf $1 2>/dev/null || true
docker-compose -f $DC_FILE up --build --no-color --abort-on-container-exit --exit-code-from $1 $1 | head -n-1
