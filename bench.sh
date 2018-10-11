#!/bin/bash

set -o errexit;
set -o nounset;
set -o pipefail;

if [ ! -f sample.log ]; then
  echo "generating sample.log"
  flog -o sample.log -t log -b $((100 * 1024 * 1024))
fi

cargo build --release

echo "input: $(wc -l < sample.log) lines"
./target/release/router | wc -l &
time cat sample.log | pv | nc localhost 1234
wait
