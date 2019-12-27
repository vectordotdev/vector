#!/usr/bin/env ruby

# check-version.rb
#
# SUMMARY
#
#   Checks that the version in Cargo.toml is up-to-date

require "git"
require "semantic"
require "semantic/core_ext"
require "toml-rb"
require_relative "setup"

class Semantic::Version
  #
  # Returns new version after applying a commit with message
  # following Conventional Commits (https://www.conventionalcommits.org)
  #
  def after_conventional_commit(commit_message)
    case commit_message
    when /!:/
      self.increment!(self.major > 0 ? :major : :minor)
    when /^feat/
      self.increment!(:minor)
    when /^(enhancement|fix|perf)/
      self.increment!(:patch)
    else
      self
    end
  end
end

# read version from Cargo.toml
cargo_toml = TomlRB.load_file("#{ROOT_DIR}/Cargo.toml")
cargo_version = cargo_toml["package"]["version"].to_version

# get latest Git tag and extract version from it
git = Git.open(ROOT_DIR)
git_tag = git.describe("HEAD", { :tags => true, :abbrev => 0 })
git_version = git_tag.delete_prefix("v").to_version

# determine minimal required Cargo version using commits since the last Git tag
commit_messages = git.log.between(git_tag, "HEAD").map { |commit| commit.message.lines.first }
min_cargo_version = commit_messages.map { |message| git_version.after_conventional_commit(message) }.max || git_version

puts "Latest tagged version: #{git_version}"
puts "Version in Cargo.toml: #{cargo_version}"
puts "Minimal required version in Cargo.toml: #{min_cargo_version}"

if cargo_version < min_cargo_version
  puts "Error: version in Cargo.toml is smaller than required"
  exit 1
end
