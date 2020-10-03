package metadata

components: transforms: filter: {
  title: "#{component.title}"
  short_description: "Accepts log and metric events and allows you to select events based on a set of logical conditions."
  description: "Accepts log and metric events and allows you to select events based on a set of logical conditions."

  _features: {
    checkpoint: enabled: false
    multiline: enabled: false
    tls: enabled: false
  }

  classes: {
    commonly_used: true
    function: "filter"
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
    condition: {
      common: true
      description: "The set of logical conditions to be matched against every input event. Only messages that pass all conditions will be forwarded."
      required: true
        type: object: {
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
  }
}