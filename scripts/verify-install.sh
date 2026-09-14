#!/usr/bin/env bash
set -euo pipefail

# verify-install.sh <package>
#
# SUMMARY
#
#   Verifies vector packages have been built and installed correctly

package="${1:?must pass package as argument}"

# Statically inspects a built .deb (no root, no installation required) to
# confirm the control metadata and file layout match the intended fix:
#   - /etc/vector/vector.yaml and /etc/default/vector are declared conffiles
#     in DEBIAN/conffiles, so dpkg preserves local edits across upgrades.
#   - the bundled example configs are installed under /usr/share/vector/examples/
#     and are NOT declared (or installable) as conffiles under /etc.
verify_deb_static () {
  local pkg="$1"
  local ctrl_dir
  ctrl_dir="$(mktemp -d)"
  dpkg-deb -e "$pkg" "$ctrl_dir"

  if [ ! -f "$ctrl_dir/conffiles" ]; then
    echo "package is missing a DEBIAN/conffiles control file"
    rm -rf "$ctrl_dir"
    exit 1
  fi

  if ! grep -qx "/etc/vector/vector.yaml" "$ctrl_dir/conffiles"; then
    echo "/etc/vector/vector.yaml is not declared in DEBIAN/conffiles"
    rm -rf "$ctrl_dir"
    exit 1
  fi

  if ! grep -qx "/etc/default/vector" "$ctrl_dir/conffiles"; then
    echo "/etc/default/vector is not declared in DEBIAN/conffiles"
    rm -rf "$ctrl_dir"
    exit 1
  fi

  if grep -q "^/etc/vector/examples/" "$ctrl_dir/conffiles"; then
    echo "example configs must not be declared as conffiles"
    rm -rf "$ctrl_dir"
    exit 1
  fi

  rm -rf "$ctrl_dir"

  local file_list
  file_list="$(dpkg-deb -c "$pkg")"

  if ! echo "$file_list" | grep -qE '\./etc/vector/vector\.yaml$'; then
    echo "package does not install /etc/vector/vector.yaml"
    exit 1
  fi

  if ! echo "$file_list" | grep -qE '\./usr/share/vector/examples/stdio\.yaml$'; then
    echo "package does not install /usr/share/vector/examples/stdio.yaml"
    exit 1
  fi

  if echo "$file_list" | grep -qE '\./etc/vector/examples/'; then
    echo "example configs must not be installed under /etc/vector/examples/"
    exit 1
  fi

  # Check the shipped default's content directly from the archive (not after
  # a live install) so this holds regardless of whether the test environment
  # already has a pre-existing /etc/vector/vector.yaml on disk.
  if dpkg-deb --fsys-tarfile "$pkg" | tar -xO ./etc/vector/vector.yaml | grep -q "dummy_logs"; then
    echo "/etc/vector/vector.yaml must not ship the demo_logs pipeline as a fresh-install default"
    exit 1
  fi

  echo "verify-install.sh: .deb static checks passed (conffiles + file paths)"
}

case "$package" in
  *.deb)
    verify_deb_static "$package"
    ;;
esac

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

# On Debian, simulate the real-world migration scenario before the first
# install: an admin created /etc/vector/vector.yaml by hand on a pre-fix
# version, where the package did not own that path at all. dpkg has no
# checksum on record for a path it never tracked, so the preinst/postinst
# backup-and-restore migration (not dpkg's ordinary conffile handling) is
# what has to preserve this file across the transition to this version.
case "$package" in
  *.deb)
    mkdir -p /etc/vector
    echo "pre-existing-admin-config: true" > /etc/vector/vector.yaml
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
    grep -q "pre-existing-admin-config: true" /etc/vector/vector.yaml || (echo "pre-existing, not-yet-tracked /etc/vector/vector.yaml was not preserved when it became a conffile" && exit 1)
    # Sample configs are documentation, not admin-managed config, so they
    # must not live under /etc (cargo-deb treats every file it installs
    # under /etc as a conffile).
    test ! -e /etc/vector/examples || (echo "/etc/vector/examples should not be installed on Debian; sample configs are not admin-managed config" && exit 1)
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
