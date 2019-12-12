#!/usr/bin/env ruby

# release-github.sh
#
# SUMMARY
#
#   Uploads target/artifacts to Github releases

require_relative "setup"

metadata = Metadata.load!(META_ROOT, DOCS_ROOT, PAGES_ROOT)
release = metadata.releases.to_h.fetch(:"#{VERSION}")

#
# Constants
#

VERSION = ENV.fetch("VERSION")
SHA1 = ENV.fetch("CIRCLE_SHA1")

#
# Release
#

title("Releasing artifacts to Github")

flags = [
  "--assets 'target/artifacts/*'",
  "--notes '[View release notes](#{HOST}/releases/#{VERSION})'"
]

if release.pre?
  flags << "--pre"
end

command = "grease --debug create-release timberio/vector v#{VERSION} #{SHA1} #{flags.join(" ")}"

say(
  <<~EOF
  Running command:

    #{command}
  EOF
)

puts `#{command}`