require "erb"

require "active_support/core_ext/string/output_safety"

require_relative "templates/config_spec"
require_relative "templates/integration_guide"
require_relative "templates/interface_start"

# Renders templates in the templates sub-dir
#
# ==== Partials
#
# Partials are contained within the provided `partials_path` folder. Partials
# can be rendered directly via #render_partial or call from a custom method,
# as is the case for `#components_table`. Notice that custom methods capture
# the binding in the method directy, this ensures variables within the
# scope of that method are available when rendering the template.
#
# ==== Sub-Objects
#
# There are times where it makes sense to represent logic in a sub-object.
# This is usually true for complicated partials. For example, the
# `config_schema` partial also instantiates an `Templates::ConfigSchema` object
# that is made available to the `config_schema` partial. This reduces the
# noise and complexity for the global `Templates` object.
#
# ==== Keep It Simple
#
# In most cases it is easier to avoid partials and sub-objects. A simple
# template with some global methods added to the `Templates` object will
# generally suffice.
class Templates
  attr_reader :metadata, :partials_path, :root_dir

  def initialize(root_dir, metadata)
    @root_dir = root_dir
    @partials_path = "scripts/generate/templates/_partials"
    @metadata = metadata
  end

  def common_component_links(type, limit = 5)
    components = metadata.send("#{type.to_s.pluralize}_list")

    links =
      components.select(&:common?)[0..limit].collect do |component|
        "[#{component.name}][#{component_short_link(component)}]"
      end

    num_leftover = components.size - links.size

    if num_leftover > 0
      links << "and [#{num_leftover} more...][docs.#{type.to_s.pluralize}]"
    end

    links.join(", ")
  end

  def component_config_example(component, advanced: true)
    groups = []

    if component.option_groups.empty?
      groups << AccessibleHash.new({
        label: "Common",
        group_name: nil,
        option_filter: lambda do |option|
          !advanced || option.common?
        end
      })

      if advanced
        groups << AccessibleHash.new({
          label: "Advanced",
          group_name: nil,
          option_filter: lambda do |option|
            true
          end
        })
      end
    else
      component.option_groups.each do |group_name|
        groups << AccessibleHash.new({
          label: group_name,
          group_name: group_name,
          option_filter: lambda do |option|
            option.group?(group_name) && (!advanced || option.common?)
          end
        })

        if advanced
          groups << AccessibleHash.new({
            label: "#{group_name} (adv)",
            group_name: group_name,
            option_filter: lambda do |option|
              option.group?(group_name)
            end
          })
        end
      end
    end

    render("#{partials_path}/_component_config_example.md", binding).strip
  end

  def component_default(component)
    render("#{partials_path}/_component_default.md.erb", binding).strip
  end

  def component_examples(component)
    render("#{partials_path}/_component_examples.md", binding).strip
  end

  def component_fields(component, heading_depth: 2)
    render("#{partials_path}/_component_fields.md", binding)
  end

  def component_header(component)
    render("#{partials_path}/_component_header.md", binding).strip
  end

  def component_requirements(component)
    render("#{partials_path}/_component_requirements.md", binding).strip
  end

  def component_sections(component)
    render("#{partials_path}/_component_sections.md", binding).strip
  end

  def component_short_description(component)
    send("#{component.type}_short_description", component)
  end

  def component_short_link(component)
    "docs.#{component.type.to_s.pluralize}.#{component.name}"
  end

  def components_table(components)
    if !components.is_a?(Array)
      raise ArgumentError.new("Options must be an Array")
    end

    render("#{partials_path}/_components_table.md", binding).strip
  end

  def component_warnings(component)
    warnings(component.warnings)
  end

  def config_example(options, array: false, group: nil, key_path: [], table_path: [], &block)
    if !options.is_a?(Array)
      raise ArgumentError.new("Options must be an Array")
    end

    example = ConfigWriters::ExampleWriter.new(options, array: array, group: group, key_path: key_path, table_path: table_path, &block)
    example.to_toml
  end

  def config_spec(options, opts = {})
    if !options.is_a?(Array)
      raise ArgumentError.new("Options must be an Array")
    end

    opts[:titles] = true unless opts.key?(:titles)

    spec = ConfigSpec.new(options)
    content = render("#{partials_path}/_config_spec.toml", binding).strip

    if opts[:path]
      content
    else
      content.gsub("\n  ", "\n")
    end
  end

  def deployment_strategy(strategy, describe: true, platform: nil, sink: nil, source: nil)
    render("#{partials_path}/deployment_strategies/_#{strategy.name}.md", binding).strip
  end

  def docker_docs
    render("#{partials_path}/_docker_docs.md")
  end

  def downloads_urls(downloads)
    render("#{partials_path}/_downloads_urls.md", binding)
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

  def event_types(types)
    types.collect do |type|
      "`#{type}`"
    end
  end

  def event_type_links(types)
    types.collect do |type|
      "[`#{type}`][docs.data-model.#{type}]"
    end
  end

  def fetch_interfaces(interface_names)
    interface_names.collect do |name|
      metadata.installation.interfaces.send(name)
    end
  end

  def fetch_strategies(strategy_references)
    strategy_references.collect do |reference|
      name = reference.is_a?(Hash) ? reference.name : reference
      strategy = metadata.installation.strategies.send(name).clone
      if reference.respond_to?(:source)
        strategy[:source] = reference.source
      end
      strategy
    end
  end

  def fetch_strategy(strategy_reference)
    fetch_strategies([strategy_reference]).first
  end

  def fields(fields, filters: true, heading_depth: 3, path: nil)
    if !fields.is_a?(Array)
      raise ArgumentError.new("Fields must be an Array")
    end

    render("#{partials_path}/_fields.md", binding).strip
  end

  def fields_example(fields, event_type, root_key: nil)
    if !fields.is_a?(Array)
      raise ArgumentError.new("Fields must be an Array")
    end

    render("#{partials_path}/_fields_example.md", binding).strip
  end

  def fields_hash(fields, root_key: nil)
    hash = {}

    fields.each do |field|
      if field.children?
        hash[field.name] = fields_hash(field.children_list)
      else
        example = field.examples.first

        if example.is_a?(Hash)
          hash.merge!(example)
        else
          hash[field.name] = example
        end
      end
    end

    if root_key
      {root_key => hash}
    else
      hash
    end
  end

  def full_config_spec
    render("#{partials_path}/_full_config_spec.toml", binding).strip.gsub(/ *$/, '')
  end

  def highlights(highlights, author: true, colorize: false, group_by: "type", heading_depth: 3, size: nil, tags: true, timeline: true)
    case group_by
    when "type"
      highlights.sort_by!(&:type)
    when "version"
      highlights.sort_by!(&:date)
    else
      raise ArgumentError.new("Invalid group_by value: #{group_by.inspect}")
    end

    highlight_maps =
      highlights.collect do |highlight|
        {
          authorGithub: highlight.author_github,
          dateString: "#{highlight.date}T00:00:00",
          description: highlight.description,
          permalink: highlight.permalink,
          prNumbers: highlight.pr_numbers,
          release: highlight.release,
          tags: highlight.tags,
          title: highlight.title,
          type: highlight.type
        }
      end

    render("#{partials_path}/_highlights.md", binding).strip
  end

  def installation_tutorial(interfaces, strategies, platform: nil, heading_depth: 3, show_deployment_strategy: true)
    render("#{partials_path}/_installation_tutorial.md", binding).strip
  end

  def interface_installation_tutorial(interface, sink: nil, source: nil, heading_depth: 3)
    if !sink && !source
      raise ArgumentError.new("You must supply at lease a source or sink")
    end

    # Default to common sources so that the tutorial flows. Otherwise,
    # the user is not prompted with a Vector configuration example.
    if source.nil?
      source =
        if sink.logs?
          metadata.sources.file
        elsif sink.metrics?
          metadata.sources.statsd
        else
          nil
        end
    end

    render("#{partials_path}/interface_installation_tutorial/_#{interface.name}.md", binding).strip
  end

  def interface_logs(interface)
    render("#{partials_path}/interface_logs/_#{interface.name}.md", binding).strip
  end

  def interface_reload(interface)
    render("#{partials_path}/interface_reload/_#{interface.name}.md", binding).strip
  end

  def interface_start(interface, requirements: nil)
    interface_start =
      case interface.name
      when "docker-cli"
        InterfaceStart::DockerCLI.new(interface, requirements)
      end

    render("#{partials_path}/interface_start/_#{interface.name}.md", binding).strip
  end

  def interface_stop(interface)
    render("#{partials_path}/interface_stop/_#{interface.name}.md", binding).strip
  end

  def interfaces_logs(interfaces, size: nil)
    render("#{partials_path}/_interfaces_logs.md", binding).strip
  end

  def interfaces_reload(interfaces, requirements: nil, size: nil)
    render("#{partials_path}/_interfaces_reload.md", binding).strip
  end

  def interfaces_start(interfaces, requirements: nil, size: nil)
    render("#{partials_path}/_interfaces_start.md", binding).strip
  end

  def interfaces_stop(interfaces, size: nil)
    render("#{partials_path}/_interfaces_stop.md", binding).strip
  end

  def manual_installation_next_steps(type)
    if type != :source && type != :archives
      raise ArgumentError.new("type must be one of :source or :archives")
    end

    distribution_dir = type == :source ? "distribution" : "etc"

    render("#{partials_path}/_manual_installation_next_steps.md", binding).strip
  end

  def option_description(option)
    description = option.description.strip

    if option.templateable?
      description << " This option supports dynamic values via [Vector's template syntax][docs.reference.templating]."
    end

    if option.relevant_when
      word = option.required? ? "required" : "relevant"
      description << " Only #{word} when #{option.relevant_when_kvs.to_sentence(two_words_connector: " or ")}."
    end

    description
  end

  def option_tags(option, default: true, enum: true, example: false, optionality: true, relevant_when: true, type: true, short: false, unit: true)
    tags = []

    if optionality
      if option.required?
        tags << "required"
      else
        tags << "optional"
      end
    end

    if example
      if option.default.nil? && (!option.enum || option.enum.keys.length > 1)
        tags << "example"
      end
    end

    if default
      if !option.default.nil?
        if short
          tags << "default"
        else
          tags << "default: #{option.default.inspect}"
        end
      elsif option.optional?
        tags << "no default"
      end
    end

    if type
      if short
        tags << option.type
      else
        tags << "type: #{option.type}"
      end
    end

    if unit && !option.unit.nil?
      if short
        tags << option.unit
      else
        tags << "unit: #{option.unit}"
      end
    end

    if enum && option.enum
      if short && option.enum.keys.length > 1
        tags << "enum"
      else
        escaped_values = option.enum.keys.collect { |enum| enum.to_toml }
        if escaped_values.length > 1
          tags << "enum: #{escaped_values.to_sentence(two_words_connector: " or ")}"
        else
          tag = "must be: #{escaped_values.first}"
          if option.optional?
            tag << " (if supplied)"
          end
          tags << tag
        end
      end
    end

    if relevant_when && option.relevant_when
      word = option.required? ? "required" : "relevant"
      tag = "#{word} when #{option.relevant_when_kvs.to_sentence(two_words_connector: " or ")}"
      tags << tag
    end

    tags
  end

  def option_names(options)
    options.collect { |option| "`#{option.name}`" }
  end

  def outputs_link(component)
    "outputs #{event_type_links(component.output_types).to_sentence} events"
  end

  def permissions(permissions, heading_depth: nil)
    if !permissions.is_a?(Array)
      raise ArgumentError.new("Permissions must be an Array")
    end

    render("#{partials_path}/_permissions.md", binding).strip
  end

  def partial?(template_path)
    basename = File.basename(template_path)
    basename.start_with?("_")
  end

  def install_command(prompts: true)
    "curl --proto '=https' --tlsv1.2 -sSf https://sh.vector.dev | sh#{prompts ? "" : " -s -- -y"}"
  end

  def installation_target_links(targets)
    targets.collect do |target|
      "[#{target.name}][docs.#{target.id}]"
    end
  end

  def integration_guide(platform: nil, source: nil, sink: nil)
    if platform && source
      raise ArgumentError.new("You cannot pass both a platform and a source")
    end

    interfaces = []
    strategy = nil

    if platform
      interfaces = fetch_interfaces(platform.interfaces)
      strategy = fetch_strategy(platform.strategies.first)
      source = metadata.sources.send(strategy.source)
    elsif source
      interfaces = [metadata.installation.interfaces.send("vector-cli")]
      strategy = fetch_strategy(source.strategies.first)
    elsif sink
      interfaces = [metadata.installation.interfaces.send("vector-cli")]
      strategy = metadata.installation.strategies_list.first
    end

    guide =
      IntegrationGuide.new(
        strategy,
        platform: platform,
        source: source,
        sink: sink
      )

    render("#{partials_path}/_integration_guide.md", binding).strip
  end

  def pluralize(count, word)
    count != 1 ? "#{count} #{word.pluralize}" : "#{count} #{word}"
  end

  def release_breaking_changes(release, heading_depth: 3)
    render("#{partials_path}/_release_breaking_changes.md", binding).strip
  end

  def release_header(release)
    render("#{partials_path}/_release_header.md", binding).strip
  end

  def release_highlights(release, heading_depth: 3, tags: true)
    render("#{partials_path}/_release_highlights.md", binding).strip
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

  def release_whats_next(release, heading_depth: 3)
    render("#{partials_path}/_release_whats_next.md", binding).strip
  end

  def render(template_path, template_binding = nil)
    old_template_path = @_template_path
    template_binding = binding if template_binding.nil?
    content = File.read("#{root_dir}/#{template_path}.erb")
    renderer = ERB.new(content, nil, '-')
    content =
      begin
        @_template_path = "#{root_dir}/#{template_path}"
        renderer.result(template_binding)
      rescue Exception => e
        raise(
          <<~EOF
          Error rendering template!

            #{root_dir.gsub(/#{ROOT_DIR}/, "")}/#{template_path}.erb

          Error:

            #{e.message}

          #{e.backtrace.join("\n").indent(2)}
          EOF
        )
      ensure
        @_template_path = old_template_path
      end

    if template_path.end_with?(".md") && !partial?(template_path)
      notice =
        <<~EOF

        <!--
             THIS FILE IS AUTOGENERATED!

             To make changes please edit the template located at:

             #{template_path}.erb
        -->
        EOF

      content.sub!(/\n## /, "#{notice}\n## ")
    end

    content
  end

  def sink_short_description(sink)
    strip <<~EOF
    #{write_verb_link(sink)} #{event_type_links(sink.input_types).to_sentence} events to #{sink.write_to_description}.
    EOF
  end

  def source_short_description(source)
    strip <<~EOF
    Ingests data through #{source.through_description} and #{outputs_link(source)}.
    EOF
  end

  def strategies(strategies)
    render("#{partials_path}/_strategies.md", binding).strip
  end

  def subpages(link_name = nil)
    dir =
      if link_name
        docs_dir = metadata.links.fetch(link_name).gsub(/\/$/, "")
        "#{WEBSITE_ROOT}#{docs_dir}"
      else
        dirname = File.basename(@_template_path).split(".").first
        @_template_path.split("/")[0..-2].join("/") + "/#{dirname}"
      end

    Dir.glob("#{dir}/*.md").
      to_a.
      sort.
      collect do |f|
        path = DOCS_BASE_PATH + f.gsub(DOCS_ROOT, '').split(".").first
        name = File.basename(f).split(".").first.gsub("-", " ").humanize

        loader = FrontMatterParser::Loader::Yaml.new(whitelist_classes: [Date])
        front_matter = FrontMatterParser::Parser.parse_file(f, loader: loader).front_matter
        sidebar_label = front_matter.fetch("sidebar_label", "hidden")
        if sidebar_label != "hidden"
          name = sidebar_label
        end

        "<Jump to=\"#{path}/\">#{name}</Jump>"
      end.
      join("\n").
      strip
  end

  def tags(tags)
    tags.collect { |tag| "`#{tag}`" }.join(" ")
  end

  def topologies
    render("#{partials_path}/_topologies.md", binding).strip
  end

  def transform_short_description(transform)
    if transform.input_types == transform.output_types
      strip <<~EOF
      Accepts and #{outputs_link(transform)}, allowing you to #{transform.allow_you_to_description}.
      EOF
    else
      strip <<~EOF
      Accepts #{event_type_links(transform.input_types).to_sentence} events, but #{outputs_link(transform)}, allowing you to #{transform.allow_you_to_description}.
      EOF
    end
  end

  def vector_summary
    render("#{partials_path}/_vector_summary.md", binding).strip
  end

  def warnings(warnings)
    render("#{partials_path}/_warnings.md", binding).strip
  end

  def write_verb_link(sink)
    if sink.batching?
      "[#{sink.plural_write_verb.humanize}](#buffers--batches)"
    elsif sink.streaming?
      "[#{sink.plural_write_verb.humanize}](#streaming)"
    elsif sink.exposing?
      "[#{sink.plural_write_verb.humanize}](#exposing--scraping)"
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
