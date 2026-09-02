#!/usr/bin/env bash
set -e -o verbose

brew update
brew install coreutils cue-lang/tap/cue protobuf
