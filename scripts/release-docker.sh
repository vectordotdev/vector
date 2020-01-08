#!/bin/bash

docker login -u "$DOCKER_USERNAME" -p "$DOCKER_PASSWORD"
PUSH=1 ./scripts/build-docker.sh
