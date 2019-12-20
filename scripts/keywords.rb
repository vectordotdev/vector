#!/usr/bin/env ruby

# generate.rb
#
# SUMMARY
#
#   Generates a list of keywords based on Vector's components.

#
# Setup
#

require_relative "setup"

#
# Functions
#

def name_variations(name)
  [
    name.humanize.downcase,
    name.gsub(/^aws_/, "").humanize.downcase,
    name.gsub(/^gcp_/, "").humanize.downcase,
    name.gsub(/_logs$/, "").humanize.downcase,
    name.gsub(/_metrics$/, "").humanize.downcase
  ].uniq
end

#
# Setup
#

metadata = Metadata.load!(META_ROOT, DOCS_ROOT, PAGES_ROOT)

source_names =
  [
    metadata.sources_list.collect(&:name),
    "apache",
    "application",
    "ec2",
    "ecs",
    "fluentbit",
    "fluentd",
    "drupal",
    "kubernetes",
    "java",
    "linux",
    "nginx",
    "python",
    "rails",
    "ruby",
    "ruby on rails",
    "windows",
    "wordpress"
  ].
  flatten.
  collect{ |name| name_variations(name) }.
  flatten.
  uniq

sink_names =
  [
    metadata.sinks_list.collect(&:name),
    "datadog",
    "fluentbit",
    "fluentd",
    "gcp_object_store",
    "gcp_stackdriver_logs",
    "gcp_stackdriver_metrics",
    "honeycomb",
    "papertrail",
    "splunk"
  ].
  flatten.
  collect{ |name| name_variations(name) }.
  flatten.
  uniq

#
# Execute
#

source_names.each do |source_name|
  puts "#{source_name} logging"
  puts "#{source_name} metrics"
end

sink_names.each do |sink_name|
  puts "#{sink_name} logging"
  puts "#{sink_name} metrics"
end

source_names.each do |source_name|
  sink_names.each do |sink_name|
    puts "#{source_name} to #{sink_name}"
    puts "#{source_name} logs to #{sink_name}"
    puts "#{source_name} metrics to #{sink_name}"
  end
end
