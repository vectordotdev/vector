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
metadata = Metadata.load!(META_ROOT, DOCS_ROOT, GUIDES_ROOT, PAGES_ROOT)

# the base directory with GPG keyrings
gpg_base_dir = "#{ROOT_DIR}/target/gpg/github"
# remove all previously imported GPG keys
FileUtils::remove_dir gpg_base_dir, true

# check signatures for all blog posts
metadata.posts.each do |post|
  Printer.say("Checking #{post.path}...")
  github_username = post.author_github.rpartition("/").last

  # directory with keyring for the given author
  keyring_dir = "#{gpg_base_dir}/#{github_username}"
  if not Dir.exists? keyring_dir
    FileUtils::mkpath keyring_dir
    # fetch author's GPG public keys added to GitHub
    uri = URI("https://api.github.com/users/#{github_username}/gpg_keys")
    gpg_keys = JSON.parse Net::HTTP.get(uri)

    # import each of the author's keys to GPG keyring
    gpg_keys.each do |gpg_key|
      Open3.popen3("gpg", "--homedir", keyring_dir, "--import") do |i, o, e, t|
        i.write gpg_key["raw_key"]
      end
    end
  end

  # verify the signature for the post
  res = system("gpg", "--homedir", keyring_dir, "--verify", "#{ROOT_DIR}/#{post.path}.sig")
  if not res
    Printer.error!("Cannot verify GPG signature for #{post.path}")
    exit 1
  end
end
