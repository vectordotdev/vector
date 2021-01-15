#!/usr/bin/env bash
set -o pipefail

# influxdb_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector InfluxDB Integration test environment

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
  podman pod create --replace --name vector-test-integration-influxdb -p 8086:8086 -p 8087:8087 -p 9999:9999
  podman run -d --pod=vector-test-integration-influxdb --name vector_influxdb_v1 \
	 -e INFLUXDB_REPORTING_DISABLED=true influxdb:1.8
  podman run -d --pod=vector-test-integration-influxdb --name vector_influxdb_v1_tls \
	 -e INFLUXDB_REPORTING_DISABLED=true -e INFLUXDB_HTTP_HTTPS_ENABLED=true -e INFLUXDB_HTTP_BIND_ADDRESS=:8087 -e INFLUXDB_BIND_ADDRESS=:8089 \
	 -e INFLUXDB_HTTP_HTTPS_CERTIFICATE=/etc/ssl/localhost.crt -e INFLUXDB_HTTP_HTTPS_PRIVATE_KEY=/etc/ssl/localhost.key \
	 -v "$(pwd)"/tests/data:/etc/ssl:ro influxdb:1.8
  podman run -d --pod=vector-test-integration-influxdb --name vector_influxdb_v2 \
	 -e INFLUXDB_REPORTING_DISABLED=true  quay.io/influxdb/influxdb:2.0.0-rc influxd --reporting-disabled --http-bind-address=:9999
}

start_docker () {
  docker network create vector-test-integration-influxdb
  docker run -d --network=vector-test-integration-influxdb -p 8086:8086 --name vector_influxdb_v1 \
	 -e INFLUXDB_REPORTING_DISABLED=true influxdb:1.8
  docker run -d --network=vector-test-integration-influxdb -p 8087:8087 --name vector_influxdb_v1_tls \
	 -e INFLUXDB_REPORTING_DISABLED=true -e INFLUXDB_HTTP_HTTPS_ENABLED=true -e INFLUXDB_HTTP_BIND_ADDRESS=:8087 \
	 -e INFLUXDB_HTTP_HTTPS_CERTIFICATE=/etc/ssl/localhost.crt -e INFLUXDB_HTTP_HTTPS_PRIVATE_KEY=/etc/ssl/localhost.key \
	 -v "$(pwd)"/tests/data:/etc/ssl:ro influxdb:1.8
  docker run -d --network=vector-test-integration-influxdb -p 9999:9999 --name vector_influxdb_v2 \
	 -e INFLUXDB_REPORTING_DISABLED=true  quay.io/influxdb/influxdb:2.0.0-rc influxd --reporting-disabled --http-bind-address=:9999
}

stop_podman () {
  podman rm --force vector_influxdb_v1 vector_influxdb_v1_tls vector_influxdb_v2 2>/dev/null; true
  podman pod stop vector-test-integration-influxdb 2>/dev/null; true
  podman pod rm --force vector-test-integration-influxdb 2>/dev/null; true
}

stop_docker () {
  docker rm --force vector_influxdb_v1 vector_influxdb_v1_tls vector_influxdb_v2 2>/dev/null; true
  docker network rm vector-test-integration-influxdb 2>/dev/null; true
}

echo "Running $ACTION action for InfluxDB integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
