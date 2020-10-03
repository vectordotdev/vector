package metadata

components: transforms: coercer: {
  title: "#{component.title}"
  short_description: "Accepts log events and allows you to coerce log fields into fixed types."
  description: "Accepts log events and allows you to coerce log fields into fixed types."

  _features: {
    checkpoint: enabled: false
    multiline: enabled: false
    tls: enabled: false
  }

  classes: {
    commonly_used: false
    function: "schema"
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
    drop_unspecified: {
      common: false
      description: "Set to `true` to drop all fields that are not specified in the `types` table. Make sure both `message` and `timestamp` are specified in the `types` table as their absense will cause the original message data to be dropped along with other extraneous fields."
      required: false
        type: bool: default: false
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