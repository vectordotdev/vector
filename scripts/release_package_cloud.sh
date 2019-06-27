#!/usr/bin/env bash

set -eu

# Debian
package_cloud push timberio/packages/debian/jessie target/artifacts/*.deb
package_cloud push timberio/packages/debian/stretch target/artifacts/*.deb

# Ubuntu
package_cloud push timberio/packages/ubuntu/xenial target/artifacts/*.deb
package_cloud push timberio/packages/ubuntu/zesty target/artifacts/*.deb
package_cloud push timberio/packages/ubuntu/stretch target/artifacts/*.deb
package_cloud push timberio/packages/ubuntu/disco target/artifacts/*.deb

# Enterprise Linux (CentOS, RedHat, Amazon Linux)
package_cloud push timberio/packages/el/6 target/artifacts/*.rpm
package_cloud push timberio/packages/el/7 target/artifacts/*.rpm