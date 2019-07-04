#encoding: utf-8

require_relative "../generator"

module Docs
  class TransformsGenerator < Generator
    attr_reader :transforms

    def initialize(transforms)
      @transforms = transforms
    end

    def transforms_table
      <<~EOF
      | Name | Description |
      | :--- | :---------- |
      #{transform_rows}
      EOF
    end

    private
      def transform_rows
        links = transforms.collect do |transform|
          "| [**`#{transform.name}`**][docs.#{transform.name}_transform] | Accepts #{event_type_links(transform.input_types).to_sentence} events and allows you to #{transform.allow_you_to_description}. |"
        end

        links.join("\n")
      end
  end
end