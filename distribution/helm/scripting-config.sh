#!/bin/bash

# This file holds the shared configuration for various helm automation scripts.

# The order in which the dependency updates are issued.
DEPENDENCY_UPDATE_ORDER=(
  # Lowest level.
  vector-shared

  # Intermediate level.
  vector-agent

  # Highest level.
  vector
)

# The list of charts to release to our chart repo.
# We only need to release the application charts, library charts don't need to
# be released.
CHARTS_TO_PUBLISH=(
  vector-agent
  vector
)
