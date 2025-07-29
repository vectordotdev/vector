#!/usr/bin/env bash

# set HOSTNAME to container id for `cross`
if [ -f /.docker-container-id ]; then
  HOSTNAME="$(cat /.docker-container-id)"
  export HOSTNAME
fi

if [ -z "$HOSTNAME" ]; then
  echo "Failed to properly set HOSTNAME, cross may not work"
  # Fallback if everything else fails
  HOSTNAME="vector-environment"
  export HOSTNAME
fi

exec "$@"
