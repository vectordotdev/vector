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

require "json"
require "time"
require_relative "util/commit"
require_relative "util/git_log_commit"
require_relative "util/printer"
require_relative "util/release"
require_relative "util/version"

#
# Constants
#

ROOT = ".."
RELEASE_REFERENCE_DIR = File.join(ROOT, "website", "cue", "reference", "releases")
CHANGELOG_DIR = File.join(ROOT, "changelog.d")
TYPES = ["chore", "docs", "feat", "fix", "enhancement", "perf"]
TYPES_THAT_REQUIRE_SCOPES = ["feat", "enhancement", "fix"]

#
# Functions
#
# Sorted alphabetically.
#

# Creates and updates the new release log file located at
#
#   /.meta/releases/X.X.X.log
#
# This file is created from outstanding commits since the last release.
# It's meant to be a starting point. The resulting file should be reviewed
# and edited by a human before being turned into a cue file.
def create_log_file!(current_commits, new_version)
  release_log_path = "#{RELEASE_REFERENCE_DIR}/#{new_version}.log"

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
        existing_commit.eql?(current_commit)
      end
    end

  new_commit_lines = new_commits.collect { |c| c.to_git_log_commit.to_raw }.join("\n")

  if new_commits.any?
    if File.exists?(release_log_path)
      words =
        <<~EOF
        I found #{new_commits.length} new commits since you last ran this
        command. So that I don't erase any other work in that file, please
        manually add the following commit lines:

            #{new_commit_lines.split("\n").collect { |line| "    #{line}" }.join("\n")}

        To:

            #{release_log_path}

        All done? Ready to proceed?
        EOF

      if Util::Printer.get(words, ["y", "n"]) == "n"
        Util::Printer.error!("Ok, re-run this command when you're ready.")
      end
    else
      File.open(release_log_path, 'w+') do |file|
        file.write(new_commit_lines)
      end

      words =
        <<~EOF
        I've created a release log file here:

            #{release_log_path}

        Please review the commits and *adjust the commit messages as necessary*.

        All done? Ready to proceed?
        EOF

      if Util::Printer.get(words, ["y", "n"]) == "n"
        Util::Printer.error!("Ok, re-run this command when you're ready.")
      end
    end
  end

  release_log_path
end

def retire_changelog_entries!()

  Dir.glob("#{CHANGELOG_DIR}/*.md") do |fname|
    if File.basename(fname) == "README.md"
      next
    end
    system('git', 'rm', fname)
  end
end

def generate_changelog!(new_version)

  entries = ""

  Dir.glob("#{CHANGELOG_DIR}/*.md") do |fname|

    if File.basename(fname) == "README.md"
      next
    end

    if entries != ""
      entries += ",\n"
    end

    fragment_contents = File.open(fname)

    # add the GitHub username for any fragments
    # that have an authors field at the end of the
    # fragment. This is generally used for external
    # contributor PRs.
    lines = fragment_contents.to_a
    last = lines.last
    contributors = Array.new

    if last.start_with?("authors: ")
      authors_str = last[9..]
      authors_str = authors_str.delete(" \t\r\n")
      authors_arr = authors_str.split(",")
      authors_arr.each { |author| contributors.push(author) }

      # remove that line from the description
      lines.pop()
    end

    description = lines.join("")

    # get the PR number of the changelog fragment.
    # the fragment type is not used in the Vector release currently.
    basename = File.basename(fname, ".md")
    parts = basename.split(".")

    if parts.length() != 2
       Util::Printer.error!("Changelog fragment #{fname} is invalid (exactly two period delimiters required).")
    end

    fragment_type = parts[1]

    # map the fragment type to Vector's semantic types
    # https://github.com/vectordotdev/vector/blob/master/.github/semantic.yml#L13
    # the type "chore" isn't rendered in the changelog on the website currently,
    # but we are mapping "breaking" and "deprecations" to that type, and both of
    # these are handled in the upgrade guide separately.

    # NOTE: If the fragment types are altered, update both the 'changelog.d/README.md' and
    #       'scripts/check_changelog_fragments.sh' accordingly.
    type = ""
    if fragment_type == "breaking"
      type = "chore"
    elsif fragment_type == "security" or fragment_type == "fix"
      type = "fix"
    elsif fragment_type == "deprecation"
      type = "chore"
    elsif fragment_type == "feature"
      type = "feat"
    elsif fragment_type == "enhancement"
      type = "enhancement"
    else
       Util::Printer.error!("Changelog fragment #{fname} is invalid. Fragment type #{fragment_type} unrecognized.")
    end

    # Note: `pr_numbers`, `scopes` and `breaking` are being omitted from the entries.
    #       These are currently not required for rendering in the website.
    entry = "{\n" +
      "type: #{type.to_json}\n" +
      "description: \"\"\"\n" +
      "#{description}" +
      "\"\"\"\n"

    if contributors.length() > 0
      entry += "contributors: #{contributors.to_json}\n"
    end

    entry += "}"

    entries += entry
  end

  if entries != ""
    retire_changelog_entries!()
  end

  entries
end

def create_release_file!(new_version)
  release_log_path = "#{RELEASE_REFERENCE_DIR}/#{new_version}.log"
  git_log = Vector::GitLogCommit.from_file!(release_log_path)
  commits = Vector::Commit.from_git_log!(git_log)

  release_reference_path = "#{RELEASE_REFERENCE_DIR}/#{new_version}.cue"

  if commits.any?
    commits.each(&:validate!)
    cue_commits = commits.collect(&:to_cue_struct).join(",\n    ")

    changelog_entries = generate_changelog!(new_version)

    if File.exists?(release_reference_path)
      words =
        <<~EOF
        It looks like you already have a release file:

            #{release_reference_path}

        So that I don't overwrite your work, please copy these commits into
        the release file:

        #{cue_commits}

        All done? Ready to proceed?
        EOF

      if Util::Printer.get(words, ["y", "n"]) == "n"
        Util::Printer.error!("Ok, re-run this command when you're ready.")
      end
    else
      File.open(release_reference_path, 'w+') do |file|
        file.write(
          <<~EOF
          package metadata

          releases: #{new_version.to_json}: {
            date:     #{Date.today.to_json}
            codename: ""

            whats_next: []

            changelog: [
          #{changelog_entries}
            ]

            commits: [
          #{cue_commits}
            ]
          }
          EOF
        )
      end

      `cue fmt #{release_reference_path}`

      words =
        <<~EOF
        All done! I've create a release file at:

            #{release_reference_path}

        I recommend previewing the website changes with this release.
        EOF

      Util::Printer.success(words)
    end

    `cue fmt #{release_reference_path}`

    true
  else
    false
  end
end

def get_commits_since(last_version)
  Vector::Commit.fetch_since(last_version)
end

# Grabs all existing commits that are included in the `.meta/releases/*.toml`
# files. We grab _all_ commits to ensure we do not include duplicate commits
# in the new release.
def get_existing_commits!
  releases = Vector::Release.all!(RELEASE_REFERENCE_DIR)
  release_commits = releases.collect(&:commits).flatten

  release_log_paths = Dir.glob("#{RELEASE_REFERENCE_DIR}/*.log").to_a

  log_commits =
    release_log_paths.collect do |release_log_path|
      git_log = Vector::GitLogCommit.from_file!(release_log_path)
      Vector::Commit.from_git_log!(git_log)
    end.flatten

  release_commits + log_commits
end

def get_new_version(last_version, current_commits)
  next_version =
    if current_commits.any? { |c| c.breaking_change? }
      next_version =
        if last_version.major == 0
          "0.#{last_version.minor + 1}.0"
        else
          "#{last_version.major + 1}.0.0"
        end

      words = "It looks like the new commits contain breaking changes. " +
        "Would you like to use the recommended version #{next_version} for " +
        "this release?"

      if Util::Printer.get(words, ["y", "n"]) == "y"
        next_version
      else
        nil
      end
    elsif current_commits.any? { |c| c.new_feature? }
      next_version = "#{last_version.major}.#{last_version.minor + 1}.0"

      words = "It looks like this release contains commits with new features. " +
        "Would you like to use the recommended version #{next_version} for " +
        "this release?"

      if Util::Printer.get(words, ["y", "n"]) == "y"
        next_version
      else
        nil
      end
    elsif current_commits.any? { |c| c.fix? }
      next_version = "#{last_version.major}.#{last_version.minor}.#{last_version.patch + 1}"

      words = "It looks like this release contains commits with bug fixes. " +
        "Would you like to use the recommended version #{next_version} for " +
        "this release?"

      if Util::Printer.get(words, ["y", "n"]) == "y"
        next_version
      else
        nil
      end
    end

  version_string = next_version || Util::Printer.get("What is the next version you are releasing? (current version is #{last_version})")

  version =
    begin
      Util::Version.new(version_string)
    rescue ArgumentError => e
      Util::Printer.invalid("It looks like the version you entered is invalid: #{e.message}")
      get_new_version(last_version, current_commits)
    end

  if last_version.bump_type(version).nil?
    Util::Printer.invalid("The version you entered must be a single patch, minor, or major bump")
    get_new_version(last_version, current_commits)
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

#
# Execute
#

Dir.chdir "scripts"

Util::Printer.title("Creating release meta file...")

last_tag = `git describe --tags $(git rev-list --tags --max-count=1)`.chomp
last_version = Util::Version.new(last_tag.gsub(/^v/, ''))
current_commits = get_commits_since(last_version)
new_version = get_new_version(last_version, current_commits)
log_file_path = create_log_file!(current_commits, new_version)
create_release_file!(new_version)
File.delete(log_file_path)

#Util::Printer.title("Migrating all nightly associated highlights to #{new_version}...")

#migrate_highlights(new_version)
