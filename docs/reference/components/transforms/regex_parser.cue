package metadata

components: transforms: regex_parser: {
  title: "#{component.title}"
  short_description: "Accepts log events and allows you to parse a log field's value with a [Regular Expression][urls.regex]."
  long_description: "Accepts log events and allows you to parse a log field's value with a [Regular Expression][urls.regex]."

  _features: {
    checkpoint: enabled: false
    multiline: enabled: false
    tls: enabled: false
  }

  classes: {
    commonly_used: true
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
      description: "If the specified `field` should be dropped (removed) after parsing."
      required: false
      warnings: []
      type: bool: default: true
    }
    field: {
      common: true
      description: "The log field to parse."
      required: false
      warnings: []
      type: string: {
        default: "message"
        examples: ["message","parent.child"]
      }
    }
    overwrite_target: {
      common: false
      description: "If `target_field` is set and the log contains a field of the same name as the target, it will only be overwritten if this is set to `true`."
      required: false
      warnings: []
      type: bool: default: true
    }
    patterns: {
      common: true
      description: "The Regular Expressions to apply. Do not include the leading or trailing `/` in any of the expressions."
      required: true
      warnings: []
      type: "[string]": {
        examples: [["^(?P<timestamp>[\\\\w\\\\-:\\\\+]+) (?P<level>\\\\w+) (?P<message>.*)$"]]
      }
    }
    target_field: {
      common: false
      description: "If this setting is present, the parsed fields will be inserted into the log as a sub-object with this name. If a field with the same name already exists, the parser will fail and produce an error."
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["root_field","parent.child"]
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