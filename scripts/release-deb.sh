#!/usr/bin/env bash

# release-deb.sh
#
# SUMMARY
#
#   Releases the .deb package in target/artifacts

set -eu

echo "Dsitributing .deb package via Package Cloud"

# Debian
package_cloud push timberio/packages/debian/jessie target/artifacts/*.deb
package_cloud push timberio/packages/debian/stretch target/artifacts/*.deb
package_cloud push timberio/packages/debian/buster target/artifacts/*.deb

# Ubuntu
package_cloud push timberio/packages/ubuntu/xenial target/artifacts/*.deb
package_cloud push timberio/packages/ubuntu/zesty target/artifacts/*.deb
package_cloud push timberio/packages/ubuntu/bionic target/artifacts/*.deb
package_cloud push timberio/packages/ubuntu/disco target/artifacts/*.deb
