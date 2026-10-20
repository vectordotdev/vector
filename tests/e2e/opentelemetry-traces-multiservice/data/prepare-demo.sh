#!/usr/bin/env bash
# Download the unmodified Demo 3.1.0 deployment files, pinned to its commit.
set -euo pipefail

demo_revision=dedc0178918e260823323b8d95005a8cb924b007
demo_directory="$(cd "$(dirname "$0")/../config" && pwd)/upstream"
mkdir -p "$demo_directory"

for demo_file in compose.yaml compose.full.yaml .env
do
  curl --fail --silent --show-error --location --retry 3 \
    "https://raw.githubusercontent.com/open-telemetry/opentelemetry-demo/$demo_revision/$demo_file" \
    --output "$demo_directory/$demo_file.tmp"
  mv "$demo_directory/$demo_file.tmp" "$demo_directory/$demo_file"
done
