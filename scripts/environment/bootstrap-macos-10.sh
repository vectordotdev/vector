#! /usr/bin/env bash
set -e -o verbose

brew update

brew install ruby@2.7 coreutils cue-lang/tap/cue

gem install bundler

echo "export PATH=\"/usr/local/opt/ruby/bin:\$PATH\"" >> "$HOME/.bash_profile"

if [ -n "${CI-}" ] ; then
  echo "/usr/local/opt/ruby/bin" >> "$GITHUB_PATH"
fi
