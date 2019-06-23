require_relative "../generator"

module Docs
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

        #{warning}

        # Transforms

        ![](../../../assets/transforms.svg)

        Transforms are in the middle of the [pipeline](../../../about/concepts.md#pipelines), sitting in-between [sources](../sources/) and [sinks](../sinks/). They transform [events](../../../about/data-model.md#event) or the stream as a whole.

        | Name | Input | Output | Description |
        | :--- | :---: | :----: | :---------- |
        #{transform_rows}

        [+ request a new transform](#{new_transform_url()})
      EOF
      content.strip
    end

    private
      def transform_rows
        links = transforms.collect do |transform|
          "| [**`#{transform.name}`**](#{transform.name}.md) | #{event_type_links(transform.input_types).join(" ")} | #{event_type_links(transform.output_types).join(" ")}  | Allows you to #{transform.allow_you_to_description}. |"
        end

        links.join("\n")
      end
  end
end