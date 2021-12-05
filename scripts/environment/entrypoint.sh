#! /usr/bin/env bash

# set HOSTNAME to container id for `cross`
HOSTNAME="$(head -1 /proc/self/cgroup|cut -d/ -f3)"
export HOSTNAME

exec "$@"
