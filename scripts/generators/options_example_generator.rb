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
          option_content =
            case option.type
            when "table"
              sub_generator = self.class.new(option.options)
              sub_generator.generate("#{path}.#{option.name}", format, titles: false) + "\n"
            else
              key_name = format == :examples ? (option.example_key || option.name) : option.name
              "#{key_name} = #{value(option, format)}\n"
            end

          content << option_content
        end

      content << "\n"
    end

    content = <<~EOF
      [#{path}]
      #{content.indent(2)}
    EOF

    content.strip
  end

  private
    def skip?(option, format)
      format == :defaults && option.default.nil?
    end

    def value(option, format)
      case format
      when :examples
        if option.example.nil?
          type_string(option.type)
        else
          tags = []

          if !option.default.nil?
            tags << "default"
          elsif option.optional?
            tags << "no default"
          end

          if option.unit
            tags << option.unit
          end

          if option.enum
            tags << "one of: #{option.enum.join(", ")}"
          end

          value = option.example.inspect

          if tags.any?
            value << " # #{tags.join(", ")}"
          end

          value
        end
      when :defaults
        if option.default.nil?
          type_string(option.type)
        else
          option.example.inspect
        end
      when :schema
        type_string(option)
      else
        raise("Unsupported options example format: #{format.inspect}")
      end
    end

    def type_string(option)
      if option.enum
        "{#{option.enum.join(" | ")}}"
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