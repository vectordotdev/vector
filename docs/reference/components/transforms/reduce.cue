package metadata

components: transforms: reduce: {
  title: "#{component.title}"
  short_description: "Accepts log events and allows you to combine multiple events into a single event based on a set of identifiers."
  description: "Accepts log events and allows you to combine multiple events into a single event based on a set of identifiers."

  _features: {
    checkpoint: enabled: false
    multiline: enabled: false
    tls: enabled: false
  }

  classes: {
    commonly_used: false
    function: "aggregate"
  }

  statuses: {
    development: "beta"
  }

  support: {
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
    ends_when: {
      common: true
      description: "A condition used to distinguish the final event of a transaction. If this condition resolves to true for an event the transaction it belongs to is immediately flushed."
      required: false
        type: object: {
          default: null
          examples: []
          options: {
            type: string: {
              default: "check_fields"
              enum: {
                check_fields: "Allows you to check individual fields against a list of conditions."
                is_log: "Returns true if the event is a log."
                is_metric: "Returns true if the event is a metric."
              }
            }
            type: string: {
              default: null
              examples: [{"message.eq":"this is the content to match against"},{"message.eq":["match this","or this"]}]
            }
            type: bool: default: null
            type: string: {
              default: null
              examples: [{"method.neq":"POST"},{"method.neq":["POST","GET"]}]
            }

            type: string: {
              default: null
              examples: [{"message.contains":"foo"},{"message.contains":["foo","bar"]}]
            }
            type: string: {
              default: null
              examples: [{"environment.ends_with":"-staging"},{"environment.ends_with":["-staging","-running"]}]
            }
            type: string: {
              default: null
              examples: [{"message.ip_cidr_contains":"10.0.0.0/8"},{"message.ip_cidr_contains":["2000::/10","192.168.0.0/16"]}]
            }
            type: string: {
              default: null
              examples: [{"message.regex":" (any|of|these|five|words) "}]
            }
            type: string: {
              default: null
              examples: [{"environment.starts_with":"staging-"},{"environment.starts_with":["staging-","running-"]}]
            }
          }
        }
    }
    expire_after_ms: {
      common: false
      description: "A maximum period of time to wait after the last event is received before a combined event should be considered complete."
      required: false
        type: int: {
          default: 30000
        }
    }
    flush_period_ms: {
      common: false
      description: "Controls the frequency that Vector checks for (and flushes) expired events."
      required: false
        type: int: {
          default: 1000
        }
    }
    identifier_fields: {
      common: true
      description: "An ordered list of fields by which to group events. Each group is combined independently, allowing you to keep independent events separate. When no fields are specified, all events will be combined in a single group. Events missing a specified field will be combined in their own group."
      required: false
        type: "[string]": {
          default: []
          examples: [["request_id"],["user_id","transaction_id"]]
        }
    }
    merge_strategies: {
      common: false
      description: "A map of field names to custom merge strategies. For each field specified this strategy will be used for combining events rather than the default behavior.\n\nThe default behavior is as follows:\n\n1. The first value of a string field is kept, subsequent values are discarded.\n2. For timestamp fields the first is kept and a new field `[field-name]_end` is\n   added with the last received timestamp value.\n3. Numeric values are summed."
      required: false
        type: object: {
          default: null
          examples: []
          options: {
            type: string: {
              enum: {
                array: "Each value is appended to an array."
                concat: "Concatenate each string value (delimited with a space)."
                discard: "Discard all but the first value found."
                sum: "Sum all numeric values."
                max: "The maximum of all numeric values."
                min: "The minimum of all numeric values."
              }
            }
          }
        }
    }
  }
}