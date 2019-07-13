#encoding: utf-8

require_relative "component"
require_relative "example"

class Sink < Component
  EGRESS_METHODS = ["batching", "exposing", "streaming"]

  attr_reader :delivery_guarantee,
    :egress_method,
    :input_types,
    :examples,
    :service_limits_short_link,
    :service_provider,
    :write_to_description

  def initialize(hash)
    @type = "sink"
    super(hash)

    @delivery_guarantee = hash.fetch("delivery_guarantee")
    @egress_method = hash.fetch("egress_method")
    @input_types = hash.fetch("input_types")
    @service_limits_short_link = hash["service_limits_short_link"]
    @service_provider = hash["service_provider"]
    @write_to_description = hash.fetch("write_to_description")

    if (invalid_types = @input_types - EVENT_TYPES) != []
      raise("#{self.class.name}#input_types contains invalid values: #{invalid_types.inspect}")
    end

    if !EGRESS_METHODS.include?(@egress_method)
      raise("#{self.class.name}#egress_method is invalid (#{@egress_method.inspect}, must be one of: #{EGRESS_METHODS.inspect}")
    end

    if @write_to_description.strip[-1] == "."
      raise("#{self.class.name}#write_to_description cannot not end with a period")
    end

    # Hostname option

    if service_provider == "AWS"
      buffer_option = Option.new({
        "name" => "hostname",
        "examples" => ["127.0.0.0:5000"],
        "default" => "<aws-service-hostname>",
        "description" => "Custom hostname to send requests to. Useful for testing.",
        "null" => false,
        "type" => "string"
      })
    end

    # Buffer options

    buffer_options = {}

    buffer_options["type"] = {
      "description" => "The buffer's type / location. `disk` buffers are persistent and will be retained between restarts.",
      "enum" => ["memory", "disk"],
      "default" => "memory",
      "null" => false,
      "type" => "string"
    }

    buffer_options["when_full"] = {
      "description" => "The behavior when the buffer becomes full.",
      "enum" => ["block", "drop_newest"],
      "default" => "block",
      "null" => false,
      "type" => "string"
    }

    buffer_options["max_size"] = {
      "description" => "Only relevant when `type` is `disk`. The maximum size of the buffer on the disk.",
      "examples" => [104900000],
      "null" => true,
      "relevant_when" => {"type" => "disk"},
      "type" => "int"
    }

    buffer_options["num_items"] = {
      "description" => "Only relevant when `type` is `memory`. The maximum number of [events][docs.event] allowed in the buffer.",
      "default" => 500,
      "null" => true,
      "relevant_when" => {"type" => "memory"},
      "type" => "int"
    }

    buffer_option = Option.new({
      "name" => "buffer",
      "description" => "Configures the sink specific buffer.",
      "options" => buffer_options,
      "null" => true,
      "type" => "table"
    })

    @options.buffer = buffer_option

    # examples

    @examples = (hash["examples"] || []).collect do |example_hash|
      Example.new(example_hash)
    end

    # resources

    if @service_limits_short_link
      @resources << OpenStruct.new({"name" => "Service Limits", "short_link" => @service_limits_short_link})
    end
  end

  def batching?
    egress_method == "batching"
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

  def streaming?
    egress_method == "streaming"
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