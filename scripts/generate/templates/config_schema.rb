class Templates
  class ConfigSchema
    TYPES = ["string", "int", "float", "bool", "timestamp"]
    
    attr_reader :options

    def initialize(options)
      @options = options
    end

    def categories
      @categories ||= options.collect(&:category).uniq
    end

    def grouped
      @grouped ||=
        options.group_by do |option|
          title = "#{option.required? && option.default.nil? ? "REQUIRED" : "OPTIONAL"}"

          if categories.length > 1
           "#{title} - #{option.category}"
          else
            title
          end
        end
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
end