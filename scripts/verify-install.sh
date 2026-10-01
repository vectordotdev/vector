#!/usr/bin/env bash
set -euo pipefail

# verify-install.sh <package> [<previous-package>]
#
# SUMMARY
#
#   Verifies vector packages have been built and installed correctly.
#
#   For a .deb, passing a previously released package additionally exercises the
#   real upgrade path (old package -> new package) with plain `dpkg -i`: no
#   --force-conf* flags and no terminal, as in an unattended upgrade.

package="${1:-}"
if [[ $# -ne 1 || ! -f "$package" ]]; then
  echo "Expected exactly one package file" >&2
  exit 1
fi

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

# On Debian, exercise the real-world migration scenarios before the first
# install of the package under test. Older packages did not own
# /etc/vector/vector.yaml (admins created it by hand) and shipped the sample
# configs as conffiles under /etc/vector/examples, so:
#   - the admin's file must survive becoming a conffile without any dpkg prompt
#   - obsolete unmodified example conffiles must be removed, and modified ones
#     kept as *.dpkg-bak
# With a previous package the upgrade is real; without one the admin file is
# still created so the preinst/postinst move-aside path is covered.
admin_config_marker="pre-existing-admin-config: true"
case "$package" in
  *.deb)
    if [ -n "$previous_package" ]; then
      dpkg -i "$previous_package"
      test -f /etc/vector/examples/stdio.yaml || (echo "previous package did not install /etc/vector/examples/stdio.yaml" && exit 1)
      echo "# locally modified" >> /etc/vector/examples/wrapped_json.yaml
    fi
    mkdir -p /etc/vector
    echo "$admin_config_marker" > /etc/vector/vector.yaml
    ;;
esac

install_package "$package"

getent passwd vector || (echo "vector user missing" && exit 1)
getent group vector || (echo "vector group  missing" && exit 1)
vector --version || (echo "vector --version failed" && exit 1)
test -f /etc/default/vector || (echo "/etc/default/vector doesn't exist" && exit 1)

case "$package" in
  *.deb)
    test -f /etc/vector/vector.yaml || (echo "/etc/vector/vector.yaml doesn't exist" && exit 1)
    grep -q "$admin_config_marker" /etc/vector/vector.yaml || (echo "pre-existing, not-yet-tracked /etc/vector/vector.yaml was not preserved when it became a conffile" && exit 1)
    # Sample configs are documentation, not admin-managed config, so they
    # must not live under /etc (cargo-deb treats every file it installs
    # under /etc as a conffile).
    test ! -e /etc/vector/examples/stdio.yaml || (echo "obsolete unmodified /etc/vector/examples/stdio.yaml was not removed on upgrade" && exit 1)
    if [ -n "$previous_package" ]; then
      test -f /etc/vector/examples/wrapped_json.yaml.dpkg-bak || (echo "locally modified example conffile was not preserved as .dpkg-bak" && exit 1)
    else
      test ! -e /etc/vector/examples || (echo "/etc/vector/examples should not be installed on Debian; sample configs are not admin-managed config" && exit 1)
    fi
    test ! -e /etc/vector/.vector.yaml.pre-conffile || (echo "preinst backup of vector.yaml was left behind" && exit 1)
    test -f /usr/share/vector/examples/stdio.yaml || (echo "/usr/share/vector/examples/stdio.yaml doesn't exist" && exit 1)
    ;;
  *.rpm)
    # RPM behavior is unchanged: no default config is installed, only a
    # %ghost placeholder so upgrades from older RPMs preserve any existing
    # on-disk file, plus a documentation copy under /usr/share.
    test ! -e /etc/vector/vector.yaml || (echo "/etc/vector/vector.yaml should not be installed by default on RPM" && exit 1)
    test -f /usr/share/vector/examples/vector.yaml || (echo "/usr/share/vector/examples/vector.yaml doesn't exist" && exit 1)
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
