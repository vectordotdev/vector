#!/usr/bin/env bash

# release-rpm.sh
#
# SUMMARY
#
#   Releases the .rpm package in target/artifacts

set -eu

echo "Dsitributing .rpm package via Package Cloud"

# Enterprise Linux (CentOS, RedHat, Amazon Linux)
package_cloud push timberio/packages/el/6 target/artifacts/*.rpm
package_cloud push timberio/packages/el/7 target/artifacts/*.rpm