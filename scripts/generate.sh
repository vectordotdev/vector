#!/usr/bin/env ruby

# generate.sh
#
# SUMMARY
#
#   A simple script that generates files across the Vector repo. This is used
#   for documentation, config examples, etc. The source templates are located
#   in /scripts/generate/templates/* and the results are placed in their
#   respective root directories.
#
#   See the README.md in the generate folder for more details.

Dir.chdir "scripts/generate"

#
# Require
#

require "erb"
require "ostruct"
require "rubygems"
require "bundler"
Bundler.require(:default)

require_relative "generate/context"
require_relative "generate/core_ext/hash"
require_relative "generate/core_ext/object"
require_relative "generate/core_ext/string"
require_relative "generate/post_processors/component_presence_checker"
require_relative "generate/post_processors/link_checker"
require_relative "generate/post_processors/section_sorter"

#
# Functions
#

def say(words, color = nil)
  if color
    words = Paint[words, color]
  end

  puts "---> #{words}"
end

#
# Vars
#

VECTOR_DOCS_HOST = "https://docs.vector.dev"
VECTOR_ROOT = File.join(Dir.pwd.split(File::SEPARATOR)[0..-3])
DOCS_ROOT = File.join(VECTOR_ROOT, "docs")
metadata = Metadata.load()
CHECK_URLS = ARGV.any? { |arg| arg == "--check-urls" }

#
# Render templates
#

puts ""
puts "Generating files"
puts ""

context = Context.new(metadata)
templates = Dir.glob("templates/**/*.erb").to_a
templates.each do |template|
  basename = File.basename(template)

  if !basename.start_with?("_")
    content = File.read(template)
    renderer = ERB.new(content)
    content = renderer.result(context.get_binding)

    if basename.end_with?(".md.erb")
      content =
        <<~EOF
        <!--
             THIS FILE IS AUTOOGENERATED!

             To make changes please edit the template located at:

             scripts/generate/#{template}
        -->

        #{content.lstrip}
        EOF
    end

    target = template.gsub(/^templates\//, "../../").gsub(/\.erb$/, "")

    current_content = File.read(target)
    current_content = PostProcessors::LinkChecker.remove_link_footers(current_content)

    if current_content.strip != content.strip
      action = false ? "Will be changed" : "Changed"
      say("#{action} - #{target.gsub("../../", "")}", :green)
      File.write(target, content)
    else
      action = false ? "Will not be changed" : "Not changed"
      say("#{action} - #{target.gsub("../../", "")}", :blue)
    end
  end
end

#
# Check component presence
#

docs = Dir.glob("#{DOCS_ROOT}/usage/configuration/sources/*.md").to_a
PostProcessors::ComponentPresenceChecker.check!("sources", docs, metadata.sources)

docs = Dir.glob("#{DOCS_ROOT}/usage/configuration/transforms/*.md").to_a
PostProcessors::ComponentPresenceChecker.check!("transforms", docs, metadata.transforms)

docs = Dir.glob("#{DOCS_ROOT}/usage/configuration/sinks/*.md").to_a
PostProcessors::ComponentPresenceChecker.check!("sinks", docs, metadata.sinks)

#
# Post process individual docs
#

puts ""
puts "Checking links"
puts ""

docs = Dir.glob("#{DOCS_ROOT}/**/*.md").to_a
docs = docs - ["#{DOCS_ROOT}/SUMMARY.md"]
docs.each do |doc|
  content = File.read(doc)
  content = PostProcessors::SectionSorter.sort!(content)
  content = PostProcessors::LinkChecker.check!(content, doc, metadata.links)
  say("Checked - #{doc}", :green)
  File.write(doc, content)
end