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

require "time"
require_relative "setup"

#
# Constants
#

TYPES = ["chore", "docs", "feat", "fix", "enhancement", "perf"]
TYPES_THAT_REQUIRE_SCOPES = ["feat", "enhancement", "fix"]

#
# Functions
#
# Sorted alphabetically.
#

# Determines if a commit message is a breaking change as defined by the
# Convetional Commits specification:
#
# https://www.conventionalcommits.org
def breaking_change?(commit)
  !commit.fetch("message").match(/^[a-z]*!/).nil?
end

# Creates and updates the new release meta file located at
#
#   /.meta/releases/X.X.X.toml
#
# This file is created from outstanding commits since the last release.
# It's meant to be a starting point. The resulting file should be reviewed
# and edited by a human.
def create_release_meta_file!(current_commits, new_version)
  release_meta_path = "#{RELEASE_META_DIR}/#{new_version}.toml"

  # Grab all existing commits
  existing_commits = get_existing_commits!

  # Ensure this release does not include duplicate commits. Notice that we
  # check the parsed PR numbers. This is necessary to ensure we do not include
  # cherry-picked commits made available in other releases.
  #
  # For example, if we cherry pick a commit from `master` to the `0.8` branch
  # it will have a different commit sha. Without checking something besides the
  # sha, this commit would also show up in the next release.
  new_commits =
    current_commits.select do |current_commit|
      !existing_commits.any? do |existing_commit|
        existing_commit.fetch("sha") == current_commit.fetch("sha") ||
          existing_commit.fetch("pr_number") == current_commit.fetch("pr_number")
      end
    end

  if new_commits.any?
    if File.exists?(release_meta_path)
      words =
        <<~EOF
        I found #{new_commits.length} new commits since you last generated:

            #{release_meta_path}

        So I don't erase any other work in that file, please manually add the
        following commit lines:

        #{new_commits.to_toml.indent(4)}

        Done? Ready to proceed?
        EOF

      if Printer.get(words, ["y", "n"]) == "n"
        Printer.error!("Ok, re-run this command when you're ready.")
      end
    else
      File.open(release_meta_path, 'w+') do |file|
        file.write(
          <<~EOF
          [releases."#{new_version}"]
          date = #{Time.now.utc.to_date.to_toml}
          commits = #{new_commits.to_toml}
          EOF
        )
      end

      words =
        <<~EOF
        I've created a release meta file here:

          #{release_meta_path}

        I recommend reviewing the commits and fixing any mistakes.

        Ready to proceed?
        EOF

      if Printer.get(words, ["y", "n"]) == "n"
        Printer.error!("Ok, re-run this command when you're ready.")
      end
    end
  end

  true
end

# Gets the commit log from the last version. This is used to determine
# the outstanding commits that should be included in this release.
# Notice the specificed format, this allow us to parse the lines into
# structured data.
def get_commit_log(last_version)
  range = "v#{last_version}..."
  `git log #{range} --no-merges --pretty=format:'%H\t%s\t%aN\t%ad'`.chomp
end

def get_commits_since(last_version)
  commit_log = get_commit_log(last_version)
  commit_lines = commit_log.split("\n").reverse

  commit_lines.collect do |commit_line|
    parse_commit_line!(commit_line)
  end
end

# This is used for the `files_count`, `insertions_count`, and `deletions_count`
# attributes. It helps to communicate stats and the depth of changes in our
# release notes.
def get_commit_stats(sha)
  `git show --shortstat --oneline #{sha}`.split("\n").last
end

# Grabs all existing commits that are included in the `.meta/releases/*.toml`
# files. We grab _all_ commits to ensure we do not include duplicate commits
# in the new release.
def get_existing_commits!
  release_meta_paths = Dir.glob("#{RELEASE_META_DIR}/*.toml").to_a

  release_meta_paths.collect do |release_meta_path|
    contents = File.read(release_meta_path)
    parsed_contents = TomlRB.parse(contents)
    release_hash = parsed_contents.fetch("releases").values.fetch(0)
    release_hash.fetch("commits").collect do |c|
      message_data = parse_commit_message!(c.fetch("message"))

      {
        "sha" => c.fetch("sha"),
        "message" => c.fetch("message"),
        "author" => c.fetch("author"),
        "date" => c.fetch("date"),
        "pr_number" => message_data.fetch("pr_number"),
        "files_count" => c["files_count"],
        "insertions_count" => c["insertions_count"],
        "deletions_count" => c["deletions_count"]
      }
    end
  end.flatten
end

def get_new_version(last_version, commits)
  next_version =
    if commits.any? { |c| breaking_change?(c) }
      next_version = "#{last_version.major + 1}.0.0"

      words = "It looks like the new commits contain breaking changes. " +
        "Would you like to use the recommended version #{next_version} for " +
        "this release?"

      if Printer.get(words, ["y", "n"]) == "y"
        next_version
      else
        nil
      end
    elsif commits.any? { |c| new_feature?(c) }
      next_version = "#{last_version.major}.#{last_version.minor + 1}.0"

      words = "It looks like this release contains commits with new features. " +
        "Would you like to use the recommended version #{next_version} for " +
        "this release?"

      if Printer.get(words, ["y", "n"]) == "y"
        next_version
      else
        nil
      end
    elsif commits.any? { |c| fix?(c) }
      next_version = "#{last_version.major}.#{last_version.minor}.#{last_version.patch + 1}"

      words = "It looks like this release contains commits with bug fixes. " +
        "Would you like to use the recommended version #{next_version} for " +
        "this release?"

      if Printer.get(words, ["y", "n"]) == "y"
        next_version
      else
        nil
      end
    end

  version_string = next_version || Printer.get("What is the next version you are releasing? (current version is #{last_version})")

  version =
    begin
      Version.new(version_string)
    rescue ArgumentError => e
      Printer.invalid("It looks like the version you entered is invalid: #{e.message}")
      get_new_version(last_version, commits)
    end

  if last_version.bump_type(version).nil?
    Printer.invalid("The version you entered must be a single patch, minor, or major bump")
    get_new_version(last_version, commits)
  else
    version
  end
end

def migrate_highlights(new_version)
  Dir.glob("#{HIGHLIGHTS_ROOT}/*.md").to_a.each do |highlight_path|
    content = File.read(highlight_path)
    release_line = "\nrelease: \"nightly\"\n"

    if content.include?(release_line)
      new_content = content.replace(release_line, "\nrelease: \"#{new_version}\"\n")

      File.open(highlight_path, 'w+') do |file|
        file.write(new_content)
      end
    end
  end
end

def new_feature?(commit)
  !commit.fetch("message").match(/^feat/).nil?
end

def fix?(commit)
  !commit.fetch("message").match(/^fix/).nil?
end

# Parses the commit line from `#get_commit_log`.
def parse_commit_line!(commit_line)
  # Parse the full commit line
  line_parts = commit_line.split("\t")
  sha = line_parts.fetch(0)
  message = line_parts.fetch(1)
  author = line_parts.fetch(2)
  date = Time.parse(line_parts.fetch(3))
  message_data = parse_commit_message!(message)
  pr_number = message_data.fetch("pr_number")

  attributes =
    {
      "sha" =>  sha,
      "message" => message,
      "author" => author,
      "date" => date,
      "pr_number" => pr_number
    }

  # Parse the stats
  stats = get_commit_stats(attributes.fetch("sha"))
  if /^\W*\p{Digit}+ files? changed,/.match(stats)
    stats_attributes = parse_commit_stats!(stats)
    attributes.merge!(stats_attributes)
  end

  attributes
end

# Parses the commit message. This is used to extra other information that is
# helpful in deduping commits across releases.
def parse_commit_message!(message)
  match = message.match(/ \(#(?<pr_number>[0-9]*)\)?$/)
  {
    "pr_number" => match && match[:pr_number] ? match[:pr_number].to_i : nil
  }
end

# Parses the data from `#get_commit_stats`.
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

Printer.title("Creating release meta file...")

last_tag = `git describe --tags --abbrev=0`.chomp
last_version = Version.new(last_tag.gsub(/^v/, ''))
commits = get_commits_since(last_version)
new_version = get_new_version(last_version, commits)
create_release_meta_file!(commits, new_version)

Printer.title("Migrating all nightly associated highlights to #{new_version}...")

migrate_highlights(new_version)
