require_relative "component"
require_relative "output"
require_relative "section"

class Sink < Component
  WRITE_STYLES = ["batching", "streaming"]

  attr_reader :delivery_guarantee,
    :input_types,
    :outputs,
    :service_limits_url,
    :service_provider,
    :write_style,
    :write_to_description

  def initialize(hash)
    super(hash)

    @delivery_guarantee = hash.fetch("delivery_guarantee")
    @input_types = hash.fetch("input_types")
    outputs_hashes = hash["outputs"] || []
    @service_limits_url = hash["service_limits_url"]
    @service_provider = hash["service_provider"]
    @write_style = hash.fetch("write_style")
    @write_to_description = hash.fetch("write_to_description")

    if (invalid_types = @input_types - EVENT_TYPES) != []
      raise("#{self.class.name}#input_types contains invalid values: #{invalid_types.inspect}")
    end

    if !WRITE_STYLES.include?(@write_style)
      raise("#{self.class.name}#write_style is invalid (#{@write_style.inspect}, must be one of: #{WRITE_STYLES.inspect}")
    end

    if @write_to_description.strip[-1] == "."
      raise("#{self.class.name}#write_to_description cannot not end with a period")
    end

    # options

    buffer_options = {}

    buffer_options["type"] = {
      "description" => "The buffer's type / location. `disk` buffers are persistent and will be retained between restarts.",
      "enum" => ["memory", "disk"],
      "default" => "memory",
      "type" => "string"
    }

    buffer_options["when_full"] = {
      "description" => "The behavior when the buffer becomes full.",
      "enum" => ["block", "drop_newest"],
      "default" => "block",
      "type" => "string"
    }

    buffer_options["max_size"] = {
      "description" => "Only relevant when `type` is `disk`. The maximum size of the buffer on the disk.",
      "examples" => [104900000],
      "type" => "int"
    }

    buffer_options["num_items"] = {
      "description" => "Only relevant when `type` is `memory`. The maximum number of [events][event] allowed in the buffer.",
      "default" => 500,
      "type" => "int"
    }

    buffer_option = Option.new({
      "name" => "buffer",
      "description" => "Configures the sink specific buffer.",
      "options" => buffer_options,
      "type" => "table"
    })

    @options.buffer = buffer_option

    # outputs

    @outputs = outputs_hashes.collect do |output_hash|
      Output.new(output_hash)
    end

    # sections

    if @service_provider == "AWS"
      @sections << Section.new({
        "title" => "Authentication",
        "body" =>
          <<~EOF
          Vector checks for AWS credentials in the following order:

          1. Environment variables `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY`.
          ​2. [`credential_process` command][aws_credential_process] in the AWS config file, usually located at `~/.aws/config`.
          ​3. [AWS credentials file][aws_credentials_file], usually located at `~/.aws/credentials`.
          4. ​[IAM instance profile][iam_instance_profile]. Will only work if running on an EC2 instance with an instance profile/role.

          If credentials are not found the [healtcheck](#healthchecks) will fail and an error will be [logged][monitoring_logs].

          #### Obtaining an access key

          In general, we recommend using instance profiles/roles whenever possible. In cases where this is not possible you can generate an AWS access key for any user within your AWS account. AWS provides a [detailed guide][aws_access_keys] on how to do this.
          EOF
      })
    end

    @sections << Section.new({
      "title" => "Buffers",
      "body" =>
        <<~EOF
        Vector couples [buffers](buffer.md) with each sink, this offers [a number of advantages](buffer.md#coupled-with-sinks) over a single shared global buffer. In general, you should [configure your sink's buffer](buffer.md) to exceed the `batch_size`. This is especially true when using [on-disk](buffer.md#in-memory-or-on-disk) buffers, as it ensures data is not lost in the event of restarts.

        #### Buffer Types

        The `buffer.type` option allows you to control buffer resource usage:

        | Type | Description |
        | :--- | :---------- |
        | `memory` | Pros: Fast. Cons: Not persisted across restarts. Possible data loss in the event of a crash. Uses more memory. |
        | `disk` | Pros: Persisted across restarts, durable. Uses much less memory. Cons: Slower, see below. |

        #### Buffer Overflow

        The `buffer.when_full` option allows you to control the behavior when the buffer overflows:

        | Type | Description |
        | :--- | :---------- |
        | `block` | Applies back pressure until the buffer makes room. This will help to prevent data loss but will cause data to pile up on the edge. |
        | `drop_newest` | Drops new data as it's received. This data is lost. This should be used when performance is the highest priority. |
        EOF
      })

    if @options.respond_to?("compression")
      rows = @options.compression.enum.collect do |compression|
        "| `#{compression}` | #{compression_description(compression)} |"
      end

      @sections << Section.new({
        "title" => "Compression",
        "body" =>
          <<~EOF
          The `#{@name}` sink compresses payloads before flushing. This helps to reduce the payload size, ultimately reducing bandwidth and cost. This is controlled via the `compression` option. Each compression type is described in more detail below:

          | Compression | Description |
          | :---------- | :---------- |
          #{rows.join("\n")}
          EOF
      })
    end

    if @options.respond_to?("encoding")
      rows = @options.encoding.enum.collect do |encoding|
        "| `#{encoding}` | #{encoding_description(encoding)} |"
      end

      @sections << Section.new({
        "title" => "Encodings",
        "body" =>
          <<~EOF
          The `#{@name}` sink encodes events before flushing. This is controlled via the `encoding` option. Each encoding type is described in more detail below:

          | Encoding | Description |
          | :------- | :---------- |
          #{rows.join("\n")}
          EOF
      })
    end

    healthcheck_body =
      if @options.respond_to?("healthcheck_uri")
        <<~EOF
        If the `healthcheck_uri` option is provided, Vector will issue a request to this URI to determine the service's health before initializing the sink. This ensures that the service is reachable. You can require this check with the `--require-healthy` flag upon [starting][starting] Vector.
        EOF
      else
        <<~EOF
        Vector will perform a simple health check against the underlying service before initializing this sink. This ensures that the service is reachable. You can require this check with the `--require-healthy` flag upon [starting][starting] Vector:

        ```bash
        vector --config /etc/vector/vector.toml --require-healthy
        ```
        EOF
      end

    @sections << Section.new({
      "title" => "Health Checks",
      "body" => healthcheck_body
    })

    @sections.sort!

    # resources

    if @service_limits_url
      @resources << OpenStruct.new({"name" => "Service Limits", "url" => @service_limits_url})
    end
  end

  def batching?
    write_style == "batching"
  end

  def compression_description(compression)
    case compression
    when "gzip"
      "The payload will be compressed in [Gzip][gzip] format before being sent."
    when "none"
      "The payload will not compressed at all."
    else
      raise("Unknown compression: #{compression.inspect}")
    end
  end

  def encoding_description(compression)
    case compression
    when "json"
      "The payload will be encoded as a single JSON payload."
    when "ndjson"
      "The payload will be encoded in new line delimited JSON payload, each line representing a JSON encoded event."
    when "text"
      "The payload will be encoded as new line delimited text, each line representing the value of the `\"message\"` key."
    when nil
      "The encoding type will be dynamically chosen based on the explicit structuring of the event. If the event has been explicitly structured (parsed, keys added, etc), then it will be encoded in the `json` format. If not, it will be encoded as `text`."
    else
      raise("Unknown compression: #{compression.inspect}")
    end
  end

  def plural_write_verb
    case write_style
    when "batching"
      "batches and flushes"
    when "streaming"
      "streams"
    else
      raise("Unknown write_style: #{write_style.inspect}")
    end
  end

  def streaming?
    write_style == "streaming"
  end

  def write_verb
    case write_style
    when "batching"
      "batch and flush"
    when "streaming"
      "stream"
    else
      raise("Unknown write_style: #{write_style.inspect}")
    end
  end
end