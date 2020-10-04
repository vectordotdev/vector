package metadata

components: transforms: split: {
  title: "#{component.title}"
  short_description: "Accepts log events and allows you to split a field's value on a _literal_ separator and zip the tokens into ordered field names."
  long_description: "Accepts log events and allows you to split a field's value on a _literal_ separator and zip the tokens into ordered field names."

  _features: {
    checkpoint: enabled: false
    multiline: enabled: false
    tls: enabled: false
  }

  classes: {
    commonly_used: false
    function: "parse"
    service_providers: []
  }

  statuses: {
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
    drop_field: {
      common: true
      description: "If `true` the `field` will be dropped after parsing."
      required: false
      warnings: []
      type: bool: default: true
    }
    field: {
      common: true
      description: "The field to apply the split on."
      required: false
      warnings: []
      type: string: {
        default: "message"
        examples: ["message","parent.child"]
      }
    }
    field_names: {
      common: true
      description: "The field names assigned to the resulting tokens, in order."
      required: true
      warnings: []
      type: "[string]": {
        examples: [["timestamp","level","message","parent.child"]]
      }
    }
    separator: {
      common: true
      description: "The separator to split the field on. If no separator is given, it will split on all whitespace. 'Whitespace' is defined according to the terms of the [Unicode Derived Core Property `White_Space`][urls.unicode_whitespace]."
      required: false
      warnings: []
      type: "[string]": {
        default: "[whitespace]"
        examples: [","]
      }
    }
    types: {
      common: true
      description: "Key/value pairs representing mapped log field names and types. This is used to coerce log fields into their proper types."
      required: false
      warnings: []
      type: object: {
        examples: [{"status":"int"},{"duration":"float"},{"success":"bool"},{"timestamp":"timestamp|%F"},{"timestamp":"timestamp|%a %b %e %T %Y"},{"parent":{"child":"int"}}]
        options: {}
      }
    }
  }
}