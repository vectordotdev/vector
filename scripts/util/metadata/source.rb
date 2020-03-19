#encoding: utf-8

require "ostruct"

require_relative "component"
require_relative "field"
require_relative "output"

class Source < Component
  attr_reader :delivery_guarantee,
    :output,
    :output_types,
    :through_description

  def initialize(hash)
    super(hash)

    # Init

    @delivery_guarantee = hash.fetch("delivery_guarantee")
    @output = OpenStruct.new
    @log_fields = (hash["log_fields"] || {}).to_struct_with_name(constructor: Field)
    @output_types = hash.fetch("output_types")
    @through_description = hash.fetch("through_description")

    # output

    output = hash["output"] || {}

    if output["log"]
      @output.log = Output.new(output["log"])
    end

    if output["metric"]
      @output.metric = Output.new(output["metric"])
    end

    # through_description

    if @through_description.strip[-1] == "."
      raise("#{self.class.name}#through_description cannot not end with a period")
    end
  end

  def can_receive_from?(component)
    false
  end

  def can_send_to?(component)
    component.respond_to?(:input_types) &&
      component.input_types.intersection(output_types).any?
  end

  def description
    @description ||= "Ingests data through #{through_description} and outputs #{output_types.to_sentence} events."
  end

  def log_fields_list
    @log_fields_list ||= log_fields.to_h.values.sort
  end
end
