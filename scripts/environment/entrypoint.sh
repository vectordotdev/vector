#!/usr/bin/env bash

# set HOSTNAME to container id for `cross`
if [ -f /.docker-container-id ]; then
  export HOSTNAME="$(cat /.docker-container-id)"
fi

if [ -z "$HOSTNAME" ]; then
  echo "Failed to properly set HOSTNAME, cross may not work"
  # Fallback if everything else fails
  export HOSTNAME="$(hostname || echo init)"
fi

exec "$@"
