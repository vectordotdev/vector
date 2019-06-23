require_relative "../generator"

module Config
  class ExampleGenerator < Generator
    attr_reader :component,
      :example_generator

    def initialize(component)
      @component = component
      @example_generator = OptionsExampleGenerator.new(component.options.to_h.values.sort)
    end

    def generate
      <<~EOF
      # `#{component.name}` #{component_type(component).humanize} Example
      # ------------------------------------------------------------------------------
      # A simple example demonstrating the #{component_name(component)}
      # Docs: https://docs.vector.dev/usage/configuration/#{component_type(component).pluralize}/#{component.name}

      #{example_generator.generate("#{component_type(component).pluralize}.my_#{component.name}_#{component_type(component)}", :examples)}
      EOF
    end
  end
end