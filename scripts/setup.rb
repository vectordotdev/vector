# setup.rb
#
# SUMMARY
#
#   Common header script that handles boiler plate for Ruby based scripts.

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

HOST = "https://vector.dev"
DOCS_BASE_PATH = "/docs"

ROOT_DIR = Pathname.new("#{Dir.pwd}/..").cleanpath.to_s
WEBSITE_ROOT = File.join(ROOT_DIR, "website")
ASSETS_ROOT = File.join(ROOT_DIR, "website", "static")
BLOG_HOST = "#{HOST}/blog"
DOCS_ROOT = File.join(ROOT_DIR, "website", "docs")
DOCS_HOST = "#{HOST}#{DOCS_BASE_PATH}"
META_ROOT = File.join(ROOT_DIR, ".meta")
PAGES_ROOT = File.join(ROOT_DIR, "website", "src", "pages")
POSTS_ROOT = File.join(ROOT_DIR, "website", "blog")
REFERENCE_ROOT = File.join(ROOT_DIR, "website", "docs", "reference")
RELEASE_META_DIR = "#{ROOT_DIR}/.meta/releases"
PARTIALS_DIR = File.join(ROOT_DIR, "scripts", "generate", "templates", "_partials")