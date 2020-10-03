package metadata

components: transforms: regex_parser: {
  title: "#{component.title}"
  short_description: "Accepts log events and allows you to parse a log field's value with a [Regular Expression][urls.regex]."
  description: "Accepts log events and allows you to parse a log field's value with a [Regular Expression][urls.regex]."

  _features: {
    checkpoint: enabled: false
    multiline: enabled: false
    tls: enabled: false
  }

  classes: {
    commonly_used: true
    function: "parse"
  }

  statuses: {
    development: "stable"
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
    drop_field: {
      common: true
      description: "If the specified `field` should be dropped (removed) after parsing."
      required: false
        type: bool: default: true
    }
    field: {
      common: true
      description: "The log field to parse."
      required: false
        type: string: {
          default: "message"
          examples: ["message","parent.child"]
        }
    }
    overwrite_target: {
      common: false
      description: "If `target_field` is set and the log contains a field of the same name as the target, it will only be overwritten if this is set to `true`."
      required: false
        type: bool: default: true
    }
    patterns: {
      common: true
      description: "The Regular Expressions to apply. Do not include the leading or trailing `/` in any of the expressions."
      required: true
        type: "[string]": {
          examples: [["^(?P<timestamp>[\\\\w\\\\-:\\\\+]+) (?P<level>\\\\w+) (?P<message>.*)$"]]
        }
    }
    target_field: {
      common: false
      description: "If this setting is present, the parsed fields will be inserted into the log as a sub-object with this name. If a field with the same name already exists, the parser will fail and produce an error."
      required: false
        type: string: {
          default: null
          examples: ["root_field","parent.child"]
        }
    }
    types: {
      common: true
      description: "Key/value pairs representing mapped log field names and types. This is used to coerce log fields into their proper types."
      required: false
        type: object: {
          default: null
          examples: []
          options: {
            type: string: {
              default: null
              enum: {
                bool: "Coerces `\"true\"`/`/\"false\"`, `\"1\"`/`\"0\"`, and `\"t\"`/`\"f\"` values into boolean."
                float: "Coerce to a 64 bit float."
                int: "Coerce to a 64 bit integer."
                string: "Coerce to a string."
                timestamp: "Coerces to a Vector timestamp. [`strptime` specifiers][urls.strptime_specifiers] must be used to parse the string."
              }
            }
          }
        }
    }
  }
}