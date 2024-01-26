#!/bin/sh
set -o errexit

yum install -y unzip centos-release-scl
yum install -y llvm-toolset-7

# needed to compile openssl
yum install -y perl-IPC-Cmd

