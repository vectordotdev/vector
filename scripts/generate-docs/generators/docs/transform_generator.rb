#encoding: utf-8

require_relative "../generator"
require_relative "../fields_table_generator"
require_relative "../options_example_generator"
require_relative "../options_table_generator"
require_relative "../sections_generator"
require_relative "component_generator"

module Docs
  class TransformGenerator < ComponentGenerator
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
        description: #{remove_markdown_links(transform.allow_you_to_description)}
        ---

        #{warning}

        # #{transform.name} transform

        ![](#{transform.diagram})

        #{beta(transform)}
        The `#{transform.name}` transforms accepts #{event_type_links(transform.input_types).to_sentence} events and allows you to #{transform.allow_you_to_description}.

        ## Config File

        {% code-tabs %}
        {% code-tabs-item title="example" %}
        ```toml
        #{options_example_generator.generate("transforms.my_#{transform.name}_transform", :examples)}
        ```
        {% endcode-tabs-item %}
        {% code-tabs-item title="schema" %}
        ```toml
        #{options_example_generator.generate("transforms.<transform-id>", :schema)}
        ```
        {% endcode-tabs-item %}
        {% code-tabs-item title="specification" %}
        ```toml
        #{options_example_generator.generate("transforms.#{transform.name}", :spec)}
        ```
        {% endcode-tabs-item %}
        {% endcode-tabs %}

        ## Options

        #{options_table_generator.generate}

        #{example_section(transform)}

        #{guides_section(transform)}

        #{how_it_works_section}

        #{troubleshooting_section(transform)}

        #{resources_section(transform)}
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
end