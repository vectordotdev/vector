#encoding: utf-8

require_relative "component"
require_relative "output"

class Transform < Component
  attr_reader :allow_you_to_description,
    :input_types,
    :output,
    :output_types

  def initialize(hash)
    super(hash)

    # init

    @allow_you_to_description = hash.fetch("allow_you_to_description")
    @input_types = hash.fetch("input_types")
    @output = OpenStruct.new
    @output_types = hash.fetch("output_types")

    # checks

    if @allow_you_to_description.strip[-1] == "."
      raise("#{self.class.name}#allow_you_to_description cannot not end with a period")
    end

    # output

    output = hash["output"] || {}

    if output["log"]
      @output.log = Output.new(output["log"])
    end

    if output["metric"]
      @output.metric = Output.new(output["metric"])
    end
  end

  def description
    @desription ||= "Accepts #{input_types.to_sentence} events and allows you to #{allow_you_to_description}."
  end
end
