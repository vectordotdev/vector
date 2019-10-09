#encoding: utf-8

require_relative "component"

class Sink < Component
  EGRESS_METHODS = ["batching", "exposing", "streaming"]

  attr_reader :buffer,
    :delivery_guarantee,
    :egress_method,
    :input_types,
    :healthcheck,
    :service_limits_short_link,
    :service_provider,
    :tls,
    :write_to_description

  def initialize(hash)
    @type = "sink"
    super(hash)

    @buffer = hash.fetch("buffer")
    @delivery_guarantee = hash.fetch("delivery_guarantee")
    @egress_method = hash.fetch("egress_method")
    @healthcheck = hash.fetch("healthcheck")
    @input_types = hash.fetch("input_types")
    @service_limits_short_link = hash["service_limits_short_link"]
    @service_provider = hash["service_provider"]
    tls_options = hash["tls_options"]
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

    # Healthcheck option

    @options.healthcheck = Option.new({
      "name" => "healthcheck",
      "default" => true,
      "description" => "Enables/disables the sink healthcheck upon start.",
      "null" => false,
      "type" => "bool"
    })

    # Endpoint option

    if service_provider == "AWS"
      @options.hostname = Option.new({
        "name" => "endpoint",
        "examples" => ["127.0.0.0:5000"],
        "description" => "Custom endpoint for use with AWS-compatible services.",
        "null" => true,
        "type" => "string"
      })
    end

    if buffer?
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
        "description" => "The maximum size of the buffer on the disk.",
        "examples" => [104900000],
        "null" => true,
        "relevant_when" => {"type" => "disk"},
        "type" => "int",
        "unit" => "bytes"
      }

      buffer_options["num_items"] = {
        "description" => "The maximum number of [events][docs.event] allowed in the buffer.",
        "default" => 500,
        "null" => true,
        "relevant_when" => {"type" => "memory"},
        "type" => "int",
        "unit" => "events"
      }

      buffer_option = Option.new({
        "name" => "buffer",
        "description" => "Configures the sink specific buffer.",
        "options" => buffer_options,
        "null" => true,
        "type" => "table"
      })

      @options.buffer = buffer_option
    end

    # resources

    if @service_limits_short_link
      @resources << OpenStruct.new({"name" => "Service Limits", "short_link" => @service_limits_short_link})
    end

    if tls_options
      # Standard TLS options
      options = {}

      if tls_options.include?("+enabled")
        options["enabled"] = {
          "type" => "bool",
          "null" => true,
          "default" => false,
          "description" => "Enable TLS during connections to the remote."
        }
      end

      options["ca_path"] = {
        "type" => "string",
        "null" => true,
        "examples" => ["/path/to/certificate_authority.crt"],
        "description" => "Absolute path to an additional CA certificate file, in DER or PEM format (X.509)."
      }

      options["crt_path"] = {
        "type" => "string",
        "null" => true,
        "examples" => ["/path/to/host_certificate.crt"],
        "description" => "Absolute path to a certificate file used to identify this connection, in DER or PEM format (X.509) or PKCS#12. If this is set and is not a PKCS#12 archive, `key_path` must also be set."
      }

      options["key_path"] = {
        "type" => "string",
        "null" => true,
        "examples" => ["/path/to/host_certificate.key"],
        "description" => "Absolute path to a certificate key file used to identify this connection, in DER or PEM format (PKCS#8). If this is set, `crt_path` must also be set."
      }

      options["key_pass"] = {
        "type" => "string",
        "null" => true,
        "examples" => ["PassWord1"],
        "description" => "Pass phrase used to unlock the encrypted key file. This has no effect unless `key_pass` above is set."
      }

      if !tls_options.include?("-verify")
        options["verify_certificate"] = {
          "type" => "bool",
          "null" => true,
          "default" => true,
          "description" => "If `true` (the default), Vector will validate the TLS certificate of the remote host. Do NOT set this to `false` unless you understand the risks of not verifying the remote certificate."
        }

        options["verify_hostname"] = {
          "type" => "bool",
          "null" => true,
          "default" => true,
          "description" => "If `true` (the default), Vector will validate the configured remote host name against the remote host's TLS certificate. Do NOT set this to `false` unless you understand the risks of not verifying the remote hostname."
        }
      end

      @options.tls = Option.new({
        "name" => "tls",
        "description" => "Configures the TLS options for connections from this sink.",
        "options" => options,
        "null" => true,
        "type" => "table"
      })
    end
  end

  def batching?
    egress_method == "batching"
  end

  def buffer?
    buffer == true
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
