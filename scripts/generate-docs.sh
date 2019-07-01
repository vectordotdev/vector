#!/usr/bin/env ruby

# generate-docs.sh
#
# SUMMARY
#
#   Generates documentation across the Vector repository from the
#   metadata.toml file. This is not only used to generate files in the
#   /docs folder but also the /config, README.md, and more.

#
# Requirements
#

require "ostruct"
require "rubygems"
require "bundler/setup"

begin
  require "active_support"
  require "active_support/core_ext/array/conversions"
  require "active_support/core_ext/string/indent"
  require "front_matter_parser"
  require "paint"
  require "toml-rb"
  require "unindent"
  require "word_wrap"
rescue LoadError => e
  abort <<~EOF
  Unable to load gem:

  #{e.message}

  Please install the required Ruby gems:

  sudo gem install bundler
  cd scripts
  bundle install
  EOF
end

require_relative "config_schema/schema"
require_relative "config_schema/generators/config/specification_generator"
require_relative "config_schema/generators/config/example_generator"
require_relative "config_schema/generators/docs/config_specification_generator"
require_relative "config_schema/generators/docs/global_generator"
require_relative "config_schema/generators/docs/guarantees_generator"
require_relative "config_schema/generators/docs/sink_generator"
require_relative "config_schema/generators/docs/sinks_generator"
require_relative "config_schema/generators/docs/source_generator"
require_relative "config_schema/generators/docs/sources_generator"
require_relative "config_schema/generators/docs/transform_generator"
require_relative "config_schema/generators/docs/transforms_generator"
require_relative "config_schema/generators/link_generator"
require_relative "config_schema/generators/readme_generator"

#
# Setup
#

SCHEMA_FILE_PATH = "#{File.dirname(__FILE__)}/metadata.toml"
puts "-> Reading #{SCHEMA_FILE_PATH}\n"
schema_hash = TomlRB.load_file(SCHEMA_FILE_PATH)

#
# Constants
#

DOCS_ROOT = "../../.."
ASSETS_PATH = "#{DOCS_ROOT}/assets/"
CORRECTNESS_TESTS = schema_hash.fetch("enums").fetch("correctness_tests")
DELIVERY_GUARANTEES = schema_hash.fetch("enums").fetch("delivery_guarantees")
EVENT_TYPES = schema_hash.fetch("enums").fetch("event_types")
PERFORMANCE_TESTS = schema_hash.fetch("enums").fetch("performance_tests")
REPO_ROOT = "https://github.com/timberio/vector"
REPO_ISSUES_ROOT = "#{REPO_ROOT}/issues"
REPO_LABELS_ROOT = "#{REPO_ROOT}/labels"
REPO_SRC_ROOT = "#{REPO_ROOT}/tree/master/src"

#
# Functions
#

def write(file_path, content, opts = {})
  existing_content =
    begin
      File.read(file_path)
    rescue Errno::ENOENT
      ""
    end
    
  if existing_content != content
    File.write(file_path, content)
    puts Paint["-> ✔ Updated: #{file_path}", :green] if !opts[:silent]
  else
    puts Paint["-> - Not changed: #{file_path}", :yellow] if !opts[:silent]
  end
end

#
# Load
#

schema = Schema.new(schema_hash)

#
# README
#

readme_generator = ReadmeGenerator.new(schema.sources.to_h.values, schema.transforms.to_h.values, schema.sinks.to_h.values)
readme_generator._generate_new("README.md")

#
# Guarantee
#

guarantees_generator = Docs::GuaranteesGenerator.new(schema.sources.to_h.values, schema.sinks.to_h.values)
guarantees_generator._generate_new("docs/about/guarantees.md")

#
# Configuration global options
#

global_generator = Docs::GlobalGenerator.new(schema)
global_generator._generate_new("docs/usage/configuration/README.md")

#
# Sources
#

sources_generator = Docs::SourcesGenerator.new(schema.sources.to_h.values.sort)
sources_generator._generate_new("docs/usage/configuration/sources/README.md")

schema.sources.to_h.each do |_source_name, source|
  source_generator = Docs::SourceGenerator.new(source, schema.guides)
  content = source_generator.generate
  write("docs/usage/configuration/sources/#{source.name}.md", content)

  example_generator = Config::ExampleGenerator.new(source)
  content = example_generator.generate
  write("config/examples/sources/#{source.name}.toml", content)
end

#
# Transforms
#

transforms_generator = Docs::TransformsGenerator.new(schema.transforms.to_h.values.sort)
content = transforms_generator._generate_new("docs/usage/configuration/transforms/README.md")

schema.transforms.to_h.each do |_transform_name, transform|
  transform_generator = Docs::TransformGenerator.new(transform, schema.guides)
  content = transform_generator.generate
  write("docs/usage/configuration/transforms/#{transform.name}.md", content)

  example_generator = Config::ExampleGenerator.new(transform)
  content = example_generator.generate
  write("config/examples/transforms/#{transform.name}.toml", content)
end

#
# Sinks
#

sinks_generator = Docs::SinksGenerator.new(schema.sinks.to_h.values.sort)
sinks_generator._generate_new("docs/usage/configuration/sinks/README.md")

schema.sinks.to_h.each do |_sink_name, sink|
  sink_generator = Docs::SinkGenerator.new(sink, schema.guides)
  content = sink_generator.generate
  write("docs/usage/configuration/sinks/#{sink.name}.md", content)

  example_generator = Config::ExampleGenerator.new(sink)
  content = example_generator.generate
  write("config/examples/sinks/#{sink.name}.toml", content)
end

#
# Config specification
#

sinks_generator = Config::SpecificationGenerator.new(schema)
content = sinks_generator.generate
write("config/vector.spec.toml", content)

sinks_generator = Docs::ConfigSpecificationGenerator.new(schema)
content = sinks_generator.generate
write("docs/usage/configuration/specification.md", content)

#
# Misc cleanup
#

files = Dir.glob('docs/**/*.md').to_a
files << "README.md"
files = files - ["docs/SUMMARY.md"]

files.each do |file_path|
  content = File.read(file_path)

  # Parse and add link definitions to the footer
  link_generator = LinkGenerator.new(content, file_path, schema.links)
  content = link_generator.generate

  # Convert all ```toml definitions to ```toml since Gitbook
  # does not have a toml syntax definition and coffeescript is the closest :(
  content.gsub!('```toml', '```coffeescript')

  write(file_path, content, silent: true)
end