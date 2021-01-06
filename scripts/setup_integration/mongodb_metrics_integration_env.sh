#!/usr/bin/env bash
set -uo pipefail

# mongodb_metrics_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector MongoDB metrics Integration test environment

set -x

while getopts a:t:e: flag
do
    case "${flag}" in
        a) action=${OPTARG};;
        t) tool=${OPTARG};;
        e) enclosure=${OPTARG};;

    esac
done

ACTION="${action:-"stop"}"
CONTAINER_TOOL="${tool:-"podman"}"
CONTAINER_ENCLOSURE="${enclosure:-"pod"}"

#
# Functions
#

# https://docs.mongodb.com/manual/tutorial/deploy-shard-cluster/

start_podman () {
	"${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" create --replace --name vector-test-integration-mongodb_metrics -p 27017:27017 -p 27018:27018 -p 27019:27019
	"${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-mongodb_metrics --name vector_mongodb_metrics1 mongo:4.2.10 mongod --configsvr --replSet vector
	sleep 1
	"${CONTAINER_TOOL}" exec vector_mongodb_metrics1 mongo --port 27019 --eval 'rs.initiate({_id:"vector",configsvr:true,members:[{_id:0,host:"127.0.0.1:27019"}]})'
	"${CONTAINER_TOOL}" exec -d vector_mongodb_metrics1 mongos --port 27018 --configdb vector/127.0.0.1:27019
	"${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-mongodb_metrics --name vector_mongodb_metrics2 mongo:4.2.10 mongod
}

start_docker () {
	"${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" create vector-test-integration-mongodb_metrics
	"${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-mongodb_metrics -p 27018:27018 -p 27019:27019 --name vector_mongodb_metrics1 mongo:4.2.10 mongod --configsvr --replSet vector
	sleep 1
	"${CONTAINER_TOOL}" exec vector_mongodb_metrics1 mongo --port 27019 --eval 'rs.initiate({_id:"vector",configsvr:true,members:[{_id:0,host:"127.0.0.1:27019"}]})'
	"${CONTAINER_TOOL}" exec -d vector_mongodb_metrics1 mongos --port 27018 --configdb vector/127.0.0.1:27019
	"${CONTAINER_TOOL}" run -d --"${CONTAINER_ENCLOSURE}"=vector-test-integration-mongodb_metrics -p 27017:27017 --name vector_mongodb_metrics2 mongo:4.2.10 mongod
}

stop_podman () {
	"${CONTAINER_TOOL}" rm --force vector_mongodb_metrics1 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" stop vector-test-integration-mongodb_metrics 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" rm --force vector-test-integration-mongodb_metrics 2>/dev/null; true
}

stop_docker () {
  "${CONTAINER_TOOL}" rm --force vector_mongodb_metrics1 2>/dev/null; true
  "${CONTAINER_TOOL}" "${CONTAINER_ENCLOSURE}" rm vector-test-integration-mongodb_metrics 2>/dev/null; true
}

echo "Running $ACTION action for MongoDB metrics integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
