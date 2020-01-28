#encoding: utf-8

require_relative "component"
require_relative "output"

class Sink < Component
  EGRESS_METHODS = ["batching", "exposing", "streaming"].freeze

  attr_reader :buffer,
    :delivery_guarantee,
    :egress_method,
    :input_types,
    :healthcheck,
    :output,
    :service_limits_short_link,
    :service_providers,
    :tls,
    :write_to_description

  def initialize(hash)
    @type = "sink"
    super(hash)

    @buffer = hash.fetch("buffer")
    compressions = hash["compressions"]
    @delivery_guarantee = hash.fetch("delivery_guarantee")
    @egress_method = hash.fetch("egress_method")
    encodings = hash["encodings"]
    @healthcheck = hash.fetch("healthcheck")
    @input_types = hash.fetch("input_types")
    @service_limits_short_link = hash["service_limits_short_link"]
    @service_providers = hash["service_providers"] || []
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

    # output

    if hash["output"]
      @output = Output.new(hash["output"])
    end

    # Healthcheck option

    @options.healthcheck =
      Option.new({
        "name" => "healthcheck",
        "default" => true,
        "description" => "Enables/disables the sink healthcheck upon start.",
        "null" => false,
        "type" => "bool"
      })

    # Compression option

    if !compressions.nil?
      enum =
        compressions.reduce({}) do |enum, compression|
          enum[compression] = compression_description(compression)
          enum
        end

      @options.hostname =
        Option.new({
          "name" => "compression",
          "category" => "requests",
          "default" => compressions.include?("none") ? nil : compressions.first,
          "enum" => enum,
          "description" => "The compression strategy used to compress the encoded event data before outputting.",
          "null" => !compressions.include?("none"),
          "simple" => true,
          "type" => "string"
        })
    end

    # Encoding option

    if encodings
      enum =
        encodings.reduce({}) do |enum, encoding|
          enum[encoding] = encoding_description(encoding)
          enum
        end

      @options.hostname =
        Option.new({
          "name" => "encoding",
          "category" => "requests",
          "enum" => enum,
          "description" => "The encoding format used to serialize the events before outputting.",
          "null" => false,
          "simple" => true,
          "type" => "string"
        })
    end

    # AWS

    if service_provider?("AWS")
      @env_vars.AWS_ACCESS_KEY_ID =
        Option.new({
          "description" => "Used for AWS authentication when communicating with AWS services. See relevant [AWS components][pages.aws_components] for more info.",
          "examples" => ["AKIAIOSFODNN7EXAMPLE"],
          "name" => "AWS_ACCESS_KEY_ID",
          "null" => true,
          "type" => "string"
        })

      @env_vars.AWS_SECRET_ACCESS_KEY =
        Option.new({
          "description" => "Used for AWS authentication when communicating with AWS services. See relevant [AWS components][pages.aws_components] for more info.",
          "examples" => ["wJalrXUtnFEMI/K7MDENG/FD2F4GJ"],
          "name" => "AWS_SECRET_ACCESS_KEY",
          "null" => true,
          "type" => "string"
        })

      @options.endpoint =
        Option.new({
          "description" => "Custom endpoint for use with AWS-compatible services. Providing a value for this option will make `region` moot.",
          "examples" => ["127.0.0.0:5000"],
          "name" => "endpoint",
          "null" => true,
          "required" => false,
          "type" => "string"
        })

      @options.region =
        Option.new({
          "common" => only_service_provider?("AWS"),
          "description" => "The [AWS region][urls.aws_regions] of the target service. If `endpoint` is provided it will override this value since the endpoint includes the region.",
          "examples" => ["us-east-1"],
          "name" => "region",
          "null" => true,
          "required" => only_service_provider?("AWS"),
          "type" => "string"
        })
    end

    if buffer?
      # Buffer options

      buffer_options = {}

      buffer_options["type"] =
        {
          "description" => "The buffer's type / location. `disk` buffers are persistent and will be retained between restarts.",
          "enum" => {
            "memory" => "Stores the sink's buffer in memory. This is more performant (~3x), but less durable. Data will be lost if Vector is restarted abruptly.",
            "disk" => "Stores the sink's buffer on disk. This is less performance (~3x),  but durable. Data will not be lost between restarts."
          },
          "default" => "memory",
          "null" => false,
          "type" => "string"
        }

      buffer_options["when_full"] =
        {
          "description" => "The behavior when the buffer becomes full.",
          "enum" => {
            "block" => "Applies back pressure when the buffer is full. This prevents data loss, but will cause data to pile up on the edge.",
            "drop_newest"  => "Drops new data as it's received. This data is lost. This should be used when performance is the highest priority."
          },
          "default" => "block",
          "null" => false,
          "type" => "string"
        }

      buffer_options["max_size"] =
        {
          "description" => "The maximum size of the buffer on the disk.",
          "examples" => [104900000],
          "null" => true,
          "relevant_when" => {"type" => "disk"},
          "type" => "int",
          "unit" => "bytes"
        }

      buffer_options["max_events"] =
        {
          "description" => "The maximum number of [events][docs.data-model#event] allowed in the buffer.",
          "default" => 500,
          "null" => true,
          "relevant_when" => {"type" => "memory"},
          "type" => "int",
          "unit" => "events"
        }

      buffer_option =
        Option.new({
          "name" => "buffer",
          "description" => "Configures the sink buffer behavior.",
          "options" => buffer_options,
          "null" => false,
          "type" => "table"
        })

      @options.buffer = buffer_option
    end

    # resources

    if @service_limits_short_link
      @resources << OpenStruct.new({"name" => "Service Limits", "short_link" => @service_limits_short_link})
    end

    # An empty array means TLS options are supported
    if !tls_options.nil?
      # Standard TLS options
      options = {}

      if tls_options.include?("+enabled")
        options["enabled"] =
          {
            "type" => "bool",
            "null" => true,
            "default" => false,
            "description" => "Enable TLS during connections to the remote."
          }
      end

      options["ca_path"] =
        {
          "type" => "string",
          "null" => true,
          "examples" => ["/path/to/certificate_authority.crt"],
          "description" => "Absolute path to an additional CA certificate file, in DER or PEM format (X.509)."
        }

      options["crt_path"] =
        {
          "type" => "string",
          "null" => true,
          "examples" => ["/path/to/host_certificate.crt"],
          "description" => "Absolute path to a certificate file used to identify this connection, in DER or PEM format (X.509) or PKCS#12. If this is set and is not a PKCS#12 archive, `key_path` must also be set."
        }

      options["key_path"] =
        {
          "type" => "string",
          "null" => true,
          "examples" => ["/path/to/host_certificate.key"],
          "description" => "Absolute path to a certificate key file used to identify this connection, in DER or PEM format (PKCS#8). If this is set, `crt_path` must also be set."
        }

      options["key_pass"] =
        {
          "type" => "string",
          "null" => true,
          "examples" => ["PassWord1"],
          "description" => "Pass phrase used to unlock the encrypted key file. This has no effect unless `key_pass` above is set."
        }

      if !tls_options.include?("-verify")
        options["verify_certificate"] =
          {
            "type" => "bool",
            "null" => true,
            "default" => true,
            "description" => "If `true` (the default), Vector will validate the TLS certificate of the remote host. Do NOT set this to `false` unless you understand the risks of not verifying the remote certificate."
          }

        options["verify_hostname"] =
          {
            "type" => "bool",
            "null" => true,
            "default" => true,
            "description" => "If `true` (the default), Vector will validate the configured remote host name against the remote host's TLS certificate. Do NOT set this to `false` unless you understand the risks of not verifying the remote hostname."
          }
      end

      @options.tls =
        Option.new({
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

  def description
    @description ||= "#{plural_write_verb.humanize} #{input_types.to_sentence} events to #{write_to_description}."
  end

  def exposing?
    egress_method == "exposing"
  end

  def healthcheck?
    healthcheck == true
  end

  def only_service_provider?(provider_name)
    service_providers.length == 1 && service_provider?(provider_name)
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

  def service_provider?(provider_name)
    service_providers.collect(&:downcase).include?(provider_name.downcase)
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

  private
    def compression_description(compression)
      case compression
      when "gzip"
        "The payload will be compressed in [Gzip][urls.gzip] format before being sent."
      when "none"
        "The payload will not compressed at all."
      else
        raise("Unhandled compression: #{compression.inspect}")
      end
    end

    def encoding_description(encoding)
      case encoding
      when "json"
        "Each event is encoded into JSON and the payload is represented as a JSON array."
      when "ndjson"
        "Each event is encoded into JSON and the payload is new line delimited."
      when "text"
        "Each event is encoded into text via the `message` key and the payload is new line delimited."
      else
        raise("Unhandled encoding: #{encoding.inspect}")
      end
    end
end
