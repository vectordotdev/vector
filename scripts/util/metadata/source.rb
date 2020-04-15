#encoding: utf-8

require "ostruct"

require_relative "component"
require_relative "field"
require_relative "fields"

class Source < Component
  attr_reader :delivery_guarantee,
    :fields,
    :noun,
    :output_types,
    :link_name,
    :strategies,
    :through_description

  def initialize(hash)
    super(hash)

    # Init

    @delivery_guarantee = hash.fetch("delivery_guarantee")
    @fields = OpenStruct.new
    @noun = hash.fetch("noun")
    @output_types = hash.fetch("output_types")
    @strategies = hash["strategies"] || []
    @through_description = hash.fetch("through_description")

    # fields

    fields = hash["fields"] || {}

    if fields["log"]
      @fields.log = Fields.new(fields["log"])
    end

    if fields["metric"]
      @fields.metric = Fields.new(fields["metric"])
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

  def collects?
    function_category == "collects"
  end

  def log_fields_list
    @log_fields_list ||= log_fields.to_h.values.sort
  end

  def short_description
    @short_description ||= "Ingests data through #{through_description} and outputs #{output_types.to_sentence} events."
  end

  def to_h
    super.merge(
      noun: noun,
      output_types: output_types,
      through_description: through_description.remove_markdown_links
    )
  end
end
