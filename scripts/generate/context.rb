require "active_support/core_ext/array/conversions"
require "active_support/core_ext/string/indent"
require "active_support/core_ext/string/output_safety"

require_relative "metadata"
require_relative "context/config_example"
require_relative "context/config_schema"
require_relative "context/config_spec"
require_relative "context/options_table"

# Represents the context when rendering templates
#
# This class is the context used when rendering templates. Notice the
# #get_binding method, this is passed as the binding when rendering
# each ERB template.
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
# `options_table` partial also instantiates an `Context::OptionsTable` object
# that is made available to the `options_table` partial. This reduces the
# noise and complexity for the global `Context` object.
#
# ==== Keep It Simple
#
# In most cases it is easier to avoid partials and sub-objects. A simple
# template with some global methods added to the `Context` object will
# generally suffice.
class Context
  attr_reader :metadata

  def initialize(metadata)
    @metadata = metadata
  end

  def component_config_example(component)
    render_partial("component_config_example.md", binding)
  end

  def component_description(component)
    send("#{component.type}_description", component)
  end

  def component_header(component)
    render_partial("component_header.md", binding)
  end

  def component_resources(component)
    render_partial("component_resources.md", binding)
  end

  def component_sections(component)
    render_partial("component_sections.md", binding)
  end

  def components_table(components)
    if !components.is_a?(Array)
      raise ArgumentError.new("Options must be an Array")
    end

    render_partial("components_table.md", binding)
  end

  def component_troubleshooting(component)
    render_partial("component_troubleshooting.md", binding)
  end

  def compression_description(compression)
    case compression
    when "gzip"
      "The payload will be compressed in [Gzip][url.gzip] format before being sent."
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
    render_partial("config_example.toml", binding)
  end

  def config_schema(options, opts = {})
    if !options.is_a?(Array)
      raise ArgumentError.new("Options must be an Array")
    end

    opts[:titles] = true unless opts.key?(:titles)

    schema = ConfigSchema.new(options)
    render_partial("config_schema.toml", binding)
  end

  def config_spec(options, opts = {})
    if !options.is_a?(Array)
      raise ArgumentError.new("Options must be an Array")
    end

    opts[:titles] = true unless opts.key?(:titles)

    spec = ConfigSpec.new(options)
    render_partial("config_spec.toml", binding)
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
      "[`#{type}`][docs.#{type}_event]"
    end
  end

  def full_config_spec
    render_partial("full_config_spec.toml", binding)
  end

  def get_binding
    binding
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
    render_partial("options_table.md", binding)
  end

  def render_partial(name, binding = nil)
    path = "#{Dir.pwd}/templates/_partials/_#{name}.erb"
    content = File.read(path)
    renderer = ERB.new(content, nil, '-')
    renderer.result(binding).strip
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
    def is_primitive_type?(value)
      value.is_a?(String) ||
        value.is_a?(Integer) ||
        value.is_a?(TrueClass) ||
        value.is_a?(FalseClass) ||
        value.is_a?(NilClass) ||
        value.is_a?(Float)
    end

    def strip(content)
      content.strip
    end
end