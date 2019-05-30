#!/usr/bin/env bash

set -eou pipefail

docker build -t timberio/vector-builder-x86_64-unknown-linux-gnu:latest x86_64-unknown-linux-gnu
docker build -t timberio/vector-builder-x86_64-unknown-freebsd:latest x86_64-unknown-freebsd

docker push timberio/vector-builder-x86_64-unknown-linux-gnu:latest
docker push timberio/vector-builder-x86_64-unknown-freebsd:latest
