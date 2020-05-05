#!/bin/bash

set -e
make -f tests/Makefile "$1" -Bnd | make2graph | graph-easy --as=boxart
