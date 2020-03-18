require "erb"

require "active_support/core_ext/string/output_safety"

require_relative "templates/config_example_writer"
require_relative "templates/config_schema"
require_relative "templates/config_spec"

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
        "[#{component.name}][docs.#{type.to_s.pluralize}.#{component.name}]"
      end

    num_leftover = components.size - links.size

    if num_leftover > 0
      links << "and [#{num_leftover} more...][docs.#{type.to_s.pluralize}]"
    end

    links.join(", ")
  end

  def component_config_example(component)
    render("#{partials_path}/_component_config_example.md", binding).strip
  end

  def component_default(component)
    render("#{partials_path}/_component_default.md.erb", binding).strip
  end

  def component_description(component)
    send("#{component.type}_description", component)
  end

  def component_header(component)
    render("#{partials_path}/_component_header.md", binding).strip
  end

  def component_output(component, output, breakout_top_keys: false, heading_depth: 1)
    examples = output.examples
    fields = output.fields ? output.fields.to_h.values.sort : []
    render("#{partials_path}/_component_output.md", binding).strip
  end

  def component_requirements(component)
    render("#{partials_path}/_component_requirements.md", binding).strip
  end

  def component_sections(component)
    render("#{partials_path}/_component_sections.md", binding).strip
  end

  def components_table(components)
    if !components.is_a?(Array)
      raise ArgumentError.new("Options must be an Array")
    end

    render("#{partials_path}/_components_table.md", binding).strip
  end

  def config_example(options, array: false, key_path: [], table_path: [], &block)
    if !options.is_a?(Array)
      raise ArgumentError.new("Options must be an Array")
    end

    example = ConfigExampleWriter.new(options, array: array, key_path: key_path, table_path: table_path, &block)
    example.to_toml
  end

  def config_pipeline_example(source, sink)
    source_example =
      ConfigExampleWriter.new(source.options_list, table_path: ["sources", "in"]) do |option|
        option.required?
      end

    sink_example =
      ConfigExampleWriter.new(sink.options_list, table_path: ["sinks", "out"], values: {inputs: ["in"]}) do |option|
        option.required?
      end

    source_example.to_toml + "\n\n" + sink_example.to_toml
  end

  def config_schema(options, opts = {})
    if !options.is_a?(Array)
      raise ArgumentError.new("Options must be an Array")
    end

    opts[:titles] = true unless opts.key?(:titles)

    options = options.sort_by(&:config_file_sort_token)
    schema = ConfigSchema.new(options)
    render("#{partials_path}/_config_schema.toml", binding).strip
  end

  def config_spec(options, opts = {})
    if !options.is_a?(Array)
      raise ArgumentError.new("Options must be an Array")
    end

    opts[:titles] = true unless opts.key?(:titles)

    options = options.sort_by(&:config_file_sort_token)
    spec = ConfigSpec.new(options)
    content = render("#{partials_path}/_config_spec.toml", binding).strip

    if opts[:path]
      content
    else
      content.gsub("\n  ", "\n")
    end
  end

  def create_config_command(source: nil, sink: nil, default_sink: nil)
    if source.nil? && sink.nil?
      raise ArgumentError.new("You must supply at least one source or sink")
    end

    sources = []
    sinks = []

    if source.nil?
      sources =
        metadata.sources_list.select do |source|
          source.can_send_to?(sink)
        end
    end

    if sink.nil?
      sinks =
        metadata.sinks_list.select do |sink|
          sink.can_receive_from?(source)
        end
    end

    render("#{partials_path}/_create_config_command.md", binding).strip
  end

  def docker_docs
    render("#{partials_path}/_docker_docs.md")
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

  def fields(fields, filters: true, heading_depth: 1, level: 1, path: nil)
    if !fields.is_a?(Array)
      raise ArgumentError.new("Fields must be an Array")
    end

    render("#{partials_path}/_fields.md", binding).strip
  end

  def fields_example(fields)
    if !fields.is_a?(Array)
      raise ArgumentError.new("Fields must be an Array")
    end

    render("#{partials_path}/_fields_example.md", binding).strip
  end

  def fields_hash(fields)
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

    hash
  end

  def full_config_spec
    render("#{partials_path}/_full_config_spec.toml", binding).strip.gsub(/ *$/, '')
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
    if component.output.to_h.any?
      "[outputs #{event_types(component.output_types).to_sentence} events](#output)"
    else
      "outputs #{event_type_links(component.output_types).to_sentence} events"
    end
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

  def pluralize(count, word)
    count != 1 ? "#{count} #{word.pluralize}" : "#{count} #{word}"
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

  def setup_guide(id, tutorial, source: nil, sink: nil)
    if source.nil? && sink.nil?
      raise ArgumentError.new("You must supply at least a source or a sink")
    end

    features = []

    if source
      features += source.features
    else
      features << "Collect your logs from one or more sources"
    end

    if sink
      features += sink.features
    else
      features << "Send your logs to one or more destinations"
    end

    render("#{partials_path}/_setup_guide.md", binding).strip
  end

  def sink_description(sink)
    strip <<~EOF
    #{write_verb_link(sink)} #{event_type_links(sink.input_types).to_sentence} events to #{sink.write_to_description}.
    EOF
  end

  def source_description(source)
    strip <<~EOF
    Ingests data through #{source.through_description} and #{outputs_link(source)}.
    EOF
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

        front_matter = FrontMatterParser::Parser.parse_file(f).front_matter
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

  def tutorial_steps(steps, heading_depth: 3)
    render("#{partials_path}/_tutorial_steps.md", binding).strip
  end

  def transform_description(transform)
    if transform.input_types == transform.output_types
      strip <<~EOF
      Accepts and #{outputs_link(transform)} allowing you to #{transform.allow_you_to_description}.
      EOF
    else
      strip <<~EOF
      Accepts #{event_type_links(transform.input_types).to_sentence} events but #{outputs_link(transform)} allowing you to #{transform.allow_you_to_description}.
      EOF
    end
  end

  def vector_summary
    render("#{partials_path}/_vector_summary.md", binding).strip
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
