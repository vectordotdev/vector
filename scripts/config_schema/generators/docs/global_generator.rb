require_relative "../generator"

module Docs
  class GlobalGenerator < Generator
    attr_reader :options_table_generator,
      :sections_generator,
      :sources,
      :transforms,
      :sinks

    def initialize(schema)
      @sources = schema.sources.to_h.values.sort
      @transforms = schema.transforms.to_h.values.sort
      @sinks = schema.sinks.to_h.values.sort
      options = schema.options.to_h.values.sort
      @options_table_generator = OptionsTableGenerator.new(options, schema.sections)
      @sections_generator = SectionsGenerator.new(schema.sections.sort)
    end

    def global_options_table
      options_table_generator.generate
    end

    def sources_table
      <<~EOF
      | Name  | Description |
      | :---  | :---------- |
      #{source_rows}
      EOF
    end

    def transforms_table
      <<~EOF
      | Name  | Description |
      | :---  | :---------- |
      #{transform_rows}
      EOF
    end

    def sinks_table
      <<~EOF
      | Name  | Description |
      | :---  | :---------- |
      #{sink_rows}
      EOF
    end

    def how_it_works_sections
      sections_generator.generate
    end

    private
      def source_rows
        links = sources.collect do |source|
          "| [**`#{source.name}`**][docs.#{source.name}_source] | Ingests data through #{source.through_description} and outputs #{event_type_links(source.output_types).to_sentence} events. |"
        end

        links.join("\n")
      end

      def transform_rows
        links = transforms.collect do |transform|
          "| [**`#{transform.name}`**][docs.#{transform.name}_transform] | Accepts #{event_type_links(transform.input_types).to_sentence} events and allows you to #{transform.allow_you_to_description}. |"
        end

        links.join("\n")
      end

      def sink_rows
        links = sinks.collect do |sink|
          "| [**`#{sink.name}`**][docs.#{sink.name}_sink] | #{sink.plural_write_verb.humanize} #{event_type_links(sink.input_types).to_sentence} events to #{sink.write_to_description}. |"
        end

        links.join("\n")
      end
  end
end