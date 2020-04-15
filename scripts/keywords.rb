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
# Consts
#

COMPETITORS = ["filebeat", "fluentbit", "fluentd", "logstash", "metricbeat", "splunk forwarder", "telegraf"]
FRAMEWORKS = ["angular", "backbone", "django", "drupal", "ember", "express", "laravel", "meteor", "materialize", "rails", "react", "ruby on rails", "spring", "symfony",   "vue"]
PLATFORMS = ["aws", "docker", "ec2", "ec2", "gcp", "heroku", "kubernetes", "lambda", "linux", "netlify", "raspbian", "windows", "wordpress"]
PROGRAMMING_LANGUAGES = ["c", "c++", "c#", "clojure", "elixir", "erlang", "go", "golang", "java", "javascript", "kotlin", "lua", "node", "php", "python", "scala", "ruby", "rust", "typescript", "webassembly"]
SINK_SERVICES = ["blob store", "datadog", "honeycomb", "humio", "influxdb", "logdna", "loggly", "object store", "papertrail", "sematext", "splunk", "stackdriver", "sumologic"]
SOURCE_SERVICES = ["apache web server", "auth0", "fastly", "memcached", "mysql", "nginx", "postgresql", "redis", "rds"]
SOURCE_TERMS = ["application", "microservice", "service"]

#
# Functions
#

def name_variations(name)
  [
    name.humanize.downcase,
    name.gsub(/^aws_/, "").humanize.downcase,
    name.gsub(/^azure_/, "").humanize.downcase,
    name.gsub(/^datadog_/, "").humanize.downcase,
    name.gsub(/^gcp_/, "").humanize.downcase,
    name.gsub(/_logs$/, "").humanize.downcase,
    name.gsub(/_metrics$/, "").humanize.downcase,
    name.gsub(/^new_relic_/, "").humanize.downcase
  ].uniq
end

#
# Setup
#

metadata = Metadata.load!(META_ROOT, DOCS_ROOT, GUIDES_ROOT, PAGES_ROOT)

sources =
  (
    metadata.sources_list.collect(&:name) +
    FRAMEWORKS +
    PLATFORMS +
    PROGRAMMING_LANGUAGES +
    SOURCE_SERVICES +
    SOURCE_TERMS
  ).
  collect{ |name| name_variations(name) }.
  flatten.
  uniq

sinks =
  (
    metadata.sinks_list.collect(&:name) +
    SINK_SERVICES
  ).
  flatten.
  collect{ |name| name_variations(name) }.
  flatten.
  uniq

#
# Execute
#

(sources + sinks).each do |subject|
  puts "analyze #{subject} logs"
  puts "backup #{subject} logs"
  puts "parse #{subject} logs"
  puts "search #{subject} logs"
  puts "#{subject} logs"
  puts "#{subject} logging"
  puts "#{subject} metrics"
  puts "#{subject} monitoring"
  puts "#{subject} observability"
end

sinks.each do |sink|
  puts "send logs to #{sink}"
  puts "send metrics to #{sink}"
  puts "#{sink} alternative"

  sinks.each do |sink2|
    next if sink == sink2
    puts "#{sink} vs #{sink}"
  end
end

sources.each do |source|
  sinks.each do |sink|
    puts "#{source} to #{sink}"
    puts "#{source} logs to #{sink}"
    puts "#{source} metrics to #{sink}"
  end
end
