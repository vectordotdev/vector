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

    if @allow_you_to_description.strip[-1] == "."
      raise("#{self.class.name}#allow_you_to_description cannot not end with a period")
    end

    if (invalid_types = @input_types - EVENT_TYPES) != []
      raise("#{self.class.name}#input_types contains invalid values: #{invalid_types.inspect}")
    end

    if (invalid_types = @output_types - EVENT_TYPES) != []
      raise("#{self.class.name}#output_types contains invalid values: #{invalid_types.inspect}")
    end
  end
end