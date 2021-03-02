#!/usr/bin/env bash
set -o pipefail

# postgresql_metrics_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector PostgreSQL metrics Integration test environment

if [ $# -ne 1 ]
then
    echo "Usage: $0 {stop|start}" 1>&2; exit 1;
    exit 1
fi
ACTION=$1

#
# Functions
#

start_podman () {
  cp "$(pwd)"/tests/data/localhost.crt "$(pwd)"/tests/data/postgresql-local-socket-initdb/
  cp "$(pwd)"/tests/data/localhost.key "$(pwd)"/tests/data/postgresql-local-socket-initdb/
  podman pod create --replace --name vector-test-integration-postgresql_metrics -p 5432:5432
  podman run -d --pod=vector-test-integration-postgresql_metrics --name vector_postgresql_metrics \
  --volume "$(pwd)"/tests/data/postgresql-local-socket/:/var/run/postgresql/ \
  --volume "$(pwd)"/tests/data/postgresql-local-socket-initdb/:/docker-entrypoint-initdb.d/ \
  --env POSTGRES_USER=vector --env POSTGRES_PASSWORD=vector postgres:13.1 bash -c "\
  cp /docker-entrypoint-initdb.d/localhost.key /localhost.key && \
  chown postgres:postgres /localhost.key && \
  chmod 600 /localhost.key && \
  /docker-entrypoint.sh postgres \
  -c ssl=on \
  -c ssl_key_file=/localhost.key \
  -c ssl_cert_file=/docker-entrypoint-initdb.d/localhost.crt \
  -c ssl_ca_file=/docker-entrypoint-initdb.d/localhost.crt"
}

start_docker () {
  cp "$(pwd)"/tests/data/localhost.crt "$(pwd)"/tests/data/postgresql-local-socket-initdb/
  cp "$(pwd)"/tests/data/localhost.key "$(pwd)"/tests/data/postgresql-local-socket-initdb/
  docker network create vector-test-integration-postgresql_metrics
  docker run -d --network=vector-test-integration-postgresql_metrics -p 5432:5432 --name vector_postgresql_metrics \
  --volume "$(pwd)"/tests/data/postgresql-local-socket/:/var/run/postgresql/ \
  --volume "$(pwd)"/tests/data/postgresql-local-socket-initdb/:/docker-entrypoint-initdb.d/ \
  --env POSTGRES_USER=vector --env POSTGRES_PASSWORD=vector postgres:13.1 bash -c "\
  cp /docker-entrypoint-initdb.d/localhost.key /localhost.key && \
  chown postgres:postgres /localhost.key && \
  chmod 600 /localhost.key && \
  /docker-entrypoint.sh postgres \
  -c ssl=on \
  -c ssl_key_file=/localhost.key \
  -c ssl_cert_file=/docker-entrypoint-initdb.d/localhost.crt \
  -c ssl_ca_file=/docker-entrypoint-initdb.d/localhost.crt"
}

stop_podman () {
  podman rm --force vector_postgresql_metrics 2>/dev/null; true
  podman pod stop vector-test-integration-postgresql_metrics 2>/dev/null; true
  podman pod rm --force vector-test-integration-postgresql_metrics 2>/dev/null; true
}

stop_docker () {
  docker rm --force vector_postgresql_metrics 2>/dev/null; true
  docker network rm vector-test-integration-postgresql_metrics 2>/dev/null; true
}

echo "Running $ACTION action for PostgreSQL metrics integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
