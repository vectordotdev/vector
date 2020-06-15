#!/usr/bin/env ruby
# encoding: utf-8

# generate.rb
#
# SUMMARY
#
#   A simple script that generates files across the Vector repo. This is used
#   for documentation, config examples, etc. The source templates are located
#   in /scripts/generate/templates/* and the results are placed in their
#   respective root directories.
#
#   See the README.md in the generate folder for more details.

#
# Setup
#

require 'etc'
require 'uri'
require_relative "setup"

#
# Requires
#

require_relative "generate/post_processors/component_importer"
require_relative "generate/post_processors/front_matter_validator"
require_relative "generate/post_processors/last_modified_setter"
require_relative "generate/post_processors/link_definer"
require_relative "generate/post_processors/option_linker"
require_relative "generate/post_processors/section_referencer"
require_relative "generate/post_processors/section_sorter"
require_relative "generate/templates"

#
# Flags
#

dry_run = ARGV.include?("--dry-run")

#
# Constants
#

BLACKLISTED_SINKS = ["vector"]
BLACKLISTED_SOURCES = ["vector"]

#
# Functions
#

def doc_valid?(url_path)
  parts = url_path.split("#", 2)
  file_or_dir_path = WEBSITE_ROOT + parts[0][0..-1]
  file_or_dir_path.delete_suffix!("/")
  anchor = parts[1]
  file_path =
    if File.directory?(file_or_dir_path) && File.file?("#{file_or_dir_path}/README.md")
      "#{file_or_dir_path}/README.md"
    else
      "#{file_or_dir_path}.md"
    end

  markdown_valid?(file_path, anchor)
end

def guide_valid?(url_path)
  parts = url_path.split("#", 2)
  file_or_dir_path = WEBSITE_ROOT + parts[0][0..-1]
  file_or_dir_path.delete_suffix!("/")

  if File.directory?(file_or_dir_path)
    true
  else
    file_path = "#{file_or_dir_path}.md"
    anchor = parts[1]
    markdown_valid?(file_path, anchor)
  end
end

def link_valid?(value)
  if value.start_with?(DOCS_BASE_PATH)
    doc_valid?(value)
  elsif value.start_with?(GUIDES_BASE_PATH)
    guide_valid?(value)
  elsif value.start_with?("/")
    page_valid?(value)
  else
    url_valid?(value)
  end
end

def markdown_valid?(file_path, anchor)
  if File.exists?(file_path)
    if !anchor.nil?
      content = File.read(file_path)
      headings = content.scan(/\n###?#?#? (.*)\n/).flatten.uniq
      anchors = headings.collect(&:parameterize)
      anchors.include?(anchor)
    else
      true
    end
  else
    false
  end
end

def page_valid?(path)
  uri = URI::parse(path)

  path =
    if uri.path == "/"
      "/index"
    elsif uri.path.end_with?("/")
      uri.path[0..-2]
    else
      uri.path
    end

  File.exists?("#{PAGES_ROOT}#{path}.js")
end

def post_process(content, target_path, links)
  if target_path.end_with?(".md")
    content = content.clone
    content = PostProcessors::ComponentImporter.import!(content)
    content = PostProcessors::SectionSorter.sort!(content)
    content = PostProcessors::SectionReferencer.reference!(content)
    content = PostProcessors::OptionLinker.link!(content)
    content = PostProcessors::LinkDefiner.define!(content, target_path, links)
    # must be last
    content = PostProcessors::LastModifiedSetter.set!(content, target_path)

    PostProcessors::FrontMatterValidator.validate!(content, target_path)
  end

  content
end

def url_valid?(url)
  case url
  # We add an exception for paths on packages.timber.io because the
  # index.html file we use also serves as the error page. This is how
  # it serves directories.
  when /^https:\/\/packages\.timber\.io\/vector[^.]*$/
    true

  # Some URLs, like download URLs, contain variables and are not meant
  # to be validated.
  when /<([A-Z_\-.]*)>/
    true

  else
    uri = URI.parse(url)
    req = Net::HTTP.new(uri.host, uri.port)
    req.open_timeout = 500
    req.read_timeout = 1000
    req.ssl_timeout = 1000
    req.use_ssl = true if uri.scheme == 'https'
    path = uri.path == "" ? "/" : uri.path

    begin
      res = req.request_head(path)
      res.code.to_i != 404
    rescue Errno::ECONNREFUSED
      return false
    end
  end
end

def write_new_file(path, contents)
  if !File.exists?(path)
    dirname = File.dirname(path)

    unless File.directory?(dirname)
      FileUtils.mkdir_p(dirname)
    end

    File.open(path, 'w+') { |file| file.write(contents) }
  end
end


#
# Header
#

Printer.title("Generating files...")

#
# Setup
#

metadata = Metadata.load!(META_ROOT, DOCS_ROOT, GUIDES_ROOT, PAGES_ROOT)
templates = Templates.new(ROOT_DIR, metadata)

#
# Create missing platform integration guides
#

metadata.installation.platforms_list.each do |platform|
  template_path = "#{GUIDES_ROOT}/integrate/platforms/#{platform.name}.md.erb"
  strategy = platform.strategies.first
  source = metadata.sources.send(strategy.source)

  write_new_file(
    template_path,
    <<~EOF
    <%- platform = metadata.installation.platforms.send("#{platform.name}") -%>
    <%= integration_guide(platform: platform) %>
    EOF
  )

  metadata.sinks_list.
    select do |sink|
      source.can_send_to?(sink) &&
        !sink.function_category?("test") &&
        !BLACKLISTED_SINKS.include?(sink.name)
    end.
    each do |sink|
      template_path = "#{GUIDES_ROOT}/integrate/platforms/#{platform.name}/#{sink.name}.md.erb"

      write_new_file(
        template_path,
        <<~EOF
        <%- platform = metadata.installation.platforms.send("#{platform.name}") -%>
        <%- sink = metadata.sinks.send("#{sink.name}") -%>
        <%= integration_guide(platform: platform, sink: sink) %>
        EOF
      )
    end
end

#
# Create missing source integration guides
#

metadata.sources_list.
  select do |source|
    !source.for_platform? &&
      !source.function_category?("test") &&
      !BLACKLISTED_SOURCES.include?(source.name)
  end.
  each do |source|
    template_path = "#{GUIDES_ROOT}/integrate/sources/#{source.name}.md.erb"

    write_new_file(
      template_path,
      <<~EOF
      <%- source = metadata.sources.send("#{source.name}") -%>
      <%= integration_guide(source: source) %>
      EOF
    )

    metadata.sinks_list.
      select do |sink|
        source.can_send_to?(sink) &&
          !sink.function_category?("test") &&
          !BLACKLISTED_SINKS.include?(sink.name)
      end.
      each do |sink|
        template_path = "#{GUIDES_ROOT}/integrate/sources/#{source.name}/#{sink.name}.md.erb"

        write_new_file(
          template_path,
          <<~EOF
          <%- source = metadata.sources.send("#{source.name}") -%>
          <%- sink = metadata.sinks.send("#{sink.name}") -%>
          <%= integration_guide(source: source, sink: sink) %>
          EOF
        )
      end
  end

#
# Create missing sink integration guides
#

metadata.sinks_list.
  select do |sink|
    !sink.function_category?("test") &&
      !BLACKLISTED_SINKS.include?(sink.name)
  end.
  each do |sink|
    template_path = "#{GUIDES_ROOT}/integrate/sinks/#{sink.name}.md.erb"

    write_new_file(
      template_path,
      <<~EOF
      <%- sink = metadata.sinks.send("#{sink.name}") -%>
      <%= integration_guide(sink: sink) %>
      EOF
    )
  end

#
# Create missing release pages
#

metadata.releases_list.each do |release|
  template_path = "#{RELEASES_ROOT}/#{release.version}.md.erb"

  write_new_file(
    template_path,
    <<~EOF
    <%- release = metadata.releases.send("#{release.version}") -%>
    <%= release_header(release) %>

    <%- if release.highlights.any? -%>
    ## Highlights

    <div className="sub-title">Noteworthy changes in this release</div>

    <%= release_highlights(release, heading_depth: 3) %>

    <%- end -%>
    ## Changelog

    <div className="sub-title">A complete list of changes</div>

    <Changelog version={<%= release.version.to_json %>} />

    ## What's Next

    <%= release_whats_next(release) %>
    EOF
  )
end

#
# Create missing component templates
#

metadata.components.each do |component|
  template_path = "#{REFERENCE_ROOT}/#{component.type.pluralize}/#{component.name}.md.erb"

  if !File.exists?(template_path)
    contents = templates.component_default(component)
    File.open(template_path, 'w+') { |file| file.write(contents) }
  end
end

erb_paths =
  Dir.glob("#{ROOT_DIR}/**/[^_]*.erb", File::FNM_DOTMATCH).
  to_a.
  filter { |path| !path.start_with?("#{META_ROOT}/") }.
  filter { |path| !path.start_with?("#{ROOT_DIR}/scripts") }.
  filter { |path| !path.start_with?("#{ROOT_DIR}/distribution/nix") }

#
# Create missing .md files
#

erb_paths.each do |erb_path|
  md_path = erb_path.gsub(/\.erb$/, "")
  if !File.exists?(md_path)
    File.open(md_path, "w") {}
  end
end

#
# Render templates
#

metadata = Metadata.load!(META_ROOT, DOCS_ROOT, GUIDES_ROOT, PAGES_ROOT)
templates = Templates.new(ROOT_DIR, metadata)
root_erb_paths = erb_paths.select { |path| !templates.partial?(path) }

Parallel.map(root_erb_paths, in_threads: Etc.nprocessors) do |template_path|
  target_file = template_path.gsub(/^#{ROOT_DIR}\//, "").gsub(/\.erb$/, "")
  target_path = "#{ROOT_DIR}/#{target_file}"
  content = templates.render(target_file)
  content = post_process(content, target_path, metadata.links)
  current_content = File.read(target_path)

  if current_content != content
    action = dry_run ? "Will be changed" : "Changed"
    Printer.say("#{action} - #{target_file}", color: :green)
    File.write(target_path, content) if !dry_run
  else
    action = dry_run ? "Will not be changed" : "Not changed"
    Printer.say("#{action} - #{target_file}", color: :blue)
  end
end

if dry_run
  return
end

#
# Post process individual docs
#

Printer.title("Post processing generated files...")

docs =
  Dir.glob("#{DOCS_ROOT}/**/*.md").to_a +
    Dir.glob("#{GUIDES_ROOT}/**/*.md").to_a +
    Dir.glob("#{HIGHLIGHTS_ROOT}/**/*.md").to_a +
    Dir.glob("#{POSTS_ROOT}/**/*.md").to_a +
    ["#{ROOT_DIR}/README.md"]

docs.each do |doc|
  path = doc.gsub(/^#{ROOT_DIR}\//, "")
  original_content = File.read(doc)
  new_content = post_process(original_content, doc, metadata.links)

  if original_content != new_content
    File.write(doc, new_content)
    Printer.say("Processed - #{path}", color: :green)
  else
    Printer.say("Not changed - #{path}", color: :blue)
  end
end



#
# Check URLs
#

check_urls =
  if ENV.key?("CHECK_URLS")
    ENV.fetch("CHECK_URLS") == "true"
  else
    Printer.title("Checking URLs...")
    Printer.get("Would you like to check & verify URLs?", ["y", "n"]) == "y"
  end

if check_urls
  Parallel.map(metadata.links.values.to_a.sort, in_threads: 50) do |id, value|
    if !link_valid?(value)
      Printer.error!(
        <<~EOF
        Link `#{id}` invalid!

          #{value}

        Please make sure this path or URL exists.
        EOF
      )
    else
      Printer.say("Valid - #{id} - #{value}", color: :green)
    end
  end
end
