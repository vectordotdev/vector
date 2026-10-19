#!/usr/bin/env bash
set -euo pipefail
version=${TAG#v}
version=${version/-/\~}
if command -v apt-get >/dev/null; then
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -qq
  apt-get install -y ca-certificates gnupg
  gpg --batch --dearmor --output /usr/share/keyrings/vector.gpg /key.asc
  echo "deb [signed-by=/usr/share/keyrings/vector.gpg] $APT_URL $CHANNEL vector-0" > /etc/apt/sources.list.d/vector.list
  apt-get update
  apt-get install -y "vector=$version-1"
  test "$(dpkg-query -W -f='${Version}' vector)" = "$version-1"
else
  version=${version//-/.}
  rpm --import /key.asc
  cat > /etc/yum.repos.d/vector.repo <<EOF
[vector]
name=Vector
baseurl=${YUM_URL}${CHANNEL}/vector-0/\$basearch/
enabled=1
gpgcheck=1
repo_gpgcheck=1
gpgkey=file:///key.asc
EOF
  dnf install -y "vector-$version-1"
  test "$(rpm -q --qf '%{VERSION}-%{RELEASE}' vector)" = "$version-1"
fi
getent passwd vector
getent group vector
vector --version
test -f /usr/share/vector/examples/vector.yaml
