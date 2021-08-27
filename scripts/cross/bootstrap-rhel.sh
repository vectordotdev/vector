#!/bin/sh

yum makecache

# Needed for llvm 9 required by onig_sys
yum install -y https://dl.fedoraproject.org/pub/epel/epel-release-latest-7.noarch.rpm

# needed by onig_sys
yum install -y \
      clang \
      llvm
