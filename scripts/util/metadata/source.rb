#encoding: utf-8

require_relative "component"
require_relative "field"

class Source < Component
  attr_reader :delivery_guarantee,
    :fields,
    :output_types,
    :through_description

  def initialize(hash)
    super(hash)

    # Init

    @delivery_guarantee = hash.fetch("delivery_guarantee")
    @fields = Field.build_struct(hash["fields"] || {})
    @output_types = hash.fetch("output_types")
    @through_description = hash.fetch("through_description")

    # delivery_guarantee

    if !DELIVERY_GUARANTEES.include?(@delivery_guarantee)
      raise(
        "Source #delivery_guarantee must be one of: " +
          "#{DELIVERY_GUARANTEES.inspect}, got #{@delivery_guarantee.inspect}"
      )
    end

    # through_description

    if @through_description.strip[-1] == "."
      raise("#{self.class.name}#through_description cannot not end with a period")
    end
  end

  def fields_list
    @fields_list ||= fields.to_h.values.sort
  end
end