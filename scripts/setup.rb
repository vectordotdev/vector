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

require "date"
require_relative "util"

#
# Constants
#

HOST = "https://vector.dev".freeze
DOCS_BASE_PATH = "/docs".freeze
GUIDES_BASE_PATH = "/guides".freeze
HIGHLIGHTS_BASE_PATH = "/highlights".freeze
POSTS_BASE_PATH = "/blog".freeze
RELEASES_BASE_PATH = "/releases".freeze

ROOT_DIR = Pathname.new("#{Dir.pwd}/..").cleanpath.to_s.freeze
WEBSITE_ROOT = File.join(ROOT_DIR, "website").freeze
ASSETS_ROOT = File.join(ROOT_DIR, "website", "static").freeze
BLOG_HOST = "#{HOST}/blog".freeze
DOCS_ROOT = File.join(ROOT_DIR, "website", "docs").freeze
DOCS_HOST = "#{HOST}#{DOCS_BASE_PATH}".freeze
GUIDES_ROOT = File.join(ROOT_DIR, "website", "guides").freeze
HIGHLIGHTS_HOST = "#{HOST}#{HIGHLIGHTS_BASE_PATH}".freeze
HIGHLIGHTS_ROOT = File.join(ROOT_DIR, "website", "highlights").freeze
META_ROOT = File.join(ROOT_DIR, ".meta").freeze
PAGES_ROOT = File.join(ROOT_DIR, "website", "src", "pages").freeze
POSTS_ROOT = File.join(ROOT_DIR, "website", "blog").freeze
REFERENCE_ROOT = File.join(ROOT_DIR, "website", "docs", "reference").freeze
RELEASES_ROOT = File.join(ROOT_DIR, "website", "releases").freeze
RELEASES_HOST = "#{HOST}#{RELEASES_BASE_PATH}".freeze
RELEASE_META_DIR = "#{ROOT_DIR}/.meta/releases".freeze
PARTIALS_DIR = File.join(ROOT_DIR, "scripts", "generate", "templates", "_partials").freeze
STATIC_ROOT = File.join(ROOT_DIR, "website", "static").freeze

OPERATING_SYSTEMS = ["Linux", "MacOS", "Windows"].freeze
