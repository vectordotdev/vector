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

install_package "$package"

getent passwd vector || (echo "vector user missing" && exit 1)
getent group vector || (echo "vector group  missing" && exit 1)
vector --version || (echo "vector --version failed" && exit 1)
test -f /etc/default/vector || (echo "/etc/default/vector doesn't exist" && exit 1)
test ! -e /etc/vector/vector.yaml || (echo "/etc/vector/vector.yaml should not be installed by default" && exit 1)
test -f /usr/share/vector/examples/vector.yaml || (echo "/usr/share/vector/examples/vector.yaml doesn't exist" && exit 1)
test -f /usr/share/bash-completion/completions/vector || (echo "bash completion missing" && exit 1)
test -f /usr/share/fish/vendor_completions.d/vector.fish || (echo "fish completion missing" && exit 1)

case "$package" in
  *.deb)
    test -f /usr/share/zsh/vendor-completions/_vector || (echo "zsh completion missing" && exit 1)
    ;;
  *.rpm)
    test -f /usr/share/zsh/site-functions/_vector || (echo "zsh completion missing" && exit 1)
    ;;
esac

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
