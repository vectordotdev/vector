#!/usr/bin/env ruby

# release-github.sh
#
# SUMMARY
#
#   Uploads target/artifacts to Github releases

require_relative "setup"
require_relative "generate/templates"
require_relative "generate/post_processors/link_definer"

#
# Constants
#

VERSION = ENV.fetch("VERSION")
SHA1 = ENV.fetch("CIRCLE_SHA1")

#
# Commit
#

metadata =
  begin
    Metadata.load!(META_ROOT, DOCS_ROOT)
  rescue Exception => e
    error!(e.message)
  end

templates = Templates.new(TEMPLATES_DIR, metadata)
release = metadata.releases.to_h.fetch(:"#{VERSION}")

#
# Release
#

title("Releasing artifacts to Github")

flags = ["--debug", "--assets 'target/artifacts/*'"]

notes = templates.release_notes(release)
notes = PostProcessors::LinkDefiner.define!(notes.clone, "", metadata.links)
notes = notes.gsub("'", "")
flags << "--notes '#{notes}'"

if release.pre?
  flags << "--pre"
end

command = "grease create-release timberio/vector v#{VERSION} $#{SHA1} #{flags.join(" ")}"

say(
  <<~EOF
  Running command:

    #{command}
  EOF
)

`#{command}`