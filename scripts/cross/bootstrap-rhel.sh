#!/bin/sh

yum makecache

# needed by onig_sys
yum install -y \
      libclang1 \
      llvm
