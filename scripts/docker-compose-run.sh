#!/bin/bash
set -eu

export USER=$(id -u):$(id -g)
DC_FILE=docker-compose.test.yml
docker-compose -f $DC_FILE rm -svf $1 2>/dev/null || true
docker-compose -f $DC_FILE up --build --abort-on-container-exit --exit-code-from $1 $1 | sed $'s/^.*container exit...$/\033[0m\033[1A/'
