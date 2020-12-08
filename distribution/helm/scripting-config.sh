#!/bin/bash

# This file holds the shared configuration for various helm automation scripts.

# The order in which the dependency updates are issued.
# shellcheck disable=SC2034
DEPENDENCY_UPDATE_ORDER=(
  # Lowest level.
  vector-shared

  # Intermediate level.
  vector-agent
  vector-aggregator

  # Highest level.
  vector
)

# The list of charts to release to our chart repo.
# We only need to release the application charts, library charts don't need to
# be released.
# shellcheck disable=SC2034
CHARTS_TO_PUBLISH=(
  vector-agent
  vector-aggregator
  vector
)
