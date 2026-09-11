#!/usr/bin/env bash
set -euo pipefail

# verify-install.sh <package>
#
# SUMMARY
#
#   Verifies vector packages have been built and installed correctly

package="${1:?must pass package as argument}"

# Resolve shared-library deps that plain `dpkg -i` / `rpm -i` do not install
# (glibc ODBC builds need libodbc / unixODBC for `vector --version`).
ensure_odbc_runtime () {
  case "$1" in
    *.deb)
      export DEBIAN_FRONTEND=noninteractive
      apt-get update -qq
      apt-get install -y libodbc2 || apt-get install -y libodbc1
      ;;
    *.rpm)
      if command -v dnf >/dev/null 2>&1; then
        dnf install -y unixODBC
      elif command -v yum >/dev/null 2>&1; then
        yum install -y unixODBC
      else
        echo "No dnf/yum available to install unixODBC" >&2
        exit 1
      fi
      ;;
  esac
}

install_package () {
  case "$1" in
    *.deb)
        dpkg -i "$1"
      ;;
    *.rpm)
        rpm -i --replacepkgs "$1"
      ;;
  esac
}

ensure_odbc_runtime "$package"
install_package "$package"

getent passwd vector || (echo "vector user missing" && exit 1)
getent group vector || (echo "vector group  missing" && exit 1)
vector --version || (echo "vector --version failed" && exit 1)
test -f /etc/default/vector || (echo "/etc/default/vector doesn't exist" && exit 1)
test ! -e /etc/vector/vector.yaml || (echo "/etc/vector/vector.yaml should not be installed by default" && exit 1)
test -f /usr/share/vector/examples/vector.yaml || (echo "/usr/share/vector/examples/vector.yaml doesn't exist" && exit 1)

mkdir -p /etc/vector
echo "FOO=bar" > /etc/default/vector
echo "foo: bar" > /etc/vector/vector.yaml

install_package "$package"

getent passwd vector || (echo "vector user missing" && exit 1)
getent group vector || (echo "vector group  missing" && exit 1)
vector --version || (echo "vector --version failed" && exit 1)
grep -q "FOO=bar" "/etc/default/vector" || (echo "/etc/default/vector has incorrect contents" && exit 1)
grep -q "foo: bar" "/etc/vector/vector.yaml" || (echo "user-provided /etc/vector/vector.yaml was not preserved on reinstall" && exit 1)

dd-pkg lint "$package"
