#!/usr/bin/env ruby

# release-meta.rb
#
# SUMMARY
#
#   A script that prepares the release .meta/releases/vX.X.X.toml file.
#   Afterwards, the `make generate` command should be used to refresh
#   the generated files against the new release metadata.

#
# Setup
#

# Changes into the release-meta directory so that we can load the
# Bundler dependencies. Unfortunately, Bundler does not provide a way
# load a Gemfile outside of the cwd.
Dir.chdir "scripts/release-meta"

#
# Requires
#

require "rubygems"
require "bundler"
Bundler.require(:default)

require "time"
require_relative "util/core_ext/object"
require_relative "util/printer"
require_relative "util/version"

#
# Includes
#

include Printer

#
# Constants
#

ROOT_DIR = Pathname.new("#{Dir.pwd}/../..").cleanpath
RELEASE_META_DIR = "#{ROOT_DIR}/.meta/releases"
TYPES = ["chore", "docs", "feat", "fix", "improvement", "perf"]
TYPES_THAT_REQUIRE_SCOPES = ["feat", "improvement", "fix"]

#
# Functions
#

def create_release_meta_file!(last_version, new_version)
  release_meta_path = "#{RELEASE_META_DIR}/#{new_version}.toml"

  existing_release =
    if File.exists?(release_meta_path)
      TomlRB.parse(File.read(release_meta_path)).fetch("releases").fetch(new_version.to_s)
    else
      {"commits" => []}
    end

  existing_commits = existing_release.fetch("commits").collect do |c|
  	{
  		"sha" => c.fetch("sha"),
  		"message" => c.fetch("message"),
  		"author" => c.fetch("author"),
  		"date" => c.fetch("date"),
  		"files_count" => c["files_count"],
  		"insertions_count" => c["insertions_count"],
  		"deletions_count" => c["deletions_count"]
  	}
  end
  current_commits = get_commits(last_version, new_version)

  new_commits =
    current_commits.select do |current_commit|
      !existing_commits.any? do |existing_commit|
        existing_commit.fetch("sha") == current_commit.fetch("sha")
      end
    end

  if new_commits.any?
    commits = existing_commits + new_commits

    File.open(release_meta_path, 'w+') do |file|
      file.write(
        <<~EOF
        [releases."#{new_version}"]
        date = #{Time.now.utc.to_date.to_toml}
        commits = #{commits.to_toml}
        EOF
      )
    end

    words =
      <<~EOF
      I found #{new_commits.length} new commits for this release, and I've placed them in:

        #{release_meta_path}

      Please modify and reword as necessary.

      Ready to proceed?
      EOF

    if get(words, ["y", "n"]) == "n"
      error!("Ok, re-run this command when you're ready.")
    end
  else
    words =
      <<~EOF
      No new commits found for this release. Existing commits can be found in:

        #{release_meta_path}

      Ready to proceed?
      EOF

    if get(words, ["y", "n"]) == "n"
      error!("Ok, re-run this command when you're ready.")
    end
  end

  true
end

def get_commit_log(last_version, new_version)
  last_commit = `git rev-parse HEAD`.chomp
  range = "v#{last_version}...#{last_commit}"
  `git log #{range} --no-merges --pretty=format:'%H\t%s\t%aN\t%ad'`.chomp
end

def get_commits(last_version, new_version)
  commit_log = get_commit_log(last_version, new_version)
  commit_lines = commit_log.split("\n").reverse

  commit_lines.collect do |commit_line|
    parse_commit_line!(commit_line)
  end
end

def get_commit_stats(sha)
  `git show --shortstat --oneline #{sha}`.split("\n").last
end

def get_new_version(last_version)
  version_string = get("What is the next version you are releasing? (current version is #{last_version})")

  version =
    begin
      Version.new(version_string)
    rescue ArgumentError => e
      invalid("It looks like the version you entered is invalid: #{e.message}")
      get_new_version(last_version)
    end

  if last_version.bump_type(version).nil?
    invalid("The version you entered must be a single patch, minor, or major bump")
    get_new_version(last_version)
  else
    version
  end
end

def parse_commit_line!(commit_line)
  # Parse the full commit line
  line_parts = commit_line.split("\t")

  attributes =
    {
      "sha" =>  line_parts.fetch(0),
      "message" => line_parts.fetch(1),
      "author" => line_parts.fetch(2),
      "date" => Time.parse(line_parts.fetch(3))
    }

  # Parse the stats
  stats = get_commit_stats(attributes.fetch("sha"))
  stats_attributes = parse_commit_stats!(stats)
  attributes.merge!(stats_attributes)

  attributes
end

def parse_commit_stats!(stats)
  attributes = {}

  stats.split(", ").each do |stats_part|
    stats_part.strip!

    key =
      case stats_part
      when /insertions?/
        "insertions_count"
      when /deletions?/
        "deletions_count"
      when /files? changed/
        "files_count"
      else
        raise "Invalid commit stat: #{stats_part}"
      end

    count = stats_part.match(/^(?<count>[0-9]*) /)[:count].to_i
    attributes[key] = count
  end

  attributes
end

#
# Execute
#

title("Creating release meta file...")

last_tag = `git describe --abbrev=0`.chomp
last_version = Version.new(last_tag.gsub(/^v/, ''))
new_version = get_new_version(last_version)
create_release_meta_file!(last_version, new_version)