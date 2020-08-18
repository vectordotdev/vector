#! /usr/bin/env bash
set -e -o verbose

brew install ruby coreutils

echo "export PATH=\"/usr/local/opt/ruby/bin:\$PATH\"" >> "$HOME/.bash_profile"

if [ -n "${CI-}" ] ; then
  echo "::add-path::/usr/local/opt/ruby/bin"
fi
