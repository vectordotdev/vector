#!/usr/bin/env bash
set -o pipefail

# influxdb_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector InfluxDB Integration test environment

if [ $# -ne 2 ]
then
    echo "Usage: $0 {stop|start} {docker|podman}" 1>&2; exit 1;
    exit 1
fi
ACTION=$1
CONTAINER_TOOL=$2
#
# Functions
#

start_podman () {
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" create --replace --name vector-test-integration-influxdb -p 8086:8086 -p 8087:8087 -p 9999:9999
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-influxdb --name vector_influxdb_v1 \
	 -e INFLUXDB_REPORTING_DISABLED=true influxdb:1.8
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-influxdb --name vector_influxdb_v1_tls \
	 -e INFLUXDB_REPORTING_DISABLED=true -e INFLUXDB_HTTP_HTTPS_ENABLED=true -e INFLUXDB_HTTP_BIND_ADDRESS=:8087 -e INFLUXDB_BIND_ADDRESS=:8089 \
	 -e INFLUXDB_HTTP_HTTPS_CERTIFICATE=/etc/ssl/localhost.crt -e INFLUXDB_HTTP_HTTPS_PRIVATE_KEY=/etc/ssl/localhost.key \
	 -v "$(pwd)"/tests/data:/etc/ssl:ro influxdb:1.8
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-influxdb --name vector_influxdb_v2 \
	 -e INFLUXDB_REPORTING_DISABLED=true  quay.io/influxdb/influxdb:2.0.0-rc influxd --reporting-disabled --http-bind-address=:9999
}

start_docker () {
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" create vector-test-integration-influxdb
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-influxdb -p 8086:8086 --name vector_influxdb_v1 \
	 -e INFLUXDB_REPORTING_DISABLED=true influxdb:1.8
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-influxdb -p 8087:8087 --name vector_influxdb_v1_tls \
	 -e INFLUXDB_REPORTING_DISABLED=true -e INFLUXDB_HTTP_HTTPS_ENABLED=true -e INFLUXDB_HTTP_BIND_ADDRESS=:8087 \
	 -e INFLUXDB_HTTP_HTTPS_CERTIFICATE=/etc/ssl/localhost.crt -e INFLUXDB_HTTP_HTTPS_PRIVATE_KEY=/etc/ssl/localhost.key \
	 -v "$(pwd)"/tests/data:/etc/ssl:ro influxdb:1.8
  "${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-influxdb -p 9999:9999 --name vector_influxdb_v2 \
	 -e INFLUXDB_REPORTING_DISABLED=true  quay.io/influxdb/influxdb:2.0.0-rc influxd --reporting-disabled --http-bind-address=:9999
}

stop_podman () {
  "${CONTAINER_TOOL}" rm --force vector_influxdb_v1 vector_influxdb_v1_tls vector_influxdb_v2 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" stop vector-test-integration-influxdb 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" rm --force vector-test-integration-influxdb 2>/dev/null; true
}

stop_docker () {
  "${CONTAINER_TOOL}" rm --force vector_influxdb_v1 vector_influxdb_v1_tls vector_influxdb_v2 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" rm vector-test-integration-influxdb 2>/dev/null; true
}

echo "Running $ACTION action for InfluxDB integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
