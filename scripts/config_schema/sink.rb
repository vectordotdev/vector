require_relative "component"
require_relative "section"

class Sink < Component
  WRITE_STYLES = ["batching", "streaming"]

  attr_reader :delivery_guarantee,
    :input_types,
    :output,
    :service_limits_url,
    :service_provider,
    :write_style,
    :write_to_description

  def initialize(hash)
    super(hash)

    @delivery_guarantee = hash.fetch("delivery_guarantee")
    @input_types = hash.fetch("input_types")
    @output = hash["output"]
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

    # Output

    if @output.nil?
      if streaming?
        if @options.respond_to?("encoding")
          @output = "The `#{name}` sink streams events in a real-time fashion. Each event is encoded as dictated by the `encoding` option. See [Encoding](#encoding) for more info."
        else
          @output = "The `#{name}` sink streams events in a real-time fashion."
        end
      elsif batching?
        if @options.respond_to?("encoding")
          @output = "The `#{name}` sink batches and flushes events over an configurable interval. Each event is encoded as dictated by the `encoding` option. See [Encoding](#encoding) for more info."
        else
          @output = "The `#{name}` sink batches and flushes events over an configurable interval."
        end
      else
        raise("Unknown write_style: #{@write_style.inspect}")
      end
    end

    # Common sections

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
        Vector will perform a simple health check against the underlying service before initializing this sink. This ensures that the service is reachable. You can require this check with the `--require-healthy` flag upon [starting][starting] Vector.
        EOF
      end

    @sections << Section.new({
      "title" => "Health Checks",
      "body" => healthcheck_body
    })

    @sections.sort!

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

  def streaming?
    write_style == "streaming"
  end
end