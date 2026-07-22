#!/bin/sh
set -eu

if [ -z "${VECTOR_IGGY_URL:-}" ] \
  && [ -n "${IGGY_USERNAME_FILE:-}" ] \
  && [ -n "${IGGY_PASSWORD_FILE:-}" ]; then
  VECTOR_IGGY_URL="iggy://$(cat "$IGGY_USERNAME_FILE"):$(cat "$IGGY_PASSWORD_FILE")@${IGGY_ADDRESS:-iggy:8090}"
  export VECTOR_IGGY_URL
fi

if [ -z "${VECTOR_IGGY_URL:-}" ]; then
  echo "VECTOR_IGGY_URL or Iggy credential files are required" >&2
  exit 1
fi

exec "$@"
