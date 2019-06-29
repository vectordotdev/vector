require_relative "generator"

class ReadmeGenerator < Generator
  attr_reader :sources, :transforms, :sinks

  def initialize(sources, transforms, sinks)
    @sources = sources
    @transforms = transforms
    @sinks = sinks
  end

  def sources_table
    <<~EOF
    | Name | Description |
    | :--- | :---------- |
    #{source_rows}
    EOF
  end

  def transforms_table
    <<~EOF
    | Name | Description |
    | :--- | :---------- |
    #{transform_rows}
    EOF
  end

  def sinks_table
    <<~EOF
    | Name | Description |
    | :--- | :---------- |
    #{sink_rows}
    EOF
  end

  private
    def source_rows
      links = sources.collect do |source|
        "| [**`#{source.name}`**][docs.#{source.name}_source] | Ingests data through #{remove_markdown_links(source.through_description)} and outputs #{source.output_types.to_sentence} events. |"
      end

      links.join("\n")
    end

    def transform_rows
      links = transforms.collect do |transform|
        "| [**`#{transform.name}`**][docs.#{transform.name}_transform] | Accepts #{transform.input_types.to_sentence} events and allows you to #{remove_markdown_links(transform.allow_you_to_description)}. |"
      end

      links.join("\n")
    end

    def sink_rows
      links = sinks.collect do |sink|
        "| [**`#{sink.name}`**][docs.#{sink.name}_sink] | #{sink.plural_write_verb.humanize} #{sink.input_types.to_sentence} events to #{remove_markdown_links(sink.write_to_description)}. |"
      end

      links.join("\n")
    end
end