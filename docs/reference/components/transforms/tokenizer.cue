package metadata

components: transforms: tokenizer: {
  title: "#{component.title}"
  short_description: "Accepts log events and allows you to tokenize a field's value by splitting on white space, ignoring special wrapping characters, and zip the tokens into ordered field names."
  description: "Accepts log events and allows you to tokenize a field's value by splitting on white space, ignoring special wrapping characters, and zip the tokens into ordered field names."

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
      description: "If `true` the `field` will be dropped after parsing."
      required: false
        type: bool: default: true
    }
    field: {
      common: true
      description: "The log field to tokenize."
      required: false
        type: string: {
          default: "message"
          examples: ["message","parent.child"]
        }
    }
    field_names: {
      common: true
      description: "The log field names assigned to the resulting tokens, in order."
      required: true
        type: "[string]": {
          examples: [["timestamp","level","message","parent.child"]]
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