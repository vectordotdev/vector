#!/usr/bin/env bash
set -e -o verbose

brew update
brew install ruby@3 coreutils cue-lang/tap/cue protobuf

gem install bundler

echo "export PATH=\"/usr/local/opt/ruby/bin:\$PATH\"" >> "$HOME/.bash_profile"

if [ -n "${CI-}" ] ; then
  echo "/usr/local/opt/ruby/bin" >> "$GITHUB_PATH"
fi
