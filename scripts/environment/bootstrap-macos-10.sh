#! /usr/bin/env bash
set -e -o verbose

# https://github.com/Homebrew/homebrew-cask/issues/150323
unset HOMEBREW_NO_INSTALL_FROM_API

brew update

brew install ruby@2.7 coreutils cue-lang/tap/cue protobuf

gem install bundler

echo "export PATH=\"/usr/local/opt/ruby/bin:\$PATH\"" >> "$HOME/.bash_profile"

if [ -n "${CI-}" ] ; then
  echo "/usr/local/opt/ruby/bin" >> "$GITHUB_PATH"
fi
