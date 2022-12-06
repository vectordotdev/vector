#! /usr/bin/env bash

if [[ "$(wasm-pack --version)" != "wasm-pack 0.10.3" ]] ; then
    echo "wasm-pack version 0.10.3 is not installed"
    echo "running cargo install --version 0.10.3 wasm-pack"
    cargo install --version 0.10.3 wasm-pack
else
    echo "wasm-pack version 0.10.3 is installed already"
fi
