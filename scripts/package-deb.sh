#!/usr/bin/env bash

set -eu

project_root=$(pwd)

for archive in target/artifacts/*.tar.gz; do
  [ -f "$archive" ] || break

  # Skip archives that are not generic linux since they are not relevant
  # to Debian. For example, we do not want to create a deb file for
  # x86_64-apple-darwin targets.
  [[ "$archive" == *"linux"* ]] || continue

  full_archive="$project_root/$archive"

  # The target is the last part of the file name.
  target=$(echo $archive | sed "s|target/artifacts/vector-$VERSION-||g" | sed "s|.tar.gz||g")

  echo "Packaging .deb for version $VERSION on target $target"

  # Unarchive the tar since cargo deb wants direct access to the files.
  td=$(mktemp -d)
  pushd $td
  tar -xvf $full_archive
  mkdir -p $project_root/target/$target/release
  mv vector-$VERSION/bin/vector $project_root/target/$target/release
  popd
  rm -rf $td

  # Build the deb
  # --target tells the builder everything it needs to know aboout where
  # the deb can run, including the architecture
  # --no-build because this stop should follow a build
  cargo deb --target $target --deb-version $VERSION --no-build

  # Rename the resulting .deb file to use - instead of _ since this
  # is consistent with our package naming scheme.
  rename -v 's/vector_([^_]*)_(.*)\.deb/vector-$1-$2\.deb/' target/$target/debian/*.deb

  # Move the deb into the artifactws dir
  mv -v $(find target/$target/debian/ -name *.deb) target/artifacts
done