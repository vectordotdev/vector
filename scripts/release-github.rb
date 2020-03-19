#!/usr/bin/env ruby

# release-github.sh
#
# SUMMARY
#
#   Uploads target/artifacts to Github releases

require_relative "setup"

#
# Constants
#

VERSION = ENV.fetch("VERSION")
SHA1 = ENV.fetch("CIRCLE_SHA1")

#
# Setup
#

metadata = Metadata.load!(META_ROOT, DOCS_ROOT, GUIDES_ROOT, PAGES_ROOT)
release = metadata.releases.to_h.fetch(:"#{VERSION}")

#
# Release
#

Printer.title("Releasing artifacts to Github")

flags = [
  "--assets '#{ROOT_DIR}/target/artifacts/*'",
  "--notes '[View release notes](#{HOST}/releases/#{VERSION})'",
  "--name v#{VERSION}"
]

if release.pre?
  flags << "--pre"
end

command = "grease --debug create-release timberio/vector v#{VERSION} #{SHA1} #{flags.join(" ")}"

Printer.say(
  <<~EOF
  Running command:

    #{command}
  EOF
)

puts `#{command}`
