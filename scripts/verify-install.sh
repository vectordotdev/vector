#!/usr/bin/env bash
set -euo pipefail

# verify-install.sh <package>
#
# SUMMARY
#
#   Verifies vector packages have been built and installed correctly

package="${1:?must pass package as argument}"

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

case "$package" in
  *.deb)
    # Simulate a pre-existing /etc/vector/vector.yaml that predates this file
    # being a dpkg conffile (e.g. a user-created file from before this fix, or
    # an earlier Vector version that didn't ship one at all). Installing over
    # it must not prompt (dpkg has no tty here, so a prompt would hang/fail
    # this script) and must leave the user's content untouched.
    mkdir -p /etc/vector
    echo "unmanaged: pre-existing-config" > /etc/vector/vector.yaml
    ;;
esac

install_package "$package"

case "$package" in
  *.deb)
    grep -q "unmanaged: pre-existing-config" /etc/vector/vector.yaml || \
      (echo "pre-existing, dpkg-untracked /etc/vector/vector.yaml was not preserved on install" && exit 1)
    ;;
esac

getent passwd vector || (echo "vector user missing" && exit 1)
getent group vector || (echo "vector group  missing" && exit 1)
vector --version || (echo "vector --version failed" && exit 1)
test -f /etc/default/vector || (echo "/etc/default/vector doesn't exist" && exit 1)
test -f /usr/share/vector/examples/vector.yaml || (echo "/usr/share/vector/examples/vector.yaml doesn't exist" && exit 1)
case "$package" in
  *.deb)
    test -f /etc/vector/vector.yaml || (echo "/etc/vector/vector.yaml should be installed by default" && exit 1)
    test ! -d /etc/vector/examples || (echo "examples should not be installed under /etc/vector/examples" && exit 1)
    test -f /usr/share/doc/vector/examples/stdio.yaml || (echo "/usr/share/doc/vector/examples/ examples missing" && exit 1)
    ;;
  *.rpm)
    test ! -e /etc/vector/vector.yaml || (echo "/etc/vector/vector.yaml should not be installed by default" && exit 1)
    ;;
esac

echo "FOO=bar" > /etc/default/vector
echo "foo: bar" > /etc/vector/vector.yaml

install_package "$package"

getent passwd vector || (echo "vector user missing" && exit 1)
getent group vector || (echo "vector group  missing" && exit 1)
vector --version || (echo "vector --version failed" && exit 1)
grep -q "FOO=bar" "/etc/default/vector" || (echo "/etc/default/vector has incorrect contents" && exit 1)
grep -q "foo: bar" "/etc/vector/vector.yaml" || (echo "user-provided /etc/vector/vector.yaml was not preserved on reinstall" && exit 1)

dd-pkg lint "$package"
