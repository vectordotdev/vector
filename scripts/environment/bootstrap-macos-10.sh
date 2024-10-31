#! /usr/bin/env bash
set -e -o verbose

# https://github.com/Homebrew/homebrew-cask/issues/150323
unset HOMEBREW_NO_INSTALL_FROM_API

brew update

# `brew install` attempts to upgrade python as a dependency but fails
# https://github.com/actions/setup-python/issues/577
brew list -1 | grep python | while read -r formula; do brew unlink "$formula"; brew link --overwrite "$formula"; done

brew install ruby@3 coreutils cue-lang/tap/cue protobuf

# rustup-init (renamed to rustup) is already installed in GHA, but seems to be lacking the rustup binary
# Reinstalling seems to fix it
# TODO(jszwedko): It's possible GHA just needs to update its images and this won't be needed in the
# future
brew reinstall rustup

gem install bundler

echo "export PATH=\"/usr/local/opt/ruby/bin:\$PATH\"" >> "$HOME/.bash_profile"

if [ -n "${CI-}" ] ; then
  echo "/usr/local/opt/ruby/bin" >> "$GITHUB_PATH"
fi
