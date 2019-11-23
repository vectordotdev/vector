#encoding: utf-8

require_relative "option"

class Component
  DELIVERY_GUARANTEES = ["at_least_once", "best_effort"]
  EVENT_TYPES = ["log", "metric"]

  include Comparable

  attr_reader :beta,
    :common,
    :function_category,
    :id,
    :name,
    :options,
    :resources,
    :type

  def initialize(hash)
    @beta = hash["beta"] == true
    @common = hash["common"] == true
    @function_category = hash.fetch("function_category")
    @name = hash.fetch("name")
    @type ||= self.class.name.downcase
    @id = "#{@name}_#{@type}"
    @options = OpenStruct.new()

    (hash["options"] || {}).each do |option_name, option_hash|
      option = Option.new(
        option_hash.merge({"name" => option_name}
      ))

      @options.send("#{option_name}=", option)
    end

    @resources = (hash.delete("resources") || []).collect do |resource_hash|
      OpenStruct.new(resource_hash)
    end

    @options.type =
      Option.new({
        "name" => "type",
        "description" => "The component type. This is a required field that tells Vector which component to use. The value _must_ be `#{name}`.",
        "enum" => {
          name => "The name of this component"
        },
        "null" => false,
        "type" => "string"
      })

    if type != "source"
      @options.inputs =
        Option.new({
          "name" => "inputs",
          "description" => "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [configuration][docs.configuration] for more info.",
          "examples" => [["my-source-id"]],
          "null" => false,
          "type" => "[string]"
        })
    end
  end

  def <=>(other)
    name <=> other.name
  end

  def advanced_relevant?
    options_list.any?(&:advanced?)
  end

  def beta?
    beta == true
  end

  def common?
    common == true
  end

  def context_options
    options_list.select(&:context?)
  end

  def event_types
    types = []

    if respond_to?(:input_types)
      types += input_types
    end

    if respond_to?(:output_types)
      types += output_types
    end

    types.uniq
  end

  def options_list
    @options_list ||= options.to_h.values.sort
  end

  def partition_options
    options_list.select(&:partition_key?)
  end

  def sink?
    type == "sink"
  end

  def source?
    type == "source"
  end

  def specific_options_list
    options_list.select do |option|
      !["type", "inputs"].include?(option.name)
    end
  end

  def status
    beta? ? "beta" : "prod-ready"
  end

  def templateable_options
    options_list.select(&:templateable?)
  end

  def to_h
    {
      beta: beta?,
      delivery_guarantee: (respond_to?(:delivery_guarantee, true) ? delivery_guarantee : nil),
      event_types: event_types,
      function_category: (respond_to?(:function_category, true) ? function_category : nil),
      id: id,
      name: name,
      service_provider: (respond_to?(:service_provider, true) ? service_provider : nil),
      status: status,
      type: type
    }
  end

  def transform?
    type == "transform"
  end
end