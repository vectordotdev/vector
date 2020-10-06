package metadata

components: sinks: http: {
  title: "HTTP"
  short_description: "Batches log events to a generic [HTTP][urls.http] endpoint."
  long_description: "Batches log events to a generic [HTTP][urls.http] endpoint."

  classes: {
    commonly_used: true
    function: "transmit"
    service_providers: []
  }

  features: {
    batch: {
      enabled: true
      common: true,
      max_bytes: 1049000,
      max_events: null,
      timeout_secs: 1
    }
    buffer: enabled: true
    compression: {
      enabled: true
      default: null
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
    request: {
      enabled: true
      in_flight_limit: 10,
      rate_limit_duration_secs: 1,
      rate_limit_num: 9000000000000000000,
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

    requirements: []
    warnings: []
  }

  configuration: {
    auth: {
      common: false
      description: "Options for the authentication strategy."
      required: false
      warnings: []
      type: object: {
        examples: []
        options: {
          password: {
            description: "The basic authentication password."
            required: true
            warnings: []
            type: string: {
              examples: ["${HTTP_PASSWORD}","password"]
            }
          }
          strategy: {
            description: "The authentication strategy to use."
            required: true
            warnings: []
            type: string: {
              enum: {
                basic: "The [basic authentication strategy][urls.basic_auth]."
                bearer: "The bearer token authentication strategy."
              }
            }
          }
          token: {
            description: "The token to use for bearer authentication"
            required: true
            warnings: []
            type: string: {
              examples: ["${API_TOKEN}","xyz123"]
            }
          }
          user: {
            description: "The basic authentication user name."
            required: true
            warnings: []
            type: string: {
              examples: ["${HTTP_USERNAME}","username"]
            }
          }
        }
      }
    }
    headers: {
      common: false
      description: "Options for custom headers."
      required: false
      warnings: []
      type: object: {
        examples: [{"Authorization":"${HTTP_TOKEN}"},{"X-Powered-By":"Vector"}]
        options: {}
      }
    }
    uri: {
      description: "The full URI to make HTTP requests to. This should include the protocol and host, but can also include the port, path, and any other valid part of a URI."
      required: true
      warnings: []
      type: string: {
        examples: ["https://10.22.212.22:9000/endpoint"]
      }
    }
  }
}

