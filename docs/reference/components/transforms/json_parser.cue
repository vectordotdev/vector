package metadata

components: transforms: json_parser: {
  title: "#{component.title}"
  short_description: "Accepts log events and allows you to parse a log field value as JSON."
  long_description: "Accepts log events and allows you to parse a log field value as JSON."

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
      description: "If the specified `field` should be dropped (removed) after parsing. If parsing fails, the field will not be removed, irrespective of this setting."
      required: false
      warnings: []
      type: bool: default: true
    }
    drop_invalid: {
      common: true
      description: "If `true` events with invalid JSON will be dropped, otherwise the event will be kept and passed through."
      required: true
      warnings: []
      type: bool: default: null
    }
    field: {
      common: true
      description: "The log field to decode as JSON. Must be a `string` value type."
      required: false
      warnings: []
      type: string: {
        default: "message"
        examples: ["message","parent.child","array[0]"]
      }
    }
    overwrite_target: {
      common: false
      description: "If `target_field` is set and the log contains a field of the same name as the target, it will only be overwritten if this is set to `true`."
      required: false
      warnings: []
      type: bool: default: false
    }
    target_field: {
      common: false
      description: "If this setting is present, the parsed JSON will be inserted into the log as a sub-object with this name. If a field with the same name already exists, the parser will fail and produce an error."
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["root_field","parent.child"]
      }
    }
  }
}
