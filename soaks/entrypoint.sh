#! /bin/sh

setarch $(uname --machine) --addr-no-randomize /usr/bin/vector $@
