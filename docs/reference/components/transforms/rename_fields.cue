package metadata

components: transforms: rename_fields: {
  title: "Rename Fields"
  short_description: "Accepts log events and allows you to rename one or more log fields."
  long_description: "Accepts log events and allows you to rename one or more log fields."

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
      description: "If set to `true`, after renaming fields, remove any parent objects of the old field that are now empty."
      required: false
      warnings: []
      type: bool: default: false
    }
    fields: {
      description: "A table of old-key/new-key pairs representing the keys to be moved in the event."
      required: true
      warnings: []
      type: object: {
        examples: [{"old_field_name":"new_field_name"},{"parent":{"old_child_name":"parent.new_child_name"}}]
        options: {}
      }
    }
  }
}
