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
    environment_file=/etc/default/vector
    ;;
  *.rpm)
    environment_file=/etc/sysconfig/vector
    # Emulate an upgrade from a package that used the Debian-style path.
    mkdir -p /etc/default
    echo "FOO=bar" > /etc/default/vector
    ;;
esac

install_package "$package"

getent passwd vector || (echo "vector user missing" && exit 1)
getent group vector || (echo "vector group  missing" && exit 1)
vector --version || (echo "vector --version failed" && exit 1)
test -f "$environment_file" || (echo "$environment_file doesn't exist" && exit 1)
test ! -e /etc/vector/vector.yaml || (echo "/etc/vector/vector.yaml should not be installed by default" && exit 1)
test -f /usr/share/vector/examples/vector.yaml || (echo "/usr/share/vector/examples/vector.yaml doesn't exist" && exit 1)

mkdir -p /etc/vector
if [[ "$package" == *.deb ]]; then
  echo "FOO=bar" > "$environment_file"
fi
grep -q "FOO=bar" "$environment_file" || (echo "$environment_file did not preserve existing contents" && exit 1)
echo "foo: bar" > /etc/vector/vector.yaml

install_package "$package"

getent passwd vector || (echo "vector user missing" && exit 1)
getent group vector || (echo "vector group  missing" && exit 1)
vector --version || (echo "vector --version failed" && exit 1)
grep -q "FOO=bar" "$environment_file" || (echo "$environment_file has incorrect contents" && exit 1)
grep -q "foo: bar" "/etc/vector/vector.yaml" || (echo "user-provided /etc/vector/vector.yaml was not preserved on reinstall" && exit 1)

dd-pkg lint "$package"
