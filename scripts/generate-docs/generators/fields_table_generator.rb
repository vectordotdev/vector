#encoding: utf-8

require_relative "generator"

class FieldsTableGenerator < Generator
  attr_reader :fields

  def initialize(fields)
    @fields = fields
  end

  def generate
    content = <<~EOF
      | Key  | Type  | Description |
      | :--- | :---: | :---------- |
    EOF

    fields.each do |field|
      tags = []

      if !field.example.nil?
        tags << "`example: #{field.example.inspect}`"
      end

      description = field.description

      if field.config_option
        description << "You can customize the name via the `#{field.config_option}` config option."
      end

      if tags.length > 0
        description << "<br />#{tags.join(" ")}"
      end

      content << "| `#{field.name}` | `#{field.type}` | #{description} |\n"
    end

    content.strip
  end
end