package metadata

components: transforms: swimlanes: {
  title: "#{component.title}"
  short_description: "Accepts log events and allows you to route events across parallel streams using logical filters."
  description: "Accepts log events and allows you to route events across parallel streams using logical filters."

  _features: {
    checkpoint: enabled: false
    multiline: enabled: false
    tls: enabled: false
  }

  classes: {
    commonly_used: false
    function: "route"
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
    lanes: {
      common: true
      description: "A table of swimlane identifiers to logical conditions representing the filter of the swimlane. Each swimlane can then be referenced as an input by other components with the name `<transform_name>.<swimlane_id>`."
      required: true
        type: object: {
          examples: []
          options: {
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
  }
}