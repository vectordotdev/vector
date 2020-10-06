package metadata

components: transforms: filter: {
  title: "Filter"
  short_description: "Accepts log and metric events and allows you to select events based on a set of logical conditions."
  long_description: "Accepts log and metric events and allows you to select events based on a set of logical conditions."

  classes: {
    commonly_used: true
    function: "filter"
  }

  features: {
    tls: enabled: false
  }

  statuses: {
    development: "beta"
  }

  support: {
    input_types: ["log","metric"]

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
      description: "The set of logical conditions to be matched against every input event. Only messages that pass all conditions will be forwarded."
      required: true
      warnings: []
      type: object: {
        examples: []
        options: {
          type: {
            common: true
            description: "The type of the condition to execute."
            required: false
            warnings: []
            type: string: {
              default: "check_fields"
              enum: {
                check_fields: "Allows you to check individual fields against a list of conditions."
                is_log: "Returns true if the event is a log."
                is_metric: "Returns true if the event is a metric."
              }
            }
          }
          "`[field-name]`.eq": {
            common: true
            description: "Check whether a field's contents exactly matches the value specified. This may be a single string or a list of strings, in which case this evaluates to true if any of the list matches."
            required: false
            warnings: []
            type: string: {
              default: null
              examples: [{"message.eq":"this is the content to match against"},{"message.eq":["match this","or this"]}]
            }
          }
          "`[field-name]`.exists": {
            common: false
            description: "Check whether a field exists or does not exist, depending on the provided value being `true` or `false` respectively."
            required: false
            warnings: []
            type: bool: default: null
          }
          "`[field-name]`.neq": {
            common: false
            description: "Check whether a field's contents does not match the value specified. This may be a single string or a list of strings, in which case this evaluates to false if any of the list matches."
            required: false
            warnings: []
            type: string: {
              default: null
              examples: [{"method.neq":"POST"},{"method.neq":["POST","GET"]}]
            }
          }
          "`[field-name]`.not_`[condition]`": {
            common: false
            description: "Check if the given `[condition]` does not match."
            required: false
            warnings: []
          }
          "`[field_name]`.contains": {
            common: true
            description: "Checks whether a string field contains a string argument. This may be a single string or a list of strings, in which case this evaluates to true if any of the list matches."
            required: false
            warnings: []
            type: string: {
              default: null
              examples: [{"message.contains":"foo"},{"message.contains":["foo","bar"]}]
            }
          }
          "`[field_name]`.ends_with": {
            common: true
            description: "Checks whether a string field ends with a string argument. This may be a single string or a list of strings, in which case this evaluates to true if any of the list matches."
            required: false
            warnings: []
            type: string: {
              default: null
              examples: [{"environment.ends_with":"-staging"},{"environment.ends_with":["-staging","-running"]}]
            }
          }
          "`[field_name]`.ip_cidr_contains": {
            common: false
            description: "Checks whether an IP field is contained within a given [IP CIDR][urls.cidr] (works with IPv4 and IPv6). This may be a single string or a list of strings, in which case this evaluates to true if the IP field is contained within any of the CIDRs in the list."
            required: false
            warnings: []
            type: string: {
              default: null
              examples: [{"message.ip_cidr_contains":"10.0.0.0/8"},{"message.ip_cidr_contains":["2000::/10","192.168.0.0/16"]}]
            }
          }
          "`[field_name]`.regex": {
            common: true
            description: "Checks whether a string field matches a [regular expression][urls.regex]. Vector uses the [documented Rust Regex syntax][urls.rust_regex_syntax]. Note that this condition is considerably more expensive than a regular string match (such as `starts_with` or `contains`) so the use of those conditions are preferred where possible."
            required: false
            warnings: []
            type: string: {
              default: null
              examples: [{"message.regex":" (any|of|these|five|words) "}]
            }
          }
          "`[field_name]`.starts_with": {
            common: true
            description: "Checks whether a string field starts with a string argument. This may be a single string or a list of strings, in which case this evaluates to true if any of the list matches."
            required: false
            warnings: []
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
