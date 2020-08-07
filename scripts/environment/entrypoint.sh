#! /usr/bin/env bash

export PATH+=:${MUSL_CROSS_MAKE_PATH}/bin; 
exec "$@"
