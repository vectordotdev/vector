#!/bin/bash
set -eu

cd $(dirname $0)/..

export USER=root # $(id -u):$(id -g)
docker-compose rm -svf $1 2>/dev/null || true
docker-compose up --build --abort-on-container-exit --exit-code-from $1 $1 | sed $'s/^.*container exit...$/\033[0m\033[1A/'
