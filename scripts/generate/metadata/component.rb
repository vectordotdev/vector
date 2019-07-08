#encoding: utf-8

require_relative "option"

class Component
  DELIVERY_GUARANTEES = ["at_least_once", "best_effort"]
  EVENT_TYPES = ["log", "metric"]

  include Comparable

  attr_reader :beta,
    :id,
    :name,
    :options,
    :resources,
    :type

  attr_accessor :alternatives

  def initialize(hash)
    @alternatives = []
    @beta = hash["beta"] == true
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

    @options.type = Option.new({
        "name" => "type",
        "description" => "The component type",
        "enum" => [name],
        "null" => false,
        "type" => "string"
      })

    if type != "source"
      @options.inputs = Option.new({
        "name" => "inputs",
        "description" => "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.",
        "examples" => [["my-source-id"]],
        "null" => false,
        "type" => "[string]"
      })
    end
  end

  def <=>(other)
    name <=> other.name
  end

  def beta?
    beta == true
  end

  def partition_options
    options.to_h.values.sort.select(&:partition_key?)
  end

  def sink?
    type == "sink"
  end

  def source?
    type == "source"
  end

  def transform?
    type == "transform"
  end
end