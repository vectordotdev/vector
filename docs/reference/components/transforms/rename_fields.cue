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

  output: logs: {
    // Rename fields only acts on fields specified in the `fields` setting. It remaps from one event type to another.
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

  how_it_works: {
    complex_processing: {
      title: "Complex Processing"
      body: #"""
      If you encounter limitations with the `rename_fields` transform then we recommend using a
      [runtime](vector_programmable_transforms) transform. These transforms are designed for complex
      processing and give you the power of full programming runtime.
      """#
    }
    conflicts: {
      title: "Conflicts"
      body: #"""
            Keys specified in this transform will replace existing keys.
            """#
      sub_sections: [
        {
          title: "Key Conflicts"
          body: #"""
                Keys specified in this transform will replace existing keys.

                <Alert type="warning">

                Vector makes no guarantee on the order of execution. If two rename
                operations must be performed in a specific order, it is recommended to split
                them up across two separate rename transforms.

                </Alert>
                """#
        },
        {
          title: "Nested Key Conflicts"
          body: #"""
                Keys are renamed in a deep fashion. They will not replace any ancestor
                objects. For example, given the following `log` event:

                ```javascript
                {
                  "root": "value2",
                  "parent": {
                    "child1": "value1"
                  }
                }
                ```

                And the following configuration:

                ```toml
                [transforms.rename_nested_field]
                  type = "rename_fields"
                  fields.root = "parent.child2"
                ```

                Will result in the following log event:

                ```javascript
                {
                  "parent": {
                    "child1": "value1",
                    "child2": "value2"
                  }
                }
                ```

                Notice that `parent.child1` field was preserved.
                """#
        }
      ]
    }
  }
}
