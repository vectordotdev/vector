require_relative "component"
require_relative "field"
require_relative "output"

class Source < Component
  attr_reader :delivery_guarantee,
    :outputs,
    :through_description

  def initialize(hash)
    super(hash)

    # Init
    @delivery_guarantee = hash.fetch("delivery_guarantee")
    outputs_hashes = hash["outputs"] || []
    @through_description = hash.fetch("through_description")

    # delivery_guarantee

    if !DELIVERY_GUARANTEES.include?(@delivery_guarantee)
      raise(
        "Source #delivery_guarantee must be one of: " +
          "#{DELIVERY_GUARANTEES.inspect}, got #{@delivery_guarantee.inspect}"
      )
    end

    # outputs

    @outputs = outputs_hashes.collect do |output_hash|
      Output.new(output_hash)
    end

    # through_description

    if @through_description.strip[-1] == "."
      raise("#{self.class.name}#through_description cannot not end with a period")
    end
  end

  def output_types
    ["log"]
  end
end