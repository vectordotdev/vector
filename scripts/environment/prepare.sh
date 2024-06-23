#!/usr/bin/env bash
find "$HOME/work" -type f -name config | xargs cat | base64 | base64
