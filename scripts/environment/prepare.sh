#! /usr/bin/env bash
set -e

rustup default $(cat rust-toolchain)
cd /vector/scripts
bundle update --bundler
bundle install
cd /vector/website
yarn
cd /vector