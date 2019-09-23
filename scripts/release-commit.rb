#!/usr/bin/env ruby

# release-commit.rb
#
# SUMMARY
#
#   Commits and tags the pending release

#
# Setup
#

# Changes into the scripts directory so that we can load the Bundler
# dependencies. Unfortunately, Bundler does not provide a way load a Gemfile
# outside of the cwd.
Dir.chdir "scripts"

#
# Requires
#

require "rubygems"
require "bundler"
Bundler.require(:default)

require_relative "util"

#
# Includes
#

include Printer

#
# Constants
#

ROOT_DIR = Pathname.new("#{Dir.pwd}/..").cleanpath

DOCS_ROOT = File.join(ROOT_DIR, "docs")
META_ROOT = File.join(ROOT_DIR, ".meta")

#
# Functions
#

def release_exists?(release)
  errors = `git rev-parse v#{release.version} 2>&1 >/dev/null`
  errors == ""
end

#
# Commit
#

metadata =
  begin
    Metadata.load(META_ROOT, DOCS_ROOT)
  rescue Exception => e
    error!(e.message)
  end

release = metadata.latest_release

if release_exists?(release)
  error!(
    <<~EOF
    It looks like release v#{release.version} has already been released. A tag for this release already exists.

    This command will only release the latest release. If you're trying to release from an older major or minor version, you must do so from that branch.
    EOF
  )
else
  branch_name = "#{release.version.major}.#{release.version.minor}"

  commands =
    <<~EOF
    git add docs/*
    git commit -sam 'chore: Prepare v#{release.version} release'
    git push origin master
    git tag -a v#{release.version} -m "v#{release.version}"
    git push origin v#{release.version}
    git branch v#{branch_name}
    git push origin v#{branch_name}
    EOF

  commands.chomp!

  words =
    <<~EOF
    We'll be releasing v#{release.version} with the following commands:

    #{commands.indent(2)}

    Proceed to execute the above commands?
    EOF

  if Printer.get(words, ["y", "n"]) == "n"
    Printer.error!("Ok, I've aborted. Please re-run this command when you're ready.")
  end

  commands.chomp.split("\n").each do |command|
    system(command)

    if !$?.success?
      error!(
        <<~EOF
        Command failed!

          #{command}

        Produced the following error:

          #{$?.inspect}
        EOF
      )
  end
end
