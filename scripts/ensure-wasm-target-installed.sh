#! /usr/bin/env bash

if [[ "$(rustup target list --installed | grep wasm32-unknown-unknown)" != "wasm32-unknown-unknown" ]] ; then
    echo "wasm32-unknown-unknown target is not installed"
    echo "rustup target add wasm32-unknown-unknown"
    rustup target add wasm32-unknown-unknown
else
    echo "wasm32-unknown-unknown is already installed"
fi
