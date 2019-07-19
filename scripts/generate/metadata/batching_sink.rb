#encoding: utf-8

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
      "unit" => "seconds"
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
  end
end