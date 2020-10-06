package metadata

components: transforms: tokenizer: {
  title: "Tokenizer"
  short_description: "Accepts log events and allows you to tokenize a field's value by splitting on white space, ignoring special wrapping characters, and zip the tokens into ordered field names."
  long_description: "Accepts log events and allows you to tokenize a field's value by splitting on white space, ignoring special wrapping characters, and zip the tokens into ordered field names."

  classes: {
    commonly_used: true
    function: "parse"
  }

  features: {
    tls: enabled: false
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
      description: "The log field to tokenize."
      required: false
      warnings: []
      type: string: {
        default: "message"
        examples: ["message","parent.child"]
      }
    }
    field_names: {
      description: "The log field names assigned to the resulting tokens, in order."
      required: true
      warnings: []
      type: "[string]": {
        examples: [["timestamp","level","message","parent.child"]]
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
