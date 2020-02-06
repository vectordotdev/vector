#!/bin/bash

set -e
make -f Makefile.test $1 -Bnd | make2graph | graph-easy --as=boxart
