require_relative "option"
require_relative "sink"

class BatchingSink < Sink
  attr_reader :batch_size,
    :batch_timeout,
    :rate_limit_duration,
    :rate_limit_num,
    :retry_attempts,
    :retry_backoff_secs,
    :request_in_flight_limit,
    :request_timeout_secs

  def initialize(hash)
    super(hash)

    @batch_size = hash.fetch("batch_size")
    @batch_timeout = hash.fetch("batch_timeout")
    @rate_limit_duration = hash.fetch("rate_limit_duration")
    @rate_limit_num = hash.fetch("rate_limit_num")
    @retry_attempts = hash.fetch("retry_attempts")
    @retry_backoff_secs = hash.fetch("retry_backoff_secs")
    @request_in_flight_limit = hash.fetch("request_in_flight_limit")
    @request_timeout_secs = hash.fetch("request_timeout_secs")

    # Common options - batching

    @options.batch_size = Option.new({
      "name" => "batch_size",
      "category" => "Batching",
      "default" => @batch_size,
      "description" => "The maximum size of a batch before it is flushed.",
      "null" => false,
      "type" => "int",
      "unit" => "bytes"
    })

    @options.batch_timeout = Option.new({
      "name" => "batch_timeout",
      "category" => "Batching",
      "default" => @batch_timeout,
      "description" => "The maximum age of a batch before it is flushed.",
      "null" => false,
      "type" => "int",
      "unit" => "bytes"
    })

    # Common options - requests

    @options.request_in_flight_limit = Option.new({
      "name" => "request_in_flight_limit",
      "category" => "Requests",
      "default" => @request_in_flight_limit,
      "description" => "The maximum number of in-flight requests allowed at any given time.",
      "null" => false,
      "type" => "int"
    })

    @options.request_timeout_secs = Option.new({
      "name" => "request_timeout_secs",
      "category" => "Requests",
      "default" => @request_timeout_secs,
      "description" => "The maximum time a request can take before being aborted.",
      "null" => false,
      "type" => "int",
      "unit" => "seconds"
    })

    # Common options - rate limiting

    @options.rate_limit_duration = Option.new({
      "name" => "rate_limit_duration",
      "category" => "Requests",
      "default" => @rate_limit_duration,
      "description" => "The window used for the `request_rate_limit_num` option",
      "null" => false,
      "type" => "int",
      "unit" => "seconds"
    })

    @options.rate_limit_num = Option.new({
      "name" => "rate_limit_num",
      "category" => "Requests",
      "default" => @rate_limit_num,
      "description" => "The maximum number of requests allowed within the `rate_limit_duration` window.",
      "null" => false,
      "type" => "int"
    })

    # Common options - Retries

    @options.retry_attempts = Option.new({
      "name" => "retry_attempts",
      "category" => "Requests",
      "default" => @retry_attempts,
      "description" => "The maximum number of retries to make for failed requests.",
      "null" => false,
      "type" => "int"
    })

    @options.retry_backoff_secs = Option.new({
      "name" => "retry_backoff_secs",
      "category" => "Requests",
      "default" => @retry_attempts,
      "description" => "The amount of time to wait before attempting a failed request again.",
      "null" => false,
      "type" => "int",
      "unit" => "seconds"
    })

    # Common options - Buffer

    # @options.buffer.* = Option.new({
    #   "category" => "Requests",
    #   "default" => @retry_attempts,
    #   "description" => "A table that configures the sink specific buffer. See the [`*.buffer` document][sink_buffers].",
    #   "null" => false,
    #   "type" => "table"
    # })

    # Common sections

    hash["sections"] ||= []

    if batch_timeout <= 5
      body = "By default, the `#{@name}` sink flushes every #{@batch_timeout} seconds to ensure data is available quickly. This can be changed by adjusting the `batch_timeout` and `batch_size` options."

      if service_limits_url
        body << "Keep in mind that the underlying service will only accept payloads up to [a maximum size and a maximum frequency](#{service_limits_url})."
      end

      @sections << Section.new({
        "title" => "Batching",
        "body" => body
      })
    else
      @sections << Section.new({
        "title" => "Batching",
        "body" =>
          <<~EOF
          By default, the `#{@name}` sink flushes every #{@batch_timeout} seconds to optimize cost and bandwidth. This is generally desired for the underlying service. This can be changed by adjusting the `batch_timeout` and `batch_size` options. Keep in mind that lowering this could have adverse effects with service stability and cost.
          EOF
      })
    end

    @sections << Section.new({
      "title" => "Rate Limiting",
      "body" =>
        <<~EOF
        Vector offers a few levers to control the rate and volume of requests. Start with the `rate_limit_duration` and `rate_limit_num` options to ensure Vector does not exceed the specified number of requests in the specified window. You can further control the pace at which this window is saturated with the `request_in_flight_limit` option, which will guarantee no more than the specified number of requests are in-flight at any given time.

          Please note, Vector's defaults are carefully chosen and it should be rare that you need to adjust these.
        EOF
    })

    @sections << Section.new({
      "title" => "Retry Policy",
      "body" =>
        <<~EOF
        Vector will retry failed requests (status == `429`, >= `500`, and != `501`). Other responses will not be retried. You can control the number of retry attempts and backoff rate with the `retry_attempts` and `retry_backoff_secs` options.
        EOF
    })

    @sections << Section.new({
      "title" => "Timeouts",
      "body" =>
        <<~EOF
        The default `request_timeout_secs` option is based on the underlying timeout. It is highly recommended that you do not lower this below the service's timeout, as this could create orphaned requests and pile on retries.
        EOF
    })
  end

  def type
    "sink"
  end
end