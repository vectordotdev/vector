package metadata

components: transforms: remove_fields: {
  title: "#{component.title}"
  short_description: "Accepts log events and allows you to remove one or more log fields."
  long_description: "Accepts log events and allows you to remove one or more log fields."

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
    drop_empty: {
      common: false
      description: "If set to `true`, after removing fields, remove any parent objects that are now empty."
      required: false
      warnings: []
      type: bool: default: false
    }
    fields: {
      description: "The log field names to drop."
      required: true
      warnings: []
      type: "[string]": {
        examples: [["field1","field2","parent.child"]]
      }
    }
  }
}
