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

TYPES = ["chore", "docs", "feat", "fix", "improvement", "perf"]
TYPES_THAT_REQUIRE_SCOPES = ["feat", "improvement", "fix"]

#
# Functions
#

def breaking_change?(commit)
  !commit.fetch("message").match(/^[a-z]*!/).nil?
end

def create_release_meta_file!(current_commits, new_version)
  release_meta_path = "#{RELEASE_META_DIR}/#{new_version}.toml"

  existing_release =
    if File.exists?(release_meta_path)
      existing_contents = File.read(release_meta_path)
      if existing_contents.length > 0
        TomlRB.parse(existing_contents).fetch("releases").fetch(new_version.to_s)
      else
        {"commits" => []}
      end
    else
      {"commits" => []}
    end

  existing_commits =
    existing_release.fetch("commits").collect do |c|
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

  new_commits =
    current_commits.select do |current_commit|
      !existing_commits.any? do |existing_commit|
        existing_commit.fetch("sha") == current_commit.fetch("sha")
      end
    end

  if new_commits.any?
    if existing_commits.any?
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

def get_commit_log(last_version)
  range = "v#{last_version}..."
  `git log #{range} --no-merges --pretty=format:'%H\t%s\t%aN\t%ad'`.chomp
end

def get_commits(last_version)
  commit_log = get_commit_log(last_version)
  commit_lines = commit_log.split("\n").reverse

  commit_lines.collect do |commit_line|
    parse_commit_line!(commit_line)
  end
end

def get_commit_stats(sha)
  `git show --shortstat --oneline #{sha}`.split("\n").last
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

def new_feature?(commit)
  !commit.fetch("message").match(/^feat/).nil?
end

def fix?(commit)
  !commit.fetch("message").match(/^fix/).nil?
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
  if /^\W*\p{Digit}+ files? changed,/.match(stats)
    stats_attributes = parse_commit_stats!(stats)
    attributes.merge!(stats_attributes)
  end

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

last_tag = `git describe --tags --abbrev=0`.chomp
last_version = Version.new(last_tag.gsub(/^v/, ''))
commits = get_commits(last_version)
new_version = get_new_version(last_version, commits)
create_release_meta_file!(commits, new_version)
