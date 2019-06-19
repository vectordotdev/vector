require_relative "generator"
require_relative "fields_table_generator"
require_relative "options_example_generator"
require_relative "options_table_generator"
require_relative "sections_generator"

class SinkGenerator < Generator
  ROOT_PATH = "../../../"

  attr_reader :options_example_generator,
    :options_table_generator,
    :sections_generator,
    :sink

  def initialize(sink, guides)
    super(guides)

    options = sink.options.to_h.values.sort
    @options_example_generator = OptionsExampleGenerator.new(options)
    @options_table_generator = OptionsTableGenerator.new(options, sink.sections)
    @sections_generator = SectionsGenerator.new(sink.sections)
    @sink = sink
  end

  def generate
    content = <<~EOF
      ---
      description: #{sink.plural_write_verb.humanize} #{sink.input_types.to_sentence} events to #{remove_markdown_links(sink.write_to_description)}
      ---

      #{warning}

      # #{sink.name} sink

      ![](#{sink.diagram})

      #{beta(sink)}
      The `#{sink.name}` sink #{sink.write_verb.pluralize} #{event_type_links(sink.input_types).to_sentence} events to #{sink.write_to_description}.

      ## Example

      {% code-tabs %}
      {% code-tabs-item title="vector.toml (example)" %}
      ```coffeescript
      #{options_example_generator.generate("sinks.my_#{sink.name}_sink", :examples)}
      ```
      {% endcode-tabs-item %}
      {% code-tabs-item title="vector.toml (schema)" %}
      ```coffeescript
      #{options_example_generator.generate("sinks.<sink-id>", :schema)}
      ```
      {% endcode-tabs-item %}
      {% code-tabs-item title="vector.toml (specification)" %}
      ```coffeescript
      #{options_example_generator.generate("sinks.#{sink.name}", :spec)}
      ```
      {% endcode-tabs-item %}
      {% endcode-tabs %}

      ## Options

      #{options_table_generator.generate}

      #{outputs_section(sink, output_prefix)}

      #{guides_section(sink)}

      ## How It Works

      #{sections_generator.generate}

      #{troubleshooting(sink)}
      
      #{resources(sink)}
    EOF
    content
  end

  private
    def output_prefix
      content =
        if sink.batching?
          <<~EOF
          The #{component_name(sink)} batches events up to the `batch_size` or `batch_timeout` options. When flushed, Vector will write to #{sink.write_to_description}. The encoding is dictated by the `encoding` option.#{sink.outputs.any? ? " For example:" : ""}
          EOF
        else
          <<~EOF
          The #{component_name(sink)} streams events to #{sink.write_to_description}. The encoding of each event is dictated by the `encoding` option.#{sink.outputs.any? ? " For example:" : ""}
          EOF
        end

      content.strip
    end
end