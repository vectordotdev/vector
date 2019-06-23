#!/usr/bin/env bash

set -eu

echo "Building version $VERSION for target $TARGET"

artifacts_dir="target/artifacts"
target_dir="target/$TARGET"
archive_dir_name="vector-$VERSION"
archive_dir="$target_dir/$archive_dir_name"

cargo build --target $TARGET --release

# Build the archive directory
rm -rf $archive_dir
mkdir -p $archive_dir

# Copy root level files
cp -a README.md $archive_dir
cp -a LICENSE $archive_dir

# Copy the vector binary to /bin
mkdir -p $archive_dir/bin
cp -a $target_dir/release/vector $archive_dir/bin

# Copy the entire config dir to /config
cp -rv config $archive_dir/config

# Copy /etc usefule files
mkdir -p $archive_dir/etc/systemd
cp -a distribution/systemd/vector.service $archive_dir/etc/systemd
mkdir -p $archive_dir/etc/init.d
cp -a distribution/init.d/vector $archive_dir/etc/init.d

# Build the release tar
cd $target_dir
tar -czvf vector-$VERSION-$TARGET.tar.gz ./$archive_dir_name
cd ../..

# Move to the artifacts dir
mkdir -p $artifacts_dir
mv -v $target_dir/vector-$VERSION-$TARGET.tar.gz $artifacts_dir
echo "Moved $target_dir/vector-$VERSION-$TARGET.tar.gz to $artifacts_dir"

# Cleanup
rm -rf $archive_dir