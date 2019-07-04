#encoding: utf-8

require_relative "generator"

class OptionsExampleGenerator < Generator
  TYPES = ["string", "int", "float", "bool", "timestamp"]

  attr_reader :options

  def initialize(options)
    @options = options
  end

  def generate(path, format = :examples, opts = {})
    opts[:titles] = true if !opts.key?(:titles)

    content = ""

    options.
      select { |option| !skip?(option, format) }.
      group_by do |option|
        if opts[:titles]
          "#{option.required? ? "REQUIRED" : "OPTIONAL"} - #{option.category}"
        else
          nil
        end
      end.
      each do |title, category_options|
        if !title.nil?
          content << "# #{title}\n"
        end

        category_options.each do |option|
          if option.table?
            sub_generator = self.class.new(option.options)
            content << sub_generator.generate("#{path}.#{option.name}", format, titles: false) + "\n"
          elsif format == :examples
            if option.name == "*"
              option.examples.each do |example|
                key = example.fetch("name")
                value = example.fetch("value")
                comment = example["comment"]

                content << "#{key} = #{to_toml_value(value)}"
                content << " # #{comment}" if comment
                content << "\n"
              end
            else
              content << "#{option.name} = #{example_value(option)}\n"
            end
          elsif format == :schema
            content << "#{option.name} = #{type_string(option.type, option.enum)}\n"
          elsif format == :spec
            description = editorify(option.description)
            content << "\n# #{description}\n"
            tags = build_tags(option, :full)

            if tags.any?
              content << "#\n"
            end

            tags.each do |tag|
              content << "# * #{tag}\n"
            end

            option.examples.each_with_index do |example, index|
              key = option.name
              value = example
              comment = nil

              if example.is_a?(Hash)
                key = example.fetch("name")
                value = example.fetch("value")
                comment = example["comment"]
              end

              content << "#{key} = #{to_toml_value(value)}"
              content << " # #{comment}" if comment
              content << "\n"
            end
          else
            raise("Unknown format: #{format.inspect}")
          end
        end

      content << "\n"
    end

    if path != ""
      content = <<~EOF
        [#{path}]
        #{content.indent(2)}
      EOF
    else
      content
    end

    content.strip
  end

  private
    def skip?(option, format)
      format == :defaults && option.default.nil?
    end

    def build_tags(option, type = :inline)
      tags = []

      if !option.default.nil?
        if type == :inline
          tags << "default"
        else
          tags << "default: #{option.default}"
        end
      elsif option.optional?
        tags << "no default"
      end

      if option.unit
        tags << option.unit
      end

      if option.enum
        if option.enum.length > 1
          tags << "enum: #{option.enum.join(", ")}"
        else
          tags << "must be: #{option.enum.join(", ")}"
        end
      end

      tags
    end

    def example_value(option)
      if option.examples.empty?
        type_string(option.type, option.enum)
      else
        tags = build_tags(option)
        example = option.examples.first
        value = to_toml_value(example)

        if !value.include?("\n") && tags.any?
          value << " # #{tags.join(", ")}"
        end

        value
      end
    end

    def to_toml_value(value)
      if value.is_a?(Hash)
        values = value.collect { |key, value| "#{key} = #{to_toml_value(value)}" }
        "{" + values.join(", ") + "}"
      elsif value.is_a?(Array)
        values = value.collect { |value| to_toml_value(value) }
        values.inspect
      elsif value.is_a?(Time)
        value.iso8601(6)
      elsif value.is_a?(String) && value.include?("\n")
        <<~EOF
        """
        #{value}
        """
        EOF
      elsif is_primitive_type?(value)
        value.inspect
      else
        raise "Unknown value type: #{value.class}"
      end
    end

    def is_primitive_type?(value)
      value.is_a?(String) ||
        value.is_a?(Integer) ||
        value.is_a?(TrueClass) ||
        value.is_a?(FalseClass) ||
        value.is_a?(NilClass) ||
        value.is_a?(Float)
    end

    def type_string(type, enum = nil)
      if enum
        if enum.length > 1
          "{#{enum.collect(&:inspect).join(" | ")}}"
        else
          enum.first.inspect
        end
      elsif type.start_with?("[")
        inner_type = type[1..-2]
        inner_type_string = type_string(inner_type)
        "[#{inner_type_string}, ...]"
      elsif type == "*"
        type_strings = TYPES.collect { |type| type_string(type) }
        "{#{type_strings.join(" | ")}}"
      else
        case type
        when "string"
          "\"<string>\""
        else
          "<#{type}>"
        end
      end
    end
end