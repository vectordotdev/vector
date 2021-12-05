#!/usr/bin/env bash
set -euo pipefail

# verify-install.sh
#
# SUMMARY
#
#   Verifies vector packages have been installed correctly

getent passwd vector || (echo "vector user missing" && exit 1)
getent group vector || (echo "vector group  missing" && exit 1)
vector --version || (echo "vector --version failed" && exit 1)
