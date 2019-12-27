#encoding: utf-8

require_relative "option"
require_relative "sink"

class BatchingSink < Sink
  attr_reader :batch_size,
    :batch_timeout,
    :request_in_flight_limit,
    :request_rate_limit_duration,
    :request_rate_limit_num,
    :request_retry_attempts,
    :request_retry_backoff_secs,
    :request_timeout_secs

  def initialize(hash)
    super(hash)

    batch_is_simple = hash["batch_is_simple"] == true
    @batch_size = hash.fetch("batch_size")
    @batch_timeout = hash.fetch("batch_timeout")
    @request_in_flight_limit = hash.fetch("request_in_flight_limit")
    @request_rate_limit_duration_secs = hash.fetch("request_rate_limit_duration_secs")
    @request_rate_limit_num = hash.fetch("request_rate_limit_num")
    @request_retry_attempts = hash.fetch("request_retry_attempts")
    @request_retry_backoff_secs = hash.fetch("request_retry_backoff_secs")
    @request_timeout_secs = hash.fetch("request_timeout_secs")

    # Common options - batching

    @options.batch_size =
      Option.new({
        "name" => "batch_size",
        "category" => "Batching",
        "default" => @batch_size,
        "description" => "The maximum size of a batch before it is flushed.",
        "null" => false,
        "simple" => batch_is_simple,
        "type" => "int",
        "unit" => "bytes"
      })

    @options.batch_timeout =
      Option.new({
        "name" => "batch_timeout",
        "category" => "Batching",
        "default" => @batch_timeout,
        "description" => "The maximum age of a batch before it is flushed.",
        "null" => false,
        "simple" => batch_is_simple,
        "type" => "int",
        "unit" => "seconds"
      })

    # Common options - requests

    @options.request_in_flight_limit =
      Option.new({
        "name" => "request_in_flight_limit",
        "category" => "Requests",
        "default" => @request_in_flight_limit,
        "description" => "The maximum number of in-flight requests allowed at any given time.",
        "null" => false,
        "type" => "int"
      })

    @options.request_rate_limit_duration_secs =
      Option.new({
        "name" => "request_rate_limit_duration_secs",
        "category" => "Requests",
        "default" => @request_rate_limit_duration_secs,
        "description" => "The window used for the `request_rate_limit_num` option",
        "null" => false,
        "type" => "int",
        "unit" => "seconds"
      })

    @options.request_rate_limit_num =
      Option.new({
        "name" => "request_rate_limit_num",
        "category" => "Requests",
        "default" => @request_rate_limit_num,
        "description" => "The maximum number of requests allowed within the `request_rate_limit_duration_secs` window.",
        "null" => false,
        "type" => "int"
      })

    @options.request_retry_attempts =
      Option.new({
        "name" => "request_retry_attempts",
        "category" => "Requests",
        "default" => @request_retry_attempts,
        "description" => "The maximum number of retries to make for failed requests.",
        "null" => false,
        "type" => "int"
      })

    @options.request_retry_backoff_secs =
      Option.new({
        "name" => "request_retry_backoff_secs",
        "category" => "Requests",
        "default" => @request_retry_backoff_secs,
        "description" => "The amount of time to wait before attempting a failed request again.",
        "null" => false,
        "type" => "int",
        "unit" => "seconds"
      })

    @options.request_timeout_secs =
      Option.new({
        "name" => "request_timeout_secs",
        "category" => "Requests",
        "default" => @request_timeout_secs,
        "description" => "The maximum time a request can take before being aborted. It is highly recommended that you do not lower value below the service's internal timeout, as this could create orphaned requests, pile on retries, and result in deuplicate data downstream.",
        "null" => false,
        "type" => "int",
        "unit" => "seconds"
      })
  end
end
