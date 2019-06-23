#!/usr/bin/env bash

set -eou pipefail

docker build -t timberio/vector-deb-builder:latest deb-builder
docker build -t timberio/vector-release-builder:latest release-builder
docker build -t timberio/vector-x86_64-unknown-linux-gnu-builder:latest x86_64-unknown-linux-gnu-builder
docker build -t timberio/vector-x86_64-unknown-freebsd-builder:latest x86_64-unknown-freebsd-builder

docker push timberio/vector-deb-builder:latest
docker push timberio/vector-release-builder:latest
docker push timberio/vector-x86_64-unknown-linux-gnu-builder:latest
docker push timberio/vector-x86_64-unknown-freebsd-builder:latest
