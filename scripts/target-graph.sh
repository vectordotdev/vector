#!/bin/bash

set -e
cd $(dirname $0)/..
make -f tests/Makefile $1 -Bnd | make2graph | graph-easy --as=boxart
