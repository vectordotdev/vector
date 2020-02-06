#!/usr/bin/env bash

# package-deb.sh
#
# SUMMARY
#
#   Packages a .deb file to be distributed in the APT package manager.

set -eu

project_root=$(pwd)
archive_name="vector-$TARGET.tar.gz"
archive_path="target/artifacts/$archive_name"
absolute_archive_path="$project_root/$archive_path"
package_version="$($project_root/scripts/version.sh)"
echo "Packaging .deb for $archive_name"

# Unarchive the tar since cargo deb wants direct access to the files.
td=$(mktemp -d)
pushd $td
tar -xvf $absolute_archive_path
mkdir -p $project_root/target/$TARGET/release
mv vector-$TARGET/bin/vector $project_root/target/$TARGET/release
popd
rm -rf $td

# Create short plain-text extended description for the package
cmark-gfm $project_root/README.md --to commonmark | # expand link aliases
  sed '/^## /Q' | # select text before first header
  cmark-gfm --to plaintext | # convert to plain text
  fmt -uw 80 > $project_root/target/debian-extended-description.txt

# Create the license file for binary distributions (LICENSE + NOTICE)
cat LICENSE NOTICE > $project_root/target/debian-license.txt

# Build the deb
#
#   --target
#     tells the builder everything it needs to know aboout where
#     the deb can run, including the architecture
#
#   --no-build
#     because this stop should follow a build
cargo deb --target $TARGET --deb-version $package_version --no-build

# Rename the resulting .deb file to use - instead of _ since this
# is consistent with our package naming scheme.
rename -v 's/vector_([^_]*)_(.*)\.deb/vector-$2\.deb/' target/$TARGET/debian/*.deb

# Move the deb into the artifacts dir
mv -v $(find target/$TARGET/debian/ -name *.deb) target/artifacts
