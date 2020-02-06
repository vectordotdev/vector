#encoding: utf-8

require_relative "option"
require_relative "sink"

class BatchingSink < Sink
  attr_reader :batch_max_events,
    :batch_max_size,
    :batch_timeout_secs,
    :request_in_flight_limit,
    :request_rate_limit_duration,
    :request_rate_limit_num,
    :request_retry_attempts,
    :request_retry_initial_backoff_secs,
    :request_retry_max_duration_secs,
    :request_timeout_secs

  def initialize(hash)
    super(hash)

    batch_is_common = hash["batch_is_common"] == true
    @batch_max_events = hash["batch_max_events"]
    @batch_max_size = hash["batch_max_size"]
    @batch_timeout_secs = hash.fetch("batch_timeout_secs")
    @request_in_flight_limit = hash.fetch("request_in_flight_limit")
    @request_rate_limit_duration_secs = hash.fetch("request_rate_limit_duration_secs")
    @request_rate_limit_num = hash.fetch("request_rate_limit_num")
    @request_retry_attempts = hash.fetch("request_retry_attempts")
    @request_retry_initial_backoff_secs = hash.fetch("request_retry_initial_backoff_secs")
    @request_retry_max_duration_secs = hash.fetch("request_retry_max_duration_secs")
    @request_timeout_secs = hash.fetch("request_timeout_secs")

    # Requirements

    if !@batch_max_events && !@batch_max_size
      raise("#{self.class.name} must supply one of `batch_max_events` or `batch_max_size`")
    end

    if @batch_max_events && @batch_max_size
      raise("#{self.class.name} must supply either `batch_max_events` or `batch_max_size`")
    end

    # Common options - batching

    batch_options = {}

    if @batch_max_events
      batch_options["max_events"] =
        {
          "default" => @batch_max_events,
          "description" => "The maximum size of a batch before it is flushed.",
          "null" => false,
          "common" => batch_is_common,
          "type" => "int",
          "unit" => "bytes"
        }
    end

    if @batch_max_size
      batch_options["max_size"] =
        {
          "default" => @batch_max_size,
          "description" => "The maximum size of a batch before it is flushed.",
          "null" => false,
          "common" => batch_is_common,
          "type" => "int",
          "unit" => "bytes"
        }
    end

    batch_options["timeout_secs"] =
      {
        "default" => @batch_timeout_secs,
        "description" => "The maximum age of a batch before it is flushed.",
        "null" => false,
        "common" => batch_is_common,
        "type" => "int",
        "unit" => "seconds"
      }

    @options.batch =
      Option.new({
        "name" => "batch",
        "description" => "Configures the sink batching behavior.",
        "options" => batch_options,
        "null" => false,
        "type" => "table"
      })

    # Common options - requests

    request_options = {
      "in_flight_limit" =>
        {
          "default" => @request_in_flight_limit,
          "description" => "The maximum number of in-flight requests allowed at any given time.",
          "null" => false,
          "type" => "int"
        },

      "rate_limit_duration_secs" =>
        {
          "default" => @request_rate_limit_duration_secs,
          "description" => "The window used for the `rate_limit_num` option",
          "null" => false,
          "type" => "int",
          "unit" => "seconds"
        },

      "rate_limit_num" =>
        {
          "default" => @request_rate_limit_num,
          "description" => "The maximum number of requests allowed within the `rate_limit_duration_secs` window.",
          "null" => false,
          "type" => "int"
        },

      "retry_attempts" =>
        {
          "default" => @request_retry_attempts,
          "description" => "The maximum number of retries to make for failed requests.",
          "null" => false,
          "type" => "int"
        },

      "retry_initial_backoff_secs" =>
        {
          "default" => @request_retry_initial_backoff_secs,
          "description" => "The amount of time to wait before attempting the first retry for a failed request. Once, the first retry has failed the fibonacci sequence will be used to select future backoffs.",
          "null" => false,
          "type" => "int",
          "unit" => "seconds"
        },

      "retry_max_duration_secs" =>
        {
          "default" => @request_retry_max_duration_secs,
          "description" => "The maximum amount of time to wait between retries.",
          "null" => false,
          "type" => "int",
          "unit" => "seconds"
        },

      "timeout_secs" =>
        {
          "default" => @request_timeout_secs,
          "description" => "The maximum time a request can take before being aborted. It is highly recommended that you do not lower value below the service's internal timeout, as this could create orphaned requests, pile on retries, and result in duplicate data downstream.",
          "null" => false,
          "type" => "int",
          "unit" => "seconds"
        }
    }

    @options.request =
      Option.new({
        "name" => "request",
        "description" => "Configures the sink request behavior.",
        "options" => request_options,
        "null" => false,
        "type" => "table"
      })
  end
end
