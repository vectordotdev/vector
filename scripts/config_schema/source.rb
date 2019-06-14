require_relative "component"
require_relative "field"

class Source < Component
  attr_reader :delivery_guarantee,
    :outputs,
    :through_description

  def initialize(hash)
    super(hash)

    # Init

    @delivery_guarantee = hash.fetch("delivery_guarantee")
    outputs_hash = hash.fetch("outputs")
    @through_description = hash.fetch("through_description")

    # delivery_guarantee

    if !DELIVERY_GUARANTEES.include?(@delivery_guarantee)
      raise(
        "Source #delivery_guarantee must be one of: " +
          "#{DELIVERY_GUARANTEES.inspect}, got #{@delivery_guarantee.inspect}"
      )
    end

    # outputs

    @outputs = OpenStruct.new()

    outputs_hash.each do |type, schema|
      if !EVENT_TYPES.include?(type)
        raise("Event type #{type} is not supported, must be one of: #{EVENT_TYPES.inspect}")
      end

      fields = OpenStruct.new()

      schema.collect do |field_name, field_hash|
        field = Field.new(field_hash.merge({"name" => field_name}))
        fields.send("#{field_name}=", field)
      end

      @outputs.send("#{type}=", fields)
    end

    # through_description

    if @through_description.strip[-1] == "."
      raise("#{self.class.name}#through_description cannot not end with a period")
    end
  end

  def output_types
    @outputs.to_h.keys
  end
end