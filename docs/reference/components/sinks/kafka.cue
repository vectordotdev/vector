package metadata

components: sinks: kafka: {
  title: "Kafka"
  short_description: "Streams log events to [Apache Kafka][urls.kafka] via the [Kafka protocol][urls.kafka_protocol]."
  long_description: "[Apache Kafka][urls.kafka] is an open-source project for a distributed publish-subscribe messaging system rethought as a distributed commit log. Kafka stores messages in topics that are partitioned and replicated across multiple brokers in a cluster. Producers send messages to topics from which consumers read. These features make it an excellent candidate for durably storing logs and metrics data."

  _features: {
    batch: enabled: false
    buffer: enabled: true
    checkpoint: enabled: false
    compression: {
      enabled: true
      default: "none"
      gzip: true
    }
    encoding: {
      enabled: true
      default: null
      json: null
      ndjson: null
      text: null
    }
    healthcheck: enabled: true
    multiline: enabled: false
    request: enabled: false
    tls: {
      enabled: true
      can_enable: true
      can_verify_certificate: false
      can_verify_hostname: false
      enabled_default: false
    }
  }

  classes: {
    commonly_used: true
    function: "transmit"
    service_providers: ["AWS","Confluent"]
  }

  statuses: {
    delivery: "at_least_once"
    development: "stable"
  }

  support: {
      input_types: ["log"]

    platforms: {
      "aarch64-unknown-linux-gnu": true
      "aarch64-unknown-linux-musl": true
      "x86_64-apple-darwin": true
      "x86_64-pc-windows-msv": true
      "x86_64-unknown-linux-gnu": true
      "x86_64-unknown-linux-musl": true
    }

    requirements: [
      """
      [Kafka][urls.kafka] version `>= 0.8` is required.
      """,
    ]
    warnings: []
  }

  configuration: {
    bootstrap_servers: {
      description: "A comma-separated list of host and port pairs that are the addresses of the Kafka brokers in a \"bootstrap\" Kafka cluster that a Kafka client connects to initially to bootstrap itself."
      required: true
      warnings: []
      type: string: {
        examples: ["10.14.22.123:9092,10.14.23.332:9092"]
      }
    }
    key_field: {
      description: "The log field name to use for the topic key. If unspecified, the key will be randomly generated. If the field does not exist on the log, a blank value will be used."
      required: true
      warnings: []
      type: string: {
        examples: ["user_id"]
      }
    }
    librdkafka_options: {
      common: false
      description: "Advanced options. See [librdkafka documentation][urls.librdkafka_config] for details.\n"
      required: false
      warnings: []
      type: object: {
        examples: [{"client.id":"${ENV_VAR}"},{"fetch.error.backoff.ms":"1000"},{"socket.send.buffer.bytes":"100"}]
        options: {}
      }
    }
    message_timeout_ms: {
      common: false
      description: "Local message timeout."
      required: false
      warnings: []
      type: uint: {
        default: 300000
        examples: [150000,450000]
        unit: null
      }
    }
    sasl: {
      common: false
      description: "Options for SASL/SCRAM authentication support."
      required: false
      warnings: []
      type: object: {
        examples: []
        options: {
          enabled: {
            common: true
            description: "Enable SASL/SCRAM authentication to the remote. (Not supported on Windows at this time.)"
            required: false
            warnings: []
            type: bool: default: null
          }
          mechanism: {
            common: true
            description: "The Kafka SASL/SCRAM mechanisms."
            required: false
            warnings: []
            type: string: {
              default: null
              examples: ["SCRAM-SHA-256","SCRAM-SHA-512"]
            }
          }
          password: {
            common: true
            description: "The Kafka SASL/SCRAM authentication password."
            required: false
            warnings: []
            type: string: {
              default: null
              examples: ["password"]
            }
          }
          username: {
            common: true
            description: "The Kafka SASL/SCRAM authentication username."
            required: false
            warnings: []
            type: string: {
              default: null
              examples: ["username"]
            }
          }
        }
      }
    }
    socket_timeout_ms: {
      common: false
      description: "Default timeout for network requests."
      required: false
      warnings: []
      type: uint: {
        default: 60000
        examples: [30000,90000]
        unit: null
      }
    }
    topic: {
      description: "The Kafka topic name to write events to."
      required: true
      warnings: []
      type: string: {
        examples: ["topic-1234","logs-{{unit}}-%Y-%m-%d"]
      }
    }
  }
}

