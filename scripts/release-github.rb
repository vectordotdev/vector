#!/usr/bin/env ruby

# release-github.sh
#
# SUMMARY
#
#   Uploads target/artifacts to Github releases

#
# Constants
#

VERSION = ENV.fetch("VECTOR_VERSION")
SHA1 = ENV.fetch("SHA1")

#
# Release
#

flags = [
  "--assets '#{ROOT_DIR}/target/artifacts/*'",
  "--notes '[View release notes](#{HOST}/releases/#{VERSION})'",
  "--name v#{VERSION}"
]

command = "grease --debug create-release timberio/vector v#{VERSION} #{SHA1} #{flags.join(" ")}"
puts `#{command}`
