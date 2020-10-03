package metadata

components: transforms: add_fields: {
  title: "#{component.title}"
  short_description: "Accepts log events and allows you to add one or more log fields."
  description: "Accepts log events and allows you to add one or more log fields."

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
    fields: {
      common: true
      description: "A table of key/value pairs representing the keys to be added to the event."
      required: true
        type: object: {
          examples: [{"string_field":"string value"},{"env_var_field":"${ENV_VAR}"},{"templated_field":"{{ my_other_field }}"},{"int_field":1},{"float_field":1.2},{"bool_field":true},{"timestamp_field":"1979-05-27 00:32:00 -0700"},{"parent":{"child_field":"child_value"}},{"list_field":["first","second","third"]}]
          options: {
          }
        }
    }
    overwrite: {
      common: true
      description: "By default, fields will be overridden. Set this to `false` to avoid overwriting values.\n"
      required: false
        type: bool: default: true
    }
  }
}
