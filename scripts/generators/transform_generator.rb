require_relative "generator"
require_relative "fields_table_generator"
require_relative "options_example_generator"
require_relative "options_table_generator"
require_relative "sections_generator"

class TransformGenerator < Generator
  ROOT_PATH = "../../../"

  attr_reader :options_example_generator,
    :options_table_generator,
    :sections_generator,
    :transform

  def initialize(transform, guides)
    super(guides)

    options = transform.options.to_h.values.sort
    @options_example_generator = OptionsExampleGenerator.new(options)
    @options_table_generator = OptionsTableGenerator.new(options, transform.sections)
    @sections_generator = SectionsGenerator.new(transform.sections)
    @transform = transform
  end

  def generate
    content = <<~EOF
      ---
      description: #{transform.allow_you_to_description}
      ---

      #{warning}

      # #{transform.name} transform

      ![](#{transform.diagram})

      #{beta(transform)}
      The `#{transform.name}` transforms accepts #{event_type_links(transform.input_types)} events and allows you to #{transform.allow_you_to_description}.

      ## Example

      {% code-tabs %}
      {% code-tabs-item title="vector.toml (example)" %}
      ```coffeescript
      #{options_example_generator.generate("transforms.my_#{transform.name}_transform", :examples)}
      ```
      {% endcode-tabs-item %}
      {% code-tabs-item title="vector.toml (schema)" %}
      ```coffeescript
      #{options_example_generator.generate("transforms.<transform-id>", :schema)}
      ```
      {% endcode-tabs-item %}
      {% endcode-tabs %}

      ## Options

      #{options_table_generator.generate}

      ## I/O

      The `#{transform.name}` accepts #{event_type_links(transform.input_types)} events and outputs #{event_type_links(transform.output_types)} events.

      #{guides_section(transform)}

      #{how_it_works_section}

      #{troubleshooting(transform)}

      #{resources(transform)}
    EOF
    content
  end

  private
    def how_it_works_section
      content = sections_generator.generate.strip

      if content == ""
        ""
      else
        content =
          <<~EOF
          ## How It Works

          #{content}
          EOF

        content.strip
      end
    end
end