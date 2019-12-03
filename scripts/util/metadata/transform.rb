#encoding: utf-8

require_relative "component"

class Transform < Component
  attr_reader :allow_you_to_description,
    :input_types,
    :output_types

  def initialize(hash)
    super(hash)

    @allow_you_to_description = hash.fetch("allow_you_to_description")
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
          "name" => "`[field-name]`",
          "category" => "requests",
          "enum" => {
            "bool" => "Coerces `\"true\"`/`/\"false\"`, `\"1\"`/`\"0\"`, and `\"t\"`/`\"f\"` values into boolean.",
            "float" => "Coerce to a 64 bit float.",
            "int" => "Coerce to a 64 bit integer.",
            "string" => "Coerce to a string.",
            "timestamp" => "Coerces to a Vector timestamp. [`strptime` specificiers][urls.strptime_specifiers] must be used to parse the string."
          },
          "examples" => [
            {"status" => "int"},
            {"duration" => "float"},
            {"success" => "bool"},
            {"timestamp" => "timestamp|%s"},
            {"timestamp" => "timestamp|%+"},
            {"timestamp" => "timestamp|%F"},
            {"timestamp" => "timestamp|%a %b %e %T %Y"}
          ],
          "description" => "A definition of log field type conversions. They key is the log field name and the value is the type. [`strptime` specifiers][urls.strptime_specifiers] are supported for the `timestamp` type.",
          "null" => false,
          "simple" => true,
          "type" => "string"
        }

      @options.types =
        Option.new({
          "name" => "types",
          "common" => true,
          "description" => "Key/Value pairs representing mapped log field types.",
          "null" => true,
          "options" => {"`[field-name]`" => wildcard_option},
          "type" => "table"
        })
    end
  end

  def description
    @desription ||= "Accepts #{input_types.to_sentence} events and allows you to #{allow_you_to_description}."
  end
end