#!/usr/bin/env sh
set -o errexit

# Prevent core dumps.
ulimit -c 0

# If the user is trying to run Vector with options, just pass them along.
case $1 in -*) set -- vector "$@"; esac

# Look for Vector subcommands.
if [ "$1" = 'help' ]; then
    # This needs a special case because there's no `help --help` output.
    set -- vector "$@"
# We grep Vector's help output to check for the provided subcommand.
elif vector "$1" --help 2>&1 | grep -q "vector $1"; then
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
fi

exec "$@"
