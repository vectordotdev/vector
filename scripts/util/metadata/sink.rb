#encoding: utf-8

require_relative "component"

class Sink < Component
  attr_reader :delivery_guarantee,
    :egress_method,
    :input_types,
    :healthcheck,
    :noun,
    :service_limits_short_link,
    :tls,
    :write_to_description

  def initialize(hash)
    @type = "sink"
    super(hash)

    @delivery_guarantee = hash.fetch("delivery_guarantee")
    @egress_method = hash.fetch("egress_method")
    @healthcheck = hash.fetch("healthcheck")
    @input_types = hash.fetch("input_types")
    @noun = hash.fetch("noun")
    @service_limits_short_link = hash["service_limits_short_link"]
    @write_to_description = hash.fetch("write_to_description")

    if @write_to_description.strip[-1] == "."
      raise("#{self.class.name}#write_to_description cannot not end with a period")
    end
  end

  def batching?
    egress_method == "batching"
  end

  def can_receive_from?(component)
    component.respond_to?(:output_types) &&
      component.output_types.intersection(input_types).any?
  end

  def can_send_to?(component)
    false
  end

  def exposing?
    egress_method == "exposing"
  end

  def healthcheck?
    healthcheck == true
  end

  def plural_write_verb
    case egress_method
    when "batching"
      "batches"
    when "exposing"
      "exposes"
    when "streaming"
      "streams"
    else
      raise("Unhandled egress_method: #{egress_method.inspect}")
    end
  end

  def short_description
    @short_description ||= "#{plural_write_verb.humanize} #{input_types.to_sentence} events to #{write_to_description}."
  end

  def streaming?
    egress_method == "streaming"
  end

  def to_h
    super.merge(
      input_types: input_types,
      noun: noun,
      write_to_description: write_to_description.remove_markdown_links
    )
  end

  def write_verb
    case egress_method
    when "batching"
      "batch and flush"
    when "exposing"
      "expose"
    when "streaming"
      "stream"
    else
      raise("Unhandled egress_method: #{egress_method.inspect}")
    end
  end
end
