#!/usr/bin/env bash
set -euo pipefail
version=${TAG#v}
version=${version/-/\~}
VECTOR_DIST_CHANNEL="$CHANNEL" TESTING_APT_URL="$APT_URL" TESTING_YUM_URL="$YUM_URL" bash /setup.sh
if command -v apt-get >/dev/null; then
  export DEBIAN_FRONTEND=noninteractive
  apt-get install -y "vector=$version-1"
  test "$(dpkg-query -W -f='${Version}' vector)" = "$version-1"
else
  version=${version//-/.}
  dnf install -y "vector-$version-1"
  test "$(rpm -q --qf '%{VERSION}-%{RELEASE}' vector)" = "$version-1"
fi
getent passwd vector
getent group vector
vector --version
test -f /usr/share/vector/examples/vector.yaml
