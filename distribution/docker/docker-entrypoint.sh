#!/usr/bin/env sh
set -o errexit

# Prevent core dumps
ulimit -c 0

if [ "${1:0:1}" = '-' ]; then
  set -- vector "$@"
fi

if vector "$1" --help 2>&1 | grep -q "vector $1"; then
  set -- vector "$@"
fi

# If the command is Vector, ensure we run as the `vector` user.
if [ "$1" = 'vector' ]; then
  if [ -z "$SKIP_CHOWN" ]; then
    if [ "$(stat -c %u /etc/vector)" != "$(id -u vector)" ]; then
      chown -R vector:vector /etc/vector || echo "Could not chown /etc/vector (may not have appropriate permissions)"
    fi

    if [ "$(stat -c %u /var/lib/vector)" != "$(id -u vector)" ]; then
      chown -R vector:vector /var/lib/vector || echo "Could not chown /var/lib/vector (may not have appropriate permissions)"
    fi
  fi

  if [ "$(id -u)" = '0' ]; then
    set -- gosu vector "$@"
  fi
fi

exec "$@"