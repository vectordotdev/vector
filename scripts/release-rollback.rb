#!/usr/bin/env ruby

# release-rollback.sh
#
# SUMMARY
#
#   Rolls back a fresh release. This should only be used in situations
#   where the release process failed.

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
# Commit
#

metadata =
  begin
    Metadata.load(META_ROOT, DOCS_ROOT)
  rescue Exception => e
    error!(e.message)
  end

release = metadata.latest_release
version = release.version

#
# Rollback
#

input = get("Do you want to rollback #{version}?")

if input == "n"
  error!("You can only rollback the latest release")
end

branch_commands =
  if release.version.patch == 0
    commands =
      <<~EOF
      git branch -d v#{version.major}.#{version.minor}
      git push origin --delete v#{version.major}.#{version.minor}
      EOF

    commands.chomp
  else
    ""
  end

commands =
  <<~EOF
  git tag -d v#{version}
  git push --delete origin v#{version}
  #{branch_commands}
  git reset HEAD~
  EOF

commands.chomp!

words =
  <<~EOF
  We'll be rolling back v#{version} with the following commands:

  #{commands.indent(2)}

  Proceed to execute the above commands?
  EOF

if get(words, ["y", "n"]) == "n"
  error!("Ok, I've aborted. Please re-run this command when you're ready.")
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