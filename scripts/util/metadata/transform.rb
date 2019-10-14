#encoding: utf-8

require_relative "component"

class Transform < Component
  attr_reader :allow_you_to_description,
    :function_categories,
    :input_types,
    :output_types

  def initialize(hash)
    super(hash)

    @allow_you_to_description = hash.fetch("allow_you_to_description")
    @function_categories = hash.fetch("function_categories")
    @input_types = hash.fetch("input_types")
    @output_types = hash.fetch("output_types")
    types_coercion = hash["types_coercion"] == true

    if @allow_you_to_description.strip[-1] == "."
      raise("#{self.class.name}#allow_you_to_description cannot not end with a period")
    end

    if (invalid_types = @input_types - EVENT_TYPES) != []
      raise("#{self.class.name}#input_types contains invalid values: #{invalid_types.inspect}")
    end

    if (invalid_types = @output_types - EVENT_TYPES) != []
      raise("#{self.class.name}#output_types contains invalid values: #{invalid_types.inspect}")
    end

    if types_coercion
      wildcard_option =
        {
          "name" => "*",
          "category" => "requests",
          "enum" => {
            "bool" => "Coerces `\"true\"`/`/\"false\"`, `\"1\"`/`\"0\"`, and `\"t\"`/`\"f\"` values into boolean.",
            "float" => "Coerce to a 64 bit float.",
            "int" => "Coerce to a 64 bit integer.",
            "string" => "Coerce to a string.",
            "timestamp" => "Coerces to a Vector timestamp. [`strftime` specificiers][urls.strftime_specifiers] must be used to parse the string."
          },
          "examples" => [
            {"name" => "status", "value" => "int"},
            {"name" => "duration", "value" => "float"},
            {"name" => "success", "value" => "bool"},
            {"name" => "timestamp", "value" => "timestamp|%s", "comment" => "unix"},
            {"name" => "timestamp", "value" => "timestamp|%+", "comment" => "iso8601 (date and time)"},
            {"name" => "timestamp", "value" => "timestamp|%F", "comment" => "iso8601 (date)"},
            {"name" => "timestamp", "value" => "timestamp|%a %b %e %T %Y", "comment" => "custom strftime format"},
          ],
          "description" => "A definition of log field type conversions. They key is the log field name and the value is the type. [`strftime` specifiers][urls.strftime_specifiers] are supported for the `timestamp` type.",
          "null" => false,
          "simple" => true,
          "type" => "string"
        }

      @options.types =
        Option.new({
          "name" => "types",
          "description" => "Key/Value pairs representing mapped log field types.",
          "null" => true,
          "options" => {"*" => wildcard_option},
          "type" => "table"
        })
    end
  end
end