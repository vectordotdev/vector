#!/usr/bin/env bash

git diff --quiet "${COMMIT_REF}" "${CACHED_COMMIT_REF}" -- docs/
