require "erb"

require "active_support/core_ext/string/output_safety"
require "action_view/helpers/number_helper"

require_relative "templates/config_example"
require_relative "templates/config_schema"
require_relative "templates/config_spec"
require_relative "templates/options_table"

# Renders teampltes in the templates sub-dir
#
# ==== Partials
#
# Partials are contained within the `templates/_partials` folder. Partials
# can be rendered directly via #render_partial or call from a custom method,
# as is the case for `#components_table`. Notice that custom methods capture
# the binding in the method directy, this ensures variables within the
# scope of that method are available when rendering the template.
#
# ==== Sub-Objects
#
# There are times whewre it makes sense to represent logic in a sub-object.
# This is usually true for complicated partials. For example, the
# `options_table` partial also instantiates an `Templates::OptionsTable` object
# that is made available to the `options_table` partial. This reduces the
# noise and complexity for the global `Templates` object.
#
# ==== Keep It Simple
#
# In most cases it is easier to avoid partials and sub-objects. A simple
# template with some global methods added to the `Templates` object will
# generally suffice.
class Templates
  include ActionView::Helpers::NumberHelper

  attr_reader :metadata, :dir

  def initialize(dir, metadata)
    @dir = dir
    @metadata = metadata
  end

  def commit(commit)
    render("_partials/_commit.md", binding).gsub("\n", "")
  end

  def commit_scope(commit)
    scope = commit.scope

    info =
      if scope.existing_component?
        {
          text: "`#{scope.component_name}` #{scope.component_type}",
          link: scope.short_link
        }
      else
        {
          text: scope.name,
          link: scope.short_link
        }
      end

    if info[:link] && metadata.links.exists?(info[:link])
      "[#{info[:text]}][#{info[:link]}]"
    else
      info[:text]
    end
  end

  def commit_type_category(type_name, category)
    if type_name == "new feature"
      "new #{category}"
    else
      "#{category} #{type_name}"
    end
  end

  def commit_type_commits(type_name, commits, grouped: false)
    commits =
      commits.sort_by do |commit|
        [commit.scope.name, commit.date]
      end

    render("_partials/_commit_type_commits.md", binding)
  end

  def commit_type_toc_item(type_name, commits)
    render("_partials/_commit_type_toc_item.md", binding).gsub(/,$/, "")
  end

  def component_config_example(component)
    render("_partials/_component_config_example.md", binding).strip
  end

  def component_default(component)
    render("_partials/_component_default.md.erb", binding).strip
  end

  def component_description(component)
    send("#{component.type}_description", component)
  end

  def component_header(component)
    render("_partials/_component_header.md", binding).strip
  end

  def component_resources(component)
    render("_partials/_component_resources.md", binding).strip
  end

  def component_sections(component)
    render("_partials/_component_sections.md", binding).strip
  end

  def components_table(components)
    if !components.is_a?(Array)
      raise ArgumentError.new("Options must be an Array")
    end

    render("_partials/_components_table.md", binding).strip
  end

  def component_troubleshooting(component)
    render("_partials/_component_troubleshooting.md", binding).strip
  end

  def compression_description(compression)
    case compression
    when "gzip"
      "The payload will be compressed in [Gzip][urls.gzip] format before being sent."
    when "none"
      "The payload will not compressed at all."
    else
      raise("Unhandled compression: #{compression.inspect}")
    end
  end

  def config_example(options, opts = {})
    if !options.is_a?(Array)
      raise ArgumentError.new("Options must be an Array")
    end

    opts[:titles] = true unless opts.key?(:titles)

    example = ConfigExample.new(options)
    render("_partials/_config_example.toml", binding).strip
  end

  def config_schema(options, opts = {})
    if !options.is_a?(Array)
      raise ArgumentError.new("Options must be an Array")
    end

    opts[:titles] = true unless opts.key?(:titles)

    schema = ConfigSchema.new(options)
    render("_partials/_config_schema.toml", binding).strip
  end

  def config_spec(options, opts = {})
    if !options.is_a?(Array)
      raise ArgumentError.new("Options must be an Array")
    end

    opts[:titles] = true unless opts.key?(:titles)

    spec = ConfigSpec.new(options)
    content = render("_partials/_config_spec.toml", binding).strip

    if opts[:path]
      content
    else
      content.gsub("\n  ", "\n")
    end
  end

  def encoding_description(encoding)
    case encoding
    when "json"
      "The payload will be encoded as a single JSON payload."
    when "ndjson"
      "The payload will be encoded in new line delimited JSON payload, each line representing a JSON encoded event."
    when "text"
      "The payload will be encoded as new line delimited text, each line representing the value of the `\"message\"` key."
    when nil
      "The encoding type will be dynamically chosen based on the explicit structuring of the event. If the event has been explicitly structured (parsed, keys added, etc), then it will be encoded in the `json` format. If not, it will be encoded as `text`."
    else
      raise("Unhandled compression: #{encoding.inspect}")
    end
  end

  def event_type_links(types)
    types.collect do |type|
      "[`#{type}`][docs.data-model.#{type}]"
    end
  end

  def full_config_spec
    render("_partials/_full_config_spec.toml", binding).strip
  end

  def option_names(options)
    options.collect { |option| "`#{option.name}`" }
  end

  def options_table(options, opts = {})
    if !options.is_a?(Array)
      raise ArgumentError.new("Options must be an Array")
    end

    opts[:header] = true unless opts.key?(:header)
    opts[:titles] = true unless opts.key?(:titles)

    table = OptionsTable.new(options)
    render("_partials/_options_table.md", binding).strip
  end

  def partial?(template_path)
    basename = File.basename(template_path)
    basename.start_with?("_")
  end

  def installation_target_links(targets)
    targets.collect do |target|
      "[#{target.name}][docs.#{target.id}]"
    end
  end

  def pluralize(count, word)
    count != 1 ? "#{count} #{word.pluralize}" : "#{count} #{word}"
  end

  def release_changes(release, grouped: false)
    render("_partials/_release_changes.md", binding)
  end

  def release_notes(release)
    render("_partials/_release_notes.md", binding)
  end

  def release_summary(release)
    parts = []

    if release.new_features.any?
      parts << pluralize(release.new_features.size, "new feature")
    end

    if release.enhancements.any?
      parts << pluralize(release.enhancements.size, "enhancement")
    end

    if release.bug_fixes.any?
      parts << pluralize(release.bug_fixes.size, "bug fix")
    end

    parts.join(", ")
  end

  def render(template_path, template_binding = nil)
    template_binding = binding if template_binding.nil?
    content = File.read("#{dir}/#{template_path}.erb")
    renderer = ERB.new(content, nil, '-')
    content = renderer.result(template_binding)

    if template_path.end_with?(".md") && !partial?(template_path)
      notice =
        <<~EOF

        <!--
             THIS FILE IS AUTOGENERATED!

             To make changes please edit the template located at:

             scripts/generate/templates/#{template_path}.erb
        -->
        EOF

      content.sub!(/\n# /, "#{notice}\n# ")
    end

    content
  end

  def sink_description(sink)
    strip <<~EOF
    #{write_verb_link(sink)} #{event_type_links(sink.input_types).to_sentence} events to #{sink.write_to_description}.
    EOF
  end

  def source_description(source)
    strip <<~EOF
    Ingests data through #{source.through_description} and outputs #{event_type_links(source.output_types).to_sentence} events.
    EOF
  end

  def tags(tags)
    tags.collect { |tag| "`#{tag}`" }.join(" ")
  end

  def transform_description(transform)
    strip <<~EOF
    Accepts #{event_type_links(transform.input_types).to_sentence} events and allows you to #{transform.allow_you_to_description}.
    EOF
  end

  def write_verb_link(sink)
    if sink.batching?
      "[#{sink.plural_write_verb.humanize}](#buffers-and-batches)"
    elsif sink.streaming?
      "[#{sink.plural_write_verb.humanize}](#streaming)"
    elsif sink.exposing?
      "[#{sink.plural_write_verb.humanize}](#exposing-and-scraping)"
    else
      raise "Unhandled sink egress method: #{sink.egress_method.inspect}"
    end
  end

  private
    def build_renderer(template)
      template_path = "#{Dir.pwd}/templates/#{template}.erb"
      template = File.read(template_path)
      ERB.new(template, nil, '-')
    end

    def strip(content)
      content.strip
    end
end