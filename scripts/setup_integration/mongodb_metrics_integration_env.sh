#!/usr/bin/env bash
set -o pipefail

# mongodb_metrics_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector MongoDB metrics Integration test environment

if [ $# -ne 1 ]
then
    echo "Usage: $0 {stop|start}" 1>&2; exit 1;
    exit 1
fi
ACTION=$1

#
# Functions
#

# https://docs.mongodb.com/manual/tutorial/deploy-shard-cluster/

start_podman () {
  podman pod create --replace --name vector-test-integration-mongodb_metrics -p 27017:27017 -p 27018:27018 -p 27019:27019
  podman run -d --pod=vector-test-integration-mongodb_metrics --name vector_mongodb_metrics1 mongo:4.2.10 mongod --configsvr --replSet vector
	sleep 1
  podman exec vector_mongodb_metrics1 mongo --port 27019 --eval 'rs.initiate({_id:"vector",configsvr:true,members:[{_id:0,host:"127.0.0.1:27019"}]})'
  podman exec -d vector_mongodb_metrics1 mongos --port 27018 --configdb vector/127.0.0.1:27019
  podman run -d --pod=vector-test-integration-mongodb_metrics --name vector_mongodb_metrics2 mongo:4.2.10 mongod
}

start_docker () {
  docker network create vector-test-integration-mongodb_metrics
  docker run -d --network=vector-test-integration-mongodb_metrics -p 27018:27018 -p 27019:27019 --name vector_mongodb_metrics1 mongo:4.2.10 mongod --configsvr --replSet vector
	sleep 1
  docker exec vector_mongodb_metrics1 mongo --port 27019 --eval 'rs.initiate({_id:"vector",configsvr:true,members:[{_id:0,host:"127.0.0.1:27019"}]})'
  docker exec -d vector_mongodb_metrics1 mongos --port 27018 --configdb vector/127.0.0.1:27019
  docker run -d --network=vector-test-integration-mongodb_metrics -p 27017:27017 --name vector_mongodb_metrics2 mongo:4.2.10 mongod
}

stop_podman () {
  podman rm --force vector_mongodb_metrics1 vector_mongodb_metrics2 2>/dev/null; true
  podman pod stop vector-test-integration-mongodb_metrics 2>/dev/null; true
  podman pod rm --force vector-test-integration-mongodb_metrics 2>/dev/null; true
}

stop_docker () {
  docker rm --force vector_mongodb_metrics1 vector_mongodb_metrics2 2>/dev/null; true
  docker network rm vector-test-integration-mongodb_metrics 2>/dev/null; true
}

echo "Running $ACTION action for MongoDB metrics integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
