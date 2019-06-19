require_relative "generator"

class TransformsGenerator < Generator
  attr_reader :transforms

  def initialize(transforms)
    @transforms = transforms
  end

  def generate
    content = <<~EOF
      ---
      description: Parse, structure, and transform events
      ---

      # Transforms

      ![](../../../.gitbook/assets/transforms.svg)

      Transforms are in the middle of the [pipeline](../../../about/concepts.md#pipelines), sitting in-between [sources](../sources/) and [sinks](../sinks/). They transform [events](../../../about/data-model.md#event) or the stream as a whole.

      | Name | Input | Output | Description |
      | :--- | :---: | :----: | :---------- |
      #{transform_rows}
    EOF
    content.strip
  end

  private
    def transform_rows
      links = transforms.collect do |transform|
        "| [**`#{transform.name}`**](#{transform.name}.md) | #{event_type_links(transform.input_types)} | #{event_type_links(transform.output_types)}  | Allows you to #{transform.allow_you_to_description} |"
      end

      links.join("\n")
    end
end