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

ROOT_DIR = Pathname.new("#{Dir.pwd}/..").cleanpath

DOCS_ROOT = File.join(ROOT_DIR, "docs")
META_ROOT = File.join(ROOT_DIR, ".meta")
RELEASE_META_DIR = "#{ROOT_DIR}/.meta/releases"
TEMPLATES_DIR = File.join(ROOT_DIR, "scripts", "generate", "templates")

#
# Functions
#

def load_templates!
  Templates.new(TEMPLATES_DIR, metadata)
end

def load_metadata!
  Metadata.load!(META_ROOT, DOCS_ROOT)
rescue Exception => e
  error!(e.message)
end