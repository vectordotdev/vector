package metadata

components: sinks: clickhouse: {
  title: "#{component.title}"
  short_description: "Batches log events to [Clickhouse][urls.clickhouse] via the [`HTTP` Interface][urls.clickhouse_http]."
  long_description: "[ClickHouse][urls.clickhouse] is an open-source column-oriented database management system that manages extremely large volumes of data, including non-aggregated data, in a stable and sustainable manner and allows generating custom data reports in real time. The system is linearly scalable and can be scaled up to store and process trillions of rows and petabytes of data. This makes it an best-in-class storage for logs and metrics data."

  _features: {
    batch: {
      enabled: true
      common: false,
      max_bytes: 1049000,
      max_events: null,
      timeout_secs: 1
    }
    buffer: enabled: true
    checkpoint: enabled: false
    compression: {
      enabled: true
      default: "gzip"
      gzip: true
    }
    encoding: {
      enabled: true
      default: null
      ndjson: null
      text: null
    }
    healthcheck: enabled: true
    multiline: enabled: false
    request: {
      enabled: true
      common: false,
      in_flight_limit: 5,
      rate_limit_duration_secs: 1,
      rate_limit_num: 5,
      retry_initial_backoff_secs: 1,
      retry_max_duration_secs: 10,
      timeout_secs: 30
    }
    tls: {
      enabled: true
      can_enable: false
      can_verify_certificate: true
      can_verify_hostname: true
      enabled_default: false
    }
  }

  classes: {
    commonly_used: true
    function: "transmit"
    service_providers: ["Yandex"]
  }

  statuses: {
    delivery: "at_least_once"
    development: "beta"
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
      [Clickhouse][urls.clickhouse] version `>= 1.1.54378` is required.
      """,
    ]
    warnings: []
  }

  configuration: {
    auth: {
      common: false
      description: "Options for the authentication strategy."
      groups: []
      required: false
      warnings: []
        type: object: {
          default: null
          examples: []
          options: {
            type: string: {
              examples: ["${CLICKHOUSE_PASSWORD}","password"]
            }
            type: string: {
              enum: {
                basic: "The [basic authentication strategy][urls.basic_auth]."
                bearer: "The bearer token authentication strategy."
              }
            }
            type: string: {
              examples: ["${API_TOKEN}","xyz123"]
            }
            type: string: {
              examples: ["${CLICKHOUSE_USERNAME}","username"]
            }
          }
        }
    }
    compression: {
      common: true
      description: "The compression strategy used to compress the encoded event data before transmission."
      groups: []
      required: false
      warnings: []
        type: string: {
          default: "gzip"
          enum: {
            none: "No compression."
            gzip: "[Gzip][urls.gzip] standard DEFLATE compression."
          }
        }
    }
    database: {
      common: true
      description: "The database that contains the stable that data will be inserted into."
      groups: []
      required: false
      warnings: []
        type: string: {
          default: null
          examples: ["mydatabase"]
        }
    }
    table: {
      common: true
      description: "The table that data will be inserted into."
      groups: []
      required: true
      warnings: []
        type: string: {
          examples: ["mytable"]
        }
    }
  }
}
