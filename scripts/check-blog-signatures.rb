#!/usr/bin/env ruby

# check-blog-signatures.rb
#
# SUMMARY
#
#   Checks that all blog articles are cryptographically
#   signed by their respective authors

require "json"
require "open3"
require "net/http"
require_relative "setup"
require_relative "util/metadata"
require_relative "util/printer"

# load metadata
metadata = Metadata.load!(META_ROOT, DOCS_ROOT, PAGES_ROOT)

# check signatures for all blog posts
metadata.posts.each do |post|
  say("Checking #{post.path}...")
  github_username = post.author_github.rpartition("/").last

  # remove all previously imported GPG keys
  FileUtils::remove_dir "#{Dir.home}/.gnupg", true

  # fetch author's GPG public keys added to GitHub
  uri = URI("https://api.github.com/users/#{github_username}/gpg_keys")
  gpg_keys = JSON.parse Net::HTTP.get(uri)

  # import each of the author's keys to GPG keyring
  gpg_keys.each do |gpg_key|
    Open3.popen3("gpg", "--import") do |i, o, e, t|
      i.write gpg_key["raw_key"]
    end
  end

  res = system("gpg", "--verify", "#{ROOT_DIR}/#{post.path}.sig")
  if not res
    error!("Cannot verify GPG signature for #{post.path}")
    exit 1
  end
end
