#!/usr/bin/env ruby

# generate-docs.sh
#
# SUMMARY
#
#   Generates documentation across the Vector repository from the
#   /.metadata.toml file. This is not only used to generate files in the
#   /docs folder but also the /config, README.md, and more.
#
# OPTIONS
#
#   --dry-run    Displays files that will be changed, but does not change them

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

require_relative "generate-docs/metadata"
require_relative "generate-docs/generators/config/specification_generator"
require_relative "generate-docs/generators/config/example_generator"
require_relative "generate-docs/generators/docs/config_specification_generator"
require_relative "generate-docs/generators/docs/global_generator"
require_relative "generate-docs/generators/docs/guarantees_generator"
require_relative "generate-docs/generators/docs/sink_generator"
require_relative "generate-docs/generators/docs/sinks_generator"
require_relative "generate-docs/generators/docs/source_generator"
require_relative "generate-docs/generators/docs/sources_generator"
require_relative "generate-docs/generators/docs/transform_generator"
require_relative "generate-docs/generators/docs/transforms_generator"
require_relative "generate-docs/generators/link_generator"
require_relative "generate-docs/generators/readme_generator"

#
# Setup
#

puts ""
puts "Starting..."
puts ""

DRY_RUN = ARGV[0] == "--dry-run"
METADATA_FILE_PATH = ".metadata.toml"
Generator.say "Loading #{METADATA_FILE_PATH}\n"
metadata_toml = TomlRB.load_file(METADATA_FILE_PATH)

#
# Constants
#

DOCS_ROOT = "../../.."
ASSETS_PATH = "#{DOCS_ROOT}/assets/"
CORRECTNESS_TESTS = metadata_toml.fetch("enums").fetch("correctness_tests")
DELIVERY_GUARANTEES = metadata_toml.fetch("enums").fetch("delivery_guarantees")
EVENT_TYPES = metadata_toml.fetch("enums").fetch("event_types")
PERFORMANCE_TESTS = metadata_toml.fetch("enums").fetch("performance_tests")
REPO_ROOT = "https://github.com/timberio/vector"
REPO_ISSUES_ROOT = "#{REPO_ROOT}/issues"
REPO_LABELS_ROOT = "#{REPO_ROOT}/labels"
REPO_SRC_ROOT = "#{REPO_ROOT}/tree/master/src"

#
# Load
#

metadata = Metadata.new(metadata_toml)

#
# Documentation
#

puts ""
puts "Updating /docs"
puts ""

readme_generator = ReadmeGenerator.new(metadata.sources.to_h.values, metadata.transforms.to_h.values, metadata.sinks.to_h.values)
readme_generator.interpolate("README.md", dry_run: DRY_RUN)

guarantees_generator = Docs::GuaranteesGenerator.new(metadata.sources.to_h.values, metadata.sinks.to_h.values)
guarantees_generator.interpolate("docs/about/guarantees.md", dry_run: DRY_RUN)

global_generator = Docs::GlobalGenerator.new(metadata)
global_generator.interpolate("docs/usage/configuration/README.md", dry_run: DRY_RUN)

sinks_generator = Docs::ConfigSpecificationGenerator.new(metadata)
sinks_generator.write("docs/usage/configuration/specification.md", dry_run: DRY_RUN)

sources_generator = Docs::SourcesGenerator.new(metadata.sources.to_h.values.sort)
sources_generator.interpolate("docs/usage/configuration/sources/README.md", dry_run: DRY_RUN)

metadata.sources.to_h.each do |_source_name, source|
  source_generator = Docs::SourceGenerator.new(source, metadata.guides)
  source_generator.write("docs/usage/configuration/sources/#{source.name}.md", dry_run: DRY_RUN)
end

transforms_generator = Docs::TransformsGenerator.new(metadata.transforms.to_h.values.sort)
content = transforms_generator.interpolate("docs/usage/configuration/transforms/README.md", dry_run: DRY_RUN)

metadata.transforms.to_h.each do |_transform_name, transform|
  transform_generator = Docs::TransformGenerator.new(transform, metadata.guides)
  transform_generator.write("docs/usage/configuration/transforms/#{transform.name}.md", dry_run: DRY_RUN)
end

sinks_generator = Docs::SinksGenerator.new(metadata.sinks.to_h.values.sort)
sinks_generator.interpolate("docs/usage/configuration/sinks/README.md", dry_run: DRY_RUN)

metadata.sinks.to_h.each do |_sink_name, sink|
  sink_generator = Docs::SinkGenerator.new(sink, metadata.guides)
  sink_generator.write("docs/usage/configuration/sinks/#{sink.name}.md", dry_run: DRY_RUN)
end

#
# Config examples
#

puts ""
puts "Updating /config"
puts ""

specification_generator = Config::SpecificationGenerator.new(metadata)
specification_generator.write("config/vector.spec.toml", dry_run: DRY_RUN)

metadata.sources.to_h.each do |_source_name, source|
  example_generator = Config::ExampleGenerator.new(source)
  example_generator.write("config/examples/sources/#{source.name}.toml", dry_run: DRY_RUN)
end

metadata.transforms.to_h.each do |_transform_name, transform|
  example_generator = Config::ExampleGenerator.new(transform)
  example_generator.write("config/examples/transforms/#{transform.name}.toml", dry_run: DRY_RUN)
end

metadata.sinks.to_h.each do |_sink_name, sink|
  example_generator = Config::ExampleGenerator.new(sink)
  example_generator.write("config/examples/sinks/#{sink.name}.toml", dry_run: DRY_RUN)
end

#
# Misc cleanup
#

puts ""
puts "Generating and checking links..."
puts ""

files = Dir.glob('docs/**/*.md').to_a
files << "README.md"
files = files - ["docs/SUMMARY.md"]

files.each do |file_path|
  content = File.read(file_path)
  link_generator = LinkGenerator.new(content, file_path, metadata.links)
  link_generator.write(file_path, dont_clean: true, dry_run: DRY_RUN)
end