#encoding: utf-8

require_relative "../generator"
require_relative "../fields_table_generator"
require_relative "../options_example_generator"
require_relative "../options_table_generator"
require_relative "../sections_generator"
require_relative "component_generator"

module Docs
  class SourceGenerator < ComponentGenerator
    ROOT_PATH = "../../../"

    attr_reader :options_example_generator,
      :options_table_generator,
      :sections_generator,
      :source

    def initialize(source, guides)
      super(guides)

      options = source.options.to_h.values.sort
      @options_example_generator = OptionsExampleGenerator.new(options)
      @options_table_generator = OptionsTableGenerator.new(options, source.sections)
      @sections_generator = SectionsGenerator.new(source.sections)
      @source = source
    end

    def generate
      content = <<~EOF
        ---
        description: Continuously accept #{source.output_types.to_sentence} events through #{remove_markdown_links(source.through_description)}
        ---

        #{warning}

        # #{source.name} source

        ![](#{source.diagram})

        #{beta(source)}
        The `#{source.name}` source continuously ingests #{event_type_links(source.output_types).to_sentence} events through #{source.through_description}.

        ## Config File

        {% code-tabs %}
        {% code-tabs-item title="vector.toml (example)" %}
        ```toml
        #{options_example_generator.generate(
          "sources.my_#{source.name}_source",
          :examples
        )}
        ```
        {% endcode-tabs-item %}
        {% code-tabs-item title="vector.toml (schema)" %}
        ```toml
        #{options_example_generator.generate("sources.<source-id>", :schema)}
        ```
        {% endcode-tabs-item %}
        {% code-tabs-item title="vector.toml (specification)" %}
        ```toml
        #{options_example_generator.generate("sources.#{source.name}", :spec)}
        ```
        {% endcode-tabs-item %}
        {% endcode-tabs %}

        ## Options

        #{options_table_generator.generate}
        
        #{example_section(source)}

        #{guides_section(source)}

        ## How It Works

        #{sections_generator.generate}

        #{troubleshooting_section(source)}

        #{resources_section(source)}
      EOF
      content
    end
  end
end