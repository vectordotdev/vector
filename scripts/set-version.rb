#!/usr/bin/env ruby

# set-version.rb
#
# SUMMARY
#
#   Sets the Vector version to the passed version. This is used to ensure
#   the proper version is reflected in Vector's resulting binary.

require_relative "setup"

# The version is expected to be set
VERSION = ENV.fetch("VERSION")

say("Setting Vector version to #{VERSION}")

# Bump the version in the Cargo.toml file
cargo_content = File.read("#{ROOT_DIR}/Cargo.toml")

new_cargo_content =
  cargo_content.sub(
    /name = "vector"\nversion = "([a-z0-9.-]*)"\n/,
    "name = \"vector\"\nversion = \"#{VERSION}\"\n"
  )

File.write("#{ROOT_DIR}/Cargo.toml", new_cargo_content)

success("Cargo.toml updated")
execute!("cargo check")
success("Cargo.lock updated")