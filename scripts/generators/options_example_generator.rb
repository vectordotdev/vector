require_relative "generator"

class OptionsExampleGenerator < Generator
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
              content << option.examples.join("\n")
            else
              content << "#{option.name} = #{example_value(option)}\n"
            end
          elsif format == :schema
            content << "#{option.name} = #{type_string(option)}\n"
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

            option.examples.each do |example|
              if option.name == "*"
                content << "#{example}\n"
              else
                content << "#{option.name} = #{to_toml_value(example)}\n"
              end
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
        type_string(option.type)
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
      if value.is_a?(String) && value.include?("\n")
        <<~EOF
        """
        #{value}
        """
        EOF
      else
        value.inspect
      end
    end

    def type_string(option)
      if option.enum
        "{#{option.enum.collect(&:inspect).join(" | ")}}"
      else
        case option.type
        when "[string]"
          "[\"<string>\", ...]"
        when "string"
          "\"<string>\""
        else
          "<#{option.type}>"
        end
      end
    end
end