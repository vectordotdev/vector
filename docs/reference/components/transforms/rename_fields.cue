package metadata

components: transforms: rename_fields: {
  title: "Rename Fields"
  short_description: "Accepts log events and allows you to rename one or more log fields."
  long_description: "Accepts log events and allows you to rename one or more log fields."

  classes: {
    commonly_used: false
    function: "schema"
  }

  features: {
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
    notices: []
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
      warnings: [
        {
          visibility_level: "option"
          text: "Vector makes no guarantee on the order of execution. If two rename operations must be performed in a specific order, it is recommended to split them up across two separate rename transforms."
        }
      ]
      type: object: {
        examples: [{"old_field_name":"new_field_name"},{"parent":{"old_child_name":"parent.new_child_name"}}]
        options: {}
      }
    }
  }
}
