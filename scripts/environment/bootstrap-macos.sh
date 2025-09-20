#!/usr/bin/env bash
set -e -o verbose

brew update
brew install ruby@3 coreutils cue-lang/tap/cue protobuf

# Required for building aws-lc-rs
# https://github.com/aws/aws-lc/issues/2129
brew install go

gem install bundler

echo "export PATH=\"/usr/local/opt/ruby/bin:\$PATH\"" >> "$HOME/.bash_profile"

if [ -n "${CI-}" ] ; then
  echo "/usr/local/opt/ruby/bin" >> "$GITHUB_PATH"
fi
