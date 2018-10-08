#!/bin/bash

set -o errexit;
set -o nounset;
set -o pipefail;

if [ ! -f sample.log ]; then
  echo "generating sample log file"
  flog -o sample.log -t log -b $((100 * 1024 * 1024))
fi

cargo build --release

time cat sample.log | ./target/release/router | pv > /dev/null
