#!/bin/sh

mkdir -p /tmp
nixpkgs=/tmp/nixpkgs
git clone --depth=1 https://github.com/nixos/nixpkgs $nixpkgs
erb < distribution/nix/default.nix.erb > $nixpkgs/pkgs/tools/misc/vector/default.nix

nix-env -f $nixpkgs -iA vector
