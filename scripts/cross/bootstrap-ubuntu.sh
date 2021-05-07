#!/bin/sh

apt update

# needed by onig_sys
apt install -y \
      libclang1 \
      llvm
