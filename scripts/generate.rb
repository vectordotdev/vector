#!/usr/bin/env ruby

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

require_relative "setup"

#
# Requires
#

require_relative "generate/post_processors/component_importer"
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
# Functions
#

def doc_valid?(file_or_dir)
  parts = file_or_dir.split("#", 2)
  path = DOCS_ROOT + parts[0]
  anchor = parts[1]

  if File.exists?(path)
    if !anchor.nil?
      file_path = File.directory?(path) ? "#{path}/README.md" : path
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

def link_valid?(value)
  if value.start_with?("/")
    doc_valid?(value)
  else
    url_valid?(value)
  end
end

def post_process(content, doc, links)
  if doc.end_with?(".md")
    content = content.clone
    content = PostProcessors::ComponentImporter.import!(content)
    content = PostProcessors::SectionSorter.sort!(content)
    content = PostProcessors::SectionReferencer.reference!(content)
    content = PostProcessors::LinkDefiner.define!(content, doc, links)
    content = PostProcessors::OptionLinker.link!(content)
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

#
# Header
#

title("Generating files...")

#
# Setup
#

metadata = Metadata.load!(META_ROOT, DOCS_ROOT, PAGES_ROOT)
templates = Templates.new(ROOT_DIR, metadata)

#
# Create missing component templates
#

metadata.components.each do |component|
  template_path = "#{REFERENCE_ROOT}/#{component.type.pluralize}/#{component.name}.md.erb"

  if !File.exists?(template_path)
    contents = templates.component_default(component)
    File.open(template_path, 'w+') { |file| file.write(contents) }
    templates = Templates.new(ROOT_DIR, metadata)
  end
end

erb_paths =
  Dir.glob("#{ROOT_DIR}/**/*.erb", File::FNM_DOTMATCH).
  to_a.
  filter { |path| !File.basename(path).start_with?("_") }

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


erb_paths.
  select { |path| !templates.partial?(path) }.
  each do |template_path|
    target_file = template_path.gsub(/^#{ROOT_DIR}\//, "").gsub(/\.erb$/, "")
    target_path = "#{ROOT_DIR}/#{target_file}"
    content = templates.render(target_file)
    content = post_process(content, target_path, metadata.links)
    current_content = File.read(target_path)

    if current_content != content
      action = dry_run ? "Will be changed" : "Changed"
      say("#{action} - #{target_file}", color: :green)
      File.write(target_path, content) if !dry_run
    else
      action = dry_run ? "Will not be changed" : "Not changed"
      say("#{action} - #{target_file}", color: :blue)
    end
  end

if dry_run
  return
end

#
# Post process individual docs
#

title("Post processing generated files...")

docs =
  Dir.glob("#{DOCS_ROOT}/**/*.md").to_a +
    Dir.glob("#{POSTS_ROOT}/**/*.md").to_a +
    ["#{ROOT_DIR}/README.md"]

docs.each do |doc|
  path = doc.gsub(/^#{ROOT_DIR}\//, "")
  original_content = File.read(doc)
  new_content = post_process(original_content, doc, metadata.links)

  if original_content != new_content
    File.write(doc, new_content)
    say("Processed - #{path}", color: :green)
  else
    say("Not changed - #{path}", color: :blue)
  end
end

#
# Check URLs
#

check_urls =
  if ENV.key?("CHECK_URLS")
    ENV.fetch("CHECK_URLS") == "true"
  else
    title("Checking URLs...")
    get("Would you like to check & verify URLs?", ["y", "n"]) == "y"
  end

if check_urls
  Parallel.map(metadata.links.values.to_a.sort, in_threads: 50) do |id, value|
    if !link_valid?(value)
      error!(
        <<~EOF
        Link `#{id}` invalid!

          #{value}

        Please make sure this path or URL exists.
        EOF
      )
    else
      say("Valid - #{id} - #{value}", color: :green)
    end
  end
end