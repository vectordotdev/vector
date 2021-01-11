#!/usr/bin/env bash
set -o pipefail

# elasticsearch_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector Elasticsearch Integration test environment

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
  podman pod create --replace --name vector-test-integration-elasticsearch -p 4571:4571 -p 9200:9200 -p 9300:9300 -p 9201:9200 -p 9301:9300
  podman run -d --pod=vector-test-integration-elasticsearch --name vector_localstack_es \
	 -e SERVICES=elasticsearch:4571 localstack/localstack@sha256:f21f1fc770ee4bfd5012afdc902154c56b7fb18c14cf672de151b65569c8251e
  podman run -d --pod=vector-test-integration-elasticsearch \
	 --name vector_elasticsearch -e discovery.type=single-node -e ES_JAVA_OPTS="-Xms400m -Xmx400m" elasticsearch:6.6.2
  podman run -d --pod=vector-test-integration-elasticsearch \
	 --name vector_elasticsearch-tls -e discovery.type=single-node -e xpack.security.enabled=true \
	 -e xpack.security.http.ssl.enabled=true -e xpack.security.transport.ssl.enabled=true \
	 -e xpack.ssl.certificate=certs/localhost.crt -e xpack.ssl.key=certs/localhost.key \
	 -e ES_JAVA_OPTS="-Xms400m -Xmx400m" \
	 -v "$(pwd)"/tests/data:/usr/share/elasticsearch/config/certs:ro elasticsearch:6.6.2
}

start_docker () {
  docker network create vector-test-integration-elasticsearch
  docker run -d --network=vector-test-integration-elasticsearch -p 4571:4571 --name vector_localstack_es \
	 -e SERVICES=elasticsearch:4571 localstack/localstack@sha256:f21f1fc770ee4bfd5012afdc902154c56b7fb18c14cf672de151b65569c8251e
  docker run -d --network=vector-test-integration-elasticsearch -p 9200:9200 -p 9300:9300 \
	 --name vector_elasticsearch -e discovery.type=single-node -e ES_JAVA_OPTS="-Xms400m -Xmx400m" elasticsearch:6.6.2
  docker run -d --network=vector-test-integration-elasticsearch -p 9201:9200 -p 9301:9300 \
	 --name vector_elasticsearch-tls -e discovery.type=single-node -e xpack.security.enabled=true \
	 -e xpack.security.http.ssl.enabled=true -e xpack.security.transport.ssl.enabled=true \
	 -e xpack.ssl.certificate=certs/localhost.crt -e xpack.ssl.key=certs/localhost.key \
	 -e ES_JAVA_OPTS="-Xms400m -Xmx400m" \
	 -v "$(pwd)"/tests/data:/usr/share/elasticsearch/config/certs:ro elasticsearch:6.6.2
}

stop_podman () {
  podman rm --force vector_localstack_es vector_elasticsearch vector_elasticsearch-tls 2>/dev/null; true
  podman pod stop vector-test-integration-elasticsearch 2>/dev/null; true
  podman pod rm vector-test-integration-elasticsearch 2>/dev/null; true
}

stop_docker () {
  docker rm --force vector_localstack_es vector_elasticsearch vector_elasticsearch-tls 2>/dev/null; true
  docker network rm vector-test-integration-elasticsearch 2>/dev/null; true
}

echo "Running $ACTION action for Elasticsearch integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
